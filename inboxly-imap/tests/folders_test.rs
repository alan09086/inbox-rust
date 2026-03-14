use inboxly_imap::folders::{
    FolderRole, ImapFolder, map_well_known_folders, parse_special_use_attr,
    resolve_folder_role_by_name,
};

#[test]
fn parse_special_use_sent() {
    assert_eq!(parse_special_use_attr("\\Sent"), Some(FolderRole::Sent));
}

#[test]
fn parse_special_use_drafts() {
    assert_eq!(parse_special_use_attr("\\Drafts"), Some(FolderRole::Drafts));
}

#[test]
fn parse_special_use_trash() {
    assert_eq!(parse_special_use_attr("\\Trash"), Some(FolderRole::Trash));
}

#[test]
fn parse_special_use_junk() {
    assert_eq!(parse_special_use_attr("\\Junk"), Some(FolderRole::Spam));
}

#[test]
fn parse_special_use_all() {
    assert_eq!(parse_special_use_attr("\\All"), Some(FolderRole::All));
}

#[test]
fn parse_special_use_unknown() {
    assert_eq!(parse_special_use_attr("\\SomethingElse"), None);
}

#[test]
fn resolve_role_by_name_inbox() {
    assert_eq!(
        resolve_folder_role_by_name("INBOX"),
        Some(FolderRole::Inbox)
    );
    assert_eq!(
        resolve_folder_role_by_name("Inbox"),
        Some(FolderRole::Inbox)
    );
}

#[test]
fn resolve_role_by_name_sent_variations() {
    assert_eq!(resolve_folder_role_by_name("Sent"), Some(FolderRole::Sent));
    assert_eq!(
        resolve_folder_role_by_name("Sent Items"),
        Some(FolderRole::Sent)
    );
    assert_eq!(
        resolve_folder_role_by_name("Sent Messages"),
        Some(FolderRole::Sent)
    );
    assert_eq!(
        resolve_folder_role_by_name("[Gmail]/Sent Mail"),
        Some(FolderRole::Sent)
    );
}

#[test]
fn resolve_role_by_name_drafts() {
    assert_eq!(
        resolve_folder_role_by_name("Drafts"),
        Some(FolderRole::Drafts)
    );
    assert_eq!(
        resolve_folder_role_by_name("[Gmail]/Drafts"),
        Some(FolderRole::Drafts)
    );
}

#[test]
fn resolve_role_by_name_trash_variations() {
    assert_eq!(
        resolve_folder_role_by_name("Trash"),
        Some(FolderRole::Trash)
    );
    assert_eq!(
        resolve_folder_role_by_name("Deleted Items"),
        Some(FolderRole::Trash)
    );
    assert_eq!(
        resolve_folder_role_by_name("[Gmail]/Trash"),
        Some(FolderRole::Trash)
    );
    assert_eq!(
        resolve_folder_role_by_name("Deleted Messages"),
        Some(FolderRole::Trash)
    );
}

#[test]
fn resolve_role_by_name_spam_variations() {
    assert_eq!(resolve_folder_role_by_name("Spam"), Some(FolderRole::Spam));
    assert_eq!(resolve_folder_role_by_name("Junk"), Some(FolderRole::Spam));
    assert_eq!(
        resolve_folder_role_by_name("Junk E-mail"),
        Some(FolderRole::Spam)
    );
    assert_eq!(
        resolve_folder_role_by_name("[Gmail]/Spam"),
        Some(FolderRole::Spam)
    );
}

#[test]
fn resolve_role_by_name_unknown() {
    assert_eq!(resolve_folder_role_by_name("My Custom Folder"), None);
    assert_eq!(resolve_folder_role_by_name("Work"), None);
}

#[test]
fn map_well_known_folders_from_special_use() {
    let folders = vec![
        ImapFolder {
            name: "INBOX".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Inbox),
            attributes: vec![],
        },
        ImapFolder {
            name: "[Gmail]/Sent Mail".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Sent),
            attributes: vec!["\\Sent".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Drafts".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Drafts),
            attributes: vec!["\\Drafts".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Trash".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Trash),
            attributes: vec!["\\Trash".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Spam".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Spam),
            attributes: vec!["\\Junk".to_string()],
        },
    ];

    let wk = map_well_known_folders(&folders);
    assert_eq!(wk.inbox.as_deref(), Some("INBOX"));
    assert_eq!(wk.sent.as_deref(), Some("[Gmail]/Sent Mail"));
    assert_eq!(wk.drafts.as_deref(), Some("[Gmail]/Drafts"));
    assert_eq!(wk.trash.as_deref(), Some("[Gmail]/Trash"));
    assert_eq!(wk.spam.as_deref(), Some("[Gmail]/Spam"));
}

#[test]
fn map_well_known_folders_fallback_by_name() {
    // No SPECIAL-USE attributes — should fall back to name matching
    let folders = vec![
        ImapFolder {
            name: "INBOX".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Sent".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Drafts".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Trash".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Junk".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
    ];

    let wk = map_well_known_folders(&folders);
    assert_eq!(wk.inbox.as_deref(), Some("INBOX"));
    assert_eq!(wk.sent.as_deref(), Some("Sent"));
    assert_eq!(wk.drafts.as_deref(), Some("Drafts"));
    assert_eq!(wk.trash.as_deref(), Some("Trash"));
    assert_eq!(wk.spam.as_deref(), Some("Junk"));
}
