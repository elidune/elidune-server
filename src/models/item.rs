//! Item (catalog entry) model and related types.
//!
//! All structures are aligned with [marc-rs](https://docs.rs/marc-rs) data models.
//! Persistence (DB) uses the associated char/int/string representations; conversions
//! from marc-rs types are provided where applicable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use crate::models::Author;
use crate::models::specimen::SpecimenShort;

use super::specimen::Specimen;

// Re-exports: canonical MARC data types from marc-rs (via z3950-rs).
pub use z3950_rs::marc_rs::format::MarcFormat;
pub use z3950_rs::marc_rs::record::{
    ControlField, DataField, EditionInfo, PublicationStatementInfo, Subfield,
};
pub use z3950_rs::marc_rs::author::{Author as MarcAuthor, AuthorKind};
pub use z3950_rs::marc_rs::fields::{
    DeweyClassification, Isbn, LanguageData, LinkingData, PublicationData, SeriesStatementData,
};

/// Item operational status (independent of archival)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum ItemStatus {
    Active = 0,
    Unavailable = 1,
}

impl From<i16> for ItemStatus {
    fn from(v: i16) -> Self {
        match v {
            1 => ItemStatus::Unavailable,
            _ => ItemStatus::Active,
        }
    }
}

impl Default for ItemStatus {
    fn default() -> Self {
        ItemStatus::Active
    }
}

/// Media type codes for catalog items.
/// Maps from MARC Leader position 6 (record type) via `record_type_to_media_type_db` (see repository).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum MediaType {
    #[serde(rename = "")]
    All,
    #[serde(rename = "u")]
    Unknown,
    #[serde(rename = "b")]
    PrintedText,
    #[serde(rename = "m")]
    Multimedia,
    #[serde(rename = "bc")]
    Comics,
    #[serde(rename = "p")]
    Periodic,
    #[serde(rename = "v")]
    Video,
    #[serde(rename = "vt")]
    VideoTape,
    #[serde(rename = "vd")]
    VideoDvd,
    #[serde(rename = "a")]
    Audio,
    #[serde(rename = "am")]
    AudioMusic,
    #[serde(rename = "amt")]
    AudioMusicTape,
    #[serde(rename = "amc")]
    AudioMusicCd,
    #[serde(rename = "an")]
    AudioNonMusic,
    #[serde(rename = "ant")]
    AudioNonMusicTape,
    #[serde(rename = "anc")]
    AudioNonMusicCd,
    #[serde(rename = "c")]
    CdRom,
    #[serde(rename = "i")]
    Images,
}

impl MediaType {
    /// Return the legacy string code for this media type
    pub fn as_code(&self) -> &'static str {
        match self {
            MediaType::All => "",
            MediaType::Unknown => "u",
            MediaType::PrintedText => "b",
            MediaType::Multimedia => "m",
            MediaType::Comics => "bc",
            MediaType::Periodic => "p",
            MediaType::Video => "v",
            MediaType::VideoTape => "vt",
            MediaType::VideoDvd => "vd",
            MediaType::Audio => "a",
            MediaType::AudioMusic => "am",
            MediaType::AudioMusicTape => "amt",
            MediaType::AudioMusicCd => "amc",
            MediaType::AudioNonMusic => "an",
            MediaType::AudioNonMusicTape => "ant",
            MediaType::AudioNonMusicCd => "anc",
            MediaType::CdRom => "c",
            MediaType::Images => "i",
        }
    }
}

impl From<&str> for MediaType {
    fn from(s: &str) -> Self {
        match s {
            "" => MediaType::All,
            "b" => MediaType::PrintedText,
            "m" => MediaType::Multimedia,
            "bc" => MediaType::Comics,
            "p" => MediaType::Periodic,
            "v" => MediaType::Video,
            "vt" => MediaType::VideoTape,
            "vd" => MediaType::VideoDvd,
            "a" => MediaType::Audio,
            "am" => MediaType::AudioMusic,
            "amt" => MediaType::AudioMusicTape,
            "amc" => MediaType::AudioMusicCd,
            "an" => MediaType::AudioNonMusic,
            "ant" => MediaType::AudioNonMusicTape,
            "anc" => MediaType::AudioNonMusicCd,
            "c" => MediaType::CdRom,
            "i" => MediaType::Images,
            _ => MediaType::Unknown,
        }
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_code())
    }
}

/// Audience type. DB stores as i16 (97=Adult, 106=Children).
/// Derived from MARC21 008 pos.22 or UNIMARC 100 pos.17 when importing from MARC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i16)]
pub enum PublicType {
    Adult = 97,
    Children = 106,
    Unknown = 117,
}

impl From<i16> for PublicType {
    fn from(v: i16) -> Self {
        match v {
            97 => PublicType::Adult,
            106 => PublicType::Children,
            _ => PublicType::Unknown,
        }
    }
}

