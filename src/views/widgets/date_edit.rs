//! Inline Date editor widget shared across Event and Citation forms.
//!
//! The full Gramps Date model supports simple/range/span dates in
//! seven calendars with a quality prefix. For Phase 6a we cover the
//! subset users actually encounter in most trees:
//!
//! - Simple single-year date with optional month/day
//! - Quality: regular / estimated / calculated
//! - Modifier: none / before / after / about / from / to / textonly
//! - Text-only override for when the user just wants to type a string
//!
//! Range ("between 1880 and 1890") and span ("from 1880 to 1890")
//! dates are out of scope for this widget; users who need them can
//! enter text-only form, which Gramps core round-trips as-is.
//!
//! Parsing is forgiving: blank year yields `None` (no date); any
//! partial date the user can plausibly type is accepted.

use iced::widget::{column, row, text, text_input};
use iced::{Element, Length};

use crate::app::Message;
use crate::gramps::date::{Date, DateVal};

/// In-progress date input state, carried inside an EditDraft so all
/// fields persist across re-renders. Every field is a string so the
/// user can type freely; parsing happens on save.
#[derive(Debug, Clone, Default)]
pub struct DateDraft {
    /// e.g. "1890" or "15/06/1890" or "Jun 1890" — we only parse
    /// simple year + optional month/day.
    pub year_s: String,
    pub month_s: String,
    pub day_s: String,
    /// 0-8, see [`crate::gramps::enums::modifier_name`].
    pub modifier_s: String,
    /// 0-2, see [`crate::gramps::enums::quality_name`].
    pub quality_s: String,
    /// Used when modifier == 6 (textonly).
    pub text_s: String,
}

impl DateDraft {
    /// Populate a draft from an existing Date.
    pub fn from_date(date: &Date) -> Self {
        let (day, month, year) = match &date.dateval {
            Some(DateVal::Simple(d, m, y, _)) => (*d, *m, *y),
            Some(DateVal::Range(d, m, y, _, _, _, _, _)) => (*d, *m, *y),
            None => (0, 0, date.year.unwrap_or(0)),
        };
        DateDraft {
            year_s: if year == 0 { String::new() } else { year.to_string() },
            month_s: if month == 0 { String::new() } else { month.to_string() },
            day_s: if day == 0 { String::new() } else { day.to_string() },
            modifier_s: date.modifier.to_string(),
            quality_s: date.quality.to_string(),
            text_s: date.text.clone(),
        }
    }

    /// Turn the draft into a `Date`, or `None` when every field is
    /// blank and modifier is the default. Returns `None` also when
    /// the user clearly wanted "no date" (all numerics blank, no
    /// text override).
    pub fn to_date(&self) -> Option<Date> {
        let year: i32 = self.year_s.trim().parse().unwrap_or(0);
        let month: i32 = self.month_s.trim().parse().unwrap_or(0);
        let day: i32 = self.day_s.trim().parse().unwrap_or(0);
        let modifier: i32 = self.modifier_s.trim().parse().unwrap_or(0);
        let quality: i32 = self.quality_s.trim().parse().unwrap_or(0);
        let text = self.text_s.clone();

        // "Nothing" case: no year, no text, modifier 0. Return None
        // so the caller clears the event's date field.
        if year == 0 && text.is_empty() && modifier != 6 {
            return None;
        }

        Some(Date {
            class: Some("Date".to_string()),
            calendar: 0,
            modifier,
            quality,
            dateval: if modifier == 6 {
                // textonly: no dateval needed
                Some(DateVal::Simple(0, 0, 0, false))
            } else {
                Some(DateVal::Simple(day, month, year, false))
            },
            text,
            sortval: 0,
            newyear: 0,
            format: None,
            year: if year == 0 { None } else { Some(year) },
        })
    }
}

/// Message factory — the caller provides closures that wrap a
/// `String` into its enclosing enum variant so a single date widget
/// can serve both Event and Citation edit forms. This keeps the
/// widget reusable without needing a full message trait.
pub struct DateMessages {
    pub on_year: fn(String) -> Message,
    pub on_month: fn(String) -> Message,
    pub on_day: fn(String) -> Message,
    pub on_modifier: fn(String) -> Message,
    pub on_quality: fn(String) -> Message,
    pub on_text: fn(String) -> Message,
}

/// Render the date-edit form. Inputs are labeled and laid out in
/// rows; unused fields (month/day when only year is entered) are
/// still visible but blank.
pub fn view<'a>(draft: &'a DateDraft, msgs: &'a DateMessages) -> Element<'a, Message> {
    let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
    let label = |s: &'static str| text(s).size(11).color(label_color);

    let ymd = row![
        column![
            label("Year"),
            text_input("e.g. 1890", &draft.year_s)
                .on_input(msgs.on_year)
                .padding(6)
                .width(Length::Fixed(90.0)),
        ]
        .spacing(4),
        column![
            label("Month (1–12)"),
            text_input("", &draft.month_s)
                .on_input(msgs.on_month)
                .padding(6)
                .width(Length::Fixed(70.0)),
        ]
        .spacing(4),
        column![
            label("Day"),
            text_input("", &draft.day_s)
                .on_input(msgs.on_day)
                .padding(6)
                .width(Length::Fixed(60.0)),
        ]
        .spacing(4),
    ]
    .spacing(10);

    let flags = row![
        column![
            label("Modifier (0 none · 1 before · 2 after · 3 about · 6 text · 7 from · 8 to)"),
            text_input("0", &draft.modifier_s)
                .on_input(msgs.on_modifier)
                .padding(6)
                .width(Length::Fixed(60.0)),
        ]
        .spacing(4),
        column![
            label("Quality (0 regular · 1 est · 2 calc)"),
            text_input("0", &draft.quality_s)
                .on_input(msgs.on_quality)
                .padding(6)
                .width(Length::Fixed(60.0)),
        ]
        .spacing(4),
    ]
    .spacing(10);

    let text_override = column![
        label("Free-form text (used when modifier = 6)"),
        text_input("e.g. \"early 1900s\"", &draft.text_s)
            .on_input(msgs.on_text)
            .padding(6),
    ]
    .spacing(4);

    column![
        label("Date"),
        ymd,
        flags,
        text_override,
    ]
    .spacing(10)
    .into()
}
