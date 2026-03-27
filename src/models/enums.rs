//! Shared domain enums (matching original C implementation)

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Lang
// ---------------------------------------------------------------------------

/// Language codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum Lang {
    Unknown = 0,
    Fr = 1,
    En = 2,
    De = 3,
    Jp = 4,
    Es = 5,
    Po = 6,
}

impl From<i16> for Lang {
    fn from(v: i16) -> Self {
        match v {
            1 => Lang::Fr,
            2 => Lang::En,
            3 => Lang::De,
            4 => Lang::Jp,
            5 => Lang::Es,
            6 => Lang::Po,
            _ => Lang::Unknown,
        }
    }
}

impl From<Lang> for i16 {
    fn from(l: Lang) -> Self {
        l as i16
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Lang::Unknown => "Unknown",
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Jp => "日本語",
            Lang::Es => "Español",
            Lang::Po => "Português",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Genre
// ---------------------------------------------------------------------------

/// Item genre classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum Genre {
    // Literature genres
    Unknown = 0,
    LitteratureGeneral = 1,
    LitteratureFiction = 2,
    LitteratureComic = 3,
    LitteratureTheatre = 4,
    LitteraturePoem = 5,
    LitteraturePhilosophy = 6,
    LitteratureReligion = 7,
    LitteratureSocialSciences = 8,
    LitteratureLanguages = 9,
    LitteratureSciences = 10,
    LitteratureTechnical = 11,
    LitteratureArt = 12,
    LitteratureSport = 13,
    LitteratureLitterature = 14,
    LitteratureHistory = 15,
    LitteratureGeography = 16,
    LitteratureOther = 17,
    // Audio genres
    AudioUnknown = 100,
    AudioJazz = 101,
    AudioBlues = 102,
    AudioRock = 103,
    AudioWorld = 104,
    AudioClassical = 105,
    // Video genres
    VideoUnknown = 200,
    VideoFiction = 201,
    VideoHistory = 202,
    VideoArt = 203,
    VideoDocumentary = 204,
    VideoMusical = 205,
}

impl From<i16> for Genre {
    fn from(v: i16) -> Self {
        match v {
            1 => Genre::LitteratureGeneral,
            2 => Genre::LitteratureFiction,
            3 => Genre::LitteratureComic,
            4 => Genre::LitteratureTheatre,
            5 => Genre::LitteraturePoem,
            6 => Genre::LitteraturePhilosophy,
            7 => Genre::LitteratureReligion,
            8 => Genre::LitteratureSocialSciences,
            9 => Genre::LitteratureLanguages,
            10 => Genre::LitteratureSciences,
            11 => Genre::LitteratureTechnical,
            12 => Genre::LitteratureArt,
            13 => Genre::LitteratureSport,
            14 => Genre::LitteratureLitterature,
            15 => Genre::LitteratureHistory,
            16 => Genre::LitteratureGeography,
            17 => Genre::LitteratureOther,
            100 => Genre::AudioUnknown,
            101 => Genre::AudioJazz,
            102 => Genre::AudioBlues,
            103 => Genre::AudioRock,
            104 => Genre::AudioWorld,
            105 => Genre::AudioClassical,
            200 => Genre::VideoUnknown,
            201 => Genre::VideoFiction,
            202 => Genre::VideoHistory,
            203 => Genre::VideoArt,
            204 => Genre::VideoDocumentary,
            205 => Genre::VideoMusical,
            _ => Genre::Unknown,
        }
    }
}

impl From<Genre> for i16 {
    fn from(g: Genre) -> Self {
        g as i16
    }
}

// ---------------------------------------------------------------------------
// Sex (stored as 'm' / 'f' / NULL in PostgreSQL)
// ---------------------------------------------------------------------------

/// Biological / legal sex for patron records (API: `"m"` | `"f"`; omit or null = unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    M,
    F,
}

impl Sex {
    pub fn as_str(self) -> &'static str {
        match self {
            Sex::M => "m",
            Sex::F => "f",
        }
    }
}

