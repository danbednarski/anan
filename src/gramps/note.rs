//! Note, StyledText.

use serde::{Deserialize, Serialize};

use super::common::Typed;

/// Styled text interval: `{_class: "StyledTextTag", name: Typed, value: any, ranges: [[start,end], ...]}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyledTextTag {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub name: Typed,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub ranges: Vec<(i32, i32)>,
}

/// Note body with inline styling tags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyledText {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub string: String,
    #[serde(default)]
    pub tags: Vec<StyledTextTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    /// `format`: 0 = flowed, 1 = preformatted.
    #[serde(default)]
    pub format: i32,
    pub text: StyledText,
    pub r#type: Typed,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
