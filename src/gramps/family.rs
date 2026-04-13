//! Family, ChildRef.

use serde::{Deserialize, Serialize};

use super::common::{Attribute, LdsOrd, MediaRef, Typed};
use super::event::EventRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildRef {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub r#ref: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    pub frel: Typed,
    pub mrel: Typed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Family {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub father_handle: Option<String>,
    #[serde(default)]
    pub mother_handle: Option<String>,
    #[serde(default)]
    pub child_ref_list: Vec<ChildRef>,
    pub r#type: Typed,
    #[serde(default)]
    pub event_ref_list: Vec<EventRef>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub lds_ord_list: Vec<LdsOrd>,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    /// `complete` flag — not in every sample but documented in Gramps core.
    #[serde(default)]
    pub complete: i32,
}
