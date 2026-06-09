//! Repository-level test: loan return atomically advances the next pending hold.

mod common;

use axum::http::StatusCode;
use common::fixtures;
use common::TestApp;
use elidune_server::models::hold::{CreateHold, HoldStatus};
use elidune_server::models::loan::CreateLoan;
use elidune_server::repository::{HoldsRepository, LoansRepository, Repository};
use serde_json::json;

#[tokio::test]
async fn loan_return_atomically_advances_next_hold() {
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = fixtures::ensure_first_setup(&app).await;
    let (reader_a_id, _) = fixtures::create_reader(&app, &admin_token, "repohold_a").await;
    let (reader_b_id, _) = fixtures::create_reader(&app, &admin_token, "repohold_b").await;

    let biblio_payload = json!({
        "title": "Repo Hold Atomic Test",
        "mediaType": "printedText",
        "lang": "fre",
        "items": [{ "barcode": "REPO-HOLD-001", "borrowable": true }]
    });

    let (status, body) = app
        .post_json("/api/v1/biblios", &biblio_payload, Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::CREATED, "create biblio: {body}");
    let item_id = fixtures::json_id(&body["biblio"]["items"][0]["id"]);

    let repo = app.state.services.repository.as_ref().clone();

    let hold_a = repo
        .holds_create(&CreateHold {
            user_id: reader_a_id,
            item_id,
            notes: None,
        })
        .await
        .expect("hold for reader A");
    let hold_b = repo
        .holds_create(&CreateHold {
            user_id: reader_b_id,
            item_id,
            notes: None,
        })
        .await
        .expect("hold for reader B");

    assert_eq!(hold_a.status, HoldStatus::Pending);
    assert_eq!(hold_b.status, HoldStatus::Pending);
    assert!(hold_a.position < hold_b.position);

    let checkout = repo
        .loans_create(&CreateLoan {
            user_id: reader_a_id,
            item_id: Some(item_id),
            item_identification: None,
            force: false,
        })
        .await
        .expect("checkout to reader A");

    let return_outcome = repo
        .loans_return(checkout.loan_id)
        .await
        .expect("return loan");

    let readied = return_outcome
        .readied_hold
        .as_ref()
        .expect("next hold should become ready in same transaction");
    assert_eq!(readied.user_id, reader_b_id);
    assert_eq!(readied.id, hold_b.id);
    assert_eq!(readied.status, HoldStatus::Ready);
    assert!(readied.expires_at.is_some());
}
