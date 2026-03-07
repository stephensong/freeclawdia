//! JMAP-based email provider using the `jmap-client` crate.
//!
//! Works with Stalwart Mail Server and any JMAP-compliant server.

use std::sync::Arc;

use async_trait::async_trait;
use jmap_client::client::Client;
use jmap_client::client::Credentials;
use jmap_client::core::set::SetObject;
use jmap_client::email;
use jmap_client::mailbox;
use tokio::sync::RwLock;

use crate::config::EmailConfig;
use crate::email::types::*;
use crate::email::EmailProvider;
use crate::error::EmailError;

/// JMAP email provider backed by `jmap-client`.
pub struct JmapEmailProvider {
    client: Arc<RwLock<Option<Arc<Client>>>>,
    config: EmailConfig,
}

impl std::fmt::Debug for JmapEmailProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JmapEmailProvider")
            .field("url", &self.config.jmap_url)
            .finish()
    }
}

impl JmapEmailProvider {
    /// Create a new JMAP email provider (connects lazily on first use).
    pub fn new(config: EmailConfig) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Get or create the JMAP client connection.
    async fn client(&self) -> Result<Arc<Client>, EmailError> {
        {
            let guard = self.client.read().await;
            if let Some(ref client) = *guard {
                return Ok(Arc::clone(client));
            }
        }

        let url = self.config.jmap_url.as_deref().ok_or_else(|| {
            EmailError::Config {
                reason: "EMAIL_JMAP_URL not configured".to_string(),
            }
        })?;

        let username = self.config.username.as_deref().ok_or_else(|| {
            EmailError::Config {
                reason: "EMAIL_USERNAME not configured".to_string(),
            }
        })?;

        let password = self.config.password.as_deref().ok_or_else(|| {
            EmailError::Config {
                reason: "EMAIL_PASSWORD not configured".to_string(),
            }
        })?;

        // Resolve the JMAP well-known URL first to discover redirect hostnames.
        // Stalwart redirects .well-known/jmap to /jmap/session using the server's
        // reverse DNS hostname, which jmap-client blocks unless trusted.
        let well_known = format!("{}/.well-known/jmap", url.trim_end_matches('/'));
        let mut trusted: Vec<String> = Vec::new();
        if let Ok(parsed) = url::Url::parse(url)
            && let Some(host) = parsed.host_str()
        {
            trusted.push(host.to_string());
        }
        // Follow the redirect chain to discover additional hostnames
        if let Ok(resp) = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
            .get(&well_known)
            .send()
            .await
            && let Some(location) = resp.headers().get("location")
            && let Ok(loc) = location.to_str()
        {
            if let Ok(redirect_url) = url::Url::parse(loc)
                && let Some(host) = redirect_url.host_str()
            {
                trusted.push(host.to_string());
            } else if let Ok(redirect_url) =
                url::Url::parse(&format!("{}{}", url.trim_end_matches('/'), loc))
                && let Some(host) = redirect_url.host_str()
            {
                trusted.push(host.to_string());
            }
        }

        let client = Client::new()
            .credentials(Credentials::basic(username, password))
            .follow_redirects(trusted)
            .connect(url)
            .await
            .map_err(|e| EmailError::Connection {
                reason: format!("JMAP connection failed: {e}"),
            })?;

        let client = Arc::new(client);
        let mut guard = self.client.write().await;
        *guard = Some(Arc::clone(&client));
        Ok(client)
    }
}

fn op_err(msg: impl Into<String>) -> EmailError {
    EmailError::Operation {
        reason: msg.into(),
    }
}

/// Convert a mailbox Role enum to an optional string.
fn role_to_string(role: &mailbox::Role) -> Option<String> {
    match role {
        mailbox::Role::Inbox => Some("inbox".to_string()),
        mailbox::Role::Sent => Some("sent".to_string()),
        mailbox::Role::Trash => Some("trash".to_string()),
        mailbox::Role::Drafts => Some("drafts".to_string()),
        mailbox::Role::Junk => Some("junk".to_string()),
        mailbox::Role::Archive => Some("archive".to_string()),
        mailbox::Role::None => None,
        other => Some(format!("{other:?}").to_lowercase()),
    }
}

#[async_trait]
impl EmailProvider for JmapEmailProvider {
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        request.get_mailbox().properties([
            mailbox::Property::Id,
            mailbox::Property::Name,
            mailbox::Property::Role,
            mailbox::Property::TotalEmails,
            mailbox::Property::UnreadEmails,
            mailbox::Property::ParentId,
        ]);

        let response = request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP mailbox list failed: {e}")))?;

