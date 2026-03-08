#![cfg(feature = "postgres")]
//! Integration tests for the time-travel / audit log feature.
//!
//! Tests are cumulative: each test builds on the state left by previous tests,
//! verifying that we can reconstruct the system at any prior epoch.
//!
//! Requires a running PostgreSQL. Set DATABASE_URL=postgres://localhost/ironclaw_test
//!
//! The tests record epoch timestamps between operations, then use audit_as_at()
//! to verify that the audit log accurately captures every mutation, and that
//! state can be reconstructed by replaying the log.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use ironclaw::config::{DatabaseConfig, DatabaseBackend, SslMode};
use ironclaw::db::postgres::PgBackend;
use ironclaw::db::{AuditInput, AuditStore, ConversationStore, Database, SettingsStore};

const TEST_USER: &str = "time_travel_test_user";

async fn setup() -> Option<PgBackend> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/ironclaw_test".to_string());

    let config = DatabaseConfig {
        backend: DatabaseBackend::Postgres,
        url: secrecy::SecretString::from(url),
        pool_size: 4,
        ssl_mode: SslMode::Disable,
        libsql_path: None,
        libsql_url: None,
        libsql_auth_token: None,
    };

    let backend = match PgBackend::new(&config).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skipping: database unavailable ({e})");
            return None;
        }
    };
    backend.run_migrations().await.ok()?;
    Some(backend)
}

async fn cleanup(db: &PgBackend) {
    let pool = db.pool();
    let conn = pool.get().await.expect("cleanup connection");
    // Clean up test data
    conn.execute(
        "DELETE FROM audit_log WHERE user_id = $1",
        &[&TEST_USER],
    )
    .await
    .ok();
    conn.execute(
        "DELETE FROM settings WHERE user_id = $1",
        &[&TEST_USER],
    )
    .await
    .ok();
    conn.execute(
        "DELETE FROM conversation_messages WHERE conversation_id IN (SELECT id FROM conversations WHERE user_id = $1)",
        &[&TEST_USER],
    )
    .await
    .ok();
    conn.execute(
        "DELETE FROM conversations WHERE user_id = $1",
        &[&TEST_USER],
    )
    .await
    .ok();
}

/// Small sleep to ensure distinct timestamps between operations.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

