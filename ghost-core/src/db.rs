//! SQLite persistence for conversations and messages.
//!
//! Schema is versioned with `PRAGMA user_version`; add new migrations to
//! `MIGRATIONS` and never edit an existing one.

use crate::conversation::ChatMessage;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

const DEFAULT_TITLE: &str = "New chat";
const TITLE_MAX_CHARS: usize = 40;

const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE conversations (
         id         TEXT PRIMARY KEY,
         title      TEXT NOT NULL,
         created_at INTEGER NOT NULL,
         updated_at INTEGER NOT NULL
     );
     CREATE TABLE messages (
         id              INTEGER PRIMARY KEY AUTOINCREMENT,
         conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
         role            TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
         content         TEXT NOT NULL,
         status          TEXT NOT NULL DEFAULT 'complete' CHECK (status IN ('complete', 'cancelled')),
         created_at      INTEGER NOT NULL
     );
     CREATE INDEX idx_messages_conversation ON messages (conversation_id, id);
     CREATE INDEX idx_conversations_updated ON conversations (updated_at DESC);",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    /// "complete", or "cancelled" for a reply the user stopped part-way.
    pub status: String,
    /// Unix milliseconds.
    pub timestamp: i64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn db_err(e: rusqlite::Error) -> String {
    format!("Database error: {e}")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// First line of the first message, trimmed and shortened.
pub fn title_from(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() {
        return DEFAULT_TITLE.to_string();
    }
    let mut title: String = line.chars().take(TITLE_MAX_CHARS).collect();
    if line.chars().count() > TITLE_MAX_CHARS {
        title.push('…');
    }
    title
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Could not create data dir: {e}"))?;
        }
        Self::init(Connection::open(path).map_err(db_err)?)
    }

    /// Used as a fallback when the on-disk database cannot be opened (and in tests).
    pub fn open_in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(db_err)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db_err)?;
        for (i, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            conn.execute_batch(&format!(
                "BEGIN; {migration} PRAGMA user_version = {}; COMMIT;",
                i + 1
            ))
            .map_err(db_err)?;
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Creates a conversation, or returns the newest one if it is still empty so
    /// repeated "New chat" clicks do not pile up blank conversations.
    pub fn create_conversation(&self) -> Result<ConversationSummary, String> {
        let conn = self.conn.lock().unwrap();
        let empty: Option<ConversationSummary> = conn
            .query_row(
                "SELECT c.id, c.title, c.created_at, c.updated_at FROM conversations c
                 WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id)
                 ORDER BY c.updated_at DESC LIMIT 1",
                [],
                row_to_summary,
            )
            .optional()
            .map_err(db_err)?;
        if let Some(existing) = empty {
            return Ok(existing);
        }
        let summary = ConversationSummary {
            id: Uuid::new_v4().to_string(),
            title: DEFAULT_TITLE.to_string(),
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![summary.id, summary.title, summary.created_at, summary.updated_at],
        )
        .map_err(db_err)?;
        Ok(summary)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, title, created_at, updated_at FROM conversations
                 ORDER BY updated_at DESC, created_at DESC",
            )
            .map_err(db_err)?;
        let rows = stmt.query_map([], row_to_summary).map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Title must not be empty".to_string());
        }
        let changed = self
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE conversations SET title = ?1 WHERE id = ?2", params![title, id])
            .map_err(db_err)?;
        if changed == 0 {
            return Err("Conversation not found".to_string());
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), String> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(db_err)?;
        Ok(())
    }

    pub fn messages(&self, conversation_id: &str) -> Result<Vec<StoredMessage>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, status, created_at FROM messages
                 WHERE conversation_id = ?1 ORDER BY id",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![conversation_id], |r| {
                Ok(StoredMessage {
                    id: r.get(0)?,
                    role: r.get(1)?,
                    content: r.get(2)?,
                    status: r.get(3)?,
                    timestamp: r.get(4)?,
                })
            })
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    pub fn last_user_message(&self, conversation_id: &str) -> Result<Option<String>, String> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT content FROM messages WHERE conversation_id = ?1 AND role = 'user'
                 ORDER BY id DESC LIMIT 1",
                params![conversation_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)
    }

    /// Stored messages as chat messages. With `exclude_last_exchange`, everything from
    /// the last user message onward is left out (used when regenerating).
    pub fn history(&self, conversation_id: &str, exclude_last_exchange: bool) -> Result<Vec<ChatMessage>, String> {
        let mut messages = self.messages(conversation_id)?;
        if exclude_last_exchange {
            if let Some(i) = messages.iter().rposition(|m| m.role == "user") {
                messages.truncate(i);
            }
        }
        Ok(messages
            .into_iter()
            .map(|m| ChatMessage { role: m.role, content: m.content })
            .collect())
    }

    /// Saves a finished (or stopped) exchange atomically. With `replace_last`, the
    /// previous last exchange is removed in the same transaction, so a failed
    /// regeneration never loses the old answer.
    pub fn commit_exchange(
        &self,
        conversation_id: &str,
        user: &str,
        assistant: &str,
        status: &str,
        replace_last: bool,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(db_err)?;

        if replace_last {
            tx.execute(
                "DELETE FROM messages WHERE conversation_id = ?1 AND id >= (
                     SELECT MAX(id) FROM messages WHERE conversation_id = ?1 AND role = 'user')",
                params![conversation_id],
            )
            .map_err(db_err)?;
        }

        let now = now_ms();
        tx.execute(
            "INSERT INTO messages (conversation_id, role, content, status, created_at)
             VALUES (?1, 'user', ?2, 'complete', ?3)",
            params![conversation_id, user, now],
        )
        .map_err(db_err)?;
        if !assistant.is_empty() {
            tx.execute(
                "INSERT INTO messages (conversation_id, role, content, status, created_at)
                 VALUES (?1, 'assistant', ?2, ?3, ?4)",
                params![conversation_id, assistant, status, now],
            )
            .map_err(db_err)?;
        }

        let title: String = tx
            .query_row("SELECT title FROM conversations WHERE id = ?1", params![conversation_id], |r| r.get(0))
            .map_err(db_err)?;
        if title == DEFAULT_TITLE {
            tx.execute(
                "UPDATE conversations SET title = ?1 WHERE id = ?2",
                params![title_from(user), conversation_id],
            )
            .map_err(db_err)?;
        }
        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)
    }
}

