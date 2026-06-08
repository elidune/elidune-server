//! Typed publishers for the SSE broadcast channel.

use tokio::sync::broadcast;

use crate::models::dto::sse::SsePayload;

/// Wrapper around the application-wide SSE broadcast sender.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<SsePayload>,
}

impl EventBus {
    pub fn new(sender: broadcast::Sender<SsePayload>) -> Self {
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SsePayload> {
        self.sender.subscribe()
    }

    fn publish(&self, payload: SsePayload) {
        if self.sender.send(payload).is_err() {
            tracing::trace!("SSE event dropped (no subscribers)");
        }
    }

    pub fn loan_created(&self, loan_id: i64, user_id: i64, item_id: i64) {
        self.publish(SsePayload {
            event: "loan.created".into(),
            loan_id: Some(loan_id.to_string()),
            user_id: Some(user_id.to_string()),
            item_id: Some(item_id.to_string()),
            hold_id: None,
        });
    }

    pub fn loan_returned(&self, loan_id: i64, user_id: i64, item_id: i64) {
        self.publish(SsePayload {
            event: "loan.returned".into(),
            loan_id: Some(loan_id.to_string()),
            user_id: Some(user_id.to_string()),
            item_id: Some(item_id.to_string()),
            hold_id: None,
        });
    }

    pub fn hold_ready(&self, hold_id: i64, user_id: i64, item_id: i64) {
        self.publish(SsePayload {
            event: "hold.ready".into(),
            loan_id: None,
            user_id: Some(user_id.to_string()),
            item_id: Some(item_id.to_string()),
            hold_id: Some(hold_id.to_string()),
        });
    }
}
