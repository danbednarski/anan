//! Repository.

use serde::{Deserialize, Serialize};

use super::common::{Address, Typed, Url};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub name: String,
    pub r#type: Typed,
    #[serde(default)]
    pub address_list: Vec<Address>,
    #[serde(default)]
    pub urls: Vec<Url>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
