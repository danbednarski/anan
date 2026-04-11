//! Tag.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    #[serde(default)]
    pub change: i64,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub priority: i32,
}