        let mailbox_list = response
            .unwrap_method_responses()
            .pop()
            .ok_or_else(|| op_err("Empty JMAP response"))?;

        let mailboxes = mailbox_list
            .unwrap_get_mailbox()
            .map_err(|e| op_err(format!("JMAP mailbox parse failed: {e}")))?
            .take_list()
            .into_iter()
            .map(|mb| Mailbox {
                id: mb.id().unwrap_or("").to_string(),
                name: mb.name().unwrap_or("(unnamed)").to_string(),
                role: role_to_string(&mb.role()),
                total_emails: mb.total_emails() as u32,
                unread_emails: mb.unread_emails() as u32,
                parent_id: mb.parent_id().map(|id| id.to_string()),
            })
            .collect();

        Ok(mailboxes)
    }

    async fn list_emails(
        &self,
        mailbox_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        let query_ref = request
            .query_email()
            .filter(email::query::Filter::in_mailbox(mailbox_id))
            .sort([email::query::Comparator::received_at().is_ascending(false)])
            .position(offset as i32)
            .limit(limit as usize)
            .result_reference();

        request
            .get_email()
            .ids_ref(query_ref)
            .properties(summary_properties());

        let response = request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP email list failed: {e}")))?;

        let method_responses = response.unwrap_method_responses();
        let get_response = method_responses
            .into_iter()
            .nth(1)
            .ok_or_else(|| op_err("Missing email get response"))?;

        let emails = get_response
            .unwrap_get_email()
            .map_err(|e| op_err(format!("JMAP email parse failed: {e}")))?
            .take_list();

        Ok(emails.into_iter().map(jmap_email_to_summary).collect())
    }

    async fn get_email(&self, email_id: &str) -> Result<Email, EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        request
            .get_email()
            .ids([email_id])
            .properties([
                email::Property::Id,
                email::Property::ThreadId,
                email::Property::Subject,
                email::Property::From,
                email::Property::To,
                email::Property::Cc,
                email::Property::Bcc,
                email::Property::ReceivedAt,
                email::Property::Preview,
                email::Property::Keywords,
                email::Property::HasAttachment,
                email::Property::MailboxIds,
                email::Property::TextBody,
                email::Property::HtmlBody,
                email::Property::InReplyTo,
                email::Property::References,
                email::Property::Attachments,
                email::Property::BodyValues,
            ])
            .arguments()
            .fetch_all_body_values(true);

        let response = request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP email get failed: {e}")))?;

        let email = response
            .unwrap_method_responses()
            .pop()
            .ok_or_else(|| op_err("Empty JMAP response"))?
            .unwrap_get_email()
            .map_err(|e| op_err(format!("JMAP email parse failed: {e}")))?
            .take_list()
            .into_iter()
            .next()
            .ok_or_else(|| EmailError::NotFound {
                id: email_id.to_string(),
            })?;

        Ok(jmap_email_to_full(email))
    }

    async fn search_emails(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        let query_ref = request
            .query_email()
            .filter(email::query::Filter::text(query))
            .sort([email::query::Comparator::received_at().is_ascending(false)])
            .limit(limit as usize)
            .result_reference();

        request
            .get_email()
            .ids_ref(query_ref)
            .properties(summary_properties());

        let response = request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP search failed: {e}")))?;

        let method_responses = response.unwrap_method_responses();
        let get_response = method_responses
            .into_iter()
            .nth(1)
            .ok_or_else(|| op_err("Missing email get response"))?;

        let emails = get_response
            .unwrap_get_email()
            .map_err(|e| op_err(format!("JMAP email parse failed: {e}")))?
            .take_list();

        Ok(emails.into_iter().map(jmap_email_to_summary).collect())
    }

    async fn send_email(&self, draft: EmailDraft) -> Result<String, EmailError> {
        let client = self.client().await?;

        // First, find the Drafts mailbox to store the email
        let mailboxes = self.list_mailboxes().await?;
        let drafts_id = mailboxes
            .iter()
            .find(|mb| mb.role.as_deref() == Some("drafts"))
            .map(|mb| mb.id.clone());

        // Get the sender's email address from the JMAP session
        let session = client.session();
        let from_email = session
            .primary_accounts()
            .next()
            .and_then(|(_, account_id)| {
                session.account(account_id).and_then(|a| {
                    // Use the account name which is typically the email
                    let name = a.name();
                    if name.contains('@') {
                        Some(name.to_string())
                    } else {
                        None
                    }
                })
            })
            .or_else(|| {
                // Fall back to the username from config
                self.config.username.clone()
            });

        // Fetch the user's identity ID (required for submission)
        let identity_id = {
            let mut id_request = client.build();
            id_request.get_identity();
            let id_response = id_request
                .send()
                .await
                .map_err(|e| op_err(format!("JMAP identity fetch failed: {e}")))?;
            id_response
                .unwrap_method_responses()
                .pop()
                .and_then(|r| {
                    r.unwrap_get_identity()
                        .ok()
                        .and_then(|mut get| {
                            get.take_list()
                                .into_iter()
                                .next()
                                .and_then(|identity| identity.id)
                        })
                })
                .ok_or_else(|| op_err("No JMAP identity configured. Create one in Stalwart admin."))?
        };

        // Need a new request since we consumed the previous one for identity lookup
        let mut request = client.build();
        let email_create = request.set_email().create();
        email_create.subject(&draft.subject);

        if let Some(ref from) = from_email {
            email_create.from([jmap_client::email::EmailAddress::from(from.as_str())]);
        }

        let to_addrs: Vec<jmap_client::email::EmailAddress> = draft
            .to
            .iter()
            .map(|a| match &a.name {
                Some(name) => jmap_client::email::EmailAddress::from((
                    a.email.as_str(),
                    name.as_str(),
                )),
                None => jmap_client::email::EmailAddress::from(a.email.as_str()),
            })
            .collect();
        email_create.to(to_addrs);

        if !draft.cc.is_empty() {
            let cc_addrs: Vec<jmap_client::email::EmailAddress> = draft
                .cc
                .iter()
                .map(|a| match &a.name {
                    Some(name) => jmap_client::email::EmailAddress::from((
                        a.email.as_str(),
                        name.as_str(),
                    )),
                    None => jmap_client::email::EmailAddress::from(a.email.as_str()),
                })
                .collect();
            email_create.cc(cc_addrs);
        }

        email_create.body_value("body1".to_string(), draft.text_body.as_str());
        email_create.text_body(jmap_client::email::EmailBodyPart::new().part_id("body1"));

        if let Some(ref drafts) = drafts_id {
            email_create.mailbox_ids([drafts.as_str()]);
        }

        if let Some(ref irt) = draft.in_reply_to {
            email_create.in_reply_to([irt.as_str()]);
        }
        if !draft.references.is_empty() {
            let refs: Vec<&str> = draft.references.iter().map(|s| s.as_str()).collect();
            email_create.references(refs);
        }

        let create_id = email_create
            .create_id()
            .ok_or_else(|| op_err("Failed to get create ID"))?;

        // Submit the email for delivery via EmailSubmission/set.
        // Reference the just-created email by its create ID.
        let submission_set = request.set_email_submission();
        submission_set
            .create()
            .email_id(format!("#{create_id}"))
            .identity_id(&identity_id);

        // On successful submission, move email from Drafts to Sent
        let sent_id = mailboxes
            .iter()
            .find(|mb| mb.role.as_deref() == Some("sent"))
            .map(|mb| mb.id.clone());
        if let (Some(sent), Some(drafts)) = (&sent_id, &drafts_id) {
            submission_set
                .arguments()
                .on_success_update_email(&create_id)
                .mailbox_id(drafts, false)
                .mailbox_id(sent, true);
        }

        request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP send failed: {e}")))?;

        Ok(create_id)
    }

    async fn reply_to_email(
        &self,
        in_reply_to_id: &str,
        body: &str,
    ) -> Result<String, EmailError> {
        let original = self.get_email(in_reply_to_id).await?;

        let to = original.summary.from.clone();
        let subject = if original.summary.subject.starts_with("Re: ") {
            original.summary.subject.clone()
        } else {
            format!("Re: {}", original.summary.subject)
        };

        let mut references = original.references.clone();
        if let Some(ref msg_id) = original.in_reply_to
            && !references.contains(msg_id)
        {
            references.push(msg_id.clone());
        }

        let draft = EmailDraft {
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            subject,
            text_body: body.to_string(),
            html_body: None,
            in_reply_to: original.in_reply_to.clone(),
            references,
        };

        self.send_email(draft).await
    }

    async fn move_email(
        &self,
        email_id: &str,
        to_mailbox_id: &str,
    ) -> Result<(), EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        request
            .set_email()
            .update(email_id)
            .mailbox_ids([to_mailbox_id]);

        request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP move failed: {e}")))?;

        Ok(())
    }

    async fn delete_email(&self, email_id: &str) -> Result<(), EmailError> {
        let client = self.client().await?;

        let mailboxes = self.list_mailboxes().await?;
        let trash = mailboxes
            .iter()
            .find(|mb| mb.role.as_deref() == Some("trash"));

        if let Some(trash) = trash {
            self.move_email(email_id, &trash.id).await
        } else {
            let mut request = client.build();
            request.set_email().destroy([email_id]);
            request
                .send()
                .await
                .map_err(|e| op_err(format!("JMAP delete failed: {e}")))?;
            Ok(())
        }
    }

    async fn set_read(&self, email_id: &str, read: bool) -> Result<(), EmailError> {
        let client = self.client().await?;

        let mut request = client.build();
        request
            .set_email()
            .update(email_id)
            .keyword("$seen", read);

        request
            .send()
            .await
            .map_err(|e| op_err(format!("JMAP set_read failed: {e}")))?;

        Ok(())
    }

    async fn unread_count(&self, mailbox_id: Option<&str>) -> Result<u32, EmailError> {
        let mailboxes = self.list_mailboxes().await?;

        let count = if let Some(id) = mailbox_id {
            mailboxes
                .iter()
                .find(|mb| mb.id == id)
                .map(|mb| mb.unread_emails)
                .unwrap_or(0)
        } else {
            mailboxes
                .iter()
                .filter(|mb| mb.role.as_deref() == Some("inbox"))
                .map(|mb| mb.unread_emails)
                .sum()
        };

        Ok(count)
    }
}

