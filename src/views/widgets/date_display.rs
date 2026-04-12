//! Render a Gramps [`Date`] into a human-readable string.
//!
//! The Date object is load-bearing in genealogy — the same underlying
//! "event" may have a firm date, an estimate ("about 1890"), a range
//! ("between 1880 and 1890"), or just free-form text ("in the early
//! 1900s"). We fold all of those shapes down into a short display
//! string suitable for inline use in lists and summaries.
//!
//! For richer output (tooltip with quality/calendar/original-text) the
//! caller can inspect the fields directly; this module is intentionally
//! just *one* function so every view renders dates the same way.

use crate::gramps::date::{Date, DateVal};
use crate::gramps::enums::{calendar_name, modifier_name, quality_name};

/// Return a short human-readable rendering of `date`, or an empty string
/// if the date carries no information at all.
pub fn format(date: &Date) -> String {
    // Free-form text override wins.
    if !date.text.is_empty() && date.modifier == 6 {
        return date.text.clone();
    }

    if date.is_empty() {
        return String::new();
    }

    let core = match &date.dateval {
        Some(DateVal::Simple(d, m, y, _slash)) => format_ymd(*y, *m, *d),
        Some(DateVal::Range(d1, m1, y1, _, d2, m2, y2, _)) => {
            let a = format_ymd(*y1, *m1, *d1);
            let b = format_ymd(*y2, *m2, *d2);
            match date.modifier {
                5 => format!("from {a} to {b}"), // span
                _ => format!("{a} – {b}"),       // range
            }
        }
        None => String::new(),
    };

    let prefix = match date.modifier {
        1 => "before ",
        2 => "after ",
        3 => "about ",
        7 => "from ",
        8 => "to ",
        _ => "",
    };

    let quality_prefix = match date.quality {
        1 => "est. ",
        2 => "calc. ",
        _ => "",
    };

    let calendar_suffix = match date.calendar {
        0 => String::new(), // Gregorian — the default, not worth annotating
        other => calendar_name(other)
            .map(|n| format!(" ({n})"))
            .unwrap_or_default(),
    };

    let body = if core.is_empty() {
        // Range/span modifiers with no core — unusual but possible.
        if !date.text.is_empty() {
            date.text.clone()
        } else {
            String::new()
        }
    } else {
        format!("{quality_prefix}{prefix}{core}{calendar_suffix}")
    };

    body
}

fn format_ymd(year: i32, month: i32, day: i32) -> String {
    if year == 0 && month == 0 && day == 0 {
        return String::new();
    }
    if month == 0 {
        return year.to_string();
    }
    if day == 0 {
        return format!("{} {year}", month_name(month));
    }
    format!("{day} {} {year}", month_name(month))
}

fn month_name(m: i32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "?",
    }
}

/// Full multi-line breakdown for tooltips / detail views — one line each
/// for the rendered date, the quality, and the calendar if non-default.
pub fn long_form(date: &Date) -> String {
    let mut lines = vec![format(date)];
    if let Some(q) = quality_name(date.quality).filter(|s| !s.is_empty() && *s != "regular") {
        lines.push(format!("quality: {q}"));
    }
    if date.calendar != 0 {
        if let Some(cal) = calendar_name(date.calendar) {
            lines.push(format!("calendar: {cal}"));
        }
    }
    if let Some(m) = modifier_name(date.modifier).filter(|s| !s.is_empty()) {
        lines.push(format!("modifier: {m}"));
    }
    if !date.text.is_empty() && date.modifier != 6 {
        lines.push(format!("note: {}", date.text));
    }
    lines
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