// =============================================================================
// Test 1: Audit log captures setting create/update/delete
// =============================================================================
#[tokio::test]
async fn test_01_audit_log_captures_settings_mutations() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    // Epoch 0: empty state
    let epoch0 = now();
    tick().await;

    // Create a setting
    db.set_setting(TEST_USER, "color", &serde_json::json!("blue"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "color",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!("blue")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch1 = now();
    tick().await;

    // Update the setting
    db.set_setting(TEST_USER, "color", &serde_json::json!("red"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "color",
        action: "update",
        field: None,
        old_value: Some(&serde_json::json!("blue")),
        new_value: Some(&serde_json::json!("red")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch2 = now();
    tick().await;

    // Delete the setting
    db.delete_setting(TEST_USER, "color").await.unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "color",
        action: "delete",
        field: None,
        old_value: Some(&serde_json::json!("red")),
        new_value: None,
        metadata: None,
    })
    .await
    .unwrap();

    let epoch3 = now();

    // Verify: history for this entity shows 3 entries
    let history = db
        .audit_history("setting", "color", 100)
        .await
        .unwrap();
    assert_eq!(history.len(), 3, "Should have 3 audit entries for 'color'");

    // Verify: as_at epoch0 shows nothing (before any changes)
    let at_epoch0 = db.audit_as_at(epoch0, Some("setting"), 100).await.unwrap();
    assert_eq!(at_epoch0.len(), 0, "No audit entries before epoch0");

    // Verify: as_at epoch1 shows 1 entry (the create)
    let at_epoch1 = db.audit_as_at(epoch1, Some("setting"), 100).await.unwrap();
    assert_eq!(at_epoch1.len(), 1, "1 entry at epoch1");
    assert_eq!(at_epoch1[0].action, "create");

    // Verify: as_at epoch2 shows 2 entries
    let at_epoch2 = db.audit_as_at(epoch2, Some("setting"), 100).await.unwrap();
    assert_eq!(at_epoch2.len(), 2, "2 entries at epoch2");

    // Verify: as_at epoch3 shows all 3
    let at_epoch3 = db.audit_as_at(epoch3, Some("setting"), 100).await.unwrap();
    assert_eq!(at_epoch3.len(), 3, "3 entries at epoch3");

    cleanup(&db).await;
}

// =============================================================================
// Test 2: Reconstruct settings state at multiple epochs
// =============================================================================
#[tokio::test]
async fn test_02_reconstruct_settings_at_epochs() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    let epoch0 = now();
    tick().await;

    // Create multiple settings
    for (key, val) in [("theme", "dark"), ("lang", "en"), ("tz", "UTC")] {
        db.set_setting(TEST_USER, key, &serde_json::json!(val))
            .await
            .unwrap();
        db.audit_log(AuditInput {
            user_id: TEST_USER,
            entity_type: "setting",
            entity_id: key,
            action: "create",
            field: None,
            old_value: None,
            new_value: Some(&serde_json::json!(val)),
            metadata: None,
        })
        .await
        .unwrap();
    }

    let epoch1 = now();
    tick().await;

    // Mutate: change theme, delete tz
    db.set_setting(TEST_USER, "theme", &serde_json::json!("light"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "theme",
        action: "update",
        field: None,
        old_value: Some(&serde_json::json!("dark")),
        new_value: Some(&serde_json::json!("light")),
        metadata: None,
    })
    .await
    .unwrap();

    db.delete_setting(TEST_USER, "tz").await.unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "tz",
        action: "delete",
        field: None,
        old_value: Some(&serde_json::json!("UTC")),
        new_value: None,
        metadata: None,
    })
    .await
    .unwrap();

    let epoch2 = now();
    tick().await;

    // Add a new setting
    db.set_setting(TEST_USER, "font", &serde_json::json!("mono"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "font",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!("mono")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch3 = now();

    // Reconstruct at each epoch
    let s0 = reconstruct_settings(&db, epoch0).await;
    assert!(s0.is_empty(), "Epoch0: no settings yet");

    let s1 = reconstruct_settings(&db, epoch1).await;
    assert_eq!(s1.len(), 3, "Epoch1: 3 settings");
    assert_eq!(s1["theme"], serde_json::json!("dark"));
    assert_eq!(s1["lang"], serde_json::json!("en"));
    assert_eq!(s1["tz"], serde_json::json!("UTC"));

    let s2 = reconstruct_settings(&db, epoch2).await;
    assert_eq!(s2.len(), 2, "Epoch2: 2 settings (tz deleted)");
    assert_eq!(s2["theme"], serde_json::json!("light"), "theme updated to light");
    assert_eq!(s2["lang"], serde_json::json!("en"));
    assert!(!s2.contains_key("tz"), "tz should be deleted at epoch2");

    let s3 = reconstruct_settings(&db, epoch3).await;
    assert_eq!(s3.len(), 3, "Epoch3: 3 settings (font added)");
    assert_eq!(s3["font"], serde_json::json!("mono"));
    assert_eq!(s3["theme"], serde_json::json!("light"));

    // Verify current DB state matches epoch3 reconstruction
    let current = db.get_all_settings(TEST_USER).await.unwrap();
    assert_eq!(current.len(), s3.len(), "Current state matches epoch3");
    for (k, v) in &s3 {
        assert_eq!(current.get(k).unwrap(), v, "Key '{}' matches", k);
    }

    cleanup(&db).await;
}

/// Helper: replay audit log to reconstruct settings at a given time.
async fn reconstruct_settings(
    db: &PgBackend,
    as_at: DateTime<Utc>,
) -> HashMap<String, serde_json::Value> {
    let mut entries = db
        .audit_as_at(as_at, Some("setting"), 10000)
        .await
        .unwrap();
    // Reverse to replay oldest-first
    entries.reverse();

    let mut settings: HashMap<String, serde_json::Value> = HashMap::new();
    for entry in &entries {
        // Only replay entries for our test user
        if entry.user_id != TEST_USER {
            continue;
        }
        match entry.action.as_str() {
            "create" | "update" => {
                if let Some(ref val) = entry.new_value {
                    settings.insert(entry.entity_id.clone(), val.clone());
                }
            }
            "delete" => {
                settings.remove(&entry.entity_id);
            }
            _ => {}
        }
    }
    settings
}

// =============================================================================
// Test 3: Audit log captures conversation lifecycle
// =============================================================================
#[tokio::test]
async fn test_03_audit_conversation_lifecycle() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    let epoch0 = now();
    tick().await;

    // Create a conversation
    let conv_id = db
        .create_conversation("gateway", TEST_USER, Some("test-thread"))
        .await
        .unwrap();

    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!({ "channel": "gateway", "thread_id": "test-thread" })),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch1 = now();
    tick().await;

    // Rename it
    db.update_conversation_metadata_field(
        conv_id,
        "custom_title",
        &serde_json::json!("My Thread"),
    )
    .await
    .unwrap();

    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "update",
        field: Some("custom_title"),
        old_value: None,
        new_value: Some(&serde_json::json!("My Thread")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch2 = now();
    tick().await;

    // Delete it
    db.delete_conversation(conv_id).await.unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "delete",
        field: None,
        old_value: None,
        new_value: None,
        metadata: None,
    })
    .await
    .unwrap();

    let epoch3 = now();

    // Verify the full history of this conversation
    let history = db
        .audit_history("conversation", &conv_id.to_string(), 100)
        .await
        .unwrap();
    assert_eq!(history.len(), 3, "3 lifecycle events for conversation");

    // Newest first
    assert_eq!(history[0].action, "delete");
    assert_eq!(history[1].action, "update");
    assert_eq!(history[2].action, "create");

    // Verify time-bounded queries
    let at0 = db
        .audit_as_at(epoch0, Some("conversation"), 100)
        .await
        .unwrap();
    assert_eq!(at0.len(), 0, "No conversation events before epoch0");

    let at1 = db
        .audit_as_at(epoch1, Some("conversation"), 100)
        .await
        .unwrap();
    assert_eq!(at1.len(), 1, "1 event at epoch1 (create)");
    assert_eq!(at1[0].action, "create");

    let at2 = db
        .audit_as_at(epoch2, Some("conversation"), 100)
        .await
        .unwrap();
    assert_eq!(at2.len(), 2, "2 events at epoch2");

    let at3 = db
        .audit_as_at(epoch3, Some("conversation"), 100)
        .await
        .unwrap();
    assert_eq!(at3.len(), 3, "3 events at epoch3");

    cleanup(&db).await;
}

