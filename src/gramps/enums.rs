//! Hardcoded value→label lookups for Gramps tagged enums.
//!
//! Sources: `gramps-project/gramps` `gen/lib/*type.py` — each type declares
//! integer constants and a map to display strings. For Phase 1 we cover
//! every enum that appears in `test-fixtures/sample.db` plus the full
//! built-in tables for the types where we already know them. Custom user
//! types use the "custom" slot and carry their label in the `string` field
//! of the tagged enum (see `Typed::string` in `common.rs`).
//!
//! The labels are untranslated English. A future UI layer can run them
//! through i18n. For Phase 1 they exist primarily so `dump_db.rs` can
//! render something recognizable.

/// Look up the English label for an event-type value, or None if custom.
pub fn event_type_label(value: i32) -> Option<&'static str> {
    // gen/lib/eventtype.py
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Marriage",
        2 => "Marriage Settlement",
        3 => "Marriage License",
        4 => "Marriage Contract",
        5 => "Marriage Banns",
        6 => "Engagement",
        7 => "Divorce",
        8 => "Divorce Filing",
        9 => "Annulment",
        10 => "Alternate Marriage",
        11 => "Adopted",
        12 => "Birth",
        13 => "Death",
        14 => "Adult Christening",
        15 => "Baptism",
        16 => "Bar Mitzvah",
        17 => "Bas Mitzvah",
        18 => "Blessing",
        19 => "Burial",
        20 => "Cause Of Death",
        21 => "Census",
        22 => "Christening",
        23 => "Confirmation",
        24 => "Cremation",
        25 => "Degree",
        26 => "Education",
        27 => "Elected",
        28 => "Emigration",
        29 => "First Communion",
        30 => "Immigration",
        31 => "Graduation",
        32 => "Medical Information",
        33 => "Military Service",
        34 => "Naturalization",
        35 => "Nobility Title",
        36 => "Number of Marriages",
        37 => "Occupation",
        38 => "Ordination",
        39 => "Probate",
        40 => "Property",
        41 => "Religion",
        42 => "Residence",
        43 => "Retirement",
        44 => "Will",
        _ => return None,
    })
}

/// `EventRoleType` — primary/family/witness/etc.
pub fn event_role_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Primary",
        2 => "Clergy",
        3 => "Celebrant",
        4 => "Aide",
        5 => "Bride",
        6 => "Groom",
        7 => "Witness",
        8 => "Family",
        9 => "Informant",
        _ => return None,
    })
}

/// `NameType` — also_known_as / birth / married / aka.
pub fn name_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Also Known As",
        2 => "Birth Name",
        3 => "Married Name",
        _ => return None,
    })
}

/// `NameOriginType` — patronymic/given/inherited/...
pub fn name_origin_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "None",
        2 => "Inherited",
        3 => "Given",
        4 => "Taken",
        5 => "Patronymic",
        6 => "Matronymic",
        7 => "Feudal",
        8 => "Pseudonym",
        9 => "Patrilineal",
        10 => "Matrilineal",
        11 => "Occupation",
        12 => "Location",
        _ => return None,
    })
}

/// `FamilyRelType`.
pub fn family_rel_label(value: i32) -> Option<&'static str> {
    Some(match value {
        0 => "Married",
        1 => "Unmarried",
        2 => "Civil Union",
        3 => "Unknown",
        4 => "Custom",
        _ => return None,
    })
}

/// `ChildRefType`.
pub fn child_ref_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Birth",
        2 => "Adopted",
        3 => "Stepchild",
        4 => "Sponsored",
        5 => "Foster",
        _ => return None,
    })
}

/// `PlaceType`.
pub fn place_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Country",
        2 => "State",
        3 => "County",
        4 => "City",
        5 => "Parish",
        6 => "Locality",
        7 => "Street",
        8 => "Province",
        9 => "Region",
        10 => "Department",
        11 => "Neighborhood",
        12 => "District",
        13 => "Borough",
        14 => "Municipality",
        15 => "Town",
        16 => "Village",
        17 => "Hamlet",
        18 => "Farm",
        19 => "Building",
        20 => "Number",
        _ => return None,
    })
}

/// `NoteType` — a very large set in Gramps. Returning None here just means
/// the caller should fall back to the raw integer; for Phase 1 we only need
/// the handful that appear in the sample.
pub fn note_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "General",
        2 => "Research",
        3 => "Transcript",
        4 => "Source text",
        5 => "Citation",
        6 => "Report",
        7 => "Html code",
        8 => "Todo",
        9 => "Link",
        _ => return None,
    })
}

/// `AttributeType` — short list; only a few appear in the sample.
pub fn attribute_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Caste",
        2 => "Description",
        3 => "Identification Number",
        4 => "National Origin",
        5 => "Number of Children",
        6 => "Social Security Number",
        7 => "Nickname",
        8 => "Cause",
        9 => "Agency",
        10 => "Age",
        11 => "Father's Age",
        12 => "Mother's Age",
        13 => "Witness",
        14 => "Time",
        _ => return None,
    })
}

/// `SourceMediaType` (citation media type).
pub fn source_media_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Audio",
        2 => "Book",
        3 => "Card",
        4 => "Electronic",
        5 => "Fiche",
        6 => "Film",
        7 => "Magazine",
        8 => "Manuscript",
        9 => "Map",
        10 => "Newspaper",
        11 => "Photo",
        12 => "Tombstone",
        13 => "Video",
        _ => return None,
    })
}

/// `UrlType`.
pub fn url_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "E-mail",
        2 => "Web Home",
        3 => "Web Search",
        4 => "FTP",
        _ => return None,
    })
}

/// `RepositoryType`.
pub fn repository_type_label(value: i32) -> Option<&'static str> {
    Some(match value {
        -1 => "Unknown",
        0 => "Custom",
        1 => "Library",
        2 => "Cemetery",
        3 => "Church",
        4 => "Archive",
        5 => "Album",
        6 => "Web site",
        7 => "Bookstore",
        8 => "Collection",
        9 => "Safe",
        _ => return None,
    })
}

/// Date calendar id → name.
pub fn calendar_name(value: i32) -> Option<&'static str> {
    Some(match value {
        0 => "Gregorian",
        1 => "Julian",
        2 => "Hebrew",
        3 => "French Republican",
        4 => "Persian",
        5 => "Islamic",
        6 => "Swedish",
        _ => return None,
    })
}

/// Date modifier id → name.
pub fn modifier_name(value: i32) -> Option<&'static str> {
    Some(match value {
        0 => "",
        1 => "before",
        2 => "after",
        3 => "about",
        4 => "range",
        5 => "span",
        6 => "textonly",
        7 => "from",
        8 => "to",
        _ => return None,
    })
}

/// Date quality id → name.
pub fn quality_name(value: i32) -> Option<&'static str> {
    Some(match value {
        0 => "regular",
        1 => "estimated",
        2 => "calculated",
        _ => return None,
    })
}

/// Gender code → label. Gramps: 0=female, 1=male, 2=unknown, 3=other.
pub fn gender_label(value: i32) -> &'static str {
    match value {
        0 => "female",
        1 => "male",
        2 => "unknown",
        3 => "other",
        _ => "?",
    }
}
