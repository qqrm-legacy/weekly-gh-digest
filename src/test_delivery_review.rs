//! Retired summary-only trial entry point. Refuse before reading state or sources.
use anyhow::{Result, bail, ensure};
use std::env;

pub(super) fn run(_args: &[String]) -> Result<()> {
    ensure!(
        env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true"),
        "editorial trial requires test-only mode"
    );
    ensure!(
        env::var("DIGEST_DELIVERY_ENABLED").as_deref() == Ok("true"),
        "publication is disabled"
    );
    bail!(
        "test-summary is retired: digest summaries are no longer generated or sent; \
         use preview-stored <run-directory> for an offline preview"
    )
}
