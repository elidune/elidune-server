use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    api::AuthenticatedUser,
    error::AppResult,
    models::reader_assistant::{
        AskRequest, AssistantReply, CreateAssistantSessionRequest, SendAssistantMessageRequest, SessionDetail,
    },
    services::audit,
};

use super::biblios::PaginatedResponse;

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/reader-assistant/sessions",
            post(create_session).get(list_sessions),
        )
        .route(
            "/reader-assistant/sessions/:id",
            get(get_session).delete(delete_session),
        )
        .route(
            "/reader-assistant/sessions/:id/messages",
            post(post_message),
        )
        .route("/ask", post(ask_shortcut))
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SessionsQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/reader-assistant/sessions",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    request_body = CreateAssistantSessionRequest,
    responses(
        (status = 201, description = "Session created", body = crate::models::reader_assistant::AiSession)
    )
)]
pub async fn create_session(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(body): Json<CreateAssistantSessionRequest>,
) -> AppResult<(StatusCode, Json<crate::models::reader_assistant::AiSession>)> {
    let session = state
        .services
        .reader_assistant
        .create_session(claims.user_id, body.title.as_deref())
        .await?;
    state.services.audit.log(
        audit::event::READER_ASSISTANT_SESSION_CREATED,
        Some(claims.user_id),
        Some("ai_session"),
        Some(session.id),
        None,
        None::<()>,
        audit::AuditLogMeta::success(),
    );
    Ok((StatusCode::CREATED, Json(session)))
}

#[utoipa::path(
    get,
    path = "/reader-assistant/sessions",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    params(SessionsQuery),
    responses(
        (status = 200, description = "Sessions list", body = PaginatedResponse<crate::models::reader_assistant::AiSession>)
    )
)]
pub async fn list_sessions(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<SessionsQuery>,
) -> AppResult<Json<PaginatedResponse<crate::models::reader_assistant::AiSession>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let (items, total) = state
        .services
        .reader_assistant
        .list_sessions(claims.user_id, page, per_page)
        .await?;
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

#[utoipa::path(
    get,
    path = "/reader-assistant/sessions/{id}",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Session details with messages", body = SessionDetail)
    )
)]
pub async fn get_session(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<SessionDetail>> {
    Ok(Json(
        state
            .services
            .reader_assistant
            .get_session_detail(claims.user_id, id)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/reader-assistant/sessions/{id}",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses((status = 204, description = "Session deleted"))
)]
pub async fn delete_session(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    state
        .services
        .reader_assistant
        .delete_session(claims.user_id, id)
        .await?;
    state.services.audit.log(
        audit::event::READER_ASSISTANT_SESSION_DELETED,
        Some(claims.user_id),
        Some("ai_session"),
        Some(id),
        None,
        None::<()>,
        audit::AuditLogMeta::success(),
    );
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/reader-assistant/sessions/{id}/messages",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    request_body = SendAssistantMessageRequest,
    responses(
        (status = 200, description = "Assistant reply", body = AssistantReply)
    )
)]
pub async fn post_message(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
    Json(body): Json<SendAssistantMessageRequest>,
) -> AppResult<Json<AssistantReply>> {
    let response = state
        .services
        .reader_assistant
        .send_message(claims.user_id, id, &body)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/ask",
    tag = "reader_assistant",
    security(("bearer_auth" = [])),
    request_body = AskRequest,
    responses(
        (status = 200, description = "Assistant reply", body = AssistantReply)
    )
)]
pub async fn ask_shortcut(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(body): Json<AskRequest>,
) -> AppResult<Json<AssistantReply>> {
    let session_id = if let Some(id) = body.session_id {
        id
    } else {
        state
            .services
            .reader_assistant
            .create_session(claims.user_id, Some("Quick ask"))
            .await?
            .id
    };
    let response = state
        .services
        .reader_assistant
        .send_message(
            claims.user_id,
            session_id,
            &SendAssistantMessageRequest {
                content: body.content,
                include_external: body.include_external,
            },
        )
        .await?;
    Ok(Json(response))
}
