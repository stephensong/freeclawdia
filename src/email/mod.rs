//! Email integration via JMAP.
//!
//! Provides an `EmailProvider` trait for email operations and a JMAP
//! implementation backed by the `jmap-client` crate. Designed to work
//! with Stalwart Mail Server and any other JMAP-compliant server.

#[cfg(feature = "email")]
mod jmap_provider;
mod types;

pub use types::*;

#[cfg(feature = "email")]
pub use jmap_provider::JmapEmailProvider;

use async_trait::async_trait;

use crate::error::EmailError;

/// Trait for email operations.
///
/// Abstracts over the underlying email protocol (JMAP, IMAP, etc.)
/// so that tools and channels can work with any provider.
#[async_trait]
pub trait EmailProvider: Send + Sync {
    /// List mailboxes (folders) for the account.
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, EmailError>;

    /// List emails in a mailbox, with optional pagination.
    async fn list_emails(
        &self,
        mailbox_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError>;

    /// Get a full email by ID.
    async fn get_email(&self, email_id: &str) -> Result<Email, EmailError>;

    /// Search emails by query string.
    async fn search_emails(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError>;

    /// Send an email.
    async fn send_email(&self, draft: EmailDraft) -> Result<String, EmailError>;

    /// Reply to an email.
    async fn reply_to_email(
        &self,
        in_reply_to: &str,
        body: &str,
    ) -> Result<String, EmailError>;

    /// Move an email to a different mailbox.
    async fn move_email(
        &self,
        email_id: &str,
        to_mailbox_id: &str,
    ) -> Result<(), EmailError>;

    /// Delete an email (move to trash or permanent delete).
    async fn delete_email(&self, email_id: &str) -> Result<(), EmailError>;

    /// Mark an email as read or unread.
    async fn set_read(&self, email_id: &str, read: bool) -> Result<(), EmailError>;

    /// Get the number of unread emails in a mailbox (or all mailboxes if None).
    async fn unread_count(&self, mailbox_id: Option<&str>) -> Result<u32, EmailError>;
}
