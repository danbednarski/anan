//! Source (not to be confused with Citation).

use serde::{Deserialize, Serialize};

use super::common::{Attribute, MediaRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub r#ref: String,
    #[serde(default)]
    pub call_number: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub media_type: Option<super::common::Typed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub pubinfo: String,
    #[serde(default)]
    pub abbrev: String,
    #[serde(default)]
    pub reporef_list: Vec<RepoRef>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
