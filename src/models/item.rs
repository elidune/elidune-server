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
use z3950_rs::marc_rs;
use crate::models::Author;
use crate::models::specimen::SpecimenShort;

use super::specimen::Specimen;

// Re-exports: canonical MARC data types from marc-rs (via z3950-rs).
pub use crate::marc::{MarcFormat, MarcRecord};

/// Normalized ISBN/identifier stored without any special characters.
///
/// Construction from a string strips all non-ASCII alphanumeric characters
/// and uppercases ASCII letters (so `x` becomes `X`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, ToSchema)]
#[schema(value_type = String)]
pub struct Isbn(String);

#[derive(Debug, Clone, Copy)]
pub struct IsbnParseError;

impl std::fmt::Display for IsbnParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid isbn")
    }
}

impl std::error::Error for IsbnParseError {}

impl Isbn {
    pub fn new(raw: impl AsRef<str>) -> Self {
        let s = raw
            .as_ref()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
            .collect::<String>();
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for Isbn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Isbn {
    type Err = IsbnParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

impl From<String> for Isbn {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Isbn {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for Isbn {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for Isbn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Isbn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Isbn::new(s))
    }
}

impl sqlx::Type<sqlx::Postgres> for Isbn {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Isbn {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Isbn::new(s))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for Isbn {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.0.clone(), buf)
    }
}

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

/// Audience type codes for catalog items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum AudienceType {
    #[serde(rename = "97")]
    Adult,
    #[serde(rename = "106")]
    Children,
    #[serde(rename = "117")]
    Unknown,
}

/// Media type codes for catalog items.
/// Maps from MARC Leader position 6 (record type) via `record_type_to_media_type_db` (see repository).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MediaType {
    All,
    Unknown,
    PrintedText,
    Multimedia,
    Comics,
    Periodic,
    Video,
    VideoTape,
    VideoDvd,
    Audio,
    AudioMusic,
    AudioMusicTape,
    AudioMusicCd,
    AudioNonMusic,
    AudioNonMusicTape,
    AudioNonMusicCd,
    CdRom,
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

    /// Canonical DB/API string representation (camelCase).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            MediaType::All => "all",
            MediaType::Unknown => "unknown",
            MediaType::PrintedText => "printedText",
            MediaType::Multimedia => "multimedia",
            MediaType::Comics => "comics",
            MediaType::Periodic => "periodic",
            MediaType::Video => "video",
            MediaType::VideoTape => "videoTape",
            MediaType::VideoDvd => "videoDvd",
            MediaType::Audio => "audio",
            MediaType::AudioMusic => "audioMusic",
            MediaType::AudioMusicTape => "audioMusicTape",
            MediaType::AudioMusicCd => "audioMusicCd",
            MediaType::AudioNonMusic => "audioNonMusic",
            MediaType::AudioNonMusicTape => "audioNonMusicTape",
            MediaType::AudioNonMusicCd => "audioNonMusicCd",
            MediaType::CdRom => "cdRom",
            MediaType::Images => "images",
        }
    }
}

