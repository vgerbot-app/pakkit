#!/usr/bin/env python3
"""Audit AppsGit discoveries against authoritative package indexes.

AppsGit is only a discovery list.  This script never imports its descriptions,
license assertions, or package names into the official catalog.  The generated
ledger is an audit artifact, not catalog content.
"""
from __future__ import annotations

import argparse
import collections
import csv
import json
import re
from pathlib import Path
import sqlite3
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "temp" / "pakkit"


def github_repository(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    url = urlparse(value)
    parts = url.path.strip("/").split("/")
    if url.hostname not in {"github.com", "www.github.com"} or len(parts) < 2:
        return None
    return f"{parts[0]}/{parts[1].removesuffix('.git')}".lower()


def source_apps() -> list[dict]:
    apps = [
        app
        for page in range(1, 16)
        for app in json.loads(
            (CACHE / f"appsgit-page-{page}.json").read_text()
        )["data"]
    ]
    if len(apps) != 1404 or len({app["slug"] for app in apps}) != len(apps):
        raise ValueError("The AppsGit discovery snapshot is incomplete")
    return apps


def package_urls(package: dict) -> list[str]:
    urls = [package.get("homepage"), package.get("url")]
    for channel in ("stable", "head"):
        urls.append(package.get("urls", {}).get(channel, {}).get("url"))
    for variation in package.get("variations", {}).values():
        if isinstance(variation, dict):
            urls.extend((variation.get("url"), variation.get("homepage")))
    return [url for url in urls if isinstance(url, str)]


def package_indexes() -> tuple[dict[str, list[dict]], dict[str, list[dict]]]:
    formulas = collections.defaultdict(list)
    casks = collections.defaultdict(list)
    for package in json.loads((CACHE / "homebrew-formula.json").read_text()):
        for repo in {github_repository(url) for url in package_urls(package)}:
            if repo:
                formulas[repo].append(package)
    for package in json.loads((CACHE / "homebrew-cask.json").read_text()):
        for repo in {github_repository(url) for url in package_urls(package)}:
            if repo:
                casks[repo].append(package)
    return formulas, casks


def winget_packages() -> dict[str, list[dict]]:
    database = CACHE / "winget-source.db"
    if not database.exists():
        raise FileNotFoundError(f"Download the official winget source index and extract Public/index.db to {database}")
    connection = sqlite3.connect(database)
    rows = connection.execute("""
        SELECT ids.id, versions.version, manifest.pathpart
        FROM manifest
        JOIN ids ON ids.rowid = manifest.id
        JOIN versions ON versions.rowid = manifest.version
    """)
    versions: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)
    for package_id, version, leaf in rows:
        parts = []
        parent = leaf
        while parent is not None:
            parent, part = connection.execute("SELECT parent, pathpart FROM pathparts WHERE rowid = ?", (parent,)).fetchone()
            parts.append(part)
        parts.reverse()
        if len(parts) < 6 or parts[0] != "manifests" or parts[-2] != version:
            raise ValueError(f"Unexpected winget manifest path for {package_id} {version}: {parts}")
        package_path = "/".join(parts[1:-2])
        if package_id.split(".", 1)[0].lower() != parts[2].lower():
            raise ValueError(f"Package publisher/path mismatch for {package_id}: {package_path}")
        versions[package_id].append((version, package_path))
    return versions

def flathub_ids() -> set[str]:
    return set(json.loads((CACHE / "flathub-appstream.json").read_text()))

def source_flathub_candidates(slug: str, name: str, available: set[str]) -> list[str]:
    # Discovery only: appstream/source identity must be verified before emission.
    normalized = re.sub(r"[^a-z0-9]", "", f"{slug}{name}".lower())
    return sorted(app_id for app_id in available if normalized and normalized in re.sub(r"[^a-z0-9]", "", app_id.lower()))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=CACHE / "audit")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    apps = source_apps()
    formulas, casks = package_indexes()
    winget = winget_packages()
    existing = {
        github_repository(app["sourceRepository"]): app["id"]
        for path in (ROOT / "catalog").glob("*.json")
        if path.name != "index.json"
        for app in json.loads(path.read_text())["apps"]
    }
    flathub = flathub_ids()
    review_path = args.output / "ledger.json"
    previous = {row["slug"]: row for row in json.loads(review_path.read_text())} if review_path.exists() else {}
    reviewed_path = ROOT / "scripts" / "appsgit_reviewed.json"
    reviewed_dispositions = json.loads(reviewed_path.read_text()) if reviewed_path.exists() else {}
    ledger = []
    for app in apps:
        repo = github_repository(app["repoUrl"])
        stable_formulas = [
            package["name"]
            for package in formulas.get(repo, [])
            if not package["deprecated"] and not package["disabled"]
        ]
        stable_casks = [
            package["token"]
            for package in casks.get(repo, [])
            if not package["deprecated"]
            and not package["disabled"]
            and not re.search(r"@(?:beta|nightly|alpha|preview|twilight|daily|playtest|canary)$", package["token"])
        ]
        discovered = {
            "slug": app["slug"],
            "name": app["name"],
            "sourceRepository": f"https://github.com/{repo}",
            "sourceCategory": app["category"],
            "sourceLicense": app["license"],
            "sourcePlatforms": app["worksWith"],
            "existingApplicationId": existing.get(repo),
            "formulaCandidates": stable_formulas,
            "caskCandidates": stable_casks,
            "flathubDiscoveryCandidates": source_flathub_candidates(app["slug"], app["name"], flathub),
        }
        reviewed = previous.get(app["slug"], {})
        row = {
            key: value for key, value in reviewed.items()
            if key in {"verifiedLicense", "licenseEvidence", "packageEvidence", "reviewedAt", "status", "reason"}
        }
        row.update(discovered)
        row.update(reviewed_dispositions.get(app["slug"], {}))
        if repo in existing:
            row["status"] = "existing"
            row["reason"] = "The canonical repository is already present in the validated catalog."
        else:
            row.setdefault("status", "pending-license-and-package-review")
            row.setdefault("reason", "Canonical identity, upstream license and stable package mapping still require review.")
        ledger.append(row)
    (args.output / "ledger.json").write_text(
        json.dumps(ledger, ensure_ascii=False, indent=2) + "\n"
    )
    with (args.output / "winget-packages.json").open("w") as stream:
        json.dump(winget, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    with (args.output / "summary.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(dict.fromkeys(key for row in ledger for key in row)))
        writer.writeheader()
        for row in ledger:
            writer.writerow({key: json.dumps(value, ensure_ascii=False) if isinstance(value, (dict, list)) else value for key, value in row.items()})
    print(json.dumps({
        "applications": len(ledger),
        "existing": sum(row["status"] == "existing" for row in ledger),
        "formulaCandidates": sum(bool(row["formulaCandidates"]) for row in ledger),
        "caskCandidates": sum(bool(row["caskCandidates"]) for row in ledger),
        "wingetPackages": len(winget),
        "resolved": sum(row["status"] != "pending-license-and-package-review" for row in ledger),
    }, indent=2))


if __name__ == "__main__":
    main()
