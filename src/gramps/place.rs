//! Place, PlaceName, PlaceRef.

use serde::{Deserialize, Serialize};

use super::common::{MediaRef, Typed, Url};
use super::date::Date;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceName {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub date: Option<Date>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceRef {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub r#ref: String,
    #[serde(default)]
    pub date: Option<Date>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
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
    pub long: String,
    #[serde(default)]
    pub lat: String,
    #[serde(default)]
    pub code: String,
    pub name: PlaceName,
    #[serde(default)]
    pub alt_names: Vec<PlaceName>,
    pub place_type: Typed,
    #[serde(default)]
    pub alt_loc: Vec<serde_json::Value>,
    #[serde(default)]
    pub placeref_list: Vec<PlaceRef>,
    #[serde(default)]
    pub urls: Vec<Url>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}
