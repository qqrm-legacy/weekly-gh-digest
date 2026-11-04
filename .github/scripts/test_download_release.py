"""Release selection and installation fail before state or bot execution on mismatch."""
from copy import deepcopy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import textwrap
import unittest
from unittest.mock import patch

import download_release as release
from test_state_workflow import step_script, WORKFLOW


REPOSITORY = "fixture/weekly-gh-digest"
COMMIT = "a" * 40
TAG = "v0.2.11"
SCRIPT = Path(__file__).with_name("download_release.py").resolve()


def selection_fixture():
    return {"repository": REPOSITORY, "commit": COMMIT, "release": {
        "id": 17, "tag_name": TAG, "draft": False, "prerelease": False,
        "published_at": "2026-11-04T12:00:00Z", "target_commitish": "main",
        "assets": [{"name": release.ARCHIVE, "id": 101, "state": "uploaded", "size": 1},
                   {"name": release.CHECKSUMS, "id": 102, "state": "uploaded", "size": 1}]}}


def member(name, data=b"", kind=tarfile.REGTYPE, mode=0o644, linkname=""):
    info = tarfile.TarInfo(name)
    info.size = len(data) if kind == tarfile.REGTYPE else 0
    info.mode, info.type, info.linkname = mode, kind, linkname
    return info, data


class InstallReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.destination = self.root / "bin"
        self.selection = selection_fixture()
        self.expected = {"version": TAG[1:], "tag": TAG, "commit": COMMIT, "target": release.TARGET}
        self.environment = patch.dict(os.environ, {"GH_TOKEN": "fixture-token",
                                                   "GITHUB_REPOSITORY": REPOSITORY})
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.real_run = subprocess.run
        self.commands = []
        self.configure_archive()

    def configure_archive(self, metadata=None, binary_version=None, extra=(), members=None):
        metadata = self.expected if metadata is None else metadata
        version = binary_version or f"{release.BINARY} {TAG[1:]} ({COMMIT})"
        executable = ("#!/bin/sh\n"
                      "[ \"$1\" = --version ] || exit 21\n"
                      "[ -z \"${GH_TOKEN:-}\" ] && [ -z \"${GITHUB_TOKEN:-}\" ] || exit 22\n"
                      f"printf '%s\\n' '{version}'\n").encode()
        if members is None:
            members = [member(release.BINARY, executable, mode=0o755),
                       member("release.json", json.dumps(metadata).encode()), *extra]
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as package:
            for info, content in members:
                package.addfile(info, io.BytesIO(content))
        data = archive.getvalue()
        checksum = f"{hashlib.sha256(data).hexdigest()}  {release.ARCHIVE}\n".encode()
        self.assets = {"101": data, "102": checksum}
        self.update_sizes()

    def update_sizes(self):
        for asset in self.selection["release"]["assets"]:
            asset["size"] = len(self.assets[str(asset["id"])])

    def run_command(self, args, **kwargs):
        self.commands.append(args)
        if args[0] == "gh":
            self.assertEqual(args[:6], ["gh", "api", "--hostname", "github.com", "-H",
                                       "Accept: application/octet-stream"])
            self.assertEqual(os.environ["GH_TOKEN"], "fixture-token")
            self.assertTrue(args[-1].startswith(f"repos/{REPOSITORY}/releases/assets/"))
            kwargs["stdout"].write(self.assets[args[-1].rsplit("/", 1)[1]])
            return subprocess.CompletedProcess(args, 0)
        return self.real_run(args, **kwargs)

    def install(self, checkout=COMMIT):
        with patch.object(release.subprocess, "check_output", return_value=checkout + "\n") as git:
            with patch.object(release.subprocess, "run", side_effect=self.run_command):
                result = release.install(self.selection, self.destination)
        git.assert_called_once_with(["git", "rev-parse", "HEAD"], text=True,
                                    stderr=subprocess.PIPE, timeout=10)
        return result

    def assert_no_install_or_execution(self):
        self.assertFalse(self.destination.exists())
        self.assertFalse(any(args[-1] == "--version" for args in self.commands))
        self.assertEqual(list(self.root.iterdir()), [])

    def test_verified_exact_asset_ids_install_without_latest_lookup_or_build(self):
        self.assertEqual(self.install(), self.expected)
        self.assertEqual(sorted(path.name for path in self.destination.iterdir()),
                         ["release.json", release.BINARY])
        self.assertEqual(json.loads((self.destination / "release.json").read_text()), self.expected)
        self.assertTrue(os.access(self.destination / release.BINARY, os.X_OK))
        self.assertEqual([args[-1] for args in self.commands[:2]],
                         [f"repos/{REPOSITORY}/releases/assets/102",
                          f"repos/{REPOSITORY}/releases/assets/101"])
        self.assertEqual(self.commands[-1][-1], "--version")

    def test_checkout_mismatch_rejects_before_asset_download(self):
        with self.assertRaisesRegex(ValueError, "Checkout commit"):
            self.install(checkout="b" * 40)
        self.assertEqual(self.commands, [])
        self.assert_no_install_or_execution()

    def test_checksum_mismatch_rejects_before_extraction_or_execution(self):
        self.assets["102"] = f"{'0' * 64}  {release.ARCHIVE}\n".encode()
        self.update_sizes()
        with self.assertRaisesRegex(ValueError, "SHA256 does not match"):
            self.install()
        self.assert_no_install_or_execution()

    def test_missing_duplicate_and_malformed_checksum_entries_reject(self):
        for checksum in [b"not a digest\n", b"0" * 64 + b"  other.tar.gz\n",
                         self.assets["102"] * 2, b"0" * 64 + b"  ../archive.tar.gz\n"]:
            with self.subTest(checksum=checksum):
                self.assets["102"] = checksum
                self.update_sizes()
                with self.assertRaises(ValueError):
                    self.install()
                self.assert_no_install_or_execution()

    def test_asset_download_size_mismatch_rejects_before_execution(self):
        self.selection["release"]["assets"][0]["size"] += 1
        with self.assertRaisesRegex(ValueError, "Downloaded size"):
            self.install()
        self.assert_no_install_or_execution()

    def test_metadata_version_tag_commit_and_target_must_all_match(self):
        for field, value in [("version", "0.2.99"), ("tag", "v0.2.99"),
                             ("commit", "b" * 40), ("target", "aarch64-unknown-linux-gnu")]:
            with self.subTest(field=field):
                self.configure_archive(metadata={**self.expected, field: value})
                with self.assertRaisesRegex(ValueError, "metadata differs"):
                    self.install()
                self.assert_no_install_or_execution()

    def test_archive_rejects_paths_links_directories_and_duplicate_files(self):
        forbidden = [member("../escaped", b"bad"), member("/tmp/escaped", b"bad"),
                     member("./weekly-gh-digest", b"bad"), member("subdir", kind=tarfile.DIRTYPE),
                     member("release.json", b"{}"),
                     member("link", kind=tarfile.SYMTYPE, linkname="../../escaped"),
                     member("link", kind=tarfile.LNKTYPE, linkname=release.BINARY)]
        for extra in forbidden:
            with self.subTest(name=extra[0].name, kind=extra[0].type):
                self.configure_archive(extra=[extra])
                with self.assertRaisesRegex(ValueError, "unexpected path"):
                    self.install()
                self.assert_no_install_or_execution()

    def test_expected_filename_cannot_be_a_symlink_or_missing(self):
        for members in [[member(release.BINARY, kind=tarfile.SYMTYPE, linkname="/bin/sh")],
                        [member("release.json", json.dumps(self.expected).encode())]]:
            with self.subTest(members=members):
                self.configure_archive(members=members)
                with self.assertRaises(ValueError):
                    self.install()
                self.assert_no_install_or_execution()

    def test_wrong_binary_version_is_not_installed(self):
        self.configure_archive(binary_version="weekly-gh-digest 0.2.0 (unknown)")
        with self.assertRaisesRegex(ValueError, "binary --version"):
            self.install()
        self.assertFalse(self.destination.exists())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_existing_installation_is_never_used_as_fallback(self):
        self.destination.mkdir()
        old = self.destination / release.BINARY
        old.write_text("old binary")
        with self.assertRaisesRegex(ValueError, "must be new"):
            self.install()
        self.assertEqual(old.read_text(), "old binary")
        self.assertEqual(self.commands, [])

    def test_invalid_release_metadata_and_assets_fail_before_network(self):
        original = deepcopy(self.selection)
        for fault in ["draft", "prerelease", "unpublished", "repository", "missing-asset",
                      "duplicate-asset", "incomplete-asset", "unsafe-tag"]:
            with self.subTest(fault=fault):
                self.selection = deepcopy(original)
                selected = self.selection["release"]
                if fault in ["draft", "prerelease"]:
                    selected[fault] = True
                elif fault == "unpublished": selected["published_at"] = None
                elif fault == "repository": self.selection["repository"] = "other/repository"
                elif fault == "missing-asset": selected["assets"].pop()
                elif fault == "duplicate-asset": selected["assets"].append(selected["assets"][0])
                elif fault == "incomplete-asset": selected["assets"][0]["state"] = "new"
                elif fault == "unsafe-tag": selected["tag_name"] = "v0.2.11\ncommit=bad"
                with self.assertRaises(ValueError):
                    self.install()
                self.assertEqual(self.commands, [])
                self.assert_no_install_or_execution()


