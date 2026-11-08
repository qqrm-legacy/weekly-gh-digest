use crate::State;
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

/// Only a structured Telegram client-error response proves rejection. Transport
/// failures and invalid/server responses must never use this marker.
#[derive(Debug)]
pub(super) struct ConfirmedRejection {
    pub(super) status: u16,
}

impl std::fmt::Display for ConfirmedRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Telegram sendMessage rejected the message (HTTP {})",
            self.status
        )
    }
}

impl std::error::Error for ConfirmedRejection {}

/// An ambiguous send leaves a durable intent and requires reconciliation.
/// A confirmed rejection clears that intent durably before permitting a retry.
/// A successful acknowledgement and its cursor are checkpointed together.
pub fn send_one(
    text: &str,
    progress_key: &str,
    message_key: &str,
    state: &mut State,
    mut send: impl FnMut(&str) -> Result<i64>,
    mut checkpoint: impl FnMut(&State) -> Result<()>,
) -> Result<i64> {
    ensure!(
        state.uncertain_delivery.is_none(),
        "unconfirmed Telegram delivery; operator reconciliation required"
    );
    let progress = state
        .delivery_progress
        .entry(progress_key.into())
        .or_default();
    if let Some(previous) = progress.message_texts.get(message_key) {
        ensure!(
            previous == text,
            "delivery content changed; operator reconciliation required"
        );
    }
    if let Some(id) = progress.project_message_ids.get(message_key) {
        return Ok(*id);
    }
    progress
        .message_texts
        .insert(message_key.into(), text.into());
    state.uncertain_delivery = Some(format!("{progress_key}/{message_key}"));
    checkpoint(state)?;
    let id = match send(text) {
        Ok(id) => id,
        Err(error) => {
            if error.downcast_ref::<ConfirmedRejection>().is_some() {
                let intent = state.uncertain_delivery.take();
                if let Err(checkpoint_error) = checkpoint(state) {
                    // A failed clear must also block another call in this process.
                    // The remote checkpoint may still contain the original intent.
                    state.uncertain_delivery = intent;
                    return Err(checkpoint_error).context(
                        "cannot checkpoint rejected Telegram send; reconciliation required",
                    );
                }
            }
            return Err(error);
        }
    };
    state
        .delivery_progress
        .get_mut(progress_key)
        .unwrap()
        .project_message_ids
        .insert(message_key.into(), id);
    state.uncertain_delivery = None;
    checkpoint(state)?;
    Ok(id)
}

/// Validate every stored receipt and text against the complete intended payload.
/// This is read-only and can also guard a saved technical-preview receipt.
pub fn validate_chunks(chunks: &[String], progress_key: &str, state: &State) -> Result<()> {
    ensure!(
        state.uncertain_delivery.is_none(),
        "unconfirmed Telegram delivery; operator reconciliation required"
    );
    // Check the complete payload before sending anything. A shorter or changed
    // renderer must not strand receipts or emit earlier chunks before a mismatch.
    if let Some(progress) = state.delivery_progress.get(progress_key) {
        let intended: BTreeMap<_, _> = chunks
            .iter()
            .enumerate()
            .map(|(index, text)| (format!("chunk-{index:02}"), text))
            .collect();
        ensure!(
            progress.overview_message_id.is_none(),
            "legacy summary progress requires operator reconciliation"
        );
        for key in progress.project_message_ids.keys() {
            ensure!(
                intended.contains_key(key) && progress.message_texts.contains_key(key),
                "delivery receipts do not match the intended chunks; operator reconciliation required"
            );
        }
        for (key, previous) in &progress.message_texts {
            ensure!(
                intended.get(key).is_some_and(|text| *text == previous),
                "delivery content changed; operator reconciliation required"
            );
        }
    }
    Ok(())
}

