//! Gramps Date object.
//!
//! The Date is the single most complex leaf type in the Gramps model.
//! Observed shape (from `test-fixtures/sample.db` and `gen/lib/date.py`):
//!
//! ```json
//! {
//!   "_class": "Date",
//!   "calendar": 0,
//!   "modifier": 0,
//!   "quality": 0,
//!   "dateval": [day, month, year, slash],
//!   "text": "",
//!   "sortval": 2448642,
//!   "newyear": 0,
//!   "format": null,
//!   "year": 1992          // sometimes present, sometimes not
//! }
//! ```
//!
//! For range/span dates the `dateval` is an 8-tuple
//! `[d1, m1, y1, s1, d2, m2, y2, s2]`. We accept either shape with an
//! untagged enum.

use serde::{Deserialize, Serialize};

/// A calendar value (cal_*) — see `calendar_name()` in enums.rs.
pub type CalendarId = i32;
/// A modifier value (MOD_*) — none/before/after/about/range/span/...
pub type ModifierId = i32;
/// A quality value (QUAL_*) — regular/estimated/calculated.
pub type QualityId = i32;

/// `dateval` may be a simple 4-tuple or a range 8-tuple.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DateVal {
    /// `[day, month, year, slash]`
    Simple(i32, i32, i32, bool),
    /// `[d1, m1, y1, s1, d2, m2, y2, s2]`
    Range(i32, i32, i32, bool, i32, i32, i32, bool),
}

impl DateVal {
    /// The primary (first) year in either shape.
    pub fn year(&self) -> i32 {
        match self {
            DateVal::Simple(_, _, y, _) => *y,
            DateVal::Range(_, _, y, _, _, _, _, _) => *y,
        }
    }
}

/// Gramps Date.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Date {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub calendar: CalendarId,
    #[serde(default)]
    pub modifier: ModifierId,
    #[serde(default)]
    pub quality: QualityId,
    #[serde(default)]
    pub dateval: Option<DateVal>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub sortval: i64,
    #[serde(default)]
    pub newyear: i32,
    /// Free-form format override (rare).
    #[serde(default)]
    pub format: Option<String>,
    /// Precomputed year — sometimes written by Gramps, sometimes not.
    #[serde(default)]
    pub year: Option<i32>,
}

impl Date {
    /// True if the date carries no usable value at all.
    pub fn is_empty(&self) -> bool {
        self.sortval == 0
            && self.text.is_empty()
            && matches!(
                self.dateval,
                None | Some(DateVal::Simple(0, 0, 0, false))
            )
    }

    /// Primary year if present, else 0.
    pub fn primary_year(&self) -> i32 {
        if let Some(y) = self.year {
            return y;
        }
        self.dateval.as_ref().map(|d| d.year()).unwrap_or(0)
    }
}
