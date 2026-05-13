//! Data models for Elidune

pub mod account_type;
pub mod audit;
pub mod author;
pub mod biblio;
pub mod biblio_author;
pub mod enums;
pub mod equipment;
pub mod event;
pub mod fine;
pub mod import_report;
pub mod inventory;
pub mod item;
pub mod loan;
pub mod public_type;
pub mod reader_assistant;
pub mod hold;
pub mod schedule;
pub mod stats_builder;
pub mod source;
pub mod task;
pub mod user;
pub mod visitor_count;

// Re-export commonly used types
pub use author::Author;
pub use biblio::{Biblio, BiblioShort, MediaType};
pub use biblio_author::BiblioAuthor;
pub use enums::{Genre, Lang, Occupation, Sex, StaffType, EquipmentType, EquipmentStatus, EventType};
pub use import_report::{ImportReport, ImportAction, DuplicateCandidate, DuplicateConfirmationRequired, DuplicateItemBarcodeRequired};
pub use equipment::Equipment;
pub use event::Event;
pub use item::{Item, ItemShort};
pub use loan::{Loan, LoanDetails};
pub use schedule::{SchedulePeriod, ScheduleSlot, ScheduleClosure};
use serde::{Deserialize, Serialize};
pub use source::Source;
pub use user::{User, UserShort};
use utoipa::ToSchema;
pub use visitor_count::VisitorCount;


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
