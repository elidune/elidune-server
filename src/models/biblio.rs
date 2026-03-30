//! Biblio (bibliographic record / catalog entry) model and related types.
//!
//! All structures are aligned with [marc-rs](https://docs.rs/marc-rs) data models.
//! Persistence (DB) uses the associated char/int/string representations; conversions
//! from marc-rs types are provided where applicable.

use chrono::{DateTime, Utc};
use serde::{de::Error as SerdeError, Deserialize, Deserializer, Serialize};
use serde::de::Visitor;
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use crate::models::{Author, Language};
use crate::models::item::ItemShort;

use super::item::Item;

// Re-exports: canonical MARC data types from marc-rs (via z3950-rs).
pub use crate::marc::{MarcFormat, MarcRecord};

/// Normalized ISBN/ISSN-style identifier: only ASCII letters and digits are kept.
///
/// Input may be formatted (dashes, spaces, `ISBN` prefix, underscores, etc.); all such
/// characters are stripped. Check digits `x` / `X` are uppercased.
///
/// JSON deserialization, [`FromStr`], and database reads all apply this normalization.
/// Serialization and SQL encoding use the normalized string only.
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

/// Biblio operational status (independent of archival)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum BiblioStatus {
    Active = 0,
    Unavailable = 1,
}

impl From<i16> for BiblioStatus {
    fn from(v: i16) -> Self {
        match v {
            1 => BiblioStatus::Unavailable,
            _ => BiblioStatus::Active,
        }
    }
}

impl Default for BiblioStatus {
    fn default() -> Self {
        BiblioStatus::Active
    }
}

/// Audience type codes for catalog biblios, mirroring `marc_rs::record::TargetAudience`.
///
/// DB encoding: camelCase strings (e.g. `"juvenile"`, `"general"`, `"unknown"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AudienceType {
    Juvenile,
    Preschool,
    Primary,
    Children,
    YoungAdult,
    AdultSerious,
    Adult,
    General,
    Specialized,
    Unknown,
    Other(String),
}

impl AudienceType {
    /// Canonical camelCase string stored in the DB column.
    pub fn as_db_str(&self) -> &str {
        match self {
            AudienceType::Juvenile => "juvenile",
            AudienceType::Preschool => "preschool",
            AudienceType::Primary => "primary",
            AudienceType::Children => "children",
            AudienceType::YoungAdult => "youngAdult",
            AudienceType::AdultSerious => "adultSerious",
            AudienceType::Adult => "adult",
            AudienceType::General => "general",
            AudienceType::Specialized => "specialized",
            AudienceType::Unknown => "unknown",
            AudienceType::Other(s) => s.as_str(),
        }
    }

    /// Parse from the DB string, returning `None` for unrecognised values.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "juvenile" => Some(AudienceType::Juvenile),
            "preschool" => Some(AudienceType::Preschool),
            "primary" => Some(AudienceType::Primary),
            "children" => Some(AudienceType::Children),
            "youngAdult" => Some(AudienceType::YoungAdult),
            "adultSerious" => Some(AudienceType::AdultSerious),
            "adult" => Some(AudienceType::Adult),
            "general" => Some(AudienceType::General),
            "specialized" => Some(AudienceType::Specialized),
            "unknown" => Some(AudienceType::Unknown),
            other => Some(AudienceType::Other(other.to_string())),
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for AudienceType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for AudienceType {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(AudienceType::from_db_str(&s).unwrap_or(AudienceType::Unknown))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for AudienceType {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.as_db_str().to_string(), buf)
    }
}

/// Media type codes for catalog biblios.
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

/// Full biblio model (DB + API). Data aligns with marc-rs `Record`: title, author, edition,
/// ISBNs, classifications, language codes, items (physical copies), etc.
/// Built from MARC via the translator.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Biblio {
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
    pub audience_type: Option<AudienceType>,
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
    pub is_valid: Option<bool>,
    /// Resolved series IDs (same order as `series_volume_numbers` and `series`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[schema(value_type = Vec<String>)]
    #[serde(default)]
    #[sqlx(skip)]
    pub series_ids: Vec<i64>,
    /// Volume within each series for this biblio (parallel to `series_ids`).
    #[serde(default)]
    #[sqlx(skip)]
    pub series_volume_numbers: Vec<Option<i16>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub edition_id: Option<i64>,
    /// Resolved collection IDs (same order as `collection_volume_numbers` and `collections`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[schema(value_type = Vec<String>)]
    #[serde(default)]
    #[sqlx(skip)]
    pub collection_ids: Vec<i64>,
    /// Volume within each collection for this biblio (parallel to `collection_ids`).
    #[serde(default)]
    #[sqlx(skip)]
    pub collection_volume_numbers: Vec<Option<i16>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    // Relations (loaded separately)
    #[sqlx(skip)]
    #[serde(default)]
    pub authors: Vec<Author>,
    #[sqlx(skip)]
    #[serde(default)]
    pub series: Vec<Serie>,
    #[sqlx(skip)]
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[sqlx(skip)]
    #[serde(default)]
    pub edition: Option<Edition>,
    #[sqlx(skip)]
    #[serde(default)]
    pub items: Vec<Item>,
    #[sqlx(skip)]
    #[serde(default, skip)]
    pub marc_record: Option<MarcRecord>,
}

