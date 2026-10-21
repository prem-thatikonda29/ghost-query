//! Request-side conversation logic. Persistence lives in `db.rs`.

use serde::{Deserialize, Serialize};

/// Approximate token budget for history sent with each request. Conservative so it
/// fits small-context models; per-model limits can replace it later.
pub const HISTORY_TOKEN_BUDGET: usize = 6000;

/// Wire format for chat-completions requests.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Rough estimate (about 4 characters per token plus per-message overhead).
pub fn approx_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 4
}

/// Keeps the most recent messages that fit `budget`, always keeping the last one
/// (the new user turn), and never starting the history on an assistant message.
pub fn trim_history(mut history: Vec<ChatMessage>, budget: usize) -> Vec<ChatMessage> {
    let mut used = 0;
    let mut start = history.len();
    for (i, message) in history.iter().enumerate().rev() {
        let cost = approx_tokens(&message.content);
        if start < history.len() && used + cost > budget {
            break;
        }
        used += cost;
        start = i;
    }
    let mut kept = history.split_off(start);
    while kept.len() > 1 && kept[0].role != "user" {
        kept.remove(0);
    }
    kept
}

/// `[system, ...trimmed history, new user turn]`.
pub fn build_request_messages(system: &str, history: Vec<ChatMessage>, prompt: &str, budget: usize) -> Vec<ChatMessage> {
    let mut all = history;
    all.push(ChatMessage { role: "user".into(), content: prompt.to_string() });
    let mut messages = vec![ChatMessage { role: "system".into(), content: system.to_string() }];
    messages.extend(trim_history(all, budget));
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into() }
    }

    #[test]
    fn short_history_is_kept_whole_with_system_first_and_user_last() {
        let out = build_request_messages("sys", vec![m("user", "q1"), m("assistant", "a1")], "q2", 1000);
        let seq: Vec<_> = out.iter().map(|x| (x.role.as_str(), x.content.as_str())).collect();
        assert_eq!(seq, vec![("system", "sys"), ("user", "q1"), ("assistant", "a1"), ("user", "q2")]);
    }

    #[test]
    fn no_history_is_just_system_and_the_prompt() {
        let out = build_request_messages("sys", vec![], "hi", 1000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1], m("user", "hi"));
    }

    #[test]
    fn oldest_messages_are_dropped_first_when_over_budget() {
        let big = "x".repeat(400); // ~104 tokens each
        let history = vec![m("user", &big), m("assistant", &big), m("user", &big), m("assistant", &big)];
        let out = trim_history([history, vec![m("user", "new")]].concat(), 250);
        // Budget fits the new prompt plus about two big messages; history must start on a user turn.
        assert_eq!(out.last().unwrap().content, "new");
        assert_eq!(out.first().unwrap().role, "user");
        assert!(out.len() < 5);
    }

    #[test]
    fn the_new_prompt_is_kept_even_if_it_alone_exceeds_the_budget() {
        let huge = "x".repeat(10_000);
        let out = trim_history(vec![m("user", "old"), m("assistant", "older"), m("user", &huge)], 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, huge);
    }

    #[test]
    fn history_never_starts_with_an_assistant_message() {
        let out = trim_history(vec![m("user", "a"), m("assistant", "b"), m("user", "c")], 2 * approx_tokens("b") + 1);
        assert_eq!(out.first().unwrap().role, "user");
    }
}
