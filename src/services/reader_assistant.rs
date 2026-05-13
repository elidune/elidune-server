use std::{collections::HashMap, sync::Arc};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    config::LlmConfig,
    error::{AppError, AppResult},
    models::{
        biblio::{BiblioQuery, BiblioShort},
        loan::LoanDetails,
        reader_assistant::{
            AiRecommendationRow, AiSession, AssistantReply, NewRecommendation, RecommendationItem,
            RecommendationKind, RecommendationRef, SendAssistantMessageRequest, SessionDetail, SessionMessage,
        },
    },
    repository::ReaderAssistantRepository,
    services::{
        audit::{self, AuditLogMeta, AuditOutcome},
        catalog::CatalogService,
        llm::{LlmChatMessage, LlmChatRequest, LlmRouter, LlmToolCall, LlmToolCallRequest, LlmToolDefinition},
        loans::LoansService,
    },
};

#[derive(Clone)]
pub struct ReaderAssistantService {
    repository: Arc<dyn ReaderAssistantRepository>,
    catalog: CatalogService,
    loans: LoansService,
    audit: audit::AuditService,
    llm_router: Option<Arc<LlmRouter>>,
    llm_config: Option<LlmConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LlmRecommendationPayload {
    biblio_id: Option<i64>,
    title: String,
    author: Option<String>,
    rationale: String,
    in_catalog: Option<bool>,
    score: Option<f64>,
    publication_year: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LlmAssistantPayload {
    answer: String,
    recommendations: Vec<LlmRecommendationPayload>,
}

#[derive(Debug)]
struct ToolExecutionResult {
    payload: Value,
    catalog_items: Vec<BiblioShort>,
}

impl ReaderAssistantService {
    pub fn new(
        repository: Arc<dyn ReaderAssistantRepository>,
        catalog: CatalogService,
        loans: LoansService,
        audit: audit::AuditService,
        llm_router: Option<Arc<LlmRouter>>,
        llm_config: Option<LlmConfig>,
    ) -> Self {
        Self {
            repository,
            catalog,
            loans,
            audit,
            llm_router,
            llm_config,
        }
    }

    pub async fn create_session(&self, user_id: i64, title: Option<&str>) -> AppResult<AiSession> {
        self.repository.ai_session_create(user_id, title).await
    }

    pub async fn list_sessions(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<AiSession>, i64)> {
        self.repository.ai_sessions_list(user_id, page, per_page).await
    }

    pub async fn get_session_detail(&self, user_id: i64, session_id: i64) -> AppResult<SessionDetail> {
        let session = self.repository.ai_session_get(user_id, session_id).await?;
        let messages = self.repository.ai_messages_list(user_id, session_id).await?;
        let rows = self
            .repository
            .ai_recommendations_for_session(user_id, session_id)
            .await?;
        let mapped = self
            .map_recommendation_rows(rows)
            .await?
            .into_iter()
            .fold(HashMap::<i64, Vec<RecommendationItem>>::new(), |mut acc, item| {
                acc.entry(item.0).or_default().push(item.1);
                acc
            });

        let mut payload_messages = Vec::with_capacity(messages.len());
        for message in messages {
            payload_messages.push(SessionMessage {
                id: message.id,
                role: message.role,
                content: message.content_redacted,
                provider: message.provider,
                model: message.model,
                token_usage: message.token_usage,
                latency_ms: message.latency_ms,
                created_at: message.created_at,
                recommendations: mapped.get(&message.id).cloned().unwrap_or_default(),
            });
        }
        Ok(SessionDetail {
            session,
            messages: payload_messages,
        })
    }

    pub async fn delete_session(&self, user_id: i64, session_id: i64) -> AppResult<()> {
        self.repository.ai_session_soft_delete(user_id, session_id).await
    }

    pub async fn send_message(
        &self,
        user_id: i64,
        session_id: i64,
        request: &SendAssistantMessageRequest,
    ) -> AppResult<AssistantReply> {
        let cfg = self.llm_config.clone();
        let content = request.content.trim();
        if content.is_empty() {
            return Err(AppError::Validation("Message content cannot be empty".to_string()));
        }
        let max_prompt_chars = cfg.as_ref().map(|c| c.max_prompt_chars).unwrap_or(4_000);
        if content.chars().count() > max_prompt_chars {
            return Err(AppError::Validation(format!(
                "Message too long (max {} characters)",
                max_prompt_chars
            )));
        }

        let daily_quota = cfg.as_ref().map(|c| c.daily_quota_per_user).unwrap_or(50) as i64;
        let used_today = self.repository.ai_user_message_count_today(user_id).await?;
        if used_today >= daily_quota {
            return Err(AppError::BusinessRule(format!(
                "Daily assistant quota reached ({} messages/day)",
                daily_quota
            )));
        }

        let _ = self.repository.ai_session_get(user_id, session_id).await?;
        let user_message = self
            .repository
            .ai_message_insert(
                session_id,
                "user",
                &redact_content(content),
                None,
                None,
                None,
                None,
            )
            .await?;

        let loans_context = self.user_loans_context(user_id).await?;
        let (seed_catalog_candidates, _) = self.catalog_candidates(content, 8).await?;
        let mut retrieved_catalog_candidates = seed_catalog_candidates.clone();

        let history_limit = cfg.as_ref().map(|c| c.max_history_messages).unwrap_or(12);
        let message_history = self.repository.ai_messages_list(user_id, session_id).await?;
        let mut llm_messages = Vec::new();
        llm_messages.push(LlmChatMessage {
            role: "system".to_string(),
            content: Some(system_prompt(
                request.include_external.unwrap_or(true),
                &loans_context,
            )),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        });
        for message in message_history.iter().rev().take(history_limit).rev() {
            llm_messages.push(LlmChatMessage {
                role: message.role.clone(),
                content: Some(message.content_redacted.clone()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            });
        }

        let (llm_payload, fallback_used, provider, model, token_usage, latency_ms) =
            if let Some(router) = &self.llm_router {
                let tools = reader_assistant_tools();
                let mut conversation = llm_messages;
                let mut iterations = 0usize;
                let mut aggregated_fallback = false;
                let mut meta_provider = None;
                let mut meta_model = None;
                let mut meta_token_usage = None;
                let mut meta_latency = None;
                loop {
                    if iterations >= 4 {
                        break (
                            deterministic_payload(content, &retrieved_catalog_candidates),
                            true,
                            meta_provider,
                            meta_model,
                            meta_token_usage,
                            meta_latency,
                        );
                    }
                    let llm_result = router
                        .chat_with_fallback(&LlmChatRequest {
                            messages: conversation.clone(),
                            max_output_tokens: 900,
                            tools: tools.clone(),
                            require_json_output: true,
                        })
                        .await;
                    let (llm, used_fallback) = match llm_result {
                        Ok(v) => v,
                        Err(error) => {
                            tracing::warn!("reader assistant fallback to deterministic mode: {}", error);
                            break (
                                deterministic_payload(content, &retrieved_catalog_candidates),
                                true,
                                meta_provider,
                                meta_model,
                                meta_token_usage,
                                meta_latency,
                            );
                        }
                    };
                    aggregated_fallback = aggregated_fallback || used_fallback;
                    meta_provider = Some(llm.provider.clone());
                    meta_model = Some(llm.model.clone());
                    meta_token_usage = llm.token_usage;
                    meta_latency = Some(llm.latency_ms);

                    if llm.tool_calls.is_empty() {
                        let payload = llm
                            .content
                            .as_deref()
                            .and_then(|c| serde_json::from_str::<LlmAssistantPayload>(c).ok())
                            .unwrap_or_else(|| deterministic_payload(content, &retrieved_catalog_candidates));
                        break (
                            payload,
                            aggregated_fallback,
                            meta_provider,
                            meta_model,
                            meta_token_usage,
                            meta_latency,
                        );
                    }

                    conversation.push(LlmChatMessage {
                        role: "assistant".to_string(),
                        content: llm.content,
                        tool_call_id: None,
                        name: None,
                        tool_calls: Some(
                            llm.tool_calls
                                .iter()
                                .map(LlmToolCallRequest::from)
                                .collect::<Vec<_>>(),
                        ),
                    });
                    for tool_call in &llm.tool_calls {
                        let tool_output = self.execute_tool_call(user_id, tool_call).await?;
                        retrieved_catalog_candidates.extend(tool_output.catalog_items);
                        conversation.push(LlmChatMessage {
                            role: "tool".to_string(),
                            content: Some(tool_output.payload.to_string()),
                            tool_call_id: Some(tool_call.id.clone()),
                            name: Some(tool_call.name.clone()),
                            tool_calls: None,
                        });
                    }
                    iterations += 1;
                }
            } else {
                (
                    deterministic_payload(content, &retrieved_catalog_candidates),
                    true,
                    None,
                    None,
                    None,
                    None,
                )
            };

        let (recommendations, rows_to_persist) =
            self.merge_recommendations(llm_payload.recommendations, &retrieved_catalog_candidates);
        let answer = llm_payload.answer.trim().to_string();
        let assistant_msg = self
            .repository
            .ai_message_insert(
                session_id,
                "assistant",
                &redact_content(&answer),
                provider.as_deref(),
                model.as_deref(),
                token_usage,
                latency_ms,
            )
            .await?;
        self.repository
            .ai_recommendations_insert(assistant_msg.id, &rows_to_persist)
            .await?;

        self.audit.log(
            audit::event::READER_ASSISTANT_MESSAGE,
            Some(user_id),
            Some("ai_session"),
            Some(session_id),
            None,
            Some(serde_json::json!({
                "userMessageId": user_message.id,
                "assistantMessageId": assistant_msg.id,
                "provider": provider,
                "model": model,
                "fallbackUsed": fallback_used,
                "recommendationCount": recommendations.len()
            })),
            if fallback_used {
                AuditLogMeta {
                    outcome: AuditOutcome::Failure,
                    http_status: Some(200),
                    error_code: Some("llm_fallback".to_string()),
                    error_message: Some("LLM unavailable, deterministic fallback used".to_string()),
                }
            } else {
                AuditLogMeta::success()
            },
        );
        tracing::info!(
            target: "reader_assistant",
            user_id = user_id,
            session_id = session_id,
            provider = provider.as_deref().unwrap_or("deterministic"),
            model = model.as_deref().unwrap_or("none"),
            fallback_used = fallback_used,
            token_usage = token_usage,
            latency_ms = latency_ms,
            recommendation_count = recommendations.len(),
            "reader assistant response generated"
        );

        Ok(AssistantReply {
            session_id,
            assistant_message_id: assistant_msg.id,
            answer,
            provider,
            model,
            fallback_used,
            recommendations,
        })
    }

    async fn map_recommendation_rows(
        &self,
        rows: Vec<AiRecommendationRow>,
    ) -> AppResult<Vec<(i64, RecommendationItem)>> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let biblio = if let Some(id) = row.biblio_id {
                self.catalog.get_biblio(id).await.map(Into::into).ok()
            } else {
                None
            };
            let external_ref = row
                .external_ref
                .clone()
                .and_then(|v| serde_json::from_value::<RecommendationRef>(v).ok());
            out.push((
                row.message_id,
                RecommendationItem {
                    id: row.id,
                    kind: RecommendationKind::from(row.kind.as_str()),
                    biblio_id: row.biblio_id,
                    biblio,
                    external_ref,
                    score: row.score,
                    rationale: row.rationale,
                },
            ));
        }
        Ok(out)
    }

    async fn user_loans_context(&self, user_id: i64) -> AppResult<String> {
        let (archived, _) = self.loans.get_user_archived_loans(user_id, 1, 10).await?;
        Ok(format_loans_context(&archived))
    }

    async fn catalog_candidates(&self, query: &str, limit: i64) -> AppResult<(Vec<BiblioShort>, i64)> {
        let biblio_query = BiblioQuery {
            media_type: None,
            isbn: None,
            barcode: None,
            author: None,
            title: None,
            editor: None,
            lang: None,
            subject: None,
            content: None,
            keywords: None,
            freesearch: Some(query.to_string()),
            audience_type: None,
            archive: Some(false),
            serie: None,
            serie_id: None,
            collection: None,
            collection_id: None,
            include_without_active_items: Some(false),
            page: Some(1),
            per_page: Some(limit.clamp(1, 50)),
        };
        self.catalog.search_biblios(&biblio_query).await
    }

    async fn execute_tool_call(
        &self,
        user_id: i64,
        tool_call: &LlmToolCall,
    ) -> AppResult<ToolExecutionResult> {
        match tool_call.name.as_str() {
            "search_catalog" => {
                let query = tool_call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let limit = tool_call
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(8)
                    .clamp(1, 20);
                let (items, total) = self.catalog_candidates(&query, limit).await?;
                let payload_items: Vec<Value> = items
                    .iter()
                    .map(|b| {
                        json!({
                            "id": b.id,
                            "title": b.title,
                            "author": b.author.as_ref().map(|a| {
                                format!(
                                    "{} {}",
                                    a.firstname.clone().unwrap_or_default(),
                                    a.lastname.clone().unwrap_or_default()
                                ).trim().to_string()
                            }),
                            "isbn": b.isbn.as_ref().map(|i| i.to_string()),
                            "mediaType": b.media_type,
                            "availableCopies": b.items.iter().filter(|i| i.borrowable && !i.borrowed).count()
                        })
                    })
                    .collect();
                Ok(ToolExecutionResult {
                    payload: json!({ "query": query, "total": total, "items": payload_items }),
                    catalog_items: items,
                })
            }
            "get_biblio_details" => {
                let biblio_id = tool_call
                    .arguments
                    .get("biblioId")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AppError::Validation("get_biblio_details requires biblioId".to_string()))?;
                let biblio = self.catalog.get_biblio(biblio_id).await?;
                let short: BiblioShort = biblio.clone().into();
                Ok(ToolExecutionResult {
                    payload: json!({
                        "id": biblio.id,
                        "title": biblio.title,
                        "isbn": biblio.isbn,
                        "authors": biblio.authors,
                        "subject": biblio.subject,
                        "keywords": biblio.keywords,
                        "items": short.items
                    }),
                    catalog_items: vec![short],
                })
            }
            "get_recent_loans" => {
                let limit = tool_call
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10)
                    .clamp(1, 20);
                let (loans, _) = self.loans.get_user_archived_loans(user_id, 1, limit).await?;
                let payload_loans: Vec<Value> = loans
                    .iter()
                    .map(|loan| {
                        json!({
                            "loanId": loan.id,
                            "title": loan.biblio.title,
                            "author": loan.biblio.author.as_ref().map(|a| {
                                format!(
                                    "{} {}",
                                    a.firstname.clone().unwrap_or_default(),
                                    a.lastname.clone().unwrap_or_default()
                                ).trim().to_string()
                            }),
                            "returnedAt": loan.returned_at
                        })
                    })
                    .collect();
                Ok(ToolExecutionResult {
                    payload: json!({ "loans": payload_loans }),
                    catalog_items: Vec::new(),
                })
            }
            _ => Ok(ToolExecutionResult {
                payload: json!({
                    "error": format!("Unknown tool '{}'", tool_call.name)
                }),
                catalog_items: Vec::new(),
            }),
        }
    }

    fn merge_recommendations(
        &self,
        llm_recs: Vec<LlmRecommendationPayload>,
        catalog_candidates: &[BiblioShort],
    ) -> (Vec<RecommendationItem>, Vec<NewRecommendation>) {
        let mut output = Vec::new();
        let mut db_rows = Vec::new();
        let mut biblio_map: HashMap<String, &BiblioShort> = HashMap::new();
        let mut biblio_id_map: HashMap<i64, &BiblioShort> = HashMap::new();
        for b in catalog_candidates {
            if let Some(title) = &b.title {
                biblio_map.insert(normalize_key(title), b);
            }
            biblio_id_map.insert(b.id, b);
        }

        for rec in llm_recs {
            let key = normalize_key(&rec.title);
            let matched = rec
                .biblio_id
                .and_then(|id| biblio_id_map.get(&id).copied())
                .or_else(|| biblio_map.get(&key).copied());
            let kind = if matched.is_some() || rec.in_catalog.unwrap_or(false) {
                RecommendationKind::InCatalog
            } else {
                RecommendationKind::External
            };
            let biblio = matched.cloned();
            let biblio_id = biblio.as_ref().map(|b| b.id);
            let score = rec.score.unwrap_or(0.7).clamp(0.0, 1.0);
            let external_ref = if biblio.is_none() {
                Some(RecommendationRef {
                    title: rec.title.clone(),
                    author: rec.author.clone(),
                    publication_year: rec.publication_year,
                    source_note: Some("Suggested by assistant (outside local catalog)".to_string()),
                })
            } else {
                None
            };

            output.push(RecommendationItem {
                id: 0,
                kind,
                biblio_id,
                biblio,
                external_ref: external_ref.clone(),
                score,
                rationale: rec.rationale.clone(),
            });
            db_rows.push(NewRecommendation {
                kind,
                biblio_id,
                external_ref: external_ref.and_then(|x| serde_json::to_value(x).ok()),
                score,
                rationale: rec.rationale,
            });
        }

        if output.is_empty() {
            for candidate in catalog_candidates.iter().take(3) {
                let reason = "Matches your recent interests and is currently in catalog".to_string();
                output.push(RecommendationItem {
                    id: 0,
                    kind: RecommendationKind::InCatalog,
                    biblio_id: Some(candidate.id),
                    biblio: Some(candidate.clone()),
                    external_ref: None,
                    score: 0.65,
                    rationale: reason.clone(),
                });
                db_rows.push(NewRecommendation {
                    kind: RecommendationKind::InCatalog,
                    biblio_id: Some(candidate.id),
                    external_ref: None,
                    score: 0.65,
                    rationale: reason,
                });
            }
        }
        (output, db_rows)
    }
}

