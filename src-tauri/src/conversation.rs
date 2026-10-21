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

    /// Prompt for the next request. Does not modify history: a request that
    /// fails must not leave a dangling user message behind.
    pub fn build_prompt(&self, prompt: &str) -> String {
        if self.messages.is_empty() {
            return prompt.to_string();
        }
        let context = self
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Previous conversation:\n{}\n\nUser: {}", context, prompt)
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
    fn prompt_without_history_is_unchanged() {
        assert_eq!(Conversation::default().build_prompt("hi"), "hi");
    }

    #[test]
    fn build_prompt_does_not_mutate_history() {
        let c = Conversation::default();
        let _ = c.build_prompt("hi");
        assert!(c.messages().is_empty());
    }

    #[test]
    fn commit_then_prompt_includes_context() {
        let mut c = Conversation::default();
        c.commit_exchange("q1", "a1");
        let p = c.build_prompt("q2");
        assert!(p.contains("user: q1") && p.contains("assistant: a1") && p.ends_with("User: q2"));
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