pub fn send_chunks(
    chunks: &[String],
    progress_key: &str,
    state: &mut State,
    mut send: impl FnMut(&str) -> Result<i64>,
    mut checkpoint: impl FnMut(&State) -> Result<()>,
) -> Result<Vec<i64>> {
    validate_chunks(chunks, progress_key, state)?;
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            send_one(
                chunk,
                progress_key,
                &format!("chunk-{index:02}"),
                state,
                &mut send,
                &mut checkpoint,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn failed_intent_checkpoint_prevents_send() {
        assert!(
            send_chunks(
                &["a".into()],
                "issue",
                &mut State::default(),
                |_| panic!("send before durable intent"),
                |_| bail!("offline")
            )
            .is_err()
        );
    }

    #[test]
    fn confirmed_rejection_preserves_payload_and_resumes_after_acknowledged_chunks() {
        let chunks = vec!["first".into(), "second".into()];
        let mut state = State::default();
        state.pending_digests.insert(
            "issue".into(),
            serde_json::json!({"edition":"daily", "entries":["frozen payload"]}),
        );
        let pending = state.pending_digests.clone();
        let mut remote = Vec::new();
        let error = send_chunks(
            &chunks,
            "issue",
            &mut state,
            |text| {
                if text == "first" {
                    Ok(41)
                } else {
                    Err(ConfirmedRejection { status: 429 }.into())
                }
            },
            |state| {
                remote = serde_json::to_vec(state)?;
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.downcast_ref::<ConfirmedRejection>().is_some());
        let mut restored: State = serde_json::from_slice(&remote).unwrap();
        assert!(restored.uncertain_delivery.is_none());
        assert_eq!(restored.pending_digests, pending);
        assert!(restored.sent_slots.is_empty());
        let progress = &restored.delivery_progress["issue"];
        assert_eq!(progress.project_message_ids.len(), 1);
        assert_eq!(progress.project_message_ids["chunk-00"], 41);
        assert_eq!(progress.message_texts["chunk-00"], "first");
        assert_eq!(progress.message_texts["chunk-01"], "second");
        let ids = send_chunks(
            &chunks,
            "issue",
            &mut restored,
            |text| {
                assert_eq!(text, "second");
                Ok(42)
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(ids, [41, 42]);
        assert_eq!(restored.pending_digests, pending);
    }

    #[test]
    fn failed_rejection_checkpoint_blocks_local_and_restored_retries() {
        let mut state = State::default();
        let mut remote = Vec::new();
        let mut checkpoints = 0;
        let error = send_chunks(
            &["card".into()],
            "issue",
            &mut state,
            |_| Err(ConfirmedRejection { status: 400 }.into()),
            |state| {
                checkpoints += 1;
                if checkpoints == 2 {
                    bail!("checkpoint unavailable");
                }
                remote = serde_json::to_vec(state)?;
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("reconciliation required"));
        let restored = serde_json::from_slice(&remote).unwrap();
        for mut blocked in [state, restored] {
            assert_eq!(
                blocked.uncertain_delivery.as_deref(),
                Some("issue/chunk-00")
            );
            assert!(
                blocked.delivery_progress["issue"]
                    .project_message_ids
                    .is_empty()
            );
            assert_eq!(
                blocked.delivery_progress["issue"].message_texts["chunk-00"],
                "card"
            );
            assert!(
                send_chunks(
                    &["card".into()],
                    "issue",
                    &mut blocked,
                    |_| panic!("failed clear checkpoint must block sending"),
                    |_| panic!("failed clear checkpoint must require reconciliation")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn ambiguous_send_blocks_restart_even_after_previous_chunk_succeeded() {
        let chunks = vec!["first".into(), "second".into()];
        let mut state = State::default();
        let mut remote = Vec::new();
        assert!(
            send_chunks(
                &chunks,
                "issue",
                &mut state,
                |text| if text == "first" {
                    Ok(41)
                } else {
                    bail!("timeout")
                },
                |state| {
                    remote = serde_json::to_vec(state)?;
                    Ok(())
                }
            )
            .is_err()
        );
        let mut restored: State = serde_json::from_slice(&remote).unwrap();
        assert_eq!(
            restored.uncertain_delivery.as_deref(),
            Some("issue/chunk-01")
        );
        assert!(
            send_chunks(
                &chunks,
                "issue",
                &mut restored,
                |_| panic!("ambiguous send must not retry"),
                |_| Ok(())
            )
            .is_err()
        );
        // Only after the operator verified that the second message was absent.
        restored.uncertain_delivery = None;
        let ids = send_chunks(
            &chunks,
            "issue",
            &mut restored,
            |text| {
                assert_eq!(text, "second");
                Ok(42)
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(ids, [41, 42]);
    }

    #[test]
    fn failed_ack_checkpoint_restores_uncertain_intent_without_repeating_send() {
        let mut remote = Vec::new();
        let mut checkpoints = 0;
        assert!(
            send_chunks(
                &["a".into()],
                "issue",
                &mut State::default(),
                |_| Ok(41),
                |state| {
                    checkpoints += 1;
                    if checkpoints == 2 {
                        bail!("runner lost after Telegram ack")
                    }
                    remote = serde_json::to_vec(state)?;
                    Ok(())
                }
            )
            .is_err()
        );
        let mut restored = serde_json::from_slice(&remote).unwrap();
        assert!(
            send_chunks(
                &["a".into()],
                "issue",
                &mut restored,
                |_| panic!("must reconcile"),
                |_| Ok(())
            )
            .is_err()
        );
    }

    #[test]
    fn acknowledged_chunks_resume_and_changed_content_is_rejected() {
        let mut state = State::default();
        send_chunks(&["a".into()], "issue", &mut state, |_| Ok(41), |_| Ok(())).unwrap();
        let mut restored = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            send_chunks(
                &["a".into(), "b".into()],
                "issue",
                &mut restored,
                |text| {
                    assert_eq!(text, "b");
                    Ok(42)
                },
                |_| Ok(())
            )
            .unwrap(),
            [41, 42]
        );
        assert!(
            send_chunks(
                &["changed".into()],
                "issue",
                &mut restored,
                |_| panic!("content changed"),
                |_| Ok(())
            )
            .is_err()
        );
    }

    #[test]
    fn shorter_payload_cannot_discard_acknowledged_chunks() {
        let mut state = State::default();
        send_chunks(
            &["first".into(), "second".into()],
            "issue",
            &mut state,
            |_| Ok(41),
            |_| Ok(()),
        )
        .unwrap();
        let before = serde_json::to_value(&state).unwrap();
        for chunks in [vec!["first".into()], Vec::new()] {
            assert!(
                send_chunks(
                    &chunks,
                    "issue",
                    &mut state,
                    |_| panic!("shortened payload must not send"),
                    |_| panic!("shortened payload must not change checkpoints")
                )
                .is_err()
            );
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
    }

    #[test]
    fn a_later_mismatch_is_detected_before_an_earlier_new_chunk_is_sent() {
        let mut state = State::default();
        send_one(
            "original second",
            "issue",
            "chunk-01",
            &mut state,
            |_| Ok(42),
            |_| Ok(()),
        )
        .unwrap();
        let before = serde_json::to_value(&state).unwrap();
        assert!(
            send_chunks(
                &["new first".into(), "changed second".into()],
                "issue",
                &mut state,
                |_| panic!("complete preflight must precede the first send"),
                |_| panic!("complete preflight must precede checkpoint writes")
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn legacy_summary_or_receipts_without_content_require_reconciliation() {
        for progress in [
            serde_json::json!({"overview_message_id":43}),
            serde_json::json!({"project_message_ids":{"overview":43},
                "message_texts":{"overview":"Legacy summary"}}),
            serde_json::json!({"message_texts":{"overview":"Unsent legacy summary"}}),
            serde_json::json!({"project_message_ids":{"chunk-00":41}}),
        ] {
            let mut state: State = serde_json::from_value(serde_json::json!({
                "delivery_progress":{"issue":progress}
            }))
            .unwrap();
            let before = serde_json::to_value(&state).unwrap();
            assert!(
                send_chunks(
                    &["card".into()],
                    "issue",
                    &mut state,
                    |_| panic!("legacy receipts must not trigger a send"),
                    |_| panic!("legacy receipts must stay unchanged")
                )
                .is_err()
            );
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
    }
}