fn redact_content(input: &str) -> String {
    let mut out = input.replace('@', "[at]");
    if out.len() > 2000 {
        out.truncate(2000);
        out.push('…');
    }
    out
}

fn format_loans_context(loans: &[LoanDetails]) -> String {
    if loans.is_empty() {
        return "No recent loans available".to_string();
    }
    let mut rows = Vec::new();
    for loan in loans.iter().take(10) {
        let title = loan
            .biblio
            .title
            .clone()
            .unwrap_or_else(|| "Unknown title".to_string());
        let author = loan
            .biblio
            .author
            .as_ref()
            .map(|a| {
                format!(
                    "{} {}",
                    a.firstname.clone().unwrap_or_default(),
                    a.lastname.clone().unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "Unknown author".to_string());
        rows.push(format!("{title} - {author}"));
    }
    rows.join("\n")
}

fn normalize_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn deterministic_payload(user_query: &str, catalog_candidates: &[BiblioShort]) -> LlmAssistantPayload {
    let mut recommendations = Vec::new();
    for biblio in catalog_candidates.iter().take(3) {
        recommendations.push(LlmRecommendationPayload {
            biblio_id: Some(biblio.id),
            title: biblio
                .title
                .clone()
                .unwrap_or_else(|| "Untitled work".to_string()),
            author: biblio.author.as_ref().map(|a| {
                format!(
                    "{} {}",
                    a.firstname.clone().unwrap_or_default(),
                    a.lastname.clone().unwrap_or_default()
                )
                .trim()
                .to_string()
            }),
            rationale: "Selected from your library catalog to match your request.".to_string(),
            in_catalog: Some(true),
            score: Some(0.65),
            publication_year: None,
        });
    }
    if recommendations.is_empty() {
        recommendations.push(LlmRecommendationPayload {
            biblio_id: None,
            title: "Suggestion outside catalog".to_string(),
            author: None,
            rationale: "No direct catalog match found, please refine genre or author.".to_string(),
            in_catalog: Some(false),
            score: Some(0.5),
            publication_year: None,
        });
    }
    LlmAssistantPayload {
        answer: format!(
            "I analyzed your request ({}) and selected recommendations based on your library context.",
            user_query
        ),
        recommendations,
    }
}

fn system_prompt(include_external: bool, loans_context: &str) -> String {
    format!(
        "You are a professional reading advisory assistant for a public library.\n\
Return strict JSON with this shape:\n\
{{\"answer\":\"string\",\"recommendations\":[{{\"biblioId\":\"number|null\",\"title\":\"string\",\"author\":\"string|null\",\"rationale\":\"string\",\"inCatalog\":true|false,\"score\":0.0,\"publicationYear\":2020}}]}}\n\
Use at most 5 recommendations.\n\
Use tools to fetch catalog data before recommending titles.\n\
When a recommendation is in catalog, set biblioId from tool results.\n\
When include_external is false, recommendations must stay in catalog and biblioId must be set.\n\
include_external={}\n\
Recent loan context:\n{}",
        include_external,
        loans_context
    )
}

fn reader_assistant_tools() -> Vec<LlmToolDefinition> {
    vec![
        LlmToolDefinition {
            name: "search_catalog".to_string(),
            description: "Search Elidune catalog by free-text query and return candidate biblios.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "required": ["query"]
            }),
        },
        LlmToolDefinition {
            name: "get_biblio_details".to_string(),
            description: "Load full details for one catalog biblio by ID.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "biblioId": { "type": "integer" }
                },
                "required": ["biblioId"]
            }),
        },
        LlmToolDefinition {
            name: "get_recent_loans".to_string(),
            description: "Get recent archived loans for the current user profile context.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                }
            }),
        },
    ]
}
