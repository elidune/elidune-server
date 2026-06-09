//! In-process LMS integration tests (health, auth, loan/hold golden path).

mod common;

use axum::http::StatusCode;
use common::fixtures;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn test_health_check_in_process() {
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let (status, body) = app.get_json("/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["database"]["connected"], true);
}

#[tokio::test]
async fn test_first_setup_and_login_in_process() {
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let token = fixtures::ensure_first_setup(&app).await;
    assert!(!token.is_empty());

    let (status, body) = app.get_json_with_auth("/api/v1/auth/me", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["login"], "testadmin");
}

#[tokio::test]
async fn test_golden_path_loan_hold_return() {
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = fixtures::ensure_first_setup(&app).await;
    let (reader_b_id, reader_b_token) =
        fixtures::create_reader(&app, &admin_token, "readerb").await;
    let (reader_c_id, reader_c_token) =
        fixtures::create_reader(&app, &admin_token, "readerc").await;

    // Create biblio + borrowable item
    let biblio_payload = json!({
        "title": "Golden Path Book",
        "mediaType": "printedText",
        "lang": "fre",
        "items": [{
            "barcode": "GP-001",
            "borrowable": true
        }]
    });

    let (status, body) = app
        .post_json("/api/v1/biblios", &biblio_payload, Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::CREATED, "create biblio: {body}");
    let biblio = &body["biblio"];

    let item_id = fixtures::json_id(&biblio["items"][0]["id"]);

    // Reader C places hold (second in queue)
    let hold_c = json!({ "userId": reader_c_id.to_string(), "itemId": item_id });
    let (hold_status, hold_body) = app
        .post_json("/api/v1/holds", &hold_c, Some(&reader_c_token))
        .await;
    assert_eq!(hold_status, StatusCode::CREATED, "place hold C: {hold_body}");

    // Reader B places hold (first in queue)
    let hold_b = json!({ "userId": reader_b_id.to_string(), "itemId": item_id });
    let (hold_b_status, hold_b_body) = app
        .post_json("/api/v1/holds", &hold_b, Some(&reader_b_token))
        .await;
    assert_eq!(hold_b_status, StatusCode::CREATED, "place hold B: {hold_b_body}");

    // Checkout to reader B (should fulfill their hold)
    let loan_payload = json!({
        "userId": reader_b_id.to_string(),
        "itemId": item_id
    });
    let (loan_status, loan_body) = app
        .post_json("/api/v1/loans", &loan_payload, Some(&admin_token))
        .await;
    assert_eq!(loan_status, StatusCode::CREATED, "checkout: {loan_body}");

    let loan_id = fixtures::json_id(&loan_body["id"]);

    // Return loan — reader C's hold should become ready
    let (return_status, return_body) = app
        .post_empty(
            &format!("/api/v1/loans/{loan_id}/return"),
            Some(&admin_token),
        )
        .await;
    assert_eq!(return_status, StatusCode::OK, "return: {return_body}");
}
