//! OpenAPI documentation

use axum::Router;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

use crate::api::{auth, equipment, events, health, items, loans, public_types, schedules, settings, sources, stats, users, visitor_counts, z3950};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Elidune API",
        version = "1.0.0",
        description = "Library Management System REST API",
        license(name = "GPL-2.0", url = "https://www.gnu.org/licenses/gpl-2.0.html"),
        contact(name = "Elidune Team", email = "contact@elidune.org")
    ),
    servers(
        (url = "/api/v1", description = "API v1")
    ),
    paths(
        // Health
        health::health_check,
        health::readiness_check,
        health::version,
        // Auth
        auth::login,
        auth::me,
        auth::verify_2fa,
        auth::verify_recovery,
        auth::request_password_reset,
        auth::reset_password,
        auth::setup_2fa,
        auth::disable_2fa,
        // Items
        items::list_items,
        items::get_item,
        items::create_item,
        items::upload_unimarc,
        items::import_marc_batch,
        items::update_item,
        items::delete_item,
        items::list_specimens,
        items::create_specimen,
        items::update_specimen,
        items::delete_specimen,
        // Users
        users::list_users,
        users::get_user,
        users::create_user,
        users::update_user,
        users::delete_user,
        users::update_my_profile,
        users::update_account_type,
        // Loans
        loans::get_user_loans,
        loans::create_loan,
        loans::return_loan,
        loans::renew_loan,
        // Z39.50
        z3950::search,
        z3950::import_record,
        // Stats
        stats::get_stats,
        stats::get_loan_stats,
        stats::get_user_stats,
        stats::get_catalog_stats,
        // Settings
        settings::get_settings,
        settings::update_settings,
        // Visitor counts
        visitor_counts::list_visitor_counts,
        visitor_counts::create_visitor_count,
        visitor_counts::delete_visitor_count,
        // Schedules
        schedules::list_periods,
        schedules::create_period,
        schedules::update_period,
        schedules::delete_period,
        schedules::list_slots,
        schedules::create_slot,
        schedules::delete_slot,
        schedules::list_closures,
        schedules::create_closure,
        schedules::delete_closure,
        // Sources
        sources::list_sources,
        sources::create_source,
        sources::get_source,
        sources::update_source,
        sources::archive_source,
        sources::merge_sources,
        // Equipment
        equipment::list_equipment,
        equipment::get_equipment,
        equipment::create_equipment,
        equipment::update_equipment,
        equipment::delete_equipment,
        // Events
        events::list_events,
        events::get_event,
        events::create_event,
        events::update_event,
        events::delete_event,
        // Public types
        public_types::list_public_types,
        public_types::get_public_type,
        public_types::create_public_type,
        public_types::update_public_type,
        public_types::delete_public_type,
        public_types::upsert_loan_setting,
        public_types::delete_loan_setting,
    ),
    components(
        schemas(
            // Auth
            auth::LoginRequest,
            auth::LoginResponse,
            auth::UserInfo,
            auth::Verify2FARequest,
            auth::Verify2FAResponse,
            auth::VerifyRecoveryRequest,
            auth::RequestPasswordResetRequest,
            auth::ResetPasswordRequest,
            auth::Setup2FARequest,
            auth::Setup2FAResponse,
            // Items
            crate::models::item::Item,
            crate::models::item::ItemShort,
            crate::models::item::ItemQuery,
            crate::models::item::Serie,
            crate::models::item::Collection,
            crate::models::item::Edition,
            crate::models::specimen::Specimen,
            // Pagination
            items::PaginatedResponse<crate::models::item::ItemShort>,
            items::PaginatedResponse<crate::models::user::UserShort>,
            // Users
            crate::models::user::User,
            crate::models::user::UserShort,
            crate::models::user::UserQuery,
            crate::models::user::UserPayload,
            crate::models::user::UpdateProfile,
            crate::models::user::UpdateAccountType,
            // Loans
            loans::CreateLoanRequest,
            loans::LoanResponse,
            loans::ReturnResponse,
            crate::models::loan::LoanDetails,
            // Z39.50
            z3950::Z3950SearchQuery,
            z3950::Z3950SearchResponse,
            z3950::Z3950ImportRequest,
            z3950::Z3950ImportResponse,
            z3950::ImportSpecimen,
            // Import report
            crate::models::import_report::ImportReport,
            crate::models::import_report::ImportAction,
            crate::models::import_report::DuplicateConfirmationRequired,
            crate::models::import_report::DuplicateSpecimenBarcodeRequired,
            // Items
            items::CreateItemResponse,
            // Stats
            stats::StatsResponse,
            stats::StatsQuery,
            stats::LoanStatsResponse,
            stats::UserLoanStats,
            stats::Interval,
            stats::LoanStatsQuery,
            stats::TimeSeriesEntry,
            stats::UserStatsSortBy,
            stats::UserStatsQuery,
            stats::CatalogStatsQuery,
            stats::CatalogStatsResponse,
            stats::CatalogStatsTotals,
            stats::CatalogSourceStats,
            stats::CatalogBreakdownStats,
            // Settings
            settings::SettingsResponse,
            settings::LoanSettings,
            settings::Z3950ServerConfig,
            settings::UpdateSettingsRequest,
            // Visitor counts
            crate::models::visitor_count::VisitorCount,
            crate::models::visitor_count::CreateVisitorCount,
            crate::models::visitor_count::VisitorCountQuery,
            // Schedules
            crate::models::schedule::SchedulePeriod,
            crate::models::schedule::ScheduleSlot,
            crate::models::schedule::ScheduleClosure,
            crate::models::schedule::CreateSchedulePeriod,
            crate::models::schedule::UpdateSchedulePeriod,
            crate::models::schedule::CreateScheduleSlot,
            crate::models::schedule::CreateScheduleClosure,
            crate::models::schedule::ScheduleClosureQuery,
            // Sources
            crate::models::source::Source,
            crate::models::source::CreateSource,
            crate::models::source::UpdateSource,
            crate::models::source::MergeSources,
            sources::SourcesQuery,
            // Equipment
            crate::models::equipment::Equipment,
            crate::models::equipment::CreateEquipment,
            crate::models::equipment::UpdateEquipment,
            // Events
            crate::models::event::Event,
            crate::models::event::CreateEvent,
            crate::models::event::UpdateEvent,
            crate::models::event::EventQuery,
            events::EventsListResponse,
            // Public types
            crate::models::public_type::PublicType,
            crate::models::public_type::PublicTypeLoanSettings,
            crate::models::public_type::CreatePublicType,
            crate::models::public_type::UpdatePublicType,
            public_types::UpsertLoanSettingRequest,
            crate::repository::events::EventAnnualStats,
            crate::repository::events::EventTypeStats,
            // Health
            health::HealthResponse,
            health::VersionResponse,
            // Errors
            crate::error::ErrorResponse,
        )
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "items", description = "Catalog item management"),
        (name = "users", description = "User management"),
        (name = "loans", description = "Loan management"),
        (name = "z3950", description = "Z39.50 catalog search"),
        (name = "stats", description = "Statistics"),
        (name = "settings", description = "System settings"),
        (name = "visitor_counts", description = "Visitor counting"),
        (name = "schedules", description = "Library schedules (hours, closures)"),
        (name = "sources", description = "Acquisition source management"),
        (name = "equipment", description = "Library equipment management"),
        (name = "events", description = "Cultural events and school visits"),
        (name = "public_types", description = "Borrower public types (child, adult, school, staff, senior)")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some(
                            "Bearer authentication using JWT. Use 'Authorization: Bearer <token>'"
                                .to_string(),
                        ))
                        .build(),
                ),
            );
        }
    }
}

/// Create the OpenAPI documentation router
pub fn create_openapi_router() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}