// =============================================================================
// Test 4: Interleaved mutations across entity types with epoch reconstruction
// =============================================================================
#[tokio::test]
async fn test_04_interleaved_mutations_multi_entity() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    // === Wave 1: create settings and a conversation ===
    let epoch_start = now();
    tick().await;

    db.set_setting(TEST_USER, "mode", &serde_json::json!("auto"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "mode",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!("auto")),
        metadata: None,
    })
    .await
    .unwrap();

    let conv_id = db
        .create_conversation("gateway", TEST_USER, Some("wave1"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!({ "thread_id": "wave1" })),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch_wave1 = now();
    tick().await;

    // === Wave 2: more settings, rename conversation ===
    db.set_setting(TEST_USER, "volume", &serde_json::json!(75))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "volume",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!(75)),
        metadata: None,
    })
    .await
    .unwrap();

    db.update_conversation_metadata_field(
        conv_id,
        "custom_title",
        &serde_json::json!("Renamed"),
    )
    .await
    .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "update",
        field: Some("custom_title"),
        old_value: None,
        new_value: Some(&serde_json::json!("Renamed")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch_wave2 = now();
    tick().await;

    // === Wave 3: delete conversation, update setting ===
    db.delete_conversation(conv_id).await.unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "conversation",
        entity_id: &conv_id.to_string(),
        action: "delete",
        field: None,
        old_value: None,
        new_value: None,
        metadata: None,
    })
    .await
    .unwrap();

    db.set_setting(TEST_USER, "mode", &serde_json::json!("manual"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "mode",
        action: "update",
        field: None,
        old_value: Some(&serde_json::json!("auto")),
        new_value: Some(&serde_json::json!("manual")),
        metadata: None,
    })
    .await
    .unwrap();

    let epoch_wave3 = now();

    // === Verify: timeline at each epoch ===

    // Before anything: empty
    let all_at_start = db.audit_as_at(epoch_start, None, 1000).await.unwrap();
    assert_eq!(all_at_start.len(), 0);

    // After wave 1: 1 setting + 1 conversation = 2 entries
    let all_at_w1 = db.audit_as_at(epoch_wave1, None, 1000).await.unwrap();
    let w1_test: Vec<_> = all_at_w1
        .iter()
        .filter(|e| e.user_id == TEST_USER)
        .collect();
    assert_eq!(w1_test.len(), 2, "Wave1: 2 entries");

    // After wave 2: +2 more = 4 total
    let all_at_w2 = db.audit_as_at(epoch_wave2, None, 1000).await.unwrap();
    let w2_test: Vec<_> = all_at_w2
        .iter()
        .filter(|e| e.user_id == TEST_USER)
        .collect();
    assert_eq!(w2_test.len(), 4, "Wave2: 4 entries");

    // After wave 3: +2 more = 6 total
    let all_at_w3 = db.audit_as_at(epoch_wave3, None, 1000).await.unwrap();
    let w3_test: Vec<_> = all_at_w3
        .iter()
        .filter(|e| e.user_id == TEST_USER)
        .collect();
    assert_eq!(w3_test.len(), 6, "Wave3: 6 entries");

    // Reconstruct settings at each epoch
    let s_w1 = reconstruct_settings(&db, epoch_wave1).await;
    assert_eq!(s_w1.len(), 1);
    assert_eq!(s_w1["mode"], serde_json::json!("auto"));

    let s_w2 = reconstruct_settings(&db, epoch_wave2).await;
    assert_eq!(s_w2.len(), 2);
    assert_eq!(s_w2["mode"], serde_json::json!("auto"));
    assert_eq!(s_w2["volume"], serde_json::json!(75));

    let s_w3 = reconstruct_settings(&db, epoch_wave3).await;
    assert_eq!(s_w3.len(), 2);
    assert_eq!(s_w3["mode"], serde_json::json!("manual"), "mode updated in wave3");
    assert_eq!(s_w3["volume"], serde_json::json!(75));

    // Filter by entity type
    let settings_only = db
        .audit_as_at(epoch_wave3, Some("setting"), 1000)
        .await
        .unwrap();
    let settings_test: Vec<_> = settings_only
        .iter()
        .filter(|e| e.user_id == TEST_USER)
        .collect();
    assert_eq!(settings_test.len(), 3, "3 setting events total");

    let convos_only = db
        .audit_as_at(epoch_wave3, Some("conversation"), 1000)
        .await
        .unwrap();
    let convos_test: Vec<_> = convos_only
        .iter()
        .filter(|e| e.user_id == TEST_USER)
        .collect();
    assert_eq!(convos_test.len(), 3, "3 conversation events total");

    cleanup(&db).await;
}

