//! OpenAPI documentation

use axum::Router;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

use crate::api::{admin_config, audit, auth, biblios, collections, equipment, events, health, holds, inventory, items, library_info, loans, maintenance, opac, public_types, schedules, series, sources, stats, tasks, users, visitor_counts, z3950};

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
        // Biblios and physical items
        biblios::list_biblios,
        biblios::get_biblio,
        biblios::create_biblio,
        biblios::load_marc,
        biblios::import_marc_batch,
        biblios::list_marc_batches,
        biblios::load_marc_batch,
        biblios::update_biblio,
        biblios::delete_biblio,
        biblios::list_items,
        biblios::create_item,
        items::get_biblio_by_item,
        items::update_item,
        items::delete_item,
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
        loans::export_user_loans_marc,
        loans::create_loan,
        loans::return_loan,
        loans::renew_loan,
        loans::return_loan_by_item,
        loans::renew_loan_by_item,
        loans::get_overdue_loans,
        loans::send_overdue_reminders,
        loans::get_loan_settings,
        loans::update_loan_settings,
        // Holds
        holds::list_holds,
        holds::create_hold,
        holds::list_holds_for_item,
        holds::list_holds_for_user,
        holds::cancel_hold,
        // Inventory (stocktaking)
        inventory::list_sessions,
        inventory::create_session,
        inventory::get_session,
        inventory::close_session,
        inventory::scan_barcode,
        inventory::batch_scan,
        inventory::list_scans,
        inventory::list_missing,
        inventory::get_report,
        // Z39.50
        z3950::search,
        z3950::import_record,
        z3950::get_z3950_servers,
        z3950::update_z3950_servers,
        // Stats
        stats::get_stats,
        stats::get_loan_stats,
        stats::get_user_stats,
        stats::get_catalog_stats,
        stats::get_stats_schema,
        stats::post_stats_query,
        stats::list_saved_queries,
        stats::create_saved_query,
        stats::update_saved_query,
        stats::delete_saved_query,
        stats::run_saved_query,
        // Library info
        library_info::get_library_info,
        library_info::update_library_info,
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
        // Series
        series::list_series,
        series::get_serie,
        series::get_serie_biblios,
        series::create_serie,
        series::update_serie,
        series::delete_serie,
        // Collections
        collections::list_collections,
        collections::get_collection,
        collections::get_collection_biblios,
        collections::create_collection,
        collections::update_collection,
        collections::delete_collection,
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
        events::send_event_announcement,
        // Admin config
        admin_config::get_config,
        admin_config::update_config_section,
        admin_config::reset_config_section,
        admin_config::test_email,
        // Maintenance
        maintenance::run_maintenance,
        // Background tasks
        tasks::list_tasks,
        tasks::get_task,
        // Audit
        audit::get_audit_log,
        audit::export_audit_log,
        // Public types
        public_types::list_public_types,
        public_types::get_public_type,
        public_types::create_public_type,
        public_types::update_public_type,
        public_types::delete_public_type,
        public_types::upsert_loan_setting,
        public_types::delete_loan_setting,
        // Opac
        opac::opac_search,
        opac::opac_get_biblio,
        opac::opac_availability,
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
            // Biblios (bibliographic records)
            crate::models::biblio::Biblio,
            crate::models::biblio::BiblioShort,
            crate::models::biblio::BiblioQuery,
            crate::models::biblio::Serie,
            crate::models::biblio::Collection,
            crate::models::biblio::Edition,
            crate::models::biblio::CreateSerie,
            crate::models::biblio::UpdateSerie,
            crate::models::biblio::SerieQuery,
            crate::models::biblio::CreateCollection,
            crate::models::biblio::UpdateCollection,
            crate::models::biblio::CollectionQuery,
            series::PaginatedSeries,
            collections::PaginatedCollections,
            // Items (physical copies)
            crate::models::item::Item,
            crate::models::item::ItemShort,
            // Pagination
            biblios::PaginatedResponse<crate::models::biblio::BiblioShort>,
            biblios::PaginatedResponse<crate::models::user::UserShort>,
            biblios::PaginatedResponse<crate::models::loan::LoanDetails>,
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
            loans::OverdueLoansQuery,
            // Holds
            crate::models::hold::Hold,
            crate::models::hold::HoldDetails,
            holds::CreateHoldRequest,
            holds::ListHoldsQuery,
            biblios::PaginatedResponse<crate::models::hold::HoldDetails>,
            biblios::PaginatedResponse<crate::models::inventory::InventorySession>,
            biblios::PaginatedResponse<crate::models::inventory::InventoryScan>,
            biblios::PaginatedResponse<crate::models::inventory::InventoryMissingRow>,
            crate::models::inventory::InventorySession,
            crate::models::inventory::InventoryScan,
            crate::models::inventory::InventoryScanResult,
            crate::models::inventory::InventoryStatus,
            crate::models::inventory::InventoryReport,
            crate::models::inventory::InventoryMissingRow,
            crate::models::inventory::CreateInventorySession,
            crate::models::inventory::ScanBarcode,
            crate::models::inventory::BatchScanBarcodes,
            inventory::ListInventorySessionsQuery,
            inventory::ListInventoryPageQuery,
            loans::GetUserLoansQuery,
            loans::ExportUserLoansMarcQuery,
            crate::models::loan::LoanMarcExportFormat,
            crate::models::loan::LoanMarcExportEncoding,
            loans::SendRemindersQuery,
            crate::models::loan::LoanDetails,
            crate::services::reminders::ReminderReport,
            crate::services::reminders::ReminderDetail,
            crate::services::reminders::ReminderError,
            crate::services::reminders::OverdueLoansPage,
            crate::services::reminders::OverdueLoanInfo,
            // Z39.50
            z3950::Z3950SearchQuery,
            z3950::Z3950SearchResponse,
            z3950::Z3950ImportRequest,
            z3950::Z3950ImportResponse,
            z3950::ImportItem,
            // Import report
            crate::models::import_report::ImportReport,
            crate::models::import_report::ImportAction,
            crate::models::import_report::DuplicateConfirmationRequired,
            crate::models::import_report::DuplicateItemBarcodeRequired,
            // Biblios
            biblios::CreateBiblioResponse,
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
            crate::models::stats_builder::StatsBuilderBody,
            crate::models::stats_builder::SelectField,
            crate::models::stats_builder::GroupByField,
            crate::models::stats_builder::StatsFilter,
            crate::models::stats_builder::HavingFilter,
            crate::models::stats_builder::FilterOperator,
            crate::models::stats_builder::StatsAggregation,
            crate::models::stats_builder::AggregateFunction,
            crate::models::stats_builder::TimeBucket,
            crate::models::stats_builder::TimeGranularity,
            crate::models::stats_builder::StatsOrderBy,
            crate::models::stats_builder::SortDirection,
            crate::models::stats_builder::StatsTableResponse,
            crate::models::stats_builder::ColumnMeta,
            crate::models::stats_builder::SavedStatsQuery,
            crate::models::stats_builder::SavedStatsQueryWrite,
            // Library info
            library_info::LibraryInfo,
            library_info::UpdateLibraryInfoRequest,
            loans::LoanSettings,
            loans::UpdateLoanSettingsRequest,
            z3950::Z3950ServerConfig,
            z3950::UpdateZ3950ServersRequest,
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
            crate::services::events::SendAnnouncementRequest,
            crate::services::events::AnnouncementReport,
            crate::services::events::AnnouncementError,
            // Admin config
            admin_config::ConfigResponse,
            admin_config::ConfigSectionInfo,
            admin_config::UpdateConfigSectionRequest,
            admin_config::TestEmailRequest,
            // Maintenance
            maintenance::MaintenanceRequest,
            maintenance::MaintenanceAction,
            maintenance::MaintenanceActionReport,
            maintenance::MaintenanceResponse,
            maintenance::MaintenanceTaskProgress,
            maintenance::CatalogZ3950RefreshProgress,
            maintenance::CatalogZ3950RefreshProgressStatus,
            maintenance::CatalogZ3950RefreshResult,
            // Background tasks
            tasks::TaskAcceptedResponse,
            crate::models::task::BackgroundTask,
            crate::models::task::TaskKind,
            crate::models::task::TaskStatus,
            crate::models::task::TaskProgress,
            // Audit
            audit::AuditQueryRequest,
            audit::AuditExportRequest,
            crate::models::audit::AuditLogPage,
            crate::models::audit::AuditLogEntry,
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
        (name = "biblios", description = "Bibliographic record management"),
        (name = "items", description = "Physical copies (items) — get biblio for a copy, update/delete by item id"),
        (name = "users", description = "User management"),
        (name = "loans", description = "Loan management"),
        (name = "holds", description = "Physical item hold queue"),
        (name = "inventory", description = "Stocktaking (inventory) sessions and barcode scans"),
        (name = "z3950", description = "Z39.50 catalog search"),
        (name = "stats", description = "Statistics"),
        (name = "visitor_counts", description = "Visitor counting"),
        (name = "schedules", description = "Library schedules (hours, closures)"),
        (name = "sources", description = "Acquisition source management"),
        (name = "equipment", description = "Library equipment management"),
        (name = "events", description = "Cultural events and school visits"),
        (name = "library_info", description = "Library global information (name, address, phones, email)"),
        (name = "series", description = "Series management"),
        (name = "collections", description = "Collections management"),
        (name = "public_types", description = "Borrower public types (child, adult, school, staff, senior)"),
        (name = "admin", description = "Admin runtime configuration"),
        (name = "audit", description = "Audit log"),
        (name = "maintenance", description = "Data-quality maintenance operations (admin only)"),
        (name = "tasks", description = "Background task status polling")
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
