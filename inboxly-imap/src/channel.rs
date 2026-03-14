use tokio::sync::mpsc;

use crate::folders::ImapFolder;

/// Events sent from the IMAP sync engine to the UI.
///
/// Sent via `tokio::sync::mpsc::Sender<SyncEvent>`.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Successfully connected and authenticated to an IMAP account.
    Connected { account_id: String },

    /// Disconnected from an account (intentional or error).
    Disconnected { account_id: String, reason: String },

    /// Authentication is required (token expired, password changed, etc.).
    /// The UI should prompt the user to re-authenticate.
    AuthRequired { account_id: String },

    /// Sync progress update for a folder.
    SyncProgress {
        account_id: String,
        folder: String,
        current: u64,
        total: u64,
        /// Phase name: "headers", "bodies", or "flags".
        phase: String,
    },

    /// Sync completed for a folder.
    SyncComplete { account_id: String, folder: String },

    /// An error occurred during sync.
    Error { account_id: String, message: String },

    /// New emails arrived in a folder.
    NewEmails {
        account_id: String,
        folder: String,
        count: u64,
    },

    /// Email flags changed in a folder (read, starred, etc.).
    FlagsChanged {
        account_id: String,
        folder: String,
        count: u64,
    },

    /// Folder list retrieved for an account.
    FolderList {
        account_id: String,
        folders: Vec<ImapFolder>,
    },

    // -- Phase 2 (M8): body download events --
    /// Phase 2 body download progress update for a folder.
    BodyDownloadProgress {
        account_id: String,
        folder: String,
        downloaded: u64,
        total: u64,
    },

    /// A single email body was fetched and indexed (on-demand fetch completion).
    BodyFetched { email_id: String },

    /// Phase 2 body download completed for a folder.
    BodyDownloadComplete { account_id: String, folder: String },

    /// Phase 2 body download encountered a non-fatal error on a single email.
    BodyDownloadError { email_id: String, error: String },

    // -- M9: Incremental sync + IDLE events --
    /// Emails were deleted on the server (no longer exist remotely).
    EmailsDeleted {
        account_id: String,
        folder: String,
        count: u64,
    },

    /// Incremental sync completed for a folder (post-IDLE or launch catch-up).
    IncrementalSyncComplete {
        account_id: String,
        folder: String,
        new_emails: u64,
        flag_changes: u64,
        deleted: u64,
    },

    /// Sync is now up to date for a folder (entering IDLE or poll wait).
    SyncUpToDate { account_id: String, folder: String },
}

/// Commands sent from the UI to the IMAP sync engine.
///
/// Sent via `tokio::sync::mpsc::Sender<UiCommand>`.
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Start syncing an account.
    StartSync { account_id: String },

    /// Stop syncing an account.
    StopSync { account_id: String },

    /// Force a full resync of a specific folder.
    ForceResync { account_id: String, folder: String },

    /// Gracefully shut down all sync tasks.
    Shutdown,

    /// Fetch a single email's body on demand (user opened it before Phase 2 reached it).
    FetchBodyOnDemand {
        email_id: String,
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
}

/// Create the bidirectional channel pair for sync engine <-> UI communication.
///
/// - `event_tx` / `event_rx`: Sync engine sends events, UI receives.
/// - `cmd_tx` / `cmd_rx`: UI sends commands, sync engine receives.
///
/// `buffer_size` controls the channel buffer (recommended: 64 or higher for
/// burst handling during initial sync).
pub fn create_sync_channels(
    buffer_size: usize,
) -> (
    mpsc::Sender<SyncEvent>,
    mpsc::Receiver<SyncEvent>,
    mpsc::Sender<UiCommand>,
    mpsc::Receiver<UiCommand>,
) {
    let (event_tx, event_rx) = mpsc::channel(buffer_size);
    let (cmd_tx, cmd_rx) = mpsc::channel(buffer_size);
    (event_tx, event_rx, cmd_tx, cmd_rx)
}
