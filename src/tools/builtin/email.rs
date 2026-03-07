//! Email tools for the agent.
//!
//! These tools allow the agent to list, read, search, send, reply to,
//! move, and delete emails via the `EmailProvider` trait.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::context::JobContext;
use crate::email::{EmailDraft, EmailProvider, EmailAddress};
use crate::tools::tool::{Tool, ToolError, ToolOutput, require_str};

/// Tool for listing emails in a mailbox.
pub struct EmailListTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailListTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailListTool {
    fn name(&self) -> &str {
        "email_list"
    }

    fn description(&self) -> &str {
        "List emails in a mailbox. Use email_mailboxes first to find mailbox IDs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mailbox_id": {
                    "type": "string",
                    "description": "The mailbox ID to list emails from"
                },
                "offset": {
                    "type": "integer",
                    "description": "Offset for pagination (default: 0)",
                    "default": 0
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum emails to return (default: 20, max: 50)",
                    "default": 20,
                    "maximum": 50
                }
            },
            "required": ["mailbox_id"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let mailbox_id = require_str(&params, "mailbox_id")?;
        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(50) as u32;

        let emails = self
            .provider
            .list_emails(mailbox_id, offset, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::success(
            serde_json::to_value(&emails)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            start.elapsed(),
        ))
    }

    fn requires_sanitization(&self) -> bool {
        true
    }
}

/// Tool for listing mailboxes (folders).
pub struct EmailMailboxesTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailMailboxesTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailMailboxesTool {
    fn name(&self) -> &str {
        "email_mailboxes"
    }

    fn description(&self) -> &str {
        "List all email mailboxes (folders) with their IDs, names, and unread counts."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();

        let mailboxes = self
            .provider
            .list_mailboxes()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::success(
            serde_json::to_value(&mailboxes)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            start.elapsed(),
        ))
    }

    fn requires_sanitization(&self) -> bool {
        true
    }
}

/// Tool for reading a specific email.
pub struct EmailReadTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailReadTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailReadTool {
    fn name(&self) -> &str {
        "email_read"
    }

    fn description(&self) -> &str {
        "Read a specific email by ID, including full body and attachments metadata."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "email_id": {
                    "type": "string",
                    "description": "The email ID to read"
                },
                "mark_read": {
                    "type": "boolean",
                    "description": "Mark the email as read (default: true)",
                    "default": true
                }
            },
            "required": ["email_id"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let email_id = require_str(&params, "email_id")?;
        let mark_read = params
            .get("mark_read")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let email = self
            .provider
            .get_email(email_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        if mark_read && !email.summary.is_read {
            let _ = self.provider.set_read(email_id, true).await;
        }

        Ok(ToolOutput::success(
            serde_json::to_value(&email)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            start.elapsed(),
        ))
    }

    fn requires_sanitization(&self) -> bool {
        true
    }
}

/// Tool for searching emails.
pub struct EmailSearchTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailSearchTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailSearchTool {
    fn name(&self) -> &str {
        "email_search"
    }

    fn description(&self) -> &str {
        "Search emails by text query across all mailboxes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (searches subject, body, sender, recipients)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results (default: 20, max: 50)",
                    "default": 20,
                    "maximum": 50
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let query = require_str(&params, "query")?;
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(50) as u32;

        let emails = self
            .provider
            .search_emails(query, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::success(
            serde_json::to_value(&emails)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            start.elapsed(),
        ))
    }

    fn requires_sanitization(&self) -> bool {
        true
    }
}

/// Tool for sending an email.
pub struct EmailSendTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailSendTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailSendTool {
    fn name(&self) -> &str {
        "email_send"
    }

    fn description(&self) -> &str {
        "Send a new email. Requires recipient, subject, and body."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient email address"
                },
                "subject": {
                    "type": "string",
                    "description": "Email subject"
                },
                "body": {
                    "type": "string",
                    "description": "Email body (plain text)"
                },
                "cc": {
                    "type": "string",
                    "description": "CC recipient email address (optional)"
                }
            },
            "required": ["to", "subject", "body"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let to = require_str(&params, "to")?;
        let subject = require_str(&params, "subject")?;
        let body = require_str(&params, "body")?;
        let cc = params.get("cc").and_then(|v| v.as_str());

        let draft = EmailDraft {
            to: vec![EmailAddress {
                name: None,
                email: to.to_string(),
            }],
            cc: cc
                .map(|addr| {
                    vec![EmailAddress {
                        name: None,
                        email: addr.to_string(),
                    }]
                })
                .unwrap_or_default(),
            bcc: Vec::new(),
            subject: subject.to_string(),
            text_body: body.to_string(),
            html_body: None,
            in_reply_to: None,
            references: Vec::new(),
        };

        let id = self
            .provider
            .send_email(draft)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::text(
            format!("Email sent successfully (id: {id})"),
            start.elapsed(),
        ))
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> crate::tools::tool::ApprovalRequirement {
        crate::tools::tool::ApprovalRequirement::UnlessAutoApproved
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

/// Tool for replying to an email.
pub struct EmailReplyTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailReplyTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailReplyTool {
    fn name(&self) -> &str {
        "email_reply"
    }

    fn description(&self) -> &str {
        "Reply to an existing email. Automatically sets subject, recipients, and threading headers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "email_id": {
                    "type": "string",
                    "description": "ID of the email to reply to"
                },
                "body": {
                    "type": "string",
                    "description": "Reply body (plain text)"
                }
            },
            "required": ["email_id", "body"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let email_id = require_str(&params, "email_id")?;
        let body = require_str(&params, "body")?;

        let id = self
            .provider
            .reply_to_email(email_id, body)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::text(
            format!("Reply sent successfully (id: {id})"),
            start.elapsed(),
        ))
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> crate::tools::tool::ApprovalRequirement {
        crate::tools::tool::ApprovalRequirement::UnlessAutoApproved
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

/// Tool for deleting an email.
pub struct EmailDeleteTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailDeleteTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailDeleteTool {
    fn name(&self) -> &str {
        "email_delete"
    }

    fn description(&self) -> &str {
        "Delete an email (moves to trash if available, otherwise permanently deletes)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "email_id": {
                    "type": "string",
                    "description": "ID of the email to delete"
                }
            },
            "required": ["email_id"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let email_id = require_str(&params, "email_id")?;

        self.provider
            .delete_email(email_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::text("Email deleted", start.elapsed()))
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> crate::tools::tool::ApprovalRequirement {
        crate::tools::tool::ApprovalRequirement::UnlessAutoApproved
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

/// Tool for moving an email to a different mailbox.
pub struct EmailMoveTool {
    provider: Arc<dyn EmailProvider>,
}

impl EmailMoveTool {
    pub fn new(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for EmailMoveTool {
    fn name(&self) -> &str {
        "email_move"
    }

    fn description(&self) -> &str {
        "Move an email to a different mailbox/folder."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "email_id": {
                    "type": "string",
                    "description": "ID of the email to move"
                },
                "to_mailbox_id": {
                    "type": "string",
                    "description": "ID of the destination mailbox"
                }
            },
            "required": ["email_id", "to_mailbox_id"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let email_id = require_str(&params, "email_id")?;
        let to_mailbox_id = require_str(&params, "to_mailbox_id")?;

        self.provider
            .move_email(email_id, to_mailbox_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolOutput::text("Email moved", start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}
