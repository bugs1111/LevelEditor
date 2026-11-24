#!/usr/bin/env python3
"""Version Level Editor and trigger its release workflow with a Git tag."""
import argparse
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[1]
STABLE = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
MANIFEST_VERSION = re.compile(r'(?m)^version = "([^"]+)"$')
LOCK_VERSION = re.compile(r'(?m)^\[\[package\]\]\nname = "level-editor"\nversion = "([^"]+)"$')


def parse_version(version):
    match = STABLE.fullmatch(version)
    if not match:
        raise ValueError(f"Expected a stable MAJOR.MINOR.PATCH version, got {version!r}")
    return tuple(int(part) for part in match.groups())


def select_version(requested, source, stable_tags, other_tags):
    baseline = parse_version(source)
    for tag in stable_tags:
        version = tag.removeprefix("v")
        if STABLE.fullmatch(version):
            baseline = max(baseline, parse_version(version))
    if requested is None:
        major, minor, patch = baseline
        requested = f"{major}.{minor}.{patch + 1}"
    candidate = parse_version(requested)
    if candidate < baseline:
        raise ValueError(f"Version {requested} is below {'.'.join(map(str, baseline))}")
    if {requested, f"v{requested}"} & (stable_tags | other_tags):
        raise ValueError(f"Release v{requested} already exists")
    return requested


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def run(*command):
    subprocess.run(command, cwd=ROOT, check=True)


def github_repository(url):
    match = re.fullmatch(r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)([\w.-]+/[\w.-]+?)(?:\.git)?/?", url)
    if not match:
        raise ValueError("origin must point to a GitHub repository")
    return match[1]


def publish(requested):
    if requested is not None:
        parse_version(requested)
    if capture("git", "status", "--porcelain"):
        raise ValueError("Commit the changes intended for release before running just pub")

    manifest = ROOT / "Cargo.toml"
    lock = ROOT / "Cargo.lock"
    manifest_text, lock_text = manifest.read_text(), lock.read_text()
    manifest_match = MANIFEST_VERSION.search(manifest_text.split("[workspace]", 1)[0])
    lock_match = LOCK_VERSION.search(lock_text)
    if not manifest_match or not lock_match or manifest_match[1] != lock_match[1]:
        raise ValueError("Cargo.toml and Cargo.lock must contain the same Level Editor version")
    source = manifest_match[1]

    repo = github_repository(capture("git", "remote", "get-url", "origin"))
    push_repo = github_repository(capture("git", "remote", "get-url", "--push", "origin"))
    if repo.casefold() != push_repo.casefold():
        raise ValueError("origin must fetch from and push to the same GitHub repository")
    stable, other = set(), set()
    releases = capture("gh", "api", "--paginate", f"repos/{repo}/releases?per_page=100",
                       "--jq", ".[] | [.tag_name,.draft,.prerelease] | @tsv")
    for line in releases.splitlines():
        tag, draft, prerelease = line.split("\t")
        (stable if draft == prerelease == "false" else other).add(tag)

    version = select_version(requested, source, stable, other)
    tag = f"v{version}"
    if capture("git", "ls-remote", "--tags", "--refs", "origin", f"refs/tags/{tag}"):
        raise ValueError(f"Remote tag {tag} already exists")
    run("git", "fetch", "origin", "main")
    try:
        run("git", "merge-base", "--is-ancestor", "FETCH_HEAD", "HEAD")
    except subprocess.CalledProcessError as error:
        raise ValueError("Integrate origin/main before publishing") from error

    print(f"Publishing {tag} to {repo}", flush=True)
    if version != source:
        start, end = manifest_match.span(1)
        manifest.write_text(manifest_text[:start] + version + manifest_text[end:])
        start, end = lock_match.span(1)
        lock.write_text(lock_text[:start] + version + lock_text[end:])
        run("git", "add", "--", "Cargo.toml", "Cargo.lock")
        run("git", "commit", "-m", f"Release {tag}")
    try:
        run("git", "push", "--atomic", "origin", "HEAD:refs/heads/main", f"HEAD:refs/tags/{tag}")
    except subprocess.CalledProcessError as error:
        raise ValueError(f"Push failed. After resolving the Git error, retry with just pub {version}") from error
    print(f"Build and release: https://github.com/{repo}/actions/workflows/build-release.yml", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", nargs="?", help="stable version (default: next patch)")
    args = parser.parse_args()
    try:
        publish(args.version)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Publish: {error}\n")


if __name__ == "__main__":
    main()
