use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

const MAX_MESSAGES: usize = 20;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConversationMessage {
    pub id: String,
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub timestamp: u64,
}

/// Wire format for chat-completions requests.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Default)]
pub struct Conversation {
    messages: VecDeque<ConversationMessage>,
}

fn message(role: &str, content: &str) -> ConversationMessage {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ConversationMessage {
        id: Uuid::new_v4().to_string(),
        role: role.to_string(),
        content: content.to_string(),
        timestamp,
    }
}

impl Conversation {
    pub fn messages(&self) -> Vec<ConversationMessage> {
        self.messages.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Messages for the next request: stored history plus the new user turn, as
    /// structured chat messages. Does not modify history: a request that fails
    /// must not leave a dangling user message behind.
    pub fn history_for_request(&self, prompt: &str) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .map(|m| ChatMessage { role: m.role.clone(), content: m.content.clone() })
            .chain(std::iter::once(ChatMessage { role: "user".into(), content: prompt.to_string() }))
            .collect()
    }

    /// Record a finished (or cancelled-with-partial-output) exchange.
    pub fn commit_exchange(&mut self, user: &str, assistant: &str) {
        self.messages.push_back(message("user", user));
        if !assistant.is_empty() {
            self.messages.push_back(message("assistant", assistant));
        }
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_without_history_is_just_the_user_turn() {
        let msgs = Conversation::default().history_for_request("hi");
        assert_eq!(msgs, vec![ChatMessage { role: "user".into(), content: "hi".into() }]);
    }

    #[test]
    fn building_a_request_does_not_mutate_history() {
        let c = Conversation::default();
        let _ = c.history_for_request("hi");
        assert!(c.messages().is_empty());
    }

    #[test]
    fn commit_then_request_keeps_roles_and_order() {
        let mut c = Conversation::default();
        c.commit_exchange("q1", "a1");
        let roles: Vec<_> = c.history_for_request("q2").into_iter().map(|m| (m.role, m.content)).collect();
        assert_eq!(
            roles,
            vec![
                ("user".to_string(), "q1".to_string()),
                ("assistant".to_string(), "a1".to_string()),
                ("user".to_string(), "q2".to_string()),
            ]
        );
    }

    #[test]
    fn empty_assistant_reply_is_not_stored() {
        let mut c = Conversation::default();
        c.commit_exchange("q", "");
        assert_eq!(c.messages().len(), 1);
    }

    #[test]
    fn history_is_capped() {
        let mut c = Conversation::default();
        for i in 0..30 {
            c.commit_exchange(&format!("q{i}"), &format!("a{i}"));
        }
        let msgs = c.messages();
        assert_eq!(msgs.len(), MAX_MESSAGES);
        assert_eq!(msgs.last().unwrap().content, "a29");
    }
}
