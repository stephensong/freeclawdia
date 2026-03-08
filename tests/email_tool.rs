//! Email tool tests against a local Stalwart JMAP server.
//!
//! Tests the email tools through the `ToolRegistry` — the same interface
//! the agent dispatch loop uses. Verifies that tools handle realistic
//! LLM-style inputs (role names, display names, case variations) correctly.
//!
//! Prerequisites:
//!   - Stalwart running on localhost:8080
//!   - User "gary" with password "password123" and identity gary@local.dev
//!   - User "alice" with password "password123"
//!
//! These tests are gated behind `#[cfg(feature = "email")]` and require
//! a live Stalwart instance — they are NOT run in CI by default.
//! Run with:
//!   cargo test --features email --test email_tool -- --nocapture

#![cfg(feature = "email")]

use std::sync::Arc;

use serde_json::json;

use ironclaw::config::EmailConfig;
use ironclaw::context::JobContext;
use ironclaw::email::{EmailAddress, EmailDraft, EmailProvider, JmapEmailProvider};
use ironclaw::tools::ToolRegistry;

const JMAP_URL: &str = "http://localhost:8080";
const GARY_USER: &str = "gary";
const GARY_PASS: &str = "password123";

fn gary_config() -> EmailConfig {
    EmailConfig {
        enabled: true,
        jmap_url: Some(JMAP_URL.to_string()),
        username: Some(GARY_USER.to_string()),
        password: Some(GARY_PASS.to_string()),
        poll_interval_secs: 60,
        max_fetch: 50,
    }
}

fn gary_provider() -> Arc<JmapEmailProvider> {
    Arc::new(JmapEmailProvider::new(gary_config()))
}

fn test_ctx() -> JobContext {
    JobContext::new("email-tool-test", "tool test")
}

/// Build a ToolRegistry with email tools registered, just like the real app does.
fn registry_with_email() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_email_tools(gary_provider());
    registry
}

// ── email_mailboxes ──────────────────────────────────────────────

#[tokio::test]
async fn tool_mailboxes_returns_all_standard_folders() {
    let registry = registry_with_email();
    let tool = registry.get("email_mailboxes").await.expect("tool not found");
    let ctx = test_ctx();

    let output = tool.execute(json!({}), &ctx).await.expect("execute failed");
    let mailboxes: Vec<serde_json::Value> =
        serde_json::from_value(output.result).expect("parse result");

    println!("email_mailboxes returned {} mailboxes:", mailboxes.len());
    for mb in &mailboxes {
        println!(
            "  {} (id={}, role={})",
            mb["name"], mb["id"], mb["role"]
        );
    }

    // Verify standard folders exist
    let has_role = |role: &str| mailboxes.iter().any(|mb| mb["role"] == role);
    assert!(has_role("inbox"), "missing inbox");
    assert!(has_role("sent"), "missing sent");
    assert!(has_role("drafts"), "missing drafts");
    assert!(has_role("trash"), "missing trash");
}

// ── email_list: mailbox resolution ───────────────────────────────
//
// The LLM may pass a raw JMAP ID, a role name, a display name, or
// various case/alias variations. The tool must handle all of these.