/// Short biblio representation for lists
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BiblioShort {
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
    pub is_valid: Option<bool>,
    pub archived_at: Option<DateTime<Utc>>,
    pub author: Option<Author>,
    pub items: Vec<ItemShort>,
}

impl From<Biblio> for BiblioShort {
    fn from(biblio: Biblio) -> Self {
        Self {
            id: biblio.id.unwrap_or(0),
            media_type: biblio.media_type,
            isbn: biblio.isbn,
            title: biblio.title,
            date: biblio.publication_date,
            status: 0,
            is_valid: biblio.is_valid,
            archived_at: biblio.archived_at,
            author: biblio.authors.first().cloned(),
            items: biblio.items.into_iter().map(ItemShort::from).collect(),
        }
    }
}

/// Serie model. Persistence shape for MARC series (440/490/225); source: marc-rs `SeriesStatementData` (statement → name, issn).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
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
    /// Volume of this biblio in the series (only meaningful in biblio context; not stored on `series` rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub volume_number: Option<i16>,
}

/// Collection model. Persistence shape for MARC linking (e.g. 410); source: marc-rs `LinkingData` (title → name, issn).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    pub key: Option<String>,
    /// Primary display title (replaces former `primary_title`; mirrors `Serie.name`).
    pub name: Option<String>,
    pub secondary_title: Option<String>,
    pub tertiary_title: Option<String>,
    pub issn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    /// Volume of this biblio in the collection (only meaningful in biblio context; not stored on `collections` rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub volume_number: Option<i16>,
}

/// Edition (publisher) model. Persistence shape for MARC publication (260/264/210); source: marc-rs `EditionInfo` or `PublicationData`.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
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

/// Flat document indexed in Meilisearch for catalog full-text search.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MeiliBiblioDocument {
    pub id: i64,
    pub media_type: String,
    pub isbn: Option<String>,
    pub title: Option<String>,
    pub author_names: String,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub publisher_name: Option<String>,
    pub series_name: Option<String>,
    pub collection_name: Option<String>,
    pub barcodes: Vec<String>,
    pub call_numbers: Vec<String>,
    pub abstract_text: Option<String>,
    pub notes: Option<String>,
    pub table_of_contents: Option<String>,
    pub lang: Option<String>,
    pub audience_type: Option<String>,
    pub is_archived: bool,
}

/// Query/list parameters for series.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SerieQuery {
    /// Filter by name (substring, case-insensitive).
    pub name: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Create a new series entry.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSerie {
    pub name: String,
    pub key: Option<String>,
    pub issn: Option<String>,
}

/// Update an existing series entry (all fields optional).
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSerie {
    pub name: Option<String>,
    pub key: Option<String>,
    pub issn: Option<String>,
}

/// Query/list parameters for collections.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionQuery {
    /// Filter by name (substring, case-insensitive).
    pub name: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Create a new collection entry.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollection {
    pub name: String,
    pub key: Option<String>,
    pub secondary_title: Option<String>,
    pub tertiary_title: Option<String>,
    pub issn: Option<String>,
}

/// Update an existing collection entry (all fields optional).
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollection {
    pub name: Option<String>,
    pub key: Option<String>,
    pub secondary_title: Option<String>,
    pub tertiary_title: Option<String>,
    pub issn: Option<String>,
}

