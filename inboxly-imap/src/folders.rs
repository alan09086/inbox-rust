use futures::TryStreamExt;
use tracing::{debug, info, warn};

use crate::error::Result;

/// The role a folder plays in the email workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Spam,
    All,
    Archive,
    Flagged,
}

/// A discovered IMAP folder with its parsed attributes.
#[derive(Debug, Clone)]
pub struct ImapFolder {
    /// Full IMAP folder name (e.g., `[Gmail]/Sent Mail`).
    pub name: String,
    /// Hierarchy delimiter (e.g., '/' or '.').
    pub delimiter: Option<char>,
    /// Resolved role from SPECIAL-USE attribute or name heuristic.
    pub role: Option<FolderRole>,
    /// Raw IMAP attributes (e.g., "\\Sent", "\\HasNoChildren").
    pub attributes: Vec<String>,
}

/// The five well-known folders Inboxly syncs in v1, plus the archive folder.
#[derive(Debug, Clone, Default)]
pub struct WellKnownFolders {
    /// IMAP name for Inbox (always "INBOX" per RFC 3501).
    pub inbox: Option<String>,
    /// IMAP name for Sent folder.
    pub sent: Option<String>,
    /// IMAP name for Drafts folder.
    pub drafts: Option<String>,
    /// IMAP name for Trash folder.
    pub trash: Option<String>,
    /// IMAP name for Spam/Junk folder.
    pub spam: Option<String>,
    /// IMAP name for Archive folder (provider-specific: `[Gmail]/All Mail` or `Archive`).
    pub archive: Option<String>,
}

impl WellKnownFolders {
    /// Returns all resolved folder names for iteration.
    pub fn all_names(&self) -> Vec<&str> {
        [
            &self.inbox,
            &self.sent,
            &self.drafts,
            &self.trash,
            &self.spam,
        ]
        .iter()
        .filter_map(|opt| opt.as_deref())
        .collect()
    }

    /// Returns true if all five well-known folders have been resolved.
    pub fn is_complete(&self) -> bool {
        self.inbox.is_some()
            && self.sent.is_some()
            && self.drafts.is_some()
            && self.trash.is_some()
            && self.spam.is_some()
    }
}

/// Parse a SPECIAL-USE attribute string (RFC 6154) into a `FolderRole`.
///
/// Attributes are case-insensitive and prefixed with `\`.
pub fn parse_special_use_attr(attr: &str) -> Option<FolderRole> {
    match attr.to_lowercase().as_str() {
        "\\inbox" => Some(FolderRole::Inbox),
        "\\sent" => Some(FolderRole::Sent),
        "\\drafts" => Some(FolderRole::Drafts),
        "\\trash" => Some(FolderRole::Trash),
        "\\junk" => Some(FolderRole::Spam),
        "\\all" => Some(FolderRole::All),
        "\\archive" => Some(FolderRole::Archive),
        "\\flagged" => Some(FolderRole::Flagged),
        _ => None,
    }
}

/// Convert an `async-imap` `NameAttribute` to its canonical IMAP string
/// representation, for use with `parse_special_use_attr`.
fn name_attribute_to_string(attr: &async_imap::imap_proto::types::NameAttribute<'_>) -> String {
    use async_imap::imap_proto::types::NameAttribute;
    match attr {
        NameAttribute::NoInferiors => "\\NoInferiors".to_string(),
        NameAttribute::NoSelect => "\\NoSelect".to_string(),
        NameAttribute::Marked => "\\Marked".to_string(),
        NameAttribute::Unmarked => "\\Unmarked".to_string(),
        NameAttribute::All => "\\All".to_string(),
        NameAttribute::Archive => "\\Archive".to_string(),
        NameAttribute::Drafts => "\\Drafts".to_string(),
        NameAttribute::Flagged => "\\Flagged".to_string(),
        NameAttribute::Junk => "\\Junk".to_string(),
        NameAttribute::Sent => "\\Sent".to_string(),
        NameAttribute::Trash => "\\Trash".to_string(),
        NameAttribute::Extension(s) => s.to_string(),
        // Non-exhaustive: handle any future variants as unknown attributes
        _ => "\\Unknown".to_string(),
    }
}