class ResolveReleaseTests(unittest.TestCase):
    def api_responses(self, tag=TAG, annotated=False):
        selected = selection_fixture()["release"]
        selected["tag_name"] = tag
        prefix = f"repos/{REPOSITORY}/"
        responses = {prefix + "releases/latest": selected,
                     prefix + f"releases/tags/{tag}": selected,
                     prefix + f"git/ref/tags/{tag}": {"ref": f"refs/tags/{tag}", "object": {
                         "type": "tag" if annotated else "commit", "sha": "b" * 40 if annotated else COMMIT}},
                     prefix + f"commits/{COMMIT}": {"sha": COMMIT}}
        if annotated:
            responses[prefix + "git/tags/" + "b" * 40] = {"object": {"type": "commit", "sha": COMMIT}}
        return responses

    def run_resolver(self, responses, tag=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "responses.json").write_text(json.dumps(responses))
            fake = root / "gh"
            fake.write_text(textwrap.dedent('''\
                #!/usr/bin/env python3
                import json, os, sys
                from pathlib import Path
                assert os.environ["GH_TOKEN"] == "fixture-token"
                assert sys.argv[1:6] == ["api", "--hostname", "github.com", "-H", "Accept: application/vnd.github+json"]
                with open("requests", "a") as log: log.write(sys.argv[-1] + "\\n")
                response = json.loads(Path("responses.json").read_text()).get(sys.argv[-1])
                if response is None: sys.exit(44)
                print(json.dumps(response))
                '''))
            fake.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                   "GH_TOKEN": "fixture-token", "GITHUB_REPOSITORY": REPOSITORY,
                   "RELEASE_SELECTION": str(root / "selection.json"),
                   "GITHUB_OUTPUT": str(root / "outputs")}
            command = (["bash", "-c", step_script("Resolve latest published stable release")]
                       if tag is None else [os.sys.executable, str(SCRIPT), "resolve", "--tag", tag,
                                            "--output", str(root / "selection.json")])
            result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=10)
            selection = json.loads((root / "selection.json").read_text()) if (root / "selection.json").exists() else None
            output = (root / "outputs").read_text() if (root / "outputs").exists() else ""
            requests = (root / "requests").read_text().splitlines() if (root / "requests").exists() else []
            return result, selection, output, requests

    def test_actual_workflow_resolver_uses_tag_commit_not_target_commitish(self):
        for annotated in [False, True]:
            with self.subTest(annotated=annotated):
                result, selection, output, requests = self.run_resolver(self.api_responses(annotated=annotated))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(selection["commit"], COMMIT)
                self.assertEqual(output, f"tag={TAG}\ncommit={COMMIT}\n")
                self.assertEqual(requests[0], f"repos/{REPOSITORY}/releases/latest")
                self.assertEqual(requests[-1], f"repos/{REPOSITORY}/commits/{COMMIT}")

    def test_exact_tag_helper_pins_requested_release_and_exports_commit(self):
        for annotated in [False, True]:
            with self.subTest(annotated=annotated):
                result, selection, output, requests = self.run_resolver(self.api_responses(annotated=annotated), TAG)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(selection["release"]["tag_name"], TAG)
                self.assertEqual(output, f"tag={TAG}\ncommit={COMMIT}\n")
                self.assertEqual(requests[0], f"repos/{REPOSITORY}/releases/tags/{TAG}")
                self.assertFalse(any(path.endswith("/latest") for path in requests))

    def test_stable_minor_and_major_bumps_remain_installable(self):
        for tag in ["v0.3.0", "v1.0.0"]:
            for explicit in [None, tag]:
                with self.subTest(tag=tag, explicit=explicit):
                    result, selection, output, _ = self.run_resolver(self.api_responses(tag=tag), explicit)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(selection["release"]["tag_name"], tag)
                    self.assertEqual(output, f"tag={tag}\ncommit={COMMIT}\n")

    def test_draft_prerelease_missing_and_malformed_release_never_export(self):
        for mode in [None, TAG]:
            for fault in ["draft", "prerelease", "unpublished", "missing", "unsafe-tag", "leading-zero"]:
                with self.subTest(mode=mode, fault=fault):
                    responses = self.api_responses()
                    endpoint = f"repos/{REPOSITORY}/releases/" + (f"tags/{TAG}" if mode else "latest")
                    if fault == "missing": responses.pop(endpoint)
                    elif fault == "unpublished": responses[endpoint]["published_at"] = None
                    elif fault == "unsafe-tag": responses[endpoint]["tag_name"] = "v0.2.11\ncommit=bad"
                    elif fault == "leading-zero": responses[endpoint]["tag_name"] = "v0.2.011"
                    else: responses[endpoint][fault] = True
                    result, selection, output, _ = self.run_resolver(responses, mode)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIsNone(selection)
                    self.assertEqual(output, "")

    def test_tag_or_commit_disagreement_never_exports(self):
        for mode in [None, TAG]:
            for fault in ["wrong-reference", "tree", "short-sha", "wrong-commit", "tag-cycle"]:
                with self.subTest(mode=mode, fault=fault):
                    responses = self.api_responses(annotated=fault == "tag-cycle")
                    reference = responses[f"repos/{REPOSITORY}/git/ref/tags/{TAG}"]
                    if fault == "wrong-reference": reference["ref"] = "refs/heads/main"
                    elif fault == "tree": reference["object"]["type"] = "tree"
                    elif fault == "short-sha": reference["object"]["sha"] = "abc123"
                    elif fault == "wrong-commit": responses[f"repos/{REPOSITORY}/commits/{COMMIT}"]["sha"] = "c" * 40
                    else: responses[f"repos/{REPOSITORY}/git/tags/" + "b" * 40]["object"] = {"type": "tag", "sha": "b" * 40}
                    result, selection, output, _ = self.run_resolver(responses, mode)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIsNone(selection)
                    self.assertEqual(output, "")

    def test_workflow_installs_before_state_and_never_compiles_or_uses_binary_cache(self):
        workflow = WORKFLOW.read_text()
        stages = ["Resolve latest published stable release", "Checkout selected release source",
                  "Install verified release binary", "Restore persistent state", "Collect, preview"]
        offsets = [workflow.index("      - name: " + stage) for stage in stages]
        self.assertEqual(offsets, sorted(offsets))
        self.assertEqual(workflow.count("uses: actions/checkout@"), 1)
        self.assertIn("ref: ${{ steps.release.outputs.commit }}", workflow)
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertIn("bot=./bin/weekly-gh-digest", workflow)
        for forbidden in ["cargo build", "target/release", "rust-cache", "rust-toolchain"]:
            self.assertNotIn(forbidden, workflow)


if __name__ == "__main__":
    unittest.main()