/// Properties to fetch for email summaries (shared by list and search).
fn summary_properties() -> [email::Property; 10] {
    [
        email::Property::Id,
        email::Property::ThreadId,
        email::Property::Subject,
        email::Property::From,
        email::Property::To,
        email::Property::ReceivedAt,
        email::Property::Preview,
        email::Property::Keywords,
        email::Property::HasAttachment,
        email::Property::MailboxIds,
    ]
}

/// Convert a jmap-client Email object to our EmailSummary type.
fn jmap_email_to_summary(email: jmap_client::email::Email) -> EmailSummary {
    let keywords = email.keywords();

    EmailSummary {
        id: email.id().unwrap_or("").to_string(),
        thread_id: email.thread_id().map(|id| id.to_string()),
        subject: email.subject().unwrap_or("(no subject)").to_string(),
        from: email
            .from()
            .map(|addrs| addrs.iter().map(jmap_addr_to_addr).collect())
            .unwrap_or_default(),
        to: email
            .to()
            .map(|addrs| addrs.iter().map(jmap_addr_to_addr).collect())
            .unwrap_or_default(),
        received_at: email.received_at().and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
        }),
        preview: email.preview().unwrap_or("").to_string(),
        is_read: keywords.contains(&"$seen"),
        is_flagged: keywords.contains(&"$flagged"),
        has_attachments: email.has_attachment(),
        mailbox_ids: email
            .mailbox_ids()
            .iter()
            .map(|id| id.to_string())
            .collect(),
    }
}

