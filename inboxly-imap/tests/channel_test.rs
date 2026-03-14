use inboxly_imap::channel::{SyncEvent, UiCommand, create_sync_channels};

#[test]
fn sync_event_variants_constructible() {
    // Ensure all variants can be constructed
    let _ = SyncEvent::Connected {
        account_id: "test".to_string(),
    };
    let _ = SyncEvent::Disconnected {
        account_id: "test".to_string(),
        reason: "timeout".to_string(),
    };
    let _ = SyncEvent::AuthRequired {
        account_id: "test".to_string(),
    };
    let _ = SyncEvent::SyncProgress {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        current: 50,
        total: 100,
        phase: "headers".to_string(),
    };
    let _ = SyncEvent::SyncComplete {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
    };
    let _ = SyncEvent::Error {
        account_id: "test".to_string(),
        message: "connection lost".to_string(),
    };
    let _ = SyncEvent::NewEmails {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        count: 5,
    };
    let _ = SyncEvent::FlagsChanged {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        count: 2,
    };
    let _ = SyncEvent::FolderList {
        account_id: "test".to_string(),
        folders: vec![],
    };
}

#[test]
fn ui_command_variants_constructible() {
    let _ = UiCommand::StartSync {
        account_id: "test".to_string(),
    };
    let _ = UiCommand::StopSync {
        account_id: "test".to_string(),
    };
    let _ = UiCommand::ForceResync {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
    };
    let _ = UiCommand::Shutdown;
}

#[tokio::test]
async fn channels_send_and_receive() {
    let (event_tx, mut event_rx, cmd_tx, mut cmd_rx) = create_sync_channels(16);

    // Send an event from sync engine to UI
    event_tx
        .send(SyncEvent::Connected {
            account_id: "acct1".to_string(),
        })
        .await
        .unwrap();

    // UI receives it
    let event = event_rx.recv().await.unwrap();
    assert!(matches!(event, SyncEvent::Connected { .. }));

    // Send a command from UI to sync engine
    cmd_tx
        .send(UiCommand::StartSync {
            account_id: "acct1".to_string(),
        })
        .await
        .unwrap();

    let cmd = cmd_rx.recv().await.unwrap();
    assert!(matches!(cmd, UiCommand::StartSync { .. }));
}
