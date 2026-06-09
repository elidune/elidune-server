//! RBAC smoke tests: admin-only endpoints reject reader tokens.

mod common;

use axum::http::StatusCode;
use common::fixtures;
use common::TestApp;

#[tokio::test]
async fn admin_can_access_audit_log_reader_cannot() {
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = fixtures::ensure_first_setup(&app).await;
    let (_, reader_token) = fixtures::create_reader(&app, &admin_token, "rbacreader").await;

    let (admin_status, _) = app.get_json_with_auth("/api/v1/audit", &admin_token).await;
    assert_eq!(
        admin_status,
        StatusCode::OK,
        "admin should access audit log"
    );

    let (reader_status, _) = app.get_json_with_auth("/api/v1/audit", &reader_token).await;
    assert_eq!(
        reader_status,
        StatusCode::FORBIDDEN,
        "reader must not access audit log"
    );
}
