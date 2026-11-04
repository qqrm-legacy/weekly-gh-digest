# Production releases

`Run Bot` uses the latest stable GitHub Release for every mode: collection,
preview, delivery, recovery, stored-preview refresh, and delivery retry. Its
schedule and publication gates are unchanged. `Weekly Recovery` delegates to
`Run Bot` and uses the same released executable.

## From a main commit to production

Every push to `main`, including a merged pull request, starts `Release`. The
release waits for successful checks on that exact commit: `checks`, `ci-gate`,
`dependency-policy`, `actionlint`, `zizmor`, and `workflow-security-gate`. A failed
or missing check prevents publication.

The release builds on `ubuntu-24.04` for `x86_64-unknown-linux-gnu`, then publishes
a stable GitHub Release with these assets:

- `weekly-gh-digest-x86_64-unknown-linux-gnu.tar.gz`: executable
  `weekly-gh-digest` and `release.json`, which records the version, tag, commit,
  and target.
- `SHA256SUMS`: the archive's SHA-256 checksum.

The version is `<Cargo major>.<Cargo minor>.<Release run number>`; the tag adds
`v`. For example, a `0.2.0` package and release run 42 produce version `0.2.42`
and tag `v0.2.42`. Stable tags use `vMAJOR.MINOR.PATCH`. The workflow supplies
`WEEKLY_DIGEST_VERSION` and
`WEEKLY_DIGEST_COMMIT` at build time, so `weekly-gh-digest --version` identifies
the executable independently of the current checkout. Local development builds
use the Cargo package version and report an unknown commit unless those build
variables are supplied.

Builds and main-branch CI runs are isolated by commit, so a later push does not
cancel the checks needed by an earlier release. Publication uses GitHub's automatic
semantic-version selection for Latest; a release already known to be older is
published without changing Latest. Parallel builds do not force an older version
over a newer one.

A merge does not immediately change a running bot or publish a Telegram message.
After publication, the next `Run Bot` run downloads the version marked Latest.
An already running job keeps its selected version. Failed checks or builds before
publication leave the previous Latest unchanged.

After publication, `verify-release` downloads the released assets and runs offline
smoke checks. Its failure prevents a successful workflow status, but does not
automatically change Latest or roll back the published release.

For delayed releases of older commits, GitHub may reject creation or editing if
the target's workflow files differ from the current default branch. That operation
requires `Workflows: write`, which the built-in `GITHUB_TOKEN` cannot receive.
Such runs fail explicitly and need an appropriately authorized operator; the
workflow does not change the requested commit to bypass the restriction. See the
[GitHub release API permissions](https://docs.github.com/en/rest/releases/releases#create-a-release).

## Verified runtime

`Run Bot` also uses `ubuntu-24.04`. It resolves the stable release tag to a full
commit SHA and checks out that commit for the matching runtime scripts. Downloads
use the already selected asset IDs. Before restoring state or running the bot, the
installer verifies the archive checksum, manifest, and executable identity from
`--version`. The binary and state/routing helpers use the same source revision.

The workflow performs no Cargo build and has no source-build fallback. Missing
assets, invalid metadata, or checksum failures stop the run. Before the first
valid release exists, production runs cannot start the bot; finish or repair the
release pipeline first.

The executable reads and writes `data/` relative to its working directory.
Its installation directory and the release builder's original checkout path do
not determine the state directory. `--version` does not read state or credentials.

## Rolling back

Use a previously verified stable release that contains both assets and is compatible
with the current checkpoint schema. Mark that release as Latest in GitHub, then
start `Run Bot` with the normal dry-run preview before resuming publication.
The next run follows Latest; no rebuild or source checkout change is required.
Preserve the current checkpoint, pending payloads, and acknowledgements.

A later successful `main` release becomes the new Latest and resumes forward
updates. If publication must remain stopped while investigating a version, set
`DIGEST_DELIVERY_ENABLED=false`; release creation does not alter that variable.

## Source validation scope

CI and the release builder compile source by design. Local `preview-retained`
examples and `Evening digest validation` also build the selected checkout to
validate unshipped changes; the evening workflow produces a free-provider preview
without Telegram access or production state writes. For a preview of the actual
production version, use `Run Bot`.

`Digest technical-chat trial` is a separate, bounded acceptance
test of its named development branch, with isolated state and a fixed sending
deadline. It is not a production release consumer. `Production readiness` is a
read-only configuration diagnostic and does not run a Rust executable.