// =============================================================================
// Test 5: Rapid-fire mutations preserve ordering
// =============================================================================
#[tokio::test]
async fn test_05_rapid_mutations_preserve_order() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    // Create 20 settings in rapid succession
    for i in 0..20 {
        let key = format!("rapid_{}", i);
        let val = serde_json::json!(i);
        db.set_setting(TEST_USER, &key, &val).await.unwrap();
        db.audit_log(AuditInput {
            user_id: TEST_USER,
            entity_type: "setting",
            entity_id: &key,
            action: "create",
            field: None,
            old_value: None,
            new_value: Some(&val),
            metadata: None,
        })
        .await
        .unwrap();
    }

    // All 20 should be in the log
    let all = db.audit_as_at(now(), Some("setting"), 1000).await.unwrap();
    let test_entries: Vec<_> = all
        .iter()
        .filter(|e| e.user_id == TEST_USER && e.entity_id.starts_with("rapid_"))
        .collect();
    assert_eq!(test_entries.len(), 20, "All 20 rapid entries recorded");

    // Entries are newest-first; reversed they should be in creation order
    let ids: Vec<&str> = test_entries.iter().rev().map(|e| e.entity_id.as_str()).collect();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(*id, format!("rapid_{}", i), "Order preserved at index {}", i);
    }

    // Reconstruct: all 20 settings present
    let reconstructed = reconstruct_settings(&db, now()).await;
    let rapid_keys: Vec<_> = reconstructed
        .keys()
        .filter(|k| k.starts_with("rapid_"))
        .collect();
    assert_eq!(rapid_keys.len(), 20, "All 20 settings reconstructed");

    cleanup(&db).await;
}

