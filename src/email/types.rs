//! Email data types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A mailbox (folder) in the email account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailbox {
    /// Server-assigned mailbox ID.
    pub id: String,
    /// Display name (e.g. "Inbox", "Sent", "Drafts").
    pub name: String,
    /// JMAP role if any (e.g. "inbox", "sent", "drafts", "trash", "junk").
    pub role: Option<String>,
    /// Number of total emails in this mailbox.
    pub total_emails: u32,
    /// Number of unread emails.
    pub unread_emails: u32,
    /// Parent mailbox ID (for nested folders).
    pub parent_id: Option<String>,
}

/// Summary of an email for list views (without full body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSummary {
    /// Server-assigned email ID.
    pub id: String,
    /// Thread ID for conversation grouping.
    pub thread_id: Option<String>,
    /// Subject line.
    pub subject: String,
    /// Sender address and optional name.
    pub from: Vec<EmailAddress>,
    /// Recipient addresses.
    pub to: Vec<EmailAddress>,
    /// When the email was received.
    pub received_at: Option<DateTime<Utc>>,
    /// Short preview of the body.
    pub preview: String,
    /// Whether the email has been read.
    pub is_read: bool,
    /// Whether the email has been flagged/starred.
    pub is_flagged: bool,
    /// Whether the email has attachments.
    pub has_attachments: bool,
    /// Mailbox IDs this email belongs to.
    pub mailbox_ids: Vec<String>,
}

/// Full email with body content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Email {
    /// All fields from the summary.
    #[serde(flatten)]
    pub summary: EmailSummary,
    /// Plain text body (if available).
    pub text_body: Option<String>,
    /// HTML body (if available).
    pub html_body: Option<String>,
    /// CC recipients.
    pub cc: Vec<EmailAddress>,
    /// BCC recipients.
    pub bcc: Vec<EmailAddress>,
    /// In-Reply-To message ID.
    pub in_reply_to: Option<String>,
    /// References header (for threading).
    pub references: Vec<String>,
    /// Attachments metadata.
    pub attachments: Vec<Attachment>,
}

/// An email address with optional display name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    /// Display name (e.g. "Gary Song").
    pub name: Option<String>,
    /// Email address (e.g. "gary@example.com").
    pub email: String,
}

impl std::fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{} <{}>", name, self.email),
            None => write!(f, "{}", self.email),
        }
    }
}

/// An email draft for sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDraft {
    /// Recipients.
    pub to: Vec<EmailAddress>,
    /// CC recipients.
    pub cc: Vec<EmailAddress>,
    /// BCC recipients.
    pub bcc: Vec<EmailAddress>,
    /// Subject line.
    pub subject: String,
    /// Plain text body.
    pub text_body: String,
    /// Optional HTML body.
    pub html_body: Option<String>,
    /// Optional In-Reply-To message ID (for replies).
    pub in_reply_to: Option<String>,
    /// Optional References header values.
    pub references: Vec<String>,
}

/// Attachment metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID (blob ID on the server).
    pub id: String,
    /// Filename.
    pub name: Option<String>,
    /// MIME type.
    pub content_type: String,
    /// Size in bytes.
    pub size: u64,
}
