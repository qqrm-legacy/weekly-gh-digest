//! Explicit, expiring editorial follow-up in the isolated technical chat only.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

const REVISION: &str = "test-editorial-summary-2026-10-31-v2";

pub(super) fn run(args: &[String]) -> Result<()> {
    ensure!(
        env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true"),
        "editorial trial requires test-only mode"
    );
    ensure!(
        env::var("DIGEST_DELIVERY_ENABLED").as_deref() == Ok("true"),
        "publication is disabled"
    );
    ensure!(
        env::var("DELIVERY_CHECKPOINT")
            .ok()
            .is_some_and(|value| value.ends_with("soak_test.py")),
        "editorial trial requires its isolated checkpoint helper"
    );
    let chat = env::var("TELEGRAM_TECH_CHAT_ID")?;
    crate::test_delivery::guard(&chat)?;
    let directory = args
        .get(1)
        .map(PathBuf::from)
        .context("test-summary needs a stored source directory")?;
    let reply_id: i64 = args
        .get(2)
        .context("test-summary needs an acknowledged parent message")?
        .parse()?;
    let mut state = crate::read_state_strict()?;
    ensure!(
        state.uncertain_delivery.is_none(),
        "uncertain test delivery requires reconciliation"
    );
    ensure!(
        state
            .sent_slots
            .iter()
            .any(|(slot, id)| slot.starts_with("weekly:") && *id == reply_id),
        "summary parent is not an acknowledged weekly test message"
    );
    if state.sent_digests.contains_key(REVISION) {
        println!("Editorial follow-up already acknowledged; no provider calls or sends");
        return Ok(());
    }
    let key = format!("{chat}:{REVISION}");
    let saved = state
        .delivery_progress
        .get(&key)
        .and_then(|progress| progress.message_texts.get("overview"))
        .cloned();
    let text = if let Some(text) = saved {
        text
    } else {
        let mut digest: Value =
            serde_json::from_slice(&fs::read(directory.join("ranking-input.json"))?)?;
        crate::validate_digest_selection(&directory, &digest)?;
        let entries = digest["entries"]
            .as_array()
            .context("stored digest lacks entries")?
            .iter()
            .map(|entry| {
                Ok(crate::Ranked {
                    snapshot: serde_json::from_value(entry["latest"].clone())?,
                    baseline: None,
                    delta: 0,
                    period: String::new(),
                    trending_source: false,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            !entries.is_empty() && entries.len() <= 30,
            "invalid stored editorial selection"
        );
        let path = crate::state_path();
        let clock = crate::clock::SystemClock;
        let overview = crate::providers::Groq::from_env(&path, &clock)?
            .rewrite_overview(&entries, &mut state, &directory)?;
        digest["overview"] = json!(overview);
        digest["llm_requested"] = json!(true);
        crate::editorial::require_synthesis(&digest)?;
        crate::write_json(
            &directory.join("editorial-revision.json"),
            &json!({
                "revision":REVISION,"overview":overview,"source":"stored primary metadata and README, not previous generated cards",
                "factual_review":"manual follow-up required","changes_repository_receipts":false
            }),
        )?;
        let corrections = "Уточнения к первым тестовым карточкам: в Impeccable без LLM работает отдельный детектор, а не все команды для код-агента. Local-first в Orkas не означает отсутствие обращений к облачным моделям. Эти формулировки в прежних карточках были слишком широкими.";
        format!(
            "*ТЕСТ: обновлённое саммари*\n\n{}\n\n{}\n\n{}",
            crate::markdown_v2_escape(&overview),
            crate::markdown_v2_escape(corrections),
            crate::markdown_v2_escape(
                "Это редакторская проверка находок 31 октября, не итоги полной недели. Ссылки и карточки — в выпуске, на который дан ответ. Описание возможностей основано на материалах авторов; проекты не запускались."
            )
        )
    };
    crate::telegram_format::validate_message(&text, crate::ISSUE_CHUNK_LIMIT)?;
    ensure!(
        crate::credential_markers(&text).is_empty(),
        "editorial summary failed credential scan"
    );
    let id = crate::delivery::send_one(
        &text,
        &key,
        "overview",
        &mut state,
        |text| crate::telegram_send_reply_to(&chat, Some(reply_id), text),
        |state| crate::write_json(&crate::state_path(), state),
    )?;
    state.sent_digests.insert(REVISION.to_string(), id);
    crate::write_json(&crate::state_path(), &state)?;
    crate::write_json(
        &directory.join("editorial-ack.json"),
        &json!({
            "revision":REVISION,"message_id":id,"reply_to":reply_id,"telegram_acknowledged":true,
            "production_delivery":false,"repository_queues_unchanged":true
        }),
    )?;
    println!("Editorial test summary acknowledged as message {id}");
    Ok(())
}
