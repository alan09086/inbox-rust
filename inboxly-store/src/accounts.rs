use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the accounts table.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub provider: String,
    pub auth_method: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

impl Store {
    pub fn insert_account(&self, account: &AccountRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO accounts (id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                account.id,
                account.email,
                account.display_name,
                account.provider,
                account.auth_method,
                account.imap_host,
                account.imap_port as i64,
                account.smtp_host,
                account.smtp_port as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_account(&self, id: &str) -> Result<AccountRow> {
        self.conn()
            .query_row(
                "SELECT id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port
                 FROM accounts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(AccountRow {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        display_name: row.get(2)?,
                        provider: row.get(3)?,
                        auth_method: row.get(4)?,
                        imap_host: row.get(5)?,
                        imap_port: row.get::<_, i64>(6)? as u16,
                        smtp_host: row.get(7)?,
                        smtp_port: row.get::<_, i64>(8)? as u16,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("account {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port
             FROM accounts ORDER BY email",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AccountRow {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    display_name: row.get(2)?,
                    provider: row.get(3)?,
                    auth_method: row.get(4)?,
                    imap_host: row.get(5)?,
                    imap_port: row.get::<_, i64>(6)? as u16,
                    smtp_host: row.get(7)?,
                    smtp_port: row.get::<_, i64>(8)? as u16,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_account(&self, account: &AccountRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE accounts SET email = ?2, display_name = ?3, provider = ?4, auth_method = ?5,
             imap_host = ?6, imap_port = ?7, smtp_host = ?8, smtp_port = ?9
             WHERE id = ?1",
            params![
                account.id,
                account.email,
                account.display_name,
                account.provider,
                account.auth_method,
                account.imap_host,
                account.imap_port as i64,
                account.smtp_host,
                account.smtp_port as i64,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("account {}", account.id)));
        }
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM accounts WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("account {id}")));
        }
        Ok(())
    }
}