/// Biblio query parameters (API). Filter values are strings; use `MarcFormat` when filtering by MARC format where applicable.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BiblioQuery {
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
    pub audience_type: Option<String>,
    pub archive: Option<bool>,
    /// Filter by series name (substring, case-insensitive).
    pub serie: Option<String>,
    /// Filter by series ID (exact match).
    pub serie_id: Option<i64>,
    /// Filter by collection name (substring, case-insensitive).
    pub collection: Option<String>,
    /// Filter by collection ID (exact match).
    pub collection_id: Option<i64>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::{AudienceType, BiblioShort, Isbn, MediaType};
    use serde_json;
    use z3950_rs::marc_rs::record::TargetAudience;

    #[test]
    fn audience_type_from_marc_rs_round_trip() {
        assert_eq!(AudienceType::from(TargetAudience::General), AudienceType::General);
        assert_eq!(AudienceType::from(TargetAudience::Juvenile), AudienceType::Juvenile);
        assert_eq!(AudienceType::from(TargetAudience::YoungAdult), AudienceType::YoungAdult);
        assert_eq!(AudienceType::from(TargetAudience::Specialized), AudienceType::Specialized);
        assert_eq!(AudienceType::from(TargetAudience::Unknown), AudienceType::Unknown);
        assert_eq!(AudienceType::from(TargetAudience::Preschool), AudienceType::Preschool);
        assert_eq!(AudienceType::from(TargetAudience::Children), AudienceType::Children);
        assert_eq!(AudienceType::from(TargetAudience::Adult), AudienceType::Adult);
        assert_eq!(
            AudienceType::from(TargetAudience::Other("x".to_string())),
            AudienceType::Other("x".to_string()),
        );
    }

    #[test]
    fn audience_type_db_encoding() {
        assert_eq!(AudienceType::General.as_db_str(), "general");
        assert_eq!(AudienceType::Adult.as_db_str(), "adult");
        assert_eq!(AudienceType::AdultSerious.as_db_str(), "adultSerious");
        assert_eq!(AudienceType::YoungAdult.as_db_str(), "youngAdult");
        assert_eq!(AudienceType::Specialized.as_db_str(), "specialized");
        assert_eq!(AudienceType::Juvenile.as_db_str(), "juvenile");
        assert_eq!(AudienceType::Preschool.as_db_str(), "preschool");
        assert_eq!(AudienceType::Primary.as_db_str(), "primary");
        assert_eq!(AudienceType::Children.as_db_str(), "children");
        assert_eq!(AudienceType::Unknown.as_db_str(), "unknown");
        assert_eq!(AudienceType::Other("custom".to_string()).as_db_str(), "custom");
    }

    #[test]
    fn audience_type_from_db_str() {
        assert_eq!(AudienceType::from_db_str("general"), Some(AudienceType::General));
        assert_eq!(AudienceType::from_db_str("juvenile"), Some(AudienceType::Juvenile));
        assert_eq!(AudienceType::from_db_str("unknown"), Some(AudienceType::Unknown));
        assert_eq!(
            AudienceType::from_db_str("custom"),
            Some(AudienceType::Other("custom".to_string())),
        );
    }

    #[test]
    fn biblio_short_id_serializes_as_string() {
        let biblio = BiblioShort {
            id: 12345,
            media_type: MediaType::Unknown,
            isbn: None,
            title: Some("Test".to_string()),
            date: None,
            status: 0,
            is_valid: None,
            archived_at: None,
            author: None,
            items: Vec::new(),
        };
        let json = serde_json::to_string(&biblio).unwrap();
        assert!(json.contains("\"id\":\"12345\""), "id should be string in JSON, got: {}", json);
    }

    #[test]
    fn biblio_short_id_deserializes_from_string() {
        let json = r#"{"id":"12345","mediaType":"unknown","isbn":null,"title":"Test","date":null,"status":0,"isValid":null,"archivedAt":null,"author":null,"items":[]}"#;
        let biblio: BiblioShort = serde_json::from_str(json).unwrap();
        assert_eq!(biblio.id, 12345);
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

    #[test]
    fn isbn_deserializes_from_json_formatted_string() {
        let v: Isbn = serde_json::from_value(serde_json::json!("978-2-07-040850-4")).unwrap();
        assert_eq!(v.as_str(), "9782070408504");
    }

    #[test]
    fn isbn_biblio_short_deserializes_formatted_isbn_via_display_from_str() {
        let json = r#"{"id":"1","mediaType":"unknown","isbn":"978-2-07-040850-4","title":null,"date":null,"status":0,"isValid":null,"archivedAt":null,"author":null,"items":[]}"#;
        let short: BiblioShort = serde_json::from_str(json).unwrap();
        assert_eq!(short.isbn.as_ref().map(|i| i.as_str()), Some("9782070408504"));
    }
}
