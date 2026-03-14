//! Offline action replay against IMAP on reconnect.
//!
//! Drains the offline queue from SQLite and replays each action
//! against the IMAP server. Successfully replayed actions are
//! removed from the queue.
