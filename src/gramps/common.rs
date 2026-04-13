//! Shared sub-objects used across multiple primary types.

use serde::{Deserialize, Serialize};

use super::date::Date;

/// Wrapper for Gramps tagged enum values: `{"_class": "...", "value": N, "string": "..."}`.
///
/// Built-in enum labels live in `enums.rs` (hardcoded from Gramps core source).
/// When `string` is non-empty it carries a *custom* user-defined label and
/// `value` points to the "custom" slot for that enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Typed<T = i32> {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub value: T,
    #[serde(default)]
    pub string: String,
}

/// Cross-reference to a media object from another primary object.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaRef {
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
    /// Optional crop rectangle `[x1, y1, x2, y2]` in percent.
    #[serde(default)]
    pub rect: Option<Vec<i32>>,
}

/// Generic attribute: key/value pair attached to various objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    pub r#type: Typed,
    #[serde(default)]
    pub value: String,
}

/// URL attached to an object (website, email, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Url {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub desc: String,
    pub r#type: Typed,
}

/// Mailing address — appears on persons and repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub date: Option<Date>,
    #[serde(default)]
    pub street: String,
    #[serde(default)]
    pub locality: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub county: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub postal: String,
    #[serde(default)]
    pub phone: String,
}

/// LDS ordinance record. Rare; kept permissive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdsOrd {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}
