use tokio::sync::mpsc;

/// Events emitted by the sync engine to the UI/controller layer.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Phase 1 header sync progress update.
    HeaderProgress(SyncProgress),

    /// The first batch of headers has been committed to SQLite.
    /// The inbox is now usable for display.
    FirstBatchReady {
        folder: String,
        emails_in_batch: u32,
    },

    /// Phase 1 header sync completed for a folder.
    HeaderSyncComplete { folder: String, total_emails: u32 },

    /// A non-fatal error occurred during sync (e.g., one malformed envelope skipped).
    Warning(String),
}

/// Progress data for header sync.
#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub folder: String,
    pub fetched: u32,
    pub total: u32,
}

impl std::fmt::Display for SyncProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Syncing headers: {} / {} ({})",
            self.fetched, self.total, self.folder
        )
    }
}

/// Convenience type alias for the sending half of the progress channel.
pub type SyncEventSender = mpsc::Sender<SyncEvent>;

/// Convenience type alias for the receiving half of the progress channel.
pub type SyncEventReceiver = mpsc::Receiver<SyncEvent>;

/// Create a new sync event channel with the given buffer size.
pub fn sync_event_channel(buffer: usize) -> (SyncEventSender, SyncEventReceiver) {
    mpsc::channel(buffer)
}
