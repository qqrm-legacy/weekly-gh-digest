//! Publication regressions: each repository is independently forwardable.

use super::*;

fn three_projects(edition: &str) -> Value {
    let entries = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|name| {
            json!({
                "full_name":format!("org/{name}"),
                "source_kind":if edition == "daily" {
                    "trending_daily"
                } else {
                    "trending_weekly"
                },
                "delta":42,
                "description":format!(
                    "Разбор {name}: проверяет зависимости.\n\nПоказывает полный отчёт {name} 🔍."
                ),
                "joke":format!("Шутка только про {name}."),
                "llm_used":true,
                "latest":{"language":"Rust","stars":1234}
            })
        })
        .collect::<Vec<_>>();
    json!({
        "format_version":3,
        "edition":edition,
        "generated_at":"2026-11-14T21:30:00Z",
        "llm_used":true,
        "entries":entries
    })
}

#[test]
fn each_repository_and_its_complete_review_have_their_own_message() {
    for edition in ["daily", "weekly"] {
        let digest = three_projects(edition);
        let entries = digest["entries"].as_array().unwrap();
        let messages = digest_publication_text(&digest).unwrap();
        assert_eq!(
            messages.len(),
            3,
            "{edition} must send three separate posts"
        );

        for (index, message) in messages.iter().enumerate() {
            telegram_format::validate_message(message, ISSUE_CHUNK_LIMIT).unwrap();
            let entry = &entries[index];
            let name = entry["full_name"].as_str().unwrap();
            assert!(message.contains(&format!("[{name}](https://github.com/{name})")));
            assert!(message.contains(&markdown_v2_escape(entry["description"].as_str().unwrap())));
            assert!(message.contains(&markdown_v2_escape(entry["joke"].as_str().unwrap())));
            for other in entries.iter().filter(|other| *other != entry) {
                assert!(!message.contains(other["full_name"].as_str().unwrap()));
                assert!(
                    !message.contains(&markdown_v2_escape(other["description"].as_str().unwrap()))
                );
                assert!(!message.contains(&markdown_v2_escape(other["joke"].as_str().unwrap())));
            }
        }
    }
}

#[test]
fn preview_payloads_are_sent_separately_and_resume_after_the_acknowledged_repository() {
    let directory = tempfile::tempdir().unwrap();
    let digest = three_projects("daily");
    save_preview_files(directory.path(), &digest, false).unwrap();
    let messages: Vec<String> =
        serde_json::from_slice(&fs::read(directory.path().join("telegram-messages.json")).unwrap())
            .unwrap();
    assert_eq!(messages.len(), 3);
    let audit: Value =
        serde_json::from_slice(&fs::read(directory.path().join("release-audit.json")).unwrap())
            .unwrap();
    assert_eq!(
        audit["telegram_format"]["message_utf16_units"],
        json!(
            messages
                .iter()
                .map(|message| message.encode_utf16().count())
                .collect::<Vec<_>>()
        )
    );

    let mut state = State::default();
    let mut attempted = Vec::new();
    let mut checkpoint = Vec::new();
    let error = delivery::send_chunks(
        &messages,
        "chat:separate-projects",
        &mut state,
        |text| {
            attempted.push(text.to_string());
            if attempted.len() == 2 {
                return Err(anyhow!(delivery::ConfirmedRejection { status: 429 }));
            }
            Ok(301)
        },
        |saved| {
            checkpoint = serde_json::to_vec(saved)?;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        error
            .downcast_ref::<delivery::ConfirmedRejection>()
            .is_some()
    );
    assert_eq!(attempted, messages[..2]);

    let mut restored: State = serde_json::from_slice(&checkpoint).unwrap();
    let mut resumed = Vec::new();
    let ids = delivery::send_chunks(
        &messages,
        "chat:separate-projects",
        &mut restored,
        |text| {
            resumed.push(text.to_string());
            Ok(301 + i64::try_from(resumed.len()).unwrap())
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(resumed, messages[1..]);
    assert_eq!(ids, [301, 302, 303]);
    assert_eq!(
        delivery::send_chunks(
            &messages,
            "chat:separate-projects",
            &mut restored,
            |_| panic!("a confirmed repository message must not be sent again"),
            |_| panic!("confirmed messages require no new checkpoint"),
        )
        .unwrap(),
        ids
    );
}

#[test]
fn saved_grouped_delivery_is_rejected_before_any_new_message_or_checkpoint() {
    let digest = three_projects("daily");
    let messages = digest_publication_text(&digest).unwrap();
    assert_eq!(messages.len(), 3);
    let grouped = digest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(issue_item)
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut state: State = serde_json::from_value(json!({
        "delivery_progress":{
            "chat:format-transition":{
                "project_message_ids":{"chunk-00":301},
                "message_texts":{"chunk-00":grouped}
            }
        }
    }))
    .unwrap();
    let before = serde_json::to_value(&state).unwrap();

    let error = delivery::send_chunks(
        &messages,
        "chat:format-transition",
        &mut state,
        |_| panic!("a changed publication must be rejected before sending"),
        |_| panic!("a rejected publication must not change its checkpoint"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("delivery content changed"));
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn an_oversized_later_review_blocks_the_whole_preview_without_shortening_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut digest = three_projects("daily");
    let review = "😀".repeat(1900);
    digest["entries"][2]["description"] = json!(review);

    let error = save_preview_files(directory.path(), &digest, false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("complete project card exceeds"), "{error}");
    assert!(!directory.path().join("telegram-messages.json").exists());
    assert_eq!(digest["entries"][2]["description"], review);
}