fn row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<ConversationSummary> {
    Ok(ConversationSummary {
        id: r.get(0)?,
        title: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn title_comes_from_first_line_and_is_shortened() {
        assert_eq!(title_from("  \n hello world \nmore"), "hello world");
        assert_eq!(title_from("   "), "New chat");
        let long = "x".repeat(60);
        let t = title_from(&long);
        assert_eq!(t.chars().count(), TITLE_MAX_CHARS + 1);
        assert!(t.ends_with('…'));
        assert_eq!(title_from("héllo"), "héllo");
    }

    #[test]
    fn migrations_are_idempotent_and_set_the_version() {
        let path = std::env::temp_dir().join(format!("ghost-query-test-{}-migrate.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let db = Db::open(&path).unwrap();
            let c = db.create_conversation().unwrap();
            db.commit_exchange(&c.id, "q", "a", "complete", false).unwrap();
        }
        // Reopening must not re-run migrations or lose data.
        let db = Db::open(&path).unwrap();
        let convs = db.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(db.messages(&convs[0].id).unwrap().len(), 2);
        let version: i64 = db.conn.lock().unwrap().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn new_chat_reuses_an_empty_conversation() {
        let db = db();
        let a = db.create_conversation().unwrap();
        let b = db.create_conversation().unwrap();
        assert_eq!(a.id, b.id);
        db.commit_exchange(&a.id, "hi", "hello", "complete", false).unwrap();
        let c = db.create_conversation().unwrap();
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn commit_stores_messages_in_order_and_titles_the_chat() {
        let db = db();
        let c = db.create_conversation().unwrap();
        db.commit_exchange(&c.id, "What is Rust?", "A language.", "complete", false).unwrap();
        db.commit_exchange(&c.id, "And Go?", "Another one.", "complete", false).unwrap();

        let msgs = db.messages(&c.id).unwrap();
        let seq: Vec<_> = msgs.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
        assert_eq!(
            seq,
            vec![("user", "What is Rust?"), ("assistant", "A language."), ("user", "And Go?"), ("assistant", "Another one.")]
        );
        assert_eq!(db.list_conversations().unwrap()[0].title, "What is Rust?");
    }

    #[test]
    fn stopped_reply_is_kept_with_cancelled_status() {
        let db = db();
        let c = db.create_conversation().unwrap();
        db.commit_exchange(&c.id, "q", "partial", "cancelled", false).unwrap();
        let msgs = db.messages(&c.id).unwrap();
        assert_eq!(msgs[1].status, "cancelled");
        assert_eq!(msgs[0].status, "complete");
    }

    #[test]
    fn regenerate_replaces_only_the_last_exchange() {
        let db = db();
        let c = db.create_conversation().unwrap();
        db.commit_exchange(&c.id, "q1", "a1", "complete", false).unwrap();
        db.commit_exchange(&c.id, "q2", "a2-old", "complete", false).unwrap();

        assert_eq!(db.last_user_message(&c.id).unwrap().as_deref(), Some("q2"));
        let before: Vec<_> = db.history(&c.id, true).unwrap().into_iter().map(|m| m.content).collect();
        assert_eq!(before, vec!["q1", "a1"]);

        db.commit_exchange(&c.id, "q2", "a2-new", "complete", true).unwrap();
        let contents: Vec<_> = db.messages(&c.id).unwrap().into_iter().map(|m| m.content).collect();
        assert_eq!(contents, vec!["q1", "a1", "q2", "a2-new"]);
    }

    #[test]
    fn history_for_a_missing_conversation_is_empty() {
        let db = db();
        assert!(db.history("nope", false).unwrap().is_empty());
        assert_eq!(db.last_user_message("nope").unwrap(), None);
    }

    #[test]
    fn committing_to_a_deleted_conversation_fails_and_writes_nothing() {
        let db = db();
        let c = db.create_conversation().unwrap();
        db.delete_conversation(&c.id).unwrap();
        assert!(db.commit_exchange(&c.id, "q", "a", "complete", false).is_err());
        assert!(db.messages(&c.id).unwrap().is_empty());
    }

    #[test]
    fn delete_cascades_to_messages_and_rename_works() {
        let db = db();
        let c = db.create_conversation().unwrap();
        db.commit_exchange(&c.id, "q", "a", "complete", false).unwrap();
        db.rename_conversation(&c.id, "  My chat ").unwrap();
        assert_eq!(db.list_conversations().unwrap()[0].title, "My chat");
        assert!(db.rename_conversation(&c.id, "  ").is_err());
        assert!(db.rename_conversation("nope", "x").is_err());

        db.delete_conversation(&c.id).unwrap();
        assert!(db.list_conversations().unwrap().is_empty());
        let orphans: i64 = db.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0)).unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn conversations_list_most_recently_active_first() {
        let db = db();
        let a = db.create_conversation().unwrap();
        db.commit_exchange(&a.id, "first", "x", "complete", false).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = db.create_conversation().unwrap();
        db.commit_exchange(&b.id, "second", "y", "complete", false).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        db.commit_exchange(&a.id, "again", "z", "complete", false).unwrap();

        let titles: Vec<_> = db.list_conversations().unwrap().into_iter().map(|c| c.title).collect();
        assert_eq!(titles, vec!["first", "second"]);
    }
}
