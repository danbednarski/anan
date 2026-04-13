//! Citation.

use serde::{Deserialize, Serialize};

use super::common::{Attribute, MediaRef};
use super::date::Date;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    /// Which Source this citation points at (may be empty when orphaned).
    #[serde(default)]
    pub source_handle: String,
    /// User-facing page/URL reference string.
    #[serde(default)]
    pub page: String,
    /// 0..4 — see `gen/lib/citation.py` (very low .. very high).
    #[serde(default)]
    pub confidence: i32,
    #[serde(default)]
    pub date: Option<Date>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