/// Convert a jmap-client Email object to our full Email type.
fn jmap_email_to_full(email: jmap_client::email::Email) -> Email {
    let summary = jmap_email_to_summary(email.clone());

    // Extract body values - get the first text/html body part ID and look up its value
    let text_body = email
        .text_body()
        .and_then(|parts| parts.first())
        .and_then(|part| part.part_id())
        .and_then(|id| email.body_value(id))
        .map(|bv| bv.value().to_string());

    let html_body = email
        .html_body()
        .and_then(|parts| parts.first())
        .and_then(|part| part.part_id())
        .and_then(|id| email.body_value(id))
        .map(|bv| bv.value().to_string());

    let cc = email
        .cc()
        .map(|addrs| addrs.iter().map(jmap_addr_to_addr).collect())
        .unwrap_or_default();

    let bcc = email
        .bcc()
        .map(|addrs| addrs.iter().map(jmap_addr_to_addr).collect())
        .unwrap_or_default();

    let in_reply_to = email
        .in_reply_to()
        .and_then(|ids| ids.first().map(|s| s.to_string()));

    let references = email
        .references()
        .map(|refs| refs.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let attachments = email
        .attachments()
        .map(|parts| {
            parts
                .iter()
                .map(|part| Attachment {
                    id: part.blob_id().unwrap_or("").to_string(),
                    name: part.name().map(|s| s.to_string()),
                    content_type: part
                        .content_type()
                        .unwrap_or("application/octet-stream")
                        .to_string(),
                    size: part.size() as u64,
                })
                .collect()
        })
        .unwrap_or_default();

    Email {
        summary,
        text_body,
        html_body,
        cc,
        bcc,
        in_reply_to,
        references,
        attachments,
    }
}

/// Convert a jmap-client EmailAddress to our EmailAddress type.
fn jmap_addr_to_addr(addr: &jmap_client::email::EmailAddress) -> EmailAddress {
    EmailAddress {
        name: addr.name().map(|s| s.to_string()),
        email: addr.email().to_string(),
    }
}