/// Seed a test email and return its subject (for finding it later).
async fn seed_sent_email() -> String {
    let provider = gary_provider();
    let subject = format!("ToolTest {}", uuid::Uuid::new_v4());
    let draft = EmailDraft {
        to: vec![EmailAddress {
            name: None,
            email: "alice@local.dev".to_string(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.clone(),
        text_body: "Seeded by tool test.".to_string(),
        html_body: None,
        in_reply_to: None,
        references: Vec::new(),
    };
    provider.send_email(draft).await.expect("seed send failed");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    subject
}

/// Helper: call email_list tool and return parsed results.
async fn call_email_list(
    registry: &ToolRegistry,
    mailbox_id: &str,
) -> Vec<serde_json::Value> {
    let tool = registry.get("email_list").await.expect("email_list not found");
    let ctx = test_ctx();
    let output = tool
        .execute(json!({"mailbox_id": mailbox_id}), &ctx)
        .await
        .unwrap_or_else(|e| panic!("email_list failed for mailbox_id={mailbox_id:?}: {e}"));
    serde_json::from_value(output.result).expect("parse email list")
}

#[tokio::test]
async fn tool_list_by_role_name_sent() {
    let _subject = seed_sent_email().await;
    let registry = registry_with_email();
    let emails = call_email_list(&registry, "sent").await;
    println!("email_list(\"sent\") returned {} emails", emails.len());
    assert!(!emails.is_empty(), "\"sent\" should resolve to Sent folder");
}

#[tokio::test]
async fn tool_list_by_role_name_inbox() {
    let registry = registry_with_email();
    let emails = call_email_list(&registry, "inbox").await;
    println!("email_list(\"inbox\") returned {} emails", emails.len());
    // inbox may be empty, but the call must not fail
}

#[tokio::test]
async fn tool_list_by_role_name_drafts() {
    let registry = registry_with_email();
    let emails = call_email_list(&registry, "drafts").await;
    println!("email_list(\"drafts\") returned {} emails", emails.len());
}

#[tokio::test]
async fn tool_list_by_display_name() {
    let _subject = seed_sent_email().await;
    let registry = registry_with_email();
    let emails = call_email_list(&registry, "Sent Items").await;
    println!(
        "email_list(\"Sent Items\") returned {} emails",
        emails.len()
    );
    assert!(
        !emails.is_empty(),
        "\"Sent Items\" should resolve to Sent folder"
    );
}

#[tokio::test]
async fn tool_list_by_display_name_case_insensitive() {
    let _subject = seed_sent_email().await;
    let registry = registry_with_email();
    let emails = call_email_list(&registry, "sent items").await;
    assert!(
        !emails.is_empty(),
        "\"sent items\" (lowercase) should resolve to Sent folder"
    );
}

#[tokio::test]
async fn tool_list_by_raw_jmap_id() {
    let registry = registry_with_email();

    // Get the real Sent folder ID via email_mailboxes
    let mb_tool = registry
        .get("email_mailboxes")
        .await
        .expect("email_mailboxes not found");
    let ctx = test_ctx();
    let mb_output = mb_tool.execute(json!({}), &ctx).await.expect("mailboxes");
    let mailboxes: Vec<serde_json::Value> =
        serde_json::from_value(mb_output.result).expect("parse");
    let sent_id = mailboxes
        .iter()
        .find(|mb| mb["role"] == "sent")
        .expect("no sent folder")["id"]
        .as_str()
        .expect("id not string");

    println!("Sent folder raw JMAP ID: {sent_id:?}");

    let _subject = seed_sent_email().await;
    let emails = call_email_list(&registry, sent_id).await;
    assert!(
        !emails.is_empty(),
        "Raw JMAP ID {sent_id:?} should return sent emails"
    );
}

// ── email_send through the tool layer ────────────────────────────

#[tokio::test]
async fn tool_send_email() {
    let registry = registry_with_email();
    let tool = registry.get("email_send").await.expect("email_send not found");
    let ctx = test_ctx();

    let subject = format!("ToolSendTest {}", uuid::Uuid::new_v4());
    let output = tool
        .execute(
            json!({
                "to": "alice@local.dev",
                "subject": subject,
                "body": "Sent via email_send tool."
            }),
            &ctx,
        )
        .await
        .expect("email_send failed");

    println!("email_send result: {}", output.result);
    // Result should contain the email ID
    let result_str = output.result.to_string();
    assert!(
        !result_str.is_empty(),
        "email_send should return a non-empty result"
    );

    // Verify it landed in Sent via the tool layer
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let emails = call_email_list(&registry, "sent").await;
    let found = emails.iter().any(|e| e["subject"] == subject);
    assert!(
        found,
        "Email with subject {subject:?} not found in Sent folder via tool"
    );
}

// ── email_read through the tool layer ────────────────────────────

#[tokio::test]
async fn tool_read_email() {
    let subject = seed_sent_email().await;
    let registry = registry_with_email();

    // Find the email via list
    let emails = call_email_list(&registry, "sent").await;
    let email = emails
        .iter()
        .find(|e| e["subject"].as_str() == Some(&subject))
        .expect("seeded email not in sent");
    let email_id = email["id"].as_str().expect("no id");

    // Read it via the tool
    let tool = registry.get("email_read").await.expect("email_read not found");
    let ctx = test_ctx();
    let output = tool
        .execute(json!({"email_id": email_id}), &ctx)
        .await
        .expect("email_read failed");

    let result = output.result;
    println!("email_read result: {}", serde_json::to_string_pretty(&result).unwrap());
    assert_eq!(result["subject"].as_str(), Some(subject.as_str()));
    assert!(
        result["text_body"]
            .as_str()
            .unwrap_or("")
            .contains("Seeded by tool test"),
        "Body should contain seed text"
    );
}

// ── email_search through the tool layer ──────────────────────────

#[tokio::test]
async fn tool_search_email() {
    let subject = seed_sent_email().await;
    let registry = registry_with_email();

    let tool = registry
        .get("email_search")
        .await
        .expect("email_search not found");
    let ctx = test_ctx();
    let output = tool
        .execute(json!({"query": &subject}), &ctx)
        .await
        .expect("email_search failed");

    let results: Vec<serde_json::Value> =
        serde_json::from_value(output.result).expect("parse search results");
    println!("email_search({subject:?}) returned {} results", results.len());
    let found = results.iter().any(|e| e["subject"].as_str() == Some(&subject));
    assert!(found, "Search should find the seeded email by subject");
}
