use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{LlmConfig, LlmProviderConfig},
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCallRequest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolCallRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: LlmToolFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct LlmToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl From<&LlmToolCall> for LlmToolCallRequest {
    fn from(value: &LlmToolCall) -> Self {
        Self {
            id: value.id.clone(),
            call_type: "function".to_string(),
            function: LlmToolFunctionCall {
                name: value.name.clone(),
                arguments: value.arguments.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmChatRequest {
    pub messages: Vec<LlmChatMessage>,
    pub max_output_tokens: usize,
    pub tools: Vec<LlmToolDefinition>,
    pub require_json_output: bool,
}

#[derive(Debug, Clone)]
pub struct LlmChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<LlmToolCall>,
    pub provider: String,
    pub model: String,
    pub token_usage: Option<i32>,
    pub latency_ms: i32,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn chat(&self, request: &LlmChatRequest) -> AppResult<LlmChatResponse>;
    async fn health(&self) -> bool;
}

#[derive(Clone)]
pub struct LlmRouter {
    providers: Vec<Arc<dyn LlmProvider>>,
}

impl LlmRouter {
    pub fn from_config(config: &LlmConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(config.request_timeout_ms))
            .build()
            .ok()?;

        let mut all: Vec<Arc<dyn LlmProvider>> = Vec::new();
        if let Some(provider) = build_provider("mistral", &client, config.providers.mistral.clone()) {
            all.push(provider);
        }
        if let Some(provider) = build_provider("claude", &client, config.providers.claude.clone()) {
            all.push(provider);
        }
        if let Some(provider) = build_provider("gemini", &client, config.providers.gemini.clone()) {
            all.push(provider);
        }
        if let Some(provider) = build_provider("ollama", &client, config.providers.ollama.clone()) {
            all.push(provider);
        }
        if all.is_empty() {
            return None;
        }

        let mut sorted = Vec::new();
        for id in &config.provider_priority {
            if let Some(pos) = all.iter().position(|p| p.id() == id.as_str()) {
                sorted.push(all.remove(pos));
            }
        }
        sorted.extend(all);
        Some(Self { providers: sorted })
    }

    pub async fn chat_with_fallback(
        &self,
        request: &LlmChatRequest,
    ) -> AppResult<(LlmChatResponse, bool)> {
        let mut last_error: Option<AppError> = None;
        println!("request: {:?}", request);
        for (index, provider) in self.providers.iter().enumerate() {
            match provider.chat(request).await {
                Ok(res) => return Ok((res, index > 0)),
                Err(err) => {
                    tracing::warn!("LLM provider '{}' failed: {}", provider.id(), err);
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::Internal("No LLM providers available".to_string())))
    }
}

fn build_provider(
    provider_name: &'static str,
    client: &Client,
    config: Option<LlmProviderConfig>,
) -> Option<Arc<dyn LlmProvider>> {
    let cfg = config?;
    if !cfg.enabled {
        return None;
    }
    Some(Arc::new(OpenAiCompatibleProvider {
        provider_name,
        client: client.clone(),
        base_url: cfg.base_url.trim_end_matches('/').to_string(),
        api_key: cfg.api_key,
        model: cfg.model,
    }))
}

#[derive(Clone)]
struct OpenAiCompatibleProvider {
    provider_name: &'static str,
    client: Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCall {
    id: String,
    function: ChatToolFunction,
}

#[derive(Debug, Deserialize)]
struct ChatToolFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    total_tokens: Option<i32>,
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &'static str {
        self.provider_name
    }

    async fn chat(&self, request: &LlmChatRequest) -> AppResult<LlmChatResponse> {
        let endpoint = format!("{}/chat/completions", self.base_url);
        let started = Instant::now();
        let mut payload = serde_json::json!({
            "model": self.model,
            "messages": request.messages,
            "temperature": 0.3,
            "max_tokens": request.max_output_tokens
        });
        if request.require_json_output {
            payload["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        if !request.tools.is_empty() {
            let tools_payload: Vec<Value> = request
                .tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();
            payload["tools"] = serde_json::json!(tools_payload);
            payload["tool_choice"] = serde_json::json!("auto");
        }
        let mut req = self
            .client
            .post(endpoint)
            .json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let res = req
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("{} request failed: {}", self.provider_name, e)))?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(AppError::BadRequest(format!(
                "{} returned {}: {}",
                self.provider_name, status, body
            )));
        }
        let payload: ChatCompletionsResponse = res
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("{} invalid response: {}", self.provider_name, e)))?;
        let message = payload
            .choices
            .first()
            .map(|c| &c.message)
            .ok_or_else(|| AppError::Internal(format!("{} returned no choices", self.provider_name)))?;
        let mut tool_calls = Vec::with_capacity(message.tool_calls.len());
        for tc in &message.tool_calls {
            let args = serde_json::from_str::<Value>(&tc.function.arguments).unwrap_or(Value::Object(Default::default()));
            tool_calls.push(LlmToolCall {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: args,
            });
        }
        Ok(LlmChatResponse {
            content: message.content.clone(),
            tool_calls,
            provider: self.provider_name.to_string(),
            model: self.model.clone(),
            token_usage: payload.usage.and_then(|u| u.total_tokens),
            latency_ms: started.elapsed().as_millis() as i32,
        })
    }

    async fn health(&self) -> bool {
        self.client
            .get(format!("{}/models", self.base_url))
            .send()
            .await
            .map(|res| res.status().is_success())
            .unwrap_or(false)
    }
}
