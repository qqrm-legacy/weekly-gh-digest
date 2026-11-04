"""Release gates, immutable retries, ordering and the actual packaged CLI boundary."""

import copy
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import publish_release as release


COMMIT = "a" * 40


def successful_checks(commit=COMMIT):
    return [{"name": name, "id": index + 1, "head_sha": commit,
             "app": {"slug": "github-actions"}, "status": "completed", "conclusion": "success"}
            for index, name in enumerate(release.REQUIRED_CHECKS)]


class FakeGitHub:
    """Stateful gh boundary: every mutation is recorded and uploads cannot overwrite."""

    def __init__(self, metadata):
        self.metadata = metadata
        self.check_results = [successful_checks(metadata["commit"])]
        self.records = []
        self.refs = {}
        self.mutations = []
        self.latest = None
        self.before_publish = None
        self.read_ids = []
        self.read_results = []

    def check_runs(self, commit):
        if len(self.check_results) > 1:
            return copy.deepcopy(self.check_results.pop(0))
        return copy.deepcopy(self.check_results[0])

    def releases(self):
        return copy.deepcopy(self.records)

    def release_by_id(self, identifier):
        self.read_ids.append(identifier)
        if self.read_results:
            result = self.read_results.pop(0)
            if isinstance(result, Exception):
                raise result
            return copy.deepcopy(result)
        return copy.deepcopy(next(record for record in self.records if record["id"] == identifier))

    def tag_ref(self, tag):
        return copy.deepcopy(self.refs.get(tag))

    def upload_asset(self, identifier, path):
        path = Path(path)
        self.mutations.append(("upload", identifier, (str(path),)))
        current = next(record for record in self.records if record["id"] == identifier)
        if not current["draft"]:
            raise AssertionError("Assets can only be appended to a draft")
        if any(asset["name"] == path.name for asset in current["assets"]):
            raise AssertionError("An existing asset must never be overwritten")
        asset = {"name": path.name, "state": "uploaded", "digest": f"sha256:{release.sha256(path)}"}
        current["assets"].append(asset)
        return copy.deepcopy(asset)

    def api(self, endpoint, *arguments):
        if endpoint == "releases":
            if arguments[:3] != ("--method", "POST", "--input") or len(arguments) != 4:
                raise AssertionError("Draft creation must use the REST response id")
            payload = json.loads(Path(arguments[3]).read_text())
            tag = payload["tag_name"]
            self.mutations.append(("create", tag, arguments))
            if (release.find_release(self.records, tag) is not None or payload["draft"] is not True
                    or payload["prerelease"] is not False or payload["make_latest"] != "false"):
                raise AssertionError("Must create a new unpublished draft")
            if self.metadata["commit"] not in payload["body"]:
                raise AssertionError("Notes must identify the full commit")
            created = {"id": len(self.records) + 1, "tag_name": tag, "draft": True,
                       "prerelease": False, "assets": [], "target_commitish": payload["target_commitish"]}
            self.records.append(created)
            return copy.deepcopy(created)
        self.mutations.append(("patch", endpoint, arguments))
        if arguments[:5] != ("--method", "PATCH", "-F", "draft=false", "-f"):
            raise AssertionError("Publication must use PATCH with a boolean draft=false field")
        if len(arguments) != 6 or arguments[5] not in {"make_latest=legacy", "make_latest=false"}:
            raise AssertionError("Publication must never force latest=true")
        current = next(record for record in self.records if endpoint == f"releases/{record['id']}")
        if not current["draft"]:
            raise AssertionError("Only a prepared draft can be published")
        if {asset["name"] for asset in current["assets"]} != set(release.ASSETS):
            raise AssertionError("Both assets must be uploaded before publishing")
        if self.before_publish is not None:
            self.before_publish()
        current["draft"] = False
        tag = current["tag_name"]
        self.refs[tag] = {"ref": f"refs/tags/{tag}",
                          "object": {"type": "commit", "sha": current["target_commitish"]}}
        if arguments[5] == "make_latest=legacy":
            stable = [record["tag_name"] for record in self.records
                      if not record["draft"] and not record["prerelease"]]
            self.latest = max(stable, key=lambda value: tuple(map(int, value[1:].split("."))))
        return copy.deepcopy(current)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.metadata = release.release_metadata("0.2.0", "7", COMMIT)
        self.binary = self.root / release.BINARY
        self.binary.write_text(
            "#!/usr/bin/env python3\n"
            "import os, pathlib, sys\n"
            "assert pathlib.Path.cwd().name == 'outside-checkout'\n"
            "assert not any(key.startswith('TELEGRAM_') for key in os.environ)\n"
            "assert not any(key in os.environ for key in ('GH_TOKEN', 'GITHUB_TOKEN', 'GROQ_API_KEY'))\n"
            f"if sys.argv[1:] == ['--version']: print('weekly-gh-digest 0.2.7 ({COMMIT})')\n"
            "else:\n"
            "    print('publication is disabled pending preview review', file=sys.stderr)\n"
            "    sys.exit(1)\n",
            encoding="utf-8",
        )
        self.binary.chmod(0o755)
        self.assets = self.root / "assets"
        release.package_release(self.binary, self.assets, self.metadata)
        self.github = FakeGitHub(self.metadata)

    def rewrite_archive(self, entries):
        with tarfile.open(self.assets / release.ARCHIVE, "w:gz") as archive:
            for name, contents, mode, kind in entries:
                member = tarfile.TarInfo(name)
                member.mode, member.size, member.type = mode, len(contents), kind
                archive.addfile(member, io.BytesIO(contents))
        digest = release.sha256(self.assets / release.ARCHIVE)
        (self.assets / release.CHECKSUMS).write_text(f"{digest}  {release.ARCHIVE}\n")

    def release_snapshot(self, draft=True, complete=True):
        return {"id": 1, "tag_name": self.metadata["tag"], "target_commitish": COMMIT,
                "draft": draft, "prerelease": False,
                "assets": [{"name": name, "state": "uploaded",
                            "digest": f"sha256:{release.sha256(self.assets / name)}"}
                           for name in release.ASSETS] if complete else []}

    def test_metadata_uses_major_minor_and_run_number_not_package_patch(self):
        self.assertEqual(release.release_metadata("0.2.99", "123", COMMIT),
                         {"version": "0.2.123", "tag": "v0.2.123", "commit": COMMIT,
                          "target": "x86_64-unknown-linux-gnu"})
        for version, run, commit in [("0.2.0-rc1", "7", COMMIT), ("0.2.0", "0", COMMIT),
                                     ("0.2.0", "1\nunsafe", COMMIT), ("0.2.0", "7", "main")]:
            with self.subTest(version=version, run=run, commit=commit), self.assertRaises(RuntimeError):
                release.release_metadata(version, run, commit)

    def test_manual_branch_and_untrusted_events_cannot_publish(self):
        for ref, event in [("refs/heads/feature", "workflow_dispatch"),
                           ("refs/tags/v0.2.7", "push"), ("refs/heads/main", "pull_request")]:
            with self.subTest(ref=ref, event=event), patch.dict(
                    os.environ, {"GITHUB_REF": ref, "GITHUB_EVENT_NAME": event}, clear=True):
                with self.assertRaises(RuntimeError):
                    release.workflow_metadata()

    def test_archive_is_deterministic_and_contains_only_consumer_contract(self):
        second = self.root / "another-location"
        os.utime(self.binary, (1, 1))
        release.package_release(self.binary, second, self.metadata)
        self.assertEqual((self.assets / release.ARCHIVE).read_bytes(),
                         (second / release.ARCHIVE).read_bytes())
        with tarfile.open(self.assets / release.ARCHIVE, "r:gz") as archive:
            self.assertEqual(archive.getnames(), [release.BINARY, "release.json"])
            self.assertTrue(all(member.isfile() for member in archive.getmembers()))
            self.assertEqual(archive.getmember(release.BINARY).mode, 0o755)
            self.assertEqual(json.load(archive.extractfile("release.json")), self.metadata)

    def test_archived_binary_runs_in_new_cwd_without_credentials(self):
        with patch.dict(os.environ, {"GH_TOKEN": "fixture", "GITHUB_TOKEN": "fixture",
                                     "GROQ_API_KEY": "fixture", "TELEGRAM_BOT_TOKEN": "fixture"}):
            release.verify_archive_runtime(self.assets, self.metadata)

    def test_wrong_binary_version_fails_before_publication(self):
        self.binary.write_text(self.binary.read_text().replace("0.2.7", "0.2.6"))
        release.package_release(self.binary, self.assets, self.metadata)
        with self.assertRaisesRegex(RuntimeError, "--version"):
            release.verify_archive_runtime(self.assets, self.metadata)

    def test_corrupt_checksum_prevents_any_remote_mutation(self):
        (self.assets / release.CHECKSUMS).write_text("0" * 64 + f"  {release.ARCHIVE}\n")
        with self.assertRaisesRegex(RuntimeError, "SHA256SUMS"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_archive_rejects_other_commit_paths_and_nonregular_members(self):
        manifest = json.dumps(self.metadata).encode()
        variants = [
            [("../weekly-gh-digest", b"binary", 0o755, tarfile.REGTYPE),
             ("release.json", manifest, 0o644, tarfile.REGTYPE)],
            [(release.BINARY, b"", 0o755, tarfile.SYMTYPE),
             ("release.json", manifest, 0o644, tarfile.REGTYPE)],
            [(release.BINARY, b"binary", 0o644, tarfile.REGTYPE),
             ("release.json", manifest, 0o644, tarfile.REGTYPE)],
            [(release.BINARY, b"binary", 0o755, tarfile.REGTYPE),
             ("release.json", json.dumps({**self.metadata, "commit": "b" * 40}).encode(),
              0o644, tarfile.REGTYPE)],
        ]
        for entries in variants:
            with self.subTest(entries=entries):
                self.rewrite_archive(entries)
                with self.assertRaises(RuntimeError):
                    release.inspect_assets(self.assets, self.metadata)

    def test_only_all_six_successes_on_exact_sha_from_actions_are_accepted(self):
        checks = successful_checks()
        self.assertEqual(release.pending_checks(checks, COMMIT), [])
        self.assertEqual(release.pending_checks(checks[:-1], COMMIT), ["workflow-security-gate"])
        self.assertEqual(set(release.pending_checks(successful_checks("b" * 40), COMMIT)),
                         set(release.REQUIRED_CHECKS))
        for check in checks:
            check["app"]["slug"] = "unrelated-app"
        self.assertEqual(set(release.pending_checks(checks, COMMIT)), set(release.REQUIRED_CHECKS))

    def test_newest_check_failure_cannot_be_hidden_by_older_success(self):
        checks = successful_checks()
        newer = {**checks[0], "id": 100, "conclusion": "failure"}
        for conclusion in ["failure", "cancelled", "skipped", "neutral", "timed_out", None]:
            with self.subTest(conclusion=conclusion), self.assertRaisesRegex(RuntimeError, "checks"):
                release.pending_checks([{**newer, "conclusion": conclusion}, *checks], COMMIT)

    def test_newer_running_check_replaces_old_failure_but_is_not_success(self):
        checks = successful_checks()
        checks[0]["conclusion"] = "failure"
        newer = {**checks[0], "id": 100, "status": "in_progress", "conclusion": None}
        self.assertEqual(release.pending_checks([newer, *checks], COMMIT), ["checks"])

    def test_wait_polls_missing_checks_until_the_exact_commit_succeeds(self):
        self.github.check_results = [successful_checks()[:-1], successful_checks()]
        with patch("publish_release.time.monotonic", side_effect=[0, 0]), patch(
                "publish_release.time.sleep") as sleep:
            release.wait_for_checks(self.github, COMMIT, timeout=1)
        sleep.assert_called_once_with(1)

    def test_check_failure_timeout_and_transport_error_all_fail_closed(self):
        self.github.check_results[0][0]["conclusion"] = "failure"
        with self.assertRaisesRegex(RuntimeError, "did not succeed"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.github.check_results = [[]]
        with patch("publish_release.time.monotonic", side_effect=[0, 1]), self.assertRaisesRegex(
                RuntimeError, "Timed out"):
            release.publish_release(self.github, self.assets, self.metadata, timeout=1)
        with patch.object(self.github, "check_runs", side_effect=RuntimeError("GitHub unavailable")):
            with self.assertRaisesRegex(RuntimeError, "GitHub unavailable"):
                release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_success_creates_draft_uploads_both_assets_then_publishes(self):
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual([action for action, _, _ in self.github.mutations],
                         ["create", "upload", "upload", "patch"])
        self.assertEqual(self.github.mutations[-1],
                         ("patch", "releases/1", ("--method", "PATCH", "-F", "draft=false",
                                                  "-f", "make_latest=legacy")))
        self.assertEqual(self.github.latest, "v0.2.7")
        self.assertFalse(self.github.records[0]["draft"])
        self.assertEqual(self.github.refs["v0.2.7"]["object"]["sha"], COMMIT)

    def test_release_collection_can_remain_stale_after_every_write(self):
        with patch.object(self.github, "releases", return_value=[]):
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.read_ids, [1, 1, 1])
        self.assertEqual([action for action, _, _ in self.github.mutations],
                         ["create", "upload", "upload", "patch"])
        self.assertFalse(self.github.records[0]["draft"])

    def test_delayed_id_asset_digest_and_published_reads_do_not_repeat_mutations(self):
        empty = self.release_snapshot(complete=False)
        starter = {**empty, "assets": [{"name": release.ARCHIVE, "state": "starter", "digest": None}]}
        pending_digest = self.release_snapshot()
        pending_digest["assets"][1]["digest"] = None
        self.github.read_results = [
            release.GitHubError("Not found", 404), empty,
            empty, starter, pending_digest, self.release_snapshot(),
            self.release_snapshot(), self.release_snapshot(draft=False),
        ]
        with patch("publish_release.time.sleep") as sleep:
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(sleep.call_count, 5)
        self.assertEqual(self.github.read_ids, [1] * 8)
        self.assertEqual([action for action, _, _ in self.github.mutations],
                         ["create", "upload", "upload", "patch"])

    def test_missing_id_read_times_out_without_recreating_or_uploading(self):
        self.github.read_results = [release.GitHubError("Not found", 404)]
        with patch("publish_release.time.monotonic", side_effect=[0, 0, 60]), patch(
                "publish_release.time.sleep") as sleep, self.assertRaisesRegex(RuntimeError, "Timed out"):
            release.publish_release(self.github, self.assets, self.metadata)
        sleep.assert_not_called()
        self.assertEqual([action for action, _, _ in self.github.mutations], ["create"])
        self.assertTrue(self.github.records[0]["draft"])

    def test_release_read_permissions_and_transport_errors_are_not_polled(self):
        hashes = release.inspect_assets(self.assets, self.metadata)
        for status in [401, 403, 500, None]:
            with self.subTest(status=status), patch("publish_release.time.sleep") as sleep:
                self.github.read_results = [release.GitHubError("Access or transport failure", status)]
                with self.assertRaisesRegex(release.GitHubError, "Access or transport failure"):
                    release.wait_for_release(self.github, 1, self.metadata, hashes)
                sleep.assert_not_called()

    def test_release_read_never_treats_invalid_identity_or_payload_as_pending(self):
        hashes = release.inspect_assets(self.assets, self.metadata)
        ready = self.release_snapshot()
        wrong_digest = copy.deepcopy(ready)
        wrong_digest["assets"][0]["digest"] = "sha256:" + "0" * 64
        invalid = [None, [], {**ready, "id": 2}, {**ready, "tag_name": "v0.2.8"},
                   {**ready, "target_commitish": "b" * 40}, {**ready, "assets": None},
                   {**ready, "assets": [{"name": "unrelated"}]}, wrong_digest]
        for payload in invalid:
            with self.subTest(payload=payload), patch("publish_release.time.sleep") as sleep:
                self.github.read_results = [payload]
                with self.assertRaises(RuntimeError):
                    release.wait_for_release(self.github, 1, self.metadata, hashes, require_assets=True)
                sleep.assert_not_called()

    def test_http_error_status_is_preserved_for_read_only_retry_decisions(self):
        client = release.GitHub("owner/repo")
        for status in [403, 404]:
            result = subprocess.CompletedProcess([], 1, "{}", f"gh: rejected (HTTP {status})\n")
            with self.subTest(status=status), patch("publish_release.subprocess.run", return_value=result):
                with self.assertRaises(release.GitHubError) as error:
                    client.release_by_id(123)
                self.assertEqual(error.exception.status, status)

    def test_upload_uses_known_id_and_raw_file_without_tag_discovery(self):
        client = release.GitHub("owner/repo")
        path = self.assets / release.ARCHIVE
        asset = {"name": path.name, "state": "uploaded", "digest": "sha256:" + release.sha256(path)}
        with patch.object(client, "command", return_value=json.dumps(asset)) as command:
            self.assertEqual(client.upload_asset(123, path), asset)
        command.assert_called_once_with(
            "api", f"https://uploads.github.com/repos/owner/repo/releases/123/assets?name={path.name}",
            "--method", "POST", "--input", str(path), "-H", "Content-Type: application/octet-stream",
            "-H", f"Content-Length: {path.stat().st_size}",
        )

    def test_failed_upload_is_not_retried_or_followed_by_publication(self):
        with patch.object(self.github, "upload_asset", side_effect=release.GitHubError(
                "Upload result unknown", 502)) as upload:
            with self.assertRaisesRegex(release.GitHubError, "Upload result unknown"):
                release.publish_release(self.github, self.assets, self.metadata)
        upload.assert_called_once()
        self.assertEqual([action for action, _, _ in self.github.mutations], ["create"])
        self.assertTrue(self.github.records[0]["draft"])

    def test_ambiguous_publish_result_is_read_on_rerun_without_repeating_patch(self):
        api = self.github.api

        def lose_publish_response(endpoint, *arguments):
            result = api(endpoint, *arguments)
            if "PATCH" in arguments:
                raise release.GitHubError("Connection lost after publication")
            return result

        with patch.object(self.github, "api", side_effect=lose_publish_response):
            with self.assertRaisesRegex(release.GitHubError, "Connection lost"):
                release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual([action for action, _, _ in self.github.mutations],
                         ["create", "upload", "upload", "patch"])
        self.github.mutations.clear()
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_identical_published_retry_is_noop_and_never_reuploads(self):
        release.publish_release(self.github, self.assets, self.metadata)
        self.github.mutations.clear()
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_different_or_missing_published_asset_cannot_be_replaced(self):
        release.publish_release(self.github, self.assets, self.metadata)
        self.github.mutations.clear()
        self.github.records[0]["assets"][0]["digest"] = "sha256:" + "0" * 64
        with self.assertRaisesRegex(RuntimeError, "refusing replacement"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.github.records[0]["assets"] = []
        with patch("publish_release.time.monotonic", side_effect=[0, 0, 60]), self.assertRaisesRegex(
                RuntimeError, "Timed out"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_partial_draft_resumes_only_missing_uploads(self):
        self.github.records = [{"id": 1, "tag_name": "v0.2.7", "target_commitish": COMMIT,
                                "draft": True, "prerelease": False,
                                "assets": [{"name": release.ARCHIVE, "state": "uploaded",
                                            "digest": "sha256:" + release.sha256(self.assets / release.ARCHIVE)}]}]
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual([action for action, _, _ in self.github.mutations], ["upload", "patch"])
        upload = self.github.mutations[0][2]
        self.assertEqual([Path(path).name for path in upload], [release.CHECKSUMS])

    def test_tag_collision_and_release_commit_mismatch_prevent_changes(self):
        self.github.refs["v0.2.7"] = {"object": {"type": "commit", "sha": "b" * 40}}
        with self.assertRaisesRegex(RuntimeError, "exact build commit"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.github.refs.clear()
        self.github.records = [{"id": 1, "tag_name": "v0.2.7", "target_commitish": "b" * 40,
                                "draft": True, "prerelease": False, "assets": []}]
        with self.assertRaisesRegex(RuntimeError, "exact commit and tag"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.mutations, [])

    def test_missing_or_invalid_release_id_cannot_be_used_for_publication(self):
        for identifier in [None, 0, -1, True, "1", "../other"]:
            with self.subTest(identifier=identifier):
                self.github.records = [{"id": identifier, "tag_name": "v0.2.7",
                                        "target_commitish": COMMIT, "draft": True,
                                        "prerelease": False, "assets": []}]
                with self.assertRaisesRegex(RuntimeError, "positive integer id"):
                    release.publish_release(self.github, self.assets, self.metadata)
                self.assertEqual(self.github.mutations, [])

    def test_new_failed_rerun_after_upload_leaves_release_unpublished(self):
        failed = successful_checks()
        failed[0]["id"], failed[0]["conclusion"] = 100, "failure"
        self.github.check_results = [successful_checks(), failed]
        with self.assertRaisesRegex(RuntimeError, "did not succeed"):
            release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual([action for action, _, _ in self.github.mutations], ["create", "upload", "upload"])
        self.assertTrue(self.github.records[0]["draft"])
        self.assertIsNone(self.github.latest)

    def test_out_of_order_older_release_keeps_the_newer_latest(self):
        self.github.records = [{"id": 10, "tag_name": "v0.2.10", "target_commitish": "b" * 40,
                                "draft": False, "prerelease": False, "assets": []}]
        self.github.latest = "v0.2.10"
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertEqual(self.github.latest, "v0.2.10")
        self.assertIn("make_latest=false", self.github.mutations[-1][2])
        self.assertFalse(release.find_release(self.github.records, "v0.2.7")["draft"])

    def test_concurrent_newer_release_uses_server_semver_selection(self):
        def publish_newer():
            self.github.records.append({"id": 10, "tag_name": "v0.2.10",
                                        "target_commitish": "b" * 40, "draft": False,
                                        "prerelease": False, "assets": []})
            self.github.latest = "v0.2.10"

        self.github.before_publish = publish_newer
        release.publish_release(self.github, self.assets, self.metadata)
        self.assertIn("make_latest=legacy", self.github.mutations[-1][2])
        self.assertEqual(self.github.latest, "v0.2.10")
        self.assertFalse(release.find_release(self.github.records, "v0.2.7")["draft"])

    def test_latest_order_is_numeric_and_ignores_drafts_and_prereleases(self):
        releases = [{"tag_name": "v0.2.9", "draft": False, "prerelease": False},
                    {"tag_name": "v9.0.0", "draft": True, "prerelease": False},
                    {"tag_name": "v8.0.0-rc1", "draft": False, "prerelease": True}]
        self.assertTrue(release.should_make_latest(releases, "0.2.10"))
        self.assertFalse(release.should_make_latest(releases, "0.2.7"))
        self.assertTrue(release.should_make_latest(releases, "0.3.1"))
        with self.assertRaisesRegex(RuntimeError, "Cannot safely compare"):
            release.should_make_latest([{"tag_name": "unknown"}], "0.2.7")


if __name__ == "__main__":
    unittest.main()
