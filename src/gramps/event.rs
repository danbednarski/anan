//! Event, EventRef.

use serde::{Deserialize, Serialize};

use super::common::{Attribute, MediaRef, Typed};
use super::date::Date;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRef {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub r#ref: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    pub role: Typed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    pub r#type: Typed,
    #[serde(default)]
    pub description: String,
    /// Handle of enclosing Place; empty string when unset.
    #[serde(default)]
    pub place: String,
    #[serde(default)]
    pub date: Option<Date>,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