/// Resolve a folder's role by its name using common naming conventions.
///
/// This is the fallback when SPECIAL-USE attributes are not available.
/// Handles Gmail paths (`[Gmail]/Sent Mail`), standard names (`Sent`),
/// and common variations (`Sent Items`, `Deleted Items`, etc.).
pub fn resolve_folder_role_by_name(name: &str) -> Option<FolderRole> {
    let lower = name.to_lowercase();

    // INBOX is case-insensitive per RFC 3501
    if lower == "inbox" {
        return Some(FolderRole::Inbox);
    }

    // Sent folder variants
    if lower == "sent"
        || lower == "sent items"
        || lower == "sent messages"
        || lower == "[gmail]/sent mail"
    {
        return Some(FolderRole::Sent);
    }

    // Drafts folder variants
    if lower == "drafts" || lower == "[gmail]/drafts" {
        return Some(FolderRole::Drafts);
    }

    // Trash folder variants
    if lower == "trash"
        || lower == "deleted items"
        || lower == "deleted messages"
        || lower == "[gmail]/trash"
        || lower == "[gmail]/bin"
    {
        return Some(FolderRole::Trash);
    }

    // Spam/Junk folder variants
    if lower == "spam"
        || lower == "junk"
        || lower == "junk e-mail"
        || lower == "junk email"
        || lower == "[gmail]/spam"
    {
        return Some(FolderRole::Spam);
    }

    // Archive folder variants (includes Gmail's "All Mail")
    if lower == "archive" || lower == "all mail" || lower == "[gmail]/all mail" {
        return Some(FolderRole::Archive);
    }

    None
}

