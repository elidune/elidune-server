//! Data models for Elidune

pub mod author;
pub mod enums;
pub mod equipment;
pub mod event;
pub mod import_report;
pub mod item;
pub mod item_author;
pub mod loan;
pub mod schedule;
pub mod source;
pub mod specimen;
pub mod user;
pub mod visitor_count;

// Re-export commonly used types
pub use author::Author;
pub use enums::{Genre, Lang, Occupation, Sex, StaffType, EquipmentType, EquipmentStatus, EventType};
pub use import_report::{ImportReport, ImportAction, DuplicateCandidate, DuplicateConfirmationRequired, DuplicateSpecimenBarcodeRequired};
pub use item_author::ItemAuthor;
pub use equipment::Equipment;
pub use event::Event;
pub use item::{Item, ItemShort, MediaType};
pub use loan::{Loan, LoanDetails};
pub use schedule::{SchedulePeriod, ScheduleSlot, ScheduleClosure};
pub use source::Source;
pub use specimen::Specimen;
pub use user::{User, UserShort};
pub use visitor_count::VisitorCount;


