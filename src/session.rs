//! Session management for multi-turn conversations.
//!
//! The OpenAI chat API is stateless, but Claude CLI supports continuing a
//! conversation via `--resume <session-id>`. This module bridges the two by
//! maintaining a `thread_id -> session_id` lookup table with automatic
//! TTL-based expiration.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Internal entry stored per active thread.
struct SessionEntry {
    /// The Claude CLI session ID returned in the `system` NDJSON event.
    session_id: String,
    /// Wall-clock time of the most recent access (store or get).
    last_used: Instant,
}

/// Thread-safe, TTL-aware map from OpenAI-style `thread_id` values to
/// Claude CLI session IDs.
///
/// Entries that have not been accessed within [`ttl`] are treated as expired
/// and ignored on [`get`](Self::get). A separate
/// [`cleanup_expired`](Self::cleanup_expired) method physically removes them.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    ttl: Duration,
}

impl SessionManager {
    /// Create a new session manager with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Store a session ID for a thread.
    pub fn store(&self, thread_id: &str, session_id: String) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions.insert(
            thread_id.to_string(),
            SessionEntry {
                session_id,
                last_used: Instant::now(),
            },
        );
    }

    /// Get the session ID for a thread, if it exists and hasn't expired.
    pub fn get(&self, thread_id: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions.get(thread_id).and_then(|entry| {
            if entry.last_used.elapsed() < self.ttl {
                Some(entry.session_id.clone())
            } else {
                None
            }
        })
    }

    /// Clear a specific session.
    #[allow(dead_code)]
    pub fn clear(&self, thread_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions.remove(thread_id);
    }

    /// Remove all expired sessions.
    pub fn cleanup_expired(&self) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions.retain(|_, entry| entry.last_used.elapsed() < self.ttl);
    }
}

#[cfg(test)]
mod tests {
    //! Tests for session management: store/get, TTL expiry, cleanup, and
    //! concurrent access safety.

    use super::*;

    /// Basic store-then-get should return the stored session ID.
    #[test]
    fn test_store_and_get() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        mgr.store("thread-1", "sess-abc".to_string());
        assert_eq!(mgr.get("thread-1").as_deref(), Some("sess-abc"));
    }

    /// Storing again with the same thread ID replaces the previous session.
    #[test]
    fn test_overwrite() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        mgr.store("thread-1", "sess-old".to_string());
        mgr.store("thread-1", "sess-new".to_string());
        assert_eq!(mgr.get("thread-1").as_deref(), Some("sess-new"));
    }

    /// `clear()` removes a specific thread's session.
    #[test]
    fn test_clear() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        mgr.store("thread-1", "sess-abc".to_string());
        mgr.clear("thread-1");
        assert!(mgr.get("thread-1").is_none());
    }

    /// Different thread IDs map to independent sessions.
    #[test]
    fn test_independent_threads() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        mgr.store("thread-1", "sess-1".to_string());
        mgr.store("thread-2", "sess-2".to_string());
        assert_eq!(mgr.get("thread-1").as_deref(), Some("sess-1"));
        assert_eq!(mgr.get("thread-2").as_deref(), Some("sess-2"));
    }

    /// Looking up a thread that was never stored returns `None`.
    #[test]
    fn test_missing_thread() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        assert!(mgr.get("nonexistent").is_none());
    }

    /// With a zero TTL, entries expire immediately after store.
    #[test]
    fn test_ttl_expiry() {
        let mgr = SessionManager::new(Duration::from_secs(0));
        mgr.store("thread-1", "sess-abc".to_string());
        // Entry should be expired
        assert!(mgr.get("thread-1").is_none());
    }

    /// `cleanup_expired()` physically removes all expired entries.
    #[test]
    fn test_cleanup_expired() {
        let mgr = SessionManager::new(Duration::from_secs(0));
        mgr.store("thread-1", "sess-1".to_string());
        mgr.store("thread-2", "sess-2".to_string());
        mgr.cleanup_expired();
        let sessions = mgr.sessions.lock().unwrap();
        assert!(sessions.is_empty());
    }

    /// `cleanup_expired()` keeps entries that are still within the TTL.
    #[test]
    fn test_cleanup_retains_fresh_sessions() {
        let mgr = SessionManager::new(Duration::from_secs(3600));
        mgr.store("thread-1", "sess-1".to_string());
        mgr.store("thread-2", "sess-2".to_string());
        mgr.cleanup_expired();
        // Both sessions should survive cleanup
        assert_eq!(mgr.get("thread-1").as_deref(), Some("sess-1"));
        assert_eq!(mgr.get("thread-2").as_deref(), Some("sess-2"));
    }

    /// Concurrent store/get from 10 threads should not panic or lose data.
    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(SessionManager::new(Duration::from_secs(3600)));
        let mut handles = vec![];

        // Spawn multiple threads doing concurrent store/get operations
        for i in 0..10 {
            let mgr = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let tid = format!("thread-{}", i);
                let sid = format!("sess-{}", i);
                mgr.store(&tid, sid.clone());
                assert_eq!(mgr.get(&tid).as_deref(), Some(sid.as_str()));
            }));
        }

        for handle in handles {
            handle.join().expect("thread should not panic");
        }

        // Verify all sessions are accessible
        for i in 0..10 {
            let tid = format!("thread-{}", i);
            let sid = format!("sess-{}", i);
            assert_eq!(mgr.get(&tid).as_deref(), Some(sid.as_str()));
        }
    }
}