/// Map a list of IMAP folders to well-known folder roles.
///
/// Strategy:
/// 1. First pass: use SPECIAL-USE attributes (from `role` field).
/// 2. Second pass: for any unresolved roles, fall back to name heuristics.
pub fn map_well_known_folders(folders: &[ImapFolder]) -> WellKnownFolders {
    let mut wk = WellKnownFolders::default();

    // Pass 1: SPECIAL-USE attributes (highest priority)
    for folder in folders {
        if let Some(role) = &folder.role {
            match role {
                FolderRole::Inbox => {
                    wk.inbox.get_or_insert(folder.name.clone());
                }
                FolderRole::Sent => {
                    wk.sent.get_or_insert(folder.name.clone());
                }
                FolderRole::Drafts => {
                    wk.drafts.get_or_insert(folder.name.clone());
                }
                FolderRole::Trash => {
                    wk.trash.get_or_insert(folder.name.clone());
                }
                FolderRole::Spam => {
                    wk.spam.get_or_insert(folder.name.clone());
                }
                // \Archive takes precedence over \All (which on Gmail includes everything)
                FolderRole::Archive => {
                    wk.archive = Some(folder.name.clone());
                }
                FolderRole::All => {
                    wk.archive.get_or_insert(folder.name.clone());
                }
                _ => {}
            };
        }
    }

    // INBOX is always "INBOX" per RFC 3501 — force it if not set by SPECIAL-USE
    if wk.inbox.is_none() {
        for folder in folders {
            if folder.name.eq_ignore_ascii_case("INBOX") {
                wk.inbox = Some(folder.name.clone());
                break;
            }
        }
    }

    // Pass 2: Name heuristic fallback for unresolved roles
    for folder in folders {
        if let Some(role) = resolve_folder_role_by_name(&folder.name) {
            match role {
                FolderRole::Inbox if wk.inbox.is_none() => {
                    wk.inbox = Some(folder.name.clone());
                }
                FolderRole::Sent if wk.sent.is_none() => {
                    wk.sent = Some(folder.name.clone());
                }
                FolderRole::Drafts if wk.drafts.is_none() => {
                    wk.drafts = Some(folder.name.clone());
                }
                FolderRole::Trash if wk.trash.is_none() => {
                    wk.trash = Some(folder.name.clone());
                }
                FolderRole::Spam if wk.spam.is_none() => {
                    wk.spam = Some(folder.name.clone());
                }
                FolderRole::Archive if wk.archive.is_none() => {
                    wk.archive = Some(folder.name.clone());
                }
                _ => {}
            }
        }
    }

    if !wk.is_complete() {
        warn!(
            inbox = wk.inbox.is_some(),
            sent = wk.sent.is_some(),
            drafts = wk.drafts.is_some(),
            trash = wk.trash.is_some(),
            spam = wk.spam.is_some(),
            "Not all well-known folders resolved"
        );
    }

    wk
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal `ImapFolder` with a given name and optional SPECIAL-USE attribute.
    fn make_folder(name: &str, special_use_attr: Option<&str>) -> ImapFolder {
        let role = special_use_attr
            .and_then(parse_special_use_attr)
            .or_else(|| resolve_folder_role_by_name(name));
        ImapFolder {
            name: name.to_string(),
            delimiter: Some('/'),
            role,
            attributes: special_use_attr
                .map(|a| vec![a.to_string()])
                .unwrap_or_default(),
        }
    }

    #[test]
    fn resolve_gmail_archive_folder() {
        // Gmail exposes "[Gmail]/All Mail" with the \All SPECIAL-USE attribute.
        let folders = vec![
            make_folder("INBOX", Some("\\Inbox")),
            make_folder("[Gmail]/Sent Mail", Some("\\Sent")),
            make_folder("[Gmail]/Drafts", Some("\\Drafts")),
            make_folder("[Gmail]/Trash", Some("\\Trash")),
            make_folder("[Gmail]/Spam", Some("\\Junk")),
            make_folder("[Gmail]/All Mail", Some("\\All")),
        ];
        let wkf = map_well_known_folders(&folders);
        assert_eq!(wkf.archive, Some("[Gmail]/All Mail".to_string()));
    }

    #[test]
    fn resolve_outlook_archive_folder() {
        // Outlook exposes "Archive" with the \Archive SPECIAL-USE attribute.
        let folders = vec![
            make_folder("INBOX", Some("\\Inbox")),
            make_folder("Sent Items", Some("\\Sent")),
            make_folder("Drafts", Some("\\Drafts")),
            make_folder("Deleted Items", Some("\\Trash")),
            make_folder("Junk Email", Some("\\Junk")),
            make_folder("Archive", Some("\\Archive")),
        ];
        let wkf = map_well_known_folders(&folders);
        assert_eq!(wkf.archive, Some("Archive".to_string()));
    }

    #[test]
    fn resolve_archive_by_name_heuristic() {
        // No SPECIAL-USE attributes — name heuristic only.
        let folders = vec![
            make_folder("INBOX", None),
            make_folder("Sent", None),
            make_folder("Drafts", None),
            make_folder("Trash", None),
            make_folder("Spam", None),
            make_folder("Archive", None),
        ];
        let wkf = map_well_known_folders(&folders);
        assert_eq!(wkf.archive, Some("Archive".to_string()));
    }
}

/// List all folders from an authenticated IMAP session and resolve roles.
///
/// Issues `LIST "" "*"` and parses SPECIAL-USE attributes where available.
pub async fn list_folders(
    session: &mut async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
) -> Result<Vec<ImapFolder>> {
    info!("Listing IMAP folders");

    let stream = session.list(Some(""), Some("*")).await?;
    let names: Vec<async_imap::types::Name> = stream.try_collect().await?;

    let folders: Vec<ImapFolder> = names
        .iter()
        .map(|name| {
            let attrs: Vec<String> = name
                .attributes()
                .iter()
                .map(name_attribute_to_string)
                .collect();

            let delimiter = name.delimiter().and_then(|s: &str| s.chars().next());

            // Try to resolve role from SPECIAL-USE attributes first, then name
            let role = attrs
                .iter()
                .find_map(|a| parse_special_use_attr(a))
                .or_else(|| resolve_folder_role_by_name(name.name()));

            debug!(
                folder = name.name(),
                delimiter = ?delimiter,
                attrs = ?attrs,
                role = ?role,
                "Discovered folder"
            );

            ImapFolder {
                name: name.name().to_string(),
                delimiter,
                role,
                attributes: attrs,
            }
        })
        .collect();

    info!(count = folders.len(), "Folder listing complete");
    Ok(folders)
}