/// Full item model (DB + API). Data aligns with marc-rs `Record`: title, author, edition,
/// ISBNs, classifications, language codes, specimens, etc. Built from MARC via the translator.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Item {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    pub media_type: Option<String>,
    pub isbn: Option<String>,
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub price: Option<String>,
    pub title: Option<String>,
    pub genre: Option<i16>,
    pub subject: Option<String>,
    pub audience_type: Option<i16>,
    pub lang: Option<i16>,
    pub lang_orig: Option<i16>,
    pub publication_date: Option<String>,
    pub page_extent: Option<String>,
    pub format: Option<String>,
    pub table_of_contents: Option<String>,
    pub accompanying_material: Option<String>,
    pub abstract_: Option<String>,
    pub notes: Option<String>,
    pub keywords: Option<String>,
    pub state: Option<String>,
    pub is_valid: Option<i16>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub series_id: Option<i64>,
    #[serde(default)]
    pub series_volume_number: Option<i16>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub edition_id: Option<i64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub collection_id: Option<i64>,
    #[serde(default)]
    pub collection_sequence_number: Option<i16>,
    #[serde(default)]
    pub collection_volume_number: Option<i16>,
    #[serde(default)]
    pub status: i16,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    // Relations (loaded separately)
    #[sqlx(skip)]
    #[serde(default)]
    pub authors: Vec<Author>,
    #[sqlx(skip)]
    #[serde(default)]
    pub series: Option<Serie>,
    #[sqlx(skip)]
    #[serde(default)]
    pub collection: Option<Collection>,
    #[sqlx(skip)]
    #[serde(default)]
    pub edition: Option<Edition>,
    #[sqlx(skip)]
    #[serde(default)]
    pub specimens: Vec<Specimen>,
    #[sqlx(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marc_record: Option<serde_json::Value>,
}


/// Short item representation for lists
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct ItemShort {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub media_type: Option<String>,
    pub isbn: Option<String>,
    pub title: Option<String>,
    pub date: Option<String>,
    pub status: i16,
    pub is_valid: Option<i16>,
    pub archived_at: Option<DateTime<Utc>>,
    pub author: Option<Author>,
    pub specimens: Vec<SpecimenShort>,
   
}




impl From<Item> for ItemShort {
    fn from(item: Item) -> Self {
        Self {
            id: item.id.unwrap_or(0),
            media_type: item.media_type,
            isbn: item.isbn,
            title: item.title,
            date: item.publication_date,
            status: item.status,
            is_valid: item.is_valid,
            archived_at: item.archived_at,
            author: item.authors.first().cloned(),
            specimens: item.specimens.into_iter().map(SpecimenShort::from).collect(),
        }
    }
}
/// Serie model. Persistence shape for MARC series (440/490/225); source: marc-rs `SeriesStatementData` (statement → name, issn).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Serie {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    pub key: Option<String>,
    pub name: Option<String>,
    pub issn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<&SeriesStatementData> for Serie {
    fn from(d: &SeriesStatementData) -> Self {
        Self {
            id: None,
            key: None,
            name: Some(d.statement.clone()),
            issn: d.issn.clone(),
            created_at: None,
            updated_at: None,
        }
    }
}

/// Collection model. Persistence shape for MARC linking (e.g. 410); source: marc-rs `LinkingData` (title → primary_title, issn).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Collection {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    pub key: Option<String>,
    pub primary_title: Option<String>,
    pub secondary_title: Option<String>,
    pub tertiary_title: Option<String>,
    pub issn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<&LinkingData> for Collection {
    fn from(d: &LinkingData) -> Self {
        Self {
            id: None,
            key: None,
            primary_title: d.title.clone(),
            secondary_title: None,
            tertiary_title: None,
            issn: d.issn.clone(),
            created_at: None,
            updated_at: None,
        }
    }
}

/// Edition (publisher) model. Persistence shape for MARC publication (260/264/210); source: marc-rs `EditionInfo` or `PublicationData`.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Edition {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    pub publisher_name: Option<String>,
    pub place_of_publication: Option<String>,
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<&EditionInfo> for Edition {
    fn from(e: &EditionInfo) -> Self {
        Self {
            id: None,
            publisher_name: e.publisher.clone(),
            place_of_publication: e.place.clone(),
            date: e.date.clone(),
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<&PublicationData> for Edition {
    fn from(p: &PublicationData) -> Self {
        Self {
            id: None,
            publisher_name: p.publisher().map(String::from),
            place_of_publication: p.place().map(String::from),
            date: p.date().map(String::from),
            created_at: None,
            updated_at: None,
        }
    }
}

/// Item query parameters (API). Filter values are strings; use `MarcFormat` when filtering by MARC format where applicable.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ItemQuery {
    pub media_type: Option<String>,
    pub isbn: Option<String>,
    pub barcode: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub editor: Option<String>,
    pub lang: Option<String>,
    pub subject: Option<String>,
    pub content: Option<String>,
    pub keywords: Option<String>,
    pub freesearch: Option<String>,
    pub genre: Option<String>,
    pub public_type: Option<String>,
    pub archive: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::ItemShort;
    use serde_json;

    #[test]
    fn item_short_id_serializes_as_string() {
        let item = ItemShort {
            id: 12345,
            media_type: None,
            isbn: None,
            title: Some("Test".to_string()),
            date: None,
            status: 0,
            is_valid: None,
            archived_at: None,
            author: None,
            specimens: Vec::new(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"id\":\"12345\""), "id should be string in JSON, got: {}", json);
    }

    #[test]
    fn item_short_id_deserializes_from_string() {
        let json = r#"{"id":"12345","media_type":null,"isbn":null,"title":"Test","date":null,"status":0,"is_valid":null,"archived_at":null,"author":null}"#;
        let item: ItemShort = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, 12345);
    }
}