impl std::fmt::Display for Sex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Sex {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "m" | "male" => Ok(Sex::M),
            "f" | "female" => Ok(Sex::F),
            _ => Err(format!("Invalid sex: {}", s)),
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for Sex {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Sex {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s: String = sqlx::Decode::<sqlx::Postgres>::decode(value)?;
        let t = s.trim();
        match t {
            "m" | "M" => Ok(Sex::M),
            "f" | "F" => Ok(Sex::F),
            _ => Err(format!("Invalid sex value in database: {:?}", s).into()),
        }
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for Sex {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        <&str as sqlx::Encode<sqlx::Postgres>>::encode(self.as_str(), buf)
    }
}

// ---------------------------------------------------------------------------
// Occupation
// ---------------------------------------------------------------------------

/// Occupation / socio-professional category codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum Occupation {
    Unknown = 0,
    Farm = 1,
    Crafts = 2,
    Frameworks = 3,
    Intermediate = 4,
    Operative = 5,
    Workmen = 6,
    Reprocessed = 7,
    Other = 8,
}

impl From<i16> for Occupation {
    fn from(v: i16) -> Self {
        match v {
            1 => Occupation::Farm,
            2 => Occupation::Crafts,
            3 => Occupation::Frameworks,
            4 => Occupation::Intermediate,
            5 => Occupation::Operative,
            6 => Occupation::Workmen,
            7 => Occupation::Reprocessed,
            8 => Occupation::Other,
            _ => Occupation::Unknown,
        }
    }
}

impl From<Occupation> for i16 {
    fn from(o: Occupation) -> Self {
        o as i16
    }
}

// ---------------------------------------------------------------------------
// StaffType
// ---------------------------------------------------------------------------

/// Staff type codes (stored in users.staff_type)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum StaffType {
    Employee = 0,
    Volunteer = 1,
}

impl From<i16> for StaffType {
    fn from(v: i16) -> Self {
        match v {
            0 => StaffType::Employee,
            1 => StaffType::Volunteer,
            _ => StaffType::Employee,
        }
    }
}

impl From<StaffType> for i16 {
    fn from(s: StaffType) -> Self {
        s as i16
    }
}

impl std::fmt::Display for StaffType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            StaffType::Employee => "Employee",
            StaffType::Volunteer => "Volunteer",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// EquipmentType
// ---------------------------------------------------------------------------

/// Equipment type codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum EquipmentType {
    Computer = 0,
    Tablet = 1,
    EReader = 2,
    Other = 3,
}

impl From<i16> for EquipmentType {
    fn from(v: i16) -> Self {
        match v {
            0 => EquipmentType::Computer,
            1 => EquipmentType::Tablet,
            2 => EquipmentType::EReader,
            3 => EquipmentType::Other,
            _ => EquipmentType::Other,
        }
    }
}

impl From<EquipmentType> for i16 {
    fn from(e: EquipmentType) -> Self {
        e as i16
    }
}

impl std::fmt::Display for EquipmentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            EquipmentType::Computer => "Computer",
            EquipmentType::Tablet => "Tablet",
            EquipmentType::EReader => "E-Reader",
            EquipmentType::Other => "Other",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// EquipmentStatus
// ---------------------------------------------------------------------------

/// Equipment status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum EquipmentStatus {
    Active = 0,
    Maintenance = 1,
    Retired = 2,
}

impl From<i16> for EquipmentStatus {
    fn from(v: i16) -> Self {
        match v {
            0 => EquipmentStatus::Active,
            1 => EquipmentStatus::Maintenance,
            2 => EquipmentStatus::Retired,
            _ => EquipmentStatus::Active,
        }
    }
}

impl From<EquipmentStatus> for i16 {
    fn from(e: EquipmentStatus) -> Self {
        e as i16
    }
}

// ---------------------------------------------------------------------------
// EventType
// ---------------------------------------------------------------------------

/// Event type codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum EventType {
    Animation = 0,
    SchoolVisit = 1,
    Exhibition = 2,
    Conference = 3,
    Workshop = 4,
    Show = 5,
    Other = 6,
}

impl From<i16> for EventType {
    fn from(v: i16) -> Self {
        match v {
            0 => EventType::Animation,
            1 => EventType::SchoolVisit,
            2 => EventType::Exhibition,
            3 => EventType::Conference,
            4 => EventType::Workshop,
            5 => EventType::Show,
            6 => EventType::Other,
            _ => EventType::Other,
        }
    }
}

impl From<EventType> for i16 {
    fn from(e: EventType) -> Self {
        e as i16
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            EventType::Animation => "Animation",
            EventType::SchoolVisit => "School Visit",
            EventType::Exhibition => "Exhibition",
            EventType::Conference => "Conference",
            EventType::Workshop => "Workshop",
            EventType::Show => "Show",
            EventType::Other => "Other",
        };
        write!(f, "{}", label)
    }
}
