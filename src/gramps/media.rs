//! Media (top-level object, not the `MediaRef` cross-ref).

use serde::{Deserialize, Serialize};

use super::common::Attribute;
use super::date::Date;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub checksum: String,
    #[serde(default)]
    pub date: Option<Date>,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