// =============================================================================
// Test 6: Overwrite chain — same key updated many times
// =============================================================================
#[tokio::test]
async fn test_06_overwrite_chain_reconstruction() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    let mut epochs = vec![now()];
    tick().await;

    // Update the same key 10 times, recording epochs between each
    for i in 0..10 {
        let old_val = if i == 0 {
            None
        } else {
            Some(serde_json::json!(format!("v{}", i - 1)))
        };
        let new_val = serde_json::json!(format!("v{}", i));
        let action = if i == 0 { "create" } else { "update" };

        db.set_setting(TEST_USER, "counter", &new_val)
            .await
            .unwrap();
        db.audit_log(AuditInput {
            user_id: TEST_USER,
            entity_type: "setting",
            entity_id: "counter",
            action,
            field: None,
            old_value: old_val.as_ref(),
            new_value: Some(&new_val),
            metadata: None,
        })
        .await
        .unwrap();

        tick().await;
        epochs.push(now());
    }

    // Verify: at each epoch, the reconstructed value matches what was set
    let s_before = reconstruct_settings(&db, epochs[0]).await;
    assert!(s_before.is_empty(), "Before any writes: empty");

    for i in 0..10 {
        let s = reconstruct_settings(&db, epochs[i + 1]).await;
        let expected = serde_json::json!(format!("v{}", i));
        assert_eq!(
            s["counter"], expected,
            "At epoch {}: counter should be v{}",
            i + 1,
            i
        );
    }

    // Full history shows all 10 entries
    let history = db.audit_history("setting", "counter", 100).await.unwrap();
    assert_eq!(history.len(), 10);

    cleanup(&db).await;
}

// =============================================================================
// Test 7: audit_history returns entity-scoped results only
// =============================================================================
#[tokio::test]
async fn test_07_audit_history_entity_scoped() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    // Create two different settings
    for (key, val) in [("alpha", "a"), ("beta", "b")] {
        db.set_setting(TEST_USER, key, &serde_json::json!(val))
            .await
            .unwrap();
        db.audit_log(AuditInput {
            user_id: TEST_USER,
            entity_type: "setting",
            entity_id: key,
            action: "create",
            field: None,
            old_value: None,
            new_value: Some(&serde_json::json!(val)),
            metadata: None,
        })
        .await
        .unwrap();
    }

    // Update alpha
    db.set_setting(TEST_USER, "alpha", &serde_json::json!("a2"))
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "alpha",
        action: "update",
        field: None,
        old_value: Some(&serde_json::json!("a")),
        new_value: Some(&serde_json::json!("a2")),
        metadata: None,
    })
    .await
    .unwrap();

    // History for alpha: 2 entries
    let alpha_hist = db.audit_history("setting", "alpha", 100).await.unwrap();
    assert_eq!(alpha_hist.len(), 2);
    assert!(
        alpha_hist.iter().all(|e| e.entity_id == "alpha"),
        "All entries are for alpha"
    );

    // History for beta: 1 entry
    let beta_hist = db.audit_history("setting", "beta", 100).await.unwrap();
    assert_eq!(beta_hist.len(), 1);
    assert_eq!(beta_hist[0].entity_id, "beta");

    // History for nonexistent: 0 entries
    let empty = db
        .audit_history("setting", "nonexistent", 100)
        .await
        .unwrap();
    assert_eq!(empty.len(), 0);

    cleanup(&db).await;
}

