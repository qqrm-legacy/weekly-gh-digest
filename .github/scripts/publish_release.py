"""Package an exact main commit and publish it only after every required check passes."""

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib


ROOT = Path(__file__).resolve().parents[2]
BINARY = "weekly-gh-digest"
TARGET = "x86_64-unknown-linux-gnu"
ARCHIVE = f"{BINARY}-{TARGET}.tar.gz"
CHECKSUMS = "SHA256SUMS"
ASSETS = (ARCHIVE, CHECKSUMS)
REQUIRED_CHECKS = (
    "checks", "ci-gate", "actionlint", "zizmor", "dependency-policy",
    "workflow-security-gate",
)
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def release_metadata(package_version, run_number, commit):
    match = VERSION.fullmatch(package_version)
    require(match is not None, "Cargo package version must be a stable semantic version")
    require(re.fullmatch(r"[1-9][0-9]*", run_number) is not None,
            "GITHUB_RUN_NUMBER must be a positive integer")
    require(re.fullmatch(r"[0-9a-f]{40}", commit) is not None,
            "GITHUB_SHA must be a full lowercase commit SHA")
    version = f"{match[1]}.{match[2]}.{run_number}"
    return {"version": version, "tag": f"v{version}", "commit": commit, "target": TARGET}


def workflow_metadata():
    require(os.environ.get("GITHUB_REF") == "refs/heads/main",
            "Releases can only run from main")
    require(os.environ.get("GITHUB_EVENT_NAME") in {"push", "workflow_dispatch"},
            "Releases require a main push or manual main run")
    with (ROOT / "Cargo.toml").open("rb") as source:
        package_version = tomllib.load(source)["package"]["version"]
    return release_metadata(package_version, os.environ.get("GITHUB_RUN_NUMBER", ""),
                            os.environ.get("GITHUB_SHA", ""))


def sha256(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def package_release(binary, directory, metadata):
    binary, directory = Path(binary), Path(directory)
    require(binary.is_file() and os.access(binary, os.X_OK),
            "Release binary must be an executable regular file")
    directory.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(metadata, sort_keys=True, indent=2) + "\n").encode()
    # Fixed tar headers and gzip time make reruns with identical binaries byte-identical.
    with (directory / ARCHIVE).open("wb") as output:
        with gzip.GzipFile(filename="", fileobj=output, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                executable = tarfile.TarInfo(BINARY)
                executable.size, executable.mode = binary.stat().st_size, 0o755
                with binary.open("rb") as source:
                    archive.addfile(executable, source)
                manifest = tarfile.TarInfo("release.json")
                manifest.size, manifest.mode = len(payload), 0o644
                archive.addfile(manifest, io.BytesIO(payload))
    digest = sha256(directory / ARCHIVE)
    (directory / CHECKSUMS).write_text(f"{digest}  {ARCHIVE}\n", encoding="utf-8")
    inspect_assets(directory, metadata)


def inspect_assets(directory, metadata):
    """Validate names, checksums, executable mode and metadata without extracting paths."""
    directory = Path(directory)
    for name in ASSETS:
        path = directory / name
        require(path.is_file() and not path.is_symlink(), f"Missing regular release asset: {name}")
    digest = sha256(directory / ARCHIVE)
    require((directory / CHECKSUMS).read_text(encoding="utf-8") == f"{digest}  {ARCHIVE}\n",
            "SHA256SUMS does not match the release archive")
    with tarfile.open(directory / ARCHIVE, "r:gz") as archive:
        members = archive.getmembers()
        require(len(members) == 2 and {member.name for member in members} == {BINARY, "release.json"}
                and all(member.isfile() for member in members),
                "Release archive must contain only two regular root files")
        executable = archive.getmember(BINARY)
        require(executable.mode & 0o111 and 0 < executable.size <= 256 * 1024 * 1024,
                "Archived binary must be executable and at most 256 MiB")
        require(archive.getmember("release.json").size <= 4096, "Release metadata is too large")
        with archive.extractfile("release.json") as source:
            require(json.load(source) == metadata, "Archive metadata does not match this release")
    return {name: sha256(directory / name) for name in ASSETS}


def verify_archive_runtime(directory, metadata):
    inspect_assets(directory, metadata)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("TELEGRAM_")
           and key not in {"GH_TOKEN", "GITHUB_TOKEN", "GROQ_API_KEY"}}
    env["DIGEST_DELIVERY_ENABLED"] = "false"
    with tempfile.TemporaryDirectory(prefix="release-runtime-") as temporary:
        root = Path(temporary)
        binary = root / BINARY
        with tarfile.open(Path(directory) / ARCHIVE, "r:gz") as archive:
            with archive.extractfile(BINARY) as source, binary.open("wb") as destination:
                shutil.copyfileobj(source, destination)
        binary.chmod(0o755)
        working = root / "outside-checkout"
        working.mkdir()
        version = subprocess.run([str(binary), "--version"], cwd=working, env=env,
                                 text=True, capture_output=True, timeout=10, check=True)
        require(version.stdout.strip() == f"{BINARY} {metadata['version']} ({metadata['commit']})",
                "Archived binary --version does not match the release metadata")
        subprocess.run([sys.executable, str(ROOT / ".github/scripts/verify_publication_disabled.py"),
                        str(binary)], cwd=working, env=env, check=True, timeout=60)


