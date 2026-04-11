//! Person, Name, Surname, PersonRef.

use serde::{Deserialize, Serialize};

use super::common::{Address, Attribute, LdsOrd, MediaRef, Typed, Url};
use super::date::Date;
use super::event::EventRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surname {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub surname: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub connector: String,
    pub origintype: Typed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Name {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub call: String,
    #[serde(default)]
    pub nick: String,
    #[serde(default)]
    pub famnick: String,
    #[serde(default)]
    pub group_as: String,
    #[serde(default)]
    pub sort_as: i32,
    #[serde(default)]
    pub display_as: i32,
    pub r#type: Typed,
    #[serde(default)]
    pub date: Option<Date>,
    #[serde(default)]
    pub surname_list: Vec<Surname>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
}

impl Name {
    /// Best-effort rendering of first name + primary surname.
    pub fn display(&self) -> String {
        let surname = self
            .surname_list
            .iter()
            .find(|s| s.primary)
            .or_else(|| self.surname_list.first())
            .map(|s| s.surname.as_str())
            .unwrap_or("");
        if self.first_name.is_empty() {
            surname.to_string()
        } else if surname.is_empty() {
            self.first_name.clone()
        } else {
            format!("{} {}", self.first_name, surname)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonRef {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub r#ref: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    pub rel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    #[serde(default, rename = "_class")]
    pub class: Option<String>,
    pub handle: String,
    pub gramps_id: String,
    #[serde(default)]
    pub gender: i32,
    #[serde(default)]
    pub change: i64,
    #[serde(default)]
    pub private: bool,
    pub primary_name: Name,
    #[serde(default)]
    pub alternate_names: Vec<Name>,
    #[serde(default)]
    pub event_ref_list: Vec<EventRef>,
    #[serde(default = "minus_one")]
    pub birth_ref_index: i32,
    #[serde(default = "minus_one")]
    pub death_ref_index: i32,
    #[serde(default)]
    pub family_list: Vec<String>,
    #[serde(default)]
    pub parent_family_list: Vec<String>,
    #[serde(default)]
    pub person_ref_list: Vec<PersonRef>,
    #[serde(default)]
    pub address_list: Vec<Address>,
    #[serde(default)]
    pub urls: Vec<Url>,
    #[serde(default)]
    pub lds_ord_list: Vec<LdsOrd>,
    #[serde(default)]
    pub media_list: Vec<MediaRef>,
    #[serde(default)]
    pub attribute_list: Vec<Attribute>,
    #[serde(default)]
    pub citation_list: Vec<String>,
    #[serde(default)]
    pub note_list: Vec<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
}

fn minus_one() -> i32 {
    -1
}
