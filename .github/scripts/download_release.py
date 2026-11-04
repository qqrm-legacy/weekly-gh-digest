"""Install only the selected, verified GitHub Release; never build a fallback."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile


TARGET = "x86_64-unknown-linux-gnu"
BINARY = "weekly-gh-digest"
ARCHIVE = f"{BINARY}-{TARGET}.tar.gz"
CHECKSUMS = "SHA256SUMS"
TAG_PATTERN = r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_BINARY_BYTES = 256 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def api_json(repository, path):
    return json.loads(subprocess.check_output(
        ["gh", "api", "--hostname", "github.com", "-H", "Accept: application/vnd.github+json",
         f"repos/{repository}/{path}"], text=True, stderr=subprocess.PIPE, timeout=60))


def resolve(repository, tag=None):
    """Resolve latest or an explicit stable tag to immutable assets and commit."""
    require(isinstance(repository, str) and re.fullmatch(
        r"[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*", repository),
        "Invalid GitHub repository")
    require(bool(os.environ.get("GH_TOKEN", "").strip()), "GH_TOKEN is required for release downloads")
    require(tag is None or (isinstance(tag, str) and re.fullmatch(TAG_PATTERN, tag)),
            "Release tag must be stable vMAJOR.MINOR.PATCH")
    release = api_json(repository, f"releases/tags/{tag}" if tag else "releases/latest")
    require(isinstance(release, dict) and release.get("draft") is False
            and release.get("prerelease") is False and release.get("published_at"),
            "Release must be published and stable")
    actual_tag = release.get("tag_name")
    require(isinstance(actual_tag, str) and re.fullmatch(TAG_PATTERN, actual_tag)
            and (tag is None or tag == actual_tag), "Resolved release tag does not match")
    reference = api_json(repository, f"git/ref/tags/{actual_tag}")
    require(reference.get("ref") == f"refs/tags/{actual_tag}", "Release tag reference does not match")
    obj = reference.get("object", {})
    for _ in range(8):
        sha = obj.get("sha")
        require(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}", sha),
                "Release reference has no full object SHA")
        if obj.get("type") == "commit":
            break
        require(obj.get("type") == "tag", "Release tag does not reference a commit")
        obj = api_json(repository, f"git/tags/{sha}").get("object", {})
    else:
        raise ValueError("Release tag nesting is too deep")
    require(api_json(repository, f"commits/{sha}").get("sha") == sha,
            "Resolved release commit does not match")
    selection = {"repository": repository, "release": release, "commit": sha}
    selected_release(selection, repository)
    return selection


def selected_release(selection, repository):
    require(isinstance(selection, dict), "Invalid release selection")
    require(isinstance(repository, str) and re.fullmatch(
        r"[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*", repository),
        "Invalid GitHub repository")
    require(selection.get("repository") == repository, "Release belongs to another repository")
    commit = selection.get("commit")
    require(isinstance(commit, str) and re.fullmatch(r"[0-9a-f]{40}", commit),
            "Release selection must contain a full commit SHA")
    release = selection.get("release")
    require(isinstance(release, dict), "Missing selected release")
    require(type(release.get("id")) is int and release["id"] > 0, "Invalid release ID")
    require(release.get("draft") is False and release.get("prerelease") is False
            and isinstance(release.get("published_at"), str) and release["published_at"],
            "Release must be published and stable")
    tag = release.get("tag_name")
    require(isinstance(tag, str) and re.fullmatch(TAG_PATTERN, tag),
            "Release tag must be stable vMAJOR.MINOR.PATCH")
    assets = release.get("assets")
    require(isinstance(assets, list), "Release has no asset list")
    selected = {}
    for name, limit in [(ARCHIVE, MAX_ARCHIVE_BYTES), (CHECKSUMS, 32 * 1024)]:
        matches = [asset for asset in assets if isinstance(asset, dict) and asset.get("name") == name]
        require(len(matches) == 1, f"Release must contain exactly one {name}")
        asset = matches[0]
        require(type(asset.get("id")) is int and asset["id"] > 0
                and asset.get("state") == "uploaded", f"Release asset {name} is not uploaded")
        require(type(asset.get("size")) is int and 0 < asset["size"] <= limit,
                f"Release asset {name} has an invalid size")
        selected[name] = asset
    require(selected[ARCHIVE]["id"] != selected[CHECKSUMS]["id"], "Duplicate release asset IDs")
    expected = {"version": tag[1:], "tag": tag, "commit": commit, "target": TARGET}
    return selected, expected


def download_asset(repository, asset, destination):
    # The authenticated API also works for private repositories. Pin asset IDs
    # selected before checkout; do not resolve /latest again during installation.
    with destination.open("xb") as output:
        result = subprocess.run(
            ["gh", "api", "--hostname", "github.com", "-H", "Accept: application/octet-stream",
             f"repos/{repository}/releases/assets/{asset['id']}"],
            stdout=output, stderr=subprocess.PIPE, check=False, timeout=120)
    require(result.returncode == 0, f"Failed to download release asset {asset['name']}")
    require(destination.stat().st_size == asset["size"],
            f"Downloaded size differs for {asset['name']}")


def verify_checksum(archive, checksums):
    entries = {}
    for line in checksums.read_text(encoding="ascii").splitlines():
        if not line.strip():
            continue
        parts = line.split()
        require(len(parts) == 2, "Invalid SHA256SUMS line")
        digest, name = parts
        name = name.removeprefix("*")
        require(re.fullmatch(r"[0-9a-fA-F]{64}", digest)
                and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name)
                and name not in entries, "Invalid or duplicate SHA256SUMS entry")
        entries[name] = digest.lower()
    require(ARCHIVE in entries, "SHA256SUMS does not cover the release archive")
    with archive.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    require(actual == entries[ARCHIVE], "Release archive SHA256 does not match")


def extract_verified(archive, destination, expected):
    """Copy a strict allowlist of regular files, without tar path extraction."""
    seen = set()
    with tarfile.open(archive, "r|gz") as package:
        for member in package:
            require(member.name in {BINARY, "release.json"} and member.name not in seen
                    and member.isfile() and not member.issparse(),
                    "Release archive contains an unexpected path, duplicate, or non-regular file")
            limit = MAX_BINARY_BYTES if member.name == BINARY else 8192
            require(0 < member.size <= limit, "Release archive member has an invalid size")
            if member.name == BINARY:
                require(member.mode & 0o111, "Release binary is not marked executable")
            source = package.extractfile(member)
            require(source is not None, "Release archive member cannot be read")
            with source, (destination / member.name).open("xb") as output:
                shutil.copyfileobj(source, output, length=64 * 1024)
            require((destination / member.name).stat().st_size == member.size,
                    "Release archive member is truncated")
            seen.add(member.name)
    require(seen == {BINARY, "release.json"}, "Release archive is missing required files")
    metadata = json.loads((destination / "release.json").read_text(encoding="utf-8"))
    require(metadata == expected, "Release metadata differs from the selected tag, commit, or target")
    binary = destination / BINARY
    binary.chmod(0o755)
    (destination / "release.json").chmod(0o644)
    # --version is a local identity check; no provider or Telegram credentials
    # are available in this step, and the GitHub download token is removed.
    environment = {key: value for key, value in os.environ.items()
                   if key not in {"GH_TOKEN", "GITHUB_TOKEN"}}
    result = subprocess.run([str(binary.resolve()), "--version"], cwd=destination,
                            env=environment, capture_output=True, text=True,
                            check=False, timeout=10)
    require(result.returncode == 0 and result.stdout.rstrip("\r\n") ==
            f"{BINARY} {expected['version']} ({expected['commit']})",
            "Release binary --version differs from the selected release")


def install(selection, destination):
    repository = os.environ.get("GITHUB_REPOSITORY")
    assets, expected = selected_release(selection, repository)
    require(bool(os.environ.get("GH_TOKEN", "").strip()), "GH_TOKEN is required for release downloads")
    checkout = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True,
                                       stderr=subprocess.PIPE, timeout=10).strip()
    require(checkout == expected["commit"], "Checkout commit differs from the selected release")
    destination = Path(destination).absolute()
    require(not destination.exists() and not destination.is_symlink(),
            "Release installation directory must be new")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".release-", dir=destination.parent) as directory:
        temporary = Path(directory)
        for name in [CHECKSUMS, ARCHIVE]:
            download_asset(repository, assets[name], temporary / name)
        verify_checksum(temporary / ARCHIVE, temporary / CHECKSUMS)
        staged = temporary / "bin"
        staged.mkdir()
        extract_verified(temporary / ARCHIVE, staged, expected)
        staged.rename(destination)
    return expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    resolver = commands.add_parser("resolve", help="Select latest or an explicit stable release")
    resolver.add_argument("--tag")
    resolver.add_argument("--output", required=True, type=Path)
    installer = commands.add_parser("install", help="Install the selected release on its exact checkout")
    installer.add_argument("selection", type=Path)
    installer.add_argument("--destination", type=Path, default=Path("bin"))
    args = parser.parse_args()
    try:
        if args.command == "resolve":
            selection = resolve(os.environ.get("GITHUB_REPOSITORY"), args.tag)
            args.output.write_text(json.dumps(selection) + "\n", encoding="utf-8")
            tag, commit = selection["release"]["tag_name"], selection["commit"]
            if os.environ.get("GITHUB_OUTPUT"):
                with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                    output.write(f"tag={tag}\ncommit={commit}\n")
            print(f"Selected {tag} ({commit})")
            return
        metadata = install(json.loads(args.selection.read_text(encoding="utf-8")), args.destination)
    except (ValueError, OSError, tarfile.TarError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Release installation failed: {error}") from error
    print(f"Installed {BINARY} {metadata['version']} ({metadata['commit']}) from {metadata['tag']}")


if __name__ == "__main__":
    main()