class GitHub:
    def __init__(self, repository):
        require(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) is not None,
                "GITHUB_REPOSITORY must be owner/repository")
        self.repository = repository

    @staticmethod
    def command(*arguments):
        result = subprocess.run(["gh", *arguments], text=True, capture_output=True,
                                timeout=180, check=False)
        require(result.returncode == 0, f"GitHub command failed: {result.stderr.strip()}")
        return result.stdout

    def api(self, endpoint, *arguments):
        return json.loads(self.command("api", f"repos/{self.repository}/{endpoint}", *arguments))

    def check_runs(self, commit):
        pages = self.api(f"commits/{commit}/check-runs?filter=all&per_page=100",
                         "--paginate", "--slurp")
        return [run for page in pages for run in page["check_runs"]]

    def releases(self):
        pages = self.api("releases?per_page=100", "--paginate", "--slurp")
        return [release for page in pages for release in page]

    def tag_ref(self, tag):
        matches = self.api(f"git/matching-refs/tags/{tag}")
        exact = [ref for ref in matches if ref.get("ref") == f"refs/tags/{tag}"]
        require(len(exact) <= 1, "GitHub returned duplicate release tags")
        return exact[0] if exact else None

    def release_command(self, action, tag, *arguments):
        return self.command("release", action, tag, "--repo", self.repository, *arguments)


def pending_checks(check_runs, commit):
    latest = {}
    for run in check_runs:
        name = run.get("name")
        if (name not in REQUIRED_CHECKS or run.get("head_sha") != commit
                or (run.get("app") or {}).get("slug") != "github-actions"):
            continue
        require(type(run.get("id")) is int and run["id"] > 0, "Check run has no valid id")
        if name not in latest or run["id"] > latest[name]["id"]:
            latest[name] = run
    pending = []
    for name in REQUIRED_CHECKS:
        run = latest.get(name)
        if run is None or run.get("status") != "completed":
            pending.append(name)
        else:
            require(run.get("conclusion") == "success",
                    f"Required check {name} did not succeed: {run.get('conclusion')}")
    return pending


def wait_for_checks(github, commit, timeout=2100, interval=15):
    deadline = time.monotonic() + timeout
    while True:
        pending = pending_checks(github.check_runs(commit), commit)
        if not pending:
            return
        remaining = deadline - time.monotonic()
        require(remaining > 0, f"Timed out waiting for required checks on {commit}: {', '.join(pending)}")
        print(f"Waiting for {commit}: {', '.join(pending)}", flush=True)
        time.sleep(min(interval, remaining))


def find_release(releases, tag):
    matches = [release for release in releases if release.get("tag_name") == tag]
    require(len(matches) <= 1, "GitHub returned duplicate releases for the tag")
    return matches[0] if matches else None


def validate_release(release, metadata):
    require(release is not None, "The release was not returned by GitHub")
    require(type(release.get("id")) is int and release["id"] > 0,
            "Release must have a positive integer id")
    require(release.get("tag_name") == metadata["tag"]
            and release.get("target_commitish") == metadata["commit"]
            and release.get("prerelease") is False
            and type(release.get("draft")) is bool,
            "Existing release does not match this exact commit and tag")


def validate_tag(github, metadata, allow_missing=False):
    ref = github.tag_ref(metadata["tag"])
    if ref is None:
        require(allow_missing, "Published release tag is missing")
    else:
        require(ref.get("object", {}).get("type") == "commit"
                and ref["object"].get("sha") == metadata["commit"],
                "Release tag does not point to the exact build commit")


def validate_uploaded_assets(release, hashes, allow_missing=False):
    assets = release.get("assets", [])
    names = [asset.get("name") for asset in assets]
    require(len(names) == len(set(names)) and set(names) <= set(ASSETS),
            "Existing release has unexpected or duplicate assets")
    for asset in assets:
        require(asset.get("state") == "uploaded"
                and asset.get("digest") == f"sha256:{hashes[asset['name']]}",
                f"Existing asset differs from this build: {asset['name']}; refusing replacement")
    if not allow_missing:
        require(set(names) == set(ASSETS), "Published release is missing required assets")
    return set(names)