// =============================================================================
// Test 8: Old and new values are faithfully stored
// =============================================================================
#[tokio::test]
async fn test_08_old_new_values_preserved() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    let complex_val = serde_json::json!({
        "nested": { "key": "value", "list": [1, 2, 3] },
        "flag": true,
        "count": 42
    });
    let updated_val = serde_json::json!({
        "nested": { "key": "changed", "list": [4, 5] },
        "flag": false,
        "count": 99
    });

    db.set_setting(TEST_USER, "complex", &complex_val)
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "complex",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&complex_val),
        metadata: None,
    })
    .await
    .unwrap();

    db.set_setting(TEST_USER, "complex", &updated_val)
        .await
        .unwrap();
    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "complex",
        action: "update",
        field: None,
        old_value: Some(&complex_val),
        new_value: Some(&updated_val),
        metadata: None,
    })
    .await
    .unwrap();

    let history = db.audit_history("setting", "complex", 100).await.unwrap();
    assert_eq!(history.len(), 2);

    // Newest first — the update
    let update_entry = &history[0];
    assert_eq!(update_entry.action, "update");
    assert_eq!(update_entry.old_value.as_ref().unwrap(), &complex_val);
    assert_eq!(update_entry.new_value.as_ref().unwrap(), &updated_val);

    // The create
    let create_entry = &history[1];
    assert_eq!(create_entry.action, "create");
    assert!(create_entry.old_value.is_none());
    assert_eq!(create_entry.new_value.as_ref().unwrap(), &complex_val);

    cleanup(&db).await;
}

// =============================================================================
// Test 9: Metadata field is stored and retrievable
// =============================================================================
#[tokio::test]
async fn test_09_metadata_stored() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    let meta = serde_json::json!({ "source": "web_ui", "ip": "127.0.0.1" });

    db.audit_log(AuditInput {
        user_id: TEST_USER,
        entity_type: "setting",
        entity_id: "with_meta",
        action: "create",
        field: None,
        old_value: None,
        new_value: Some(&serde_json::json!("test")),
        metadata: Some(&meta),
    })
    .await
    .unwrap();

    let history = db
        .audit_history("setting", "with_meta", 100)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].metadata.as_ref().unwrap(), &meta);

    cleanup(&db).await;
}

// =============================================================================
// Test 10: Limit parameter is respected
// =============================================================================
#[tokio::test]
async fn test_10_limit_respected() {
    let db = match setup().await {
        Some(db) => db,
        None => return,
    };
    cleanup(&db).await;

    // Create 15 entries
    for i in 0..15 {
        db.audit_log(AuditInput {
            user_id: TEST_USER,
            entity_type: "setting",
            entity_id: &format!("limit_{}", i),
            action: "create",
            field: None,
            old_value: None,
            new_value: Some(&serde_json::json!(i)),
            metadata: None,
        })
        .await
        .unwrap();
    }

    // Request with limit=5
    let limited = db
        .audit_as_at(now(), Some("setting"), 5)
        .await
        .unwrap();
    // May include entries from other tests if not cleaned up, but at most 5
    assert!(limited.len() <= 5, "Limit respected: got {}", limited.len());

    // Request with limit=100 should get all of ours
    let all = db
        .audit_as_at(now(), Some("setting"), 100)
        .await
        .unwrap();
    let ours: Vec<_> = all
        .iter()
        .filter(|e| e.user_id == TEST_USER && e.entity_id.starts_with("limit_"))
        .collect();
    assert_eq!(ours.len(), 15, "All 15 entries present with high limit");

    cleanup(&db).await;
}