impl From<&str> for MediaType {
    fn from(s: &str) -> Self {
        match s {
            // Legacy codes
            "" => MediaType::All,
            "u" => MediaType::Unknown,
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
            // New camelCase strings
            "all" => MediaType::All,
            "unknown" => MediaType::Unknown,
            "printedText" => MediaType::PrintedText,
            "multimedia" => MediaType::Multimedia,
            "comics" => MediaType::Comics,
            "periodic" => MediaType::Periodic,
            "video" => MediaType::Video,
            "videoTape" => MediaType::VideoTape,
            "videoDvd" => MediaType::VideoDvd,
            "audio" => MediaType::Audio,
            "audioMusic" => MediaType::AudioMusic,
            "audioMusicTape" => MediaType::AudioMusicTape,
            "audioMusicCd" => MediaType::AudioMusicCd,
            "audioNonMusic" => MediaType::AudioNonMusic,
            "audioNonMusicTape" => MediaType::AudioNonMusicTape,
            "audioNonMusicCd" => MediaType::AudioNonMusicCd,
            "cdRom" => MediaType::CdRom,
            "images" => MediaType::Images,
            _ => MediaType::Unknown,
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Unknown,
    French,
    English,
    German,
    Spanish,
    Italian,
    Portuguese,
    Japanese,
    Chinese,
    Russian,
    Arabic,
    Dutch,
    Swedish,
    Norwegian,
    Danish,
    Finnish,
    Polish,
    Czech,
    Hungarian,
    Romanian,
    Turkish,
    Korean,
    Latin,
    Greek,
    Croatian,
    Hindi,
    Hebrew,
    Persian,
    Catalan,
    Thai,
    Vietnamese,
    Indonesian,
    Malay,
}

impl Language {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Language::Unknown => "unknown",
            Language::French => "french",
            Language::English => "english",
            Language::German => "german",
            Language::Spanish => "spanish",
            Language::Italian => "italian",
            Language::Portuguese => "portuguese",
            Language::Japanese => "japanese",
            Language::Chinese => "chinese",
            Language::Russian => "russian",
            Language::Arabic => "arabic",
            Language::Dutch => "dutch",
            Language::Swedish => "swedish",
            Language::Norwegian => "norwegian",
            Language::Danish => "danish",
            Language::Finnish => "finnish",
            Language::Polish => "polish",
            Language::Czech => "czech",
            Language::Hungarian => "hungarian",
            Language::Romanian => "romanian",
            Language::Turkish => "turkish",
            Language::Korean => "korean",
            Language::Latin => "latin",
            Language::Greek => "greek",
            Language::Croatian => "croatian",
            Language::Hindi => "hindi",
            Language::Hebrew => "hebrew",
            Language::Persian => "persian",
            Language::Catalan => "catalan",
            Language::Thai => "thai",
            Language::Vietnamese => "vietnamese",
            Language::Indonesian => "indonesian",
            Language::Malay => "malay",
        }
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        match s {
            // Legacy numeric IDs
            "0" => Language::Unknown,
            "1" => Language::French,
            "2" => Language::English,
            "3" => Language::German,
            "4" => Language::Japanese,
            "5" => Language::Spanish,
            "6" => Language::Portuguese,
            // Legacy ISO-ish 3-letter codes (migration 027)
            "fre" | "fra" => Language::French,
            "eng" => Language::English,
            "ger" | "deu" => Language::German,
            "jpn" => Language::Japanese,
            "spa" => Language::Spanish,
            "por" => Language::Portuguese,
            // Older camelCase strings from previous version
            "langUnknown" => Language::Unknown,
            "langFr" => Language::French,
            "langEn" => Language::English,
            "langDe" => Language::German,
            "langJp" => Language::Japanese,
            "langEs" => Language::Spanish,
            "langPo" => Language::Portuguese,
            // New canonical camelCase strings
            "unknown" => Language::Unknown,
            "french" => Language::French,
            "english" => Language::English,
            "german" => Language::German,
            "japanese" => Language::Japanese,
            "spanish" => Language::Spanish,
            "portuguese" => Language::Portuguese,
            "italian" => Language::Italian,
            "chinese" => Language::Chinese,
            "russian" => Language::Russian,
            "arabic" => Language::Arabic,
            "dutch" => Language::Dutch,
            "swedish" => Language::Swedish,
            "norwegian" => Language::Norwegian,
            "danish" => Language::Danish,
            "finnish" => Language::Finnish,
            "polish" => Language::Polish,
            "czech" => Language::Czech,
            "hungarian" => Language::Hungarian,
            "romanian" => Language::Romanian,
            "turkish" => Language::Turkish,
            "korean" => Language::Korean,
            "latin" => Language::Latin,
            "greek" => Language::Greek,
            "croatian" => Language::Croatian,
            "hindi" => Language::Hindi,
            "hebrew" => Language::Hebrew,
            "persian" => Language::Persian,
            "catalan" => Language::Catalan,
            "thai" => Language::Thai,
            "vietnamese" => Language::Vietnamese,
            "indonesian" => Language::Indonesian,
            "malay" => Language::Malay,
            _ => Language::Unknown,
        }
    }
}



impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_db_str())
    }
}

impl std::str::FromStr for Language {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Language::from(s))
    }
}

impl sqlx::Type<sqlx::Postgres> for Language {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Language {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Language::from(s.as_str()))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for Language {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.to_string(), buf)
    }
}

impl sqlx::postgres::PgHasArrayType for Language {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::postgres::PgHasArrayType>::array_type_info()
    }
}



impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_db_str())
    }
}

impl std::str::FromStr for MediaType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MediaType::from(s))
    }
}

impl sqlx::Type<sqlx::Postgres> for MediaType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for MediaType {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(MediaType::from(s.as_str()))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for MediaType {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.to_string(), buf)
    }
}

impl sqlx::postgres::PgHasArrayType for MediaType {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::postgres::PgHasArrayType>::array_type_info()
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
    pub media_type: MediaType,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub isbn: Option<Isbn>,
    pub title: Option<String>,
    pub subject: Option<String>,
    pub audience_type: Option<i16>,
    pub lang: Option<Language>,
    pub lang_orig: Option<Language>,
    pub publication_date: Option<String>,
    pub page_extent: Option<String>,
    pub format: Option<String>,
    pub table_of_contents: Option<String>,
    pub accompanying_material: Option<String>,
    pub abstract_: Option<String>,
    pub notes: Option<String>,
    pub keywords: Option<Vec<String>>,
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
    pub marc_record: Option<MarcRecord>,
}


/// Short item representation for lists
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct ItemShort {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub media_type: MediaType,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub isbn: Option<Isbn>,
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
            status: 0,
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


/// Item query parameters (API). Filter values are strings; use `MarcFormat` when filtering by MARC format where applicable.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ItemQuery {
    pub media_type: Option<String>,
    pub isbn: Option<Isbn>,
    pub barcode: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub editor: Option<String>,
    pub lang: Option<String>,
    pub subject: Option<String>,
    pub content: Option<String>,
    pub keywords: Option<String>,
    pub freesearch: Option<String>,
    pub audience_type: Option<i16>,
    pub archive: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::{Isbn, ItemShort, MediaType};
    use serde_json;

    #[test]
    fn item_short_id_serializes_as_string() {
        let item = ItemShort {
            id: 12345,
            media_type: MediaType::Unknown,
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

    #[test]
    fn isbn_strips_special_chars_and_uppercases() {
        let isbn = Isbn::new("978-2-07-040850-4");
        assert_eq!(isbn.as_str(), "9782070408504");

        let isbn = Isbn::new(" 2 07 040850 x ");
        assert_eq!(isbn.as_str(), "207040850X");

        let isbn = Isbn::new("isbn: 978_2_07");
        assert_eq!(isbn.as_str(), "ISBN978207");
    }
}
