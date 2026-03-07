//! End-to-end email tests against a local Stalwart JMAP server.
//!
//! Prerequisites:
//!   - Stalwart running on localhost:8080
//!   - User "gary" with password "password123" and identity gary@local.dev
//!   - User "alice" with password "password123"
//!
//! These tests are gated behind `#[cfg(feature = "email")]` and require
//! a live Stalwart instance — they are NOT run in CI by default.
//! Run with:
//!   cargo test --features email --test email_e2e -- --nocapture

#![cfg(feature = "email")]

use ironclaw::config::EmailConfig;
use ironclaw::email::{EmailAddress, EmailDraft, EmailProvider, JmapEmailProvider};

const JMAP_URL: &str = "http://localhost:8080";
const GARY_USER: &str = "gary";
const GARY_PASS: &str = "password123";
const ALICE_USER: &str = "alice";
const ALICE_PASS: &str = "password123";

fn gary_provider() -> JmapEmailProvider {
    JmapEmailProvider::new(EmailConfig {
        enabled: true,
        jmap_url: Some(JMAP_URL.to_string()),
        username: Some(GARY_USER.to_string()),
        password: Some(GARY_PASS.to_string()),
        poll_interval_secs: 60,
        max_fetch: 50,
    })
}

fn alice_provider() -> JmapEmailProvider {
    JmapEmailProvider::new(EmailConfig {
        enabled: true,
        jmap_url: Some(JMAP_URL.to_string()),
        username: Some(ALICE_USER.to_string()),
        password: Some(ALICE_PASS.to_string()),
        poll_interval_secs: 60,
        max_fetch: 50,
    })
}

/// Verify we can connect and list mailboxes.
#[tokio::test]
async fn test_01_list_mailboxes() {
    let provider = gary_provider();
    let mailboxes = provider.list_mailboxes().await.expect("list_mailboxes");
    println!("Gary's mailboxes:");
    for mb in &mailboxes {
        println!(
            "  {} (id={}, role={:?}, total={}, unread={})",
            mb.name, mb.id, mb.role, mb.total_emails, mb.unread_emails
        );
    }
    assert!(
        mailboxes.iter().any(|mb| mb.role.as_deref() == Some("inbox")),
        "Expected an inbox mailbox"
    );
    assert!(
        mailboxes
            .iter()
            .any(|mb| mb.role.as_deref() == Some("drafts")),
        "Expected a drafts mailbox"
    );
    assert!(
        mailboxes
            .iter()
            .any(|mb| mb.role.as_deref() == Some("sent")),
        "Expected a sent mailbox"
    );
}

/// Send an email from gary to alice and verify it arrives.
#[tokio::test]
async fn test_02_send_and_receive() {
    let gary = gary_provider();
    let alice = alice_provider();

    // Use a unique subject so we can find it
    let unique_subject = format!("E2E Test {}", uuid::Uuid::new_v4());
    let body_text = "Hello Alice, this is an automated e2e test.";

    let draft = EmailDraft {
        to: vec![EmailAddress {
            name: Some("Alice".to_string()),
            email: "alice@local.dev".to_string(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: unique_subject.clone(),
        text_body: body_text.to_string(),
        html_body: None,
        in_reply_to: None,
        references: Vec::new(),
    };

    println!("Sending email with subject: {unique_subject}");
    let email_id = gary.send_email(draft).await.expect("send_email failed");
    println!("Email sent, server ID: {email_id}");

    // Give Stalwart a moment for local delivery
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Check alice's inbox
    let alice_mailboxes = alice.list_mailboxes().await.expect("alice list_mailboxes");
    let inbox = alice_mailboxes
        .iter()
        .find(|mb| mb.role.as_deref() == Some("inbox"))
        .expect("Alice has no inbox");

    println!(
        "Alice's inbox: {} total, {} unread",
        inbox.total_emails, inbox.unread_emails
    );

    let emails = alice
        .list_emails(&inbox.id, 0, 50)
        .await
        .expect("alice list_emails");

    println!("Alice's inbox emails:");
    for e in &emails {
        println!("  [{}] {} — from {:?}", e.id, e.subject, e.from);
    }

    let found = emails.iter().find(|e| e.subject == unique_subject);
    assert!(
        found.is_some(),
        "Email with subject '{}' not found in Alice's inbox. Found {} emails.",
        unique_subject,
        emails.len()
    );

    let found = found.unwrap();
    assert!(
        found.from.iter().any(|a| a.email.contains("gary")),
        "Expected from address to contain 'gary'"
    );

    // Read the full email and verify body
    let full = alice
        .get_email(&found.id)
        .await
        .expect("alice get_email");
    assert_eq!(
        full.text_body.as_deref(),
        Some(body_text),
        "Body text mismatch"
    );

    println!("E2E send-and-receive: PASSED");
}

/// Send with no display name (matches compose modal input for bare email).
#[tokio::test]
async fn test_02b_send_no_display_name() {
    let gary = gary_provider();

    let unique_subject = format!("No-Name Test {}", uuid::Uuid::new_v4());

    let draft = EmailDraft {
        to: vec![EmailAddress {
            name: None,
            email: "alice@local.dev".to_string(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: unique_subject.clone(),
        text_body: "Sent with no display name.".to_string(),
        html_body: None,
        in_reply_to: None,
        references: Vec::new(),
    };

    println!("Sending to alice@local.dev (no display name), subject: {unique_subject}");
    let id = gary.send_email(draft).await.expect("send_email failed");
    println!("Success! Email ID: {id}");
}

/// Verify gary's Sent folder contains the email after sending.
#[tokio::test]
async fn test_03_sent_folder() {
    let gary = gary_provider();

    let unique_subject = format!("Sent Folder Test {}", uuid::Uuid::new_v4());

    let draft = EmailDraft {
        to: vec![EmailAddress {
            name: None,
            email: "alice@local.dev".to_string(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: unique_subject.clone(),
        text_body: "Checking sent folder.".to_string(),
        html_body: None,
        in_reply_to: None,
        references: Vec::new(),
    };

    gary.send_email(draft).await.expect("send_email failed");

    // Give Stalwart a moment
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let mailboxes = gary.list_mailboxes().await.expect("list_mailboxes");
    let sent = mailboxes
        .iter()
        .find(|mb| mb.role.as_deref() == Some("sent"))
        .expect("Gary has no sent folder");

    let emails = gary
        .list_emails(&sent.id, 0, 50)
        .await
        .expect("gary list_emails sent");

    let found = emails.iter().find(|e| e.subject == unique_subject);
    assert!(
        found.is_some(),
        "Email with subject '{}' not found in Gary's Sent folder",
        unique_subject
    );

    println!("Sent folder test: PASSED");
}