def should_make_latest(releases, version):
    current = tuple(map(int, VERSION.fullmatch(version).groups()))
    for release in releases:
        if release.get("draft") or release.get("prerelease"):
            continue
        tag = release.get("tag_name", "")
        parsed = VERSION.fullmatch(tag.removeprefix("v")) if tag.startswith("v") else None
        require(parsed is not None, f"Cannot safely compare published release version: {tag}")
        if tuple(map(int, parsed.groups())) > current:
            return False
    return True


def publish_release(github, directory, metadata, timeout=2100):
    """Publish unique tags independently; GitHub resolves latest by semantic version."""
    directory = Path(directory).resolve()
    hashes = inspect_assets(directory, metadata)
    wait_for_checks(github, metadata["commit"], timeout=timeout)
    releases = github.releases()
    release = find_release(releases, metadata["tag"])
    validate_tag(github, metadata, allow_missing=release is None or release.get("draft") is True)
    # Fail on unorderable versions before creating or uploading anything.
    should_make_latest(releases, metadata["version"])
    if release is not None:
        validate_release(release, metadata)
        validate_uploaded_assets(release, hashes, allow_missing=release["draft"])
        if not release["draft"]:
            print(f"{metadata['tag']} already published with identical assets; no changes.")
            return
    else:
        with tempfile.TemporaryDirectory(prefix="release-notes-") as temporary:
            notes = Path(temporary) / "notes.md"
            notes.write_text(
                f"Automated build from main.\n\nCommit: `{metadata['commit']}`\n"
                f"Target: `{metadata['target']}`\n\n"
                "All required CI and workflow security checks passed for this commit.\n",
                encoding="utf-8",
            )
            github.release_command("create", metadata["tag"], "--draft", "--latest=false",
                                   "--target", metadata["commit"], "--title", metadata["tag"],
                                   "--notes-file", str(notes))
        release = find_release(github.releases(), metadata["tag"])
        validate_release(release, metadata)
        require(release["draft"], "New release must remain a draft until assets are verified")

    existing = validate_uploaded_assets(release, hashes, allow_missing=True)
    missing = [str(directory / name) for name in ASSETS if name not in existing]
    if missing:
        # Never use --clobber: published assets and partial draft uploads are immutable here.
        github.release_command("upload", metadata["tag"], *missing)
    releases = github.releases()
    release = find_release(releases, metadata["tag"])
    validate_release(release, metadata)
    require(release["draft"], "Release changed while preparing publication")
    validate_uploaded_assets(release, hashes)
    validate_tag(github, metadata, allow_missing=True)
    # A newly started check rerun must not be hidden by the earlier successful observation.
    wait_for_checks(github, metadata["commit"], timeout=timeout)
    # Never force latest=true: another run can publish a newer version after this read.
    # Server-side legacy selection handles that race without a lossy global queue.
    latest = "legacy" if should_make_latest(releases, metadata["version"]) else "false"
    github.api(f"releases/{release['id']}", "--method", "PATCH",
               "-F", "draft=false", "-f", f"make_latest={latest}")
    published = find_release(github.releases(), metadata["tag"])
    validate_release(published, metadata)
    require(not published["draft"], "Release remained a draft after publication")
    validate_uploaded_assets(published, hashes)
    validate_tag(github, metadata)
    print(f"Published {metadata['tag']} from {metadata['commit']} (latest={latest}).")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("metadata")
    package = commands.add_parser("package")
    package.add_argument("binary", type=Path)
    package.add_argument("directory", type=Path)
    verify = commands.add_parser("verify")
    verify.add_argument("directory", type=Path)
    publish = commands.add_parser("publish")
    publish.add_argument("directory", type=Path)
    publish.add_argument("--timeout", type=int, default=2100)
    arguments = parser.parse_args()
    metadata = workflow_metadata()
    if arguments.command == "metadata":
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
            for key, value in metadata.items():
                output.write(f"{key}={value}\n")
        print(json.dumps(metadata, sort_keys=True))
    elif arguments.command == "package":
        package_release(arguments.binary, arguments.directory, metadata)
    elif arguments.command == "verify":
        verify_archive_runtime(arguments.directory, metadata)
    else:
        require(arguments.timeout > 0, "Check timeout must be positive")
        publish_release(GitHub(os.environ.get("GITHUB_REPOSITORY", "")), arguments.directory,
                        metadata, timeout=arguments.timeout)


if __name__ == "__main__":
    main()
