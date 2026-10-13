use crate::State;
use anyhow::Result;

/// Resume only after the last locally persisted successful send. The transport
/// and checkpoint callbacks make partial delivery failures testable offline.
pub fn send_chunks(
    chunks: &[String],
    progress_key: &str,
    state: &mut State,
    mut send: impl FnMut(&str) -> Result<i64>,
    mut checkpoint: impl FnMut(&State) -> Result<()>,
) -> Result<Vec<i64>> {
    let mut ids = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        let key = format!("chunk-{index:02}");
        let already = state
            .delivery_progress
            .get(progress_key)
            .and_then(|progress| progress.project_message_ids.get(&key).copied());
        let id = match already {
            Some(id) => id,
            None => {
                let id = send(chunk)?;
                state
                    .delivery_progress
                    .entry(progress_key.to_string())
                    .or_default()
                    .project_message_ids
                    .insert(key, id);
                checkpoint(state)?;
                id
            }
        };
        ids.push(id);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn retry_continues_after_persisted_chunk_without_sending_it_again() {
        let chunks = vec!["first".into(), "second".into(), "third".into()];
        let mut state = State::default();
        let mut saved = Vec::new();
        assert!(
            send_chunks(
                &chunks,
                "issue",
                &mut state,
                |text| if text == "first" {
                    Ok(41)
                } else {
                    bail!("transport failed")
                },
                |state| {
                    saved = serde_json::to_vec(state)?;
                    Ok(())
                }
            )
            .is_err()
        );
        let mut restored = serde_json::from_slice(&saved).unwrap();
        let mut sent = Vec::new();
        let ids = send_chunks(
            &chunks,
            "issue",
            &mut restored,
            |text| {
                sent.push(text.to_string());
                Ok(41 + sent.len() as i64)
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(sent, ["second", "third"]);
        assert_eq!(ids, [41, 42, 43]);
        send_chunks(
            &chunks,
            "issue",
            &mut restored,
            |_| panic!("already delivered"),
            |_| panic!("no new checkpoint"),
        )
        .unwrap();
    }

    #[test]
    fn checkpoint_failure_stops_before_next_send() {
        let mut sends = 0;
        assert!(
            send_chunks(
                &["a".into(), "b".into()],
                "issue",
                &mut State::default(),
                |_| {
                    sends += 1;
                    Ok(1)
                },
                |_| bail!("disk full")
            )
            .is_err()
        );
        assert_eq!(sends, 1);
    }
}
