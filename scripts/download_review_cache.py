#!/usr/bin/env python3
"""Download authoritative catalog-review indexes into temp/pakkit atomically.

The cached documents support review; they are not catalog import evidence by
themselves. Existing valid cache files survive failed refreshes.
"""
from __future__ import annotations

import argparse
import ast
import base64
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
from pathlib import Path
import subprocess
import zipfile
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "temp" / "pakkit"
USER_AGENT = "Pakkit-catalog-review/1.0"


def fetch(url: str, attempts: int = 4) -> bytes:
    result = subprocess.run(
        ["curl", "--fail", "--silent", "--show-error", "--location",
         "--retry", str(attempts - 1), "--retry-all-errors",
         "--connect-timeout", "20", "--max-time", "240",
         "--user-agent", USER_AGENT, url],
        check=True, capture_output=True,
    )
    return result.stdout


def atomic_write(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".part")
    temporary.write_bytes(content)
    temporary.replace(path)


def json_document(url: str, path: Path) -> str:
    document = json.loads(fetch(url))
    atomic_write(path, (json.dumps(document, ensure_ascii=False) + "\n").encode())
    return f"{path.name}: valid JSON"


def github_files(manifest: Path) -> str:
    requests = json.loads(manifest.read_text())
    if not isinstance(requests, list):
        raise ValueError("GitHub file manifest must be a list")
    outputs = []
    for item in requests:
        output = Path(item["output"])
        if output.is_absolute() or ".." in output.parts:
            raise ValueError(f"Unsafe GitHub output path: {output}")
        outputs.append(item["output"])
    if len(outputs) != len(set(outputs)):
        raise ValueError("GitHub output paths must be unique")
    failures = []
    with ThreadPoolExecutor(max_workers=4) as executor:
        pending = {}
        for item in requests:
            url = f"https://api.github.com/repos/{item['repository']}/contents/{quote(item['path'], safe='/')}?ref={quote(item.get('ref', 'HEAD'), safe='')}"
            pending[executor.submit(fetch, url)] = item
        for future in as_completed(pending):
            item = pending[future]
            try:
                response = json.loads(future.result())
                target = CACHE / "github-files" / item["output"]
                if isinstance(response, list):
                    atomic_write(target, (json.dumps(response, ensure_ascii=False) + "\n").encode())
                    continue
                if response.get("encoding") != "base64":
                    raise ValueError("Expected base64 file content")
                atomic_write(target, base64.b64decode(response["content"]))
            except Exception as error:
                failures.append(item["output"])
                print(f"GitHub file {item['output']}: FAILED: {error}")
    if failures:
        raise RuntimeError("GitHub file downloads failed: " + ", ".join(sorted(failures)))
    return f"{len(requests)} GitHub files downloaded"


def winget_index() -> str:
    archive = CACHE / "winget-source.msix"
    atomic_write(archive, fetch("https://cdn.winget.microsoft.com/cache/source.msix"))
    with zipfile.ZipFile(archive) as package:
        content = package.read("Public/index.db")
    atomic_write(CACHE / "winget-source.db", content)
    return f"winget source: {len(content)} bytes"


def flatpak_apps() -> str:
    repo = fetch("https://dl.flathub.org/repo/flathub.flatpakrepo")
    atomic_write(CACHE / "flathub.flatpakrepo", repo)
    return "Flathub repository metadata downloaded; run refresh_flatpak_index to resolve application architectures"


def reviewed_winget_packages() -> dict[str, str | None]:
    tree = ast.parse((ROOT / "scripts" / "import_verified_apps.py").read_text())
    reviewed = next(node for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == "REVIEWED" for target in node.targets))
    packages = {}
    for entry in reviewed.value.values:
        fields = {key.value: value for key, value in zip(entry.keys, entry.values) if isinstance(key, ast.Constant)}
        if "winget" in fields:
            package_id = ast.literal_eval(fields["winget"])
            version = ast.literal_eval(fields["wingetVersion"]) if "wingetVersion" in fields else None
            packages[package_id] = version
    return packages


def version_key(version: str) -> tuple:
    import re
    return tuple(int(part) if part.isdigit() else part for part in re.split(r"([0-9]+)", version))


def winget_manifests() -> str:
    index = json.loads((CACHE / "audit" / "winget-packages.json").read_text())
    jobs = {}
    for package_id, pinned in reviewed_winget_packages().items():
        candidates = [item for item in index[package_id] if item[0][0].isdigit() and (pinned is None or item[0] == pinned)]
        if not candidates:
            raise ValueError(f"No reviewed winget version for {package_id}: {pinned}")
        version, package_path = max(candidates, key=lambda item: version_key(item[0]))
        cache = CACHE / "winget-reviewed" / package_id / "installer.yaml"
        if cache.exists():
            continue
        prefix = f"manifests/{package_path}/{version}/{package_id}"
        api_url = f"https://api.github.com/repos/microsoft/winget-pkgs/contents/{quote(prefix, safe='/')}.installer.yaml?ref=master"
        jobs[package_id] = (cache, api_url, version)
    failures = []
    with ThreadPoolExecutor(max_workers=4) as executor:
        pending = {executor.submit(fetch, url): (package_id, cache, version) for package_id, (cache, url, version) in jobs.items()}
        for future in as_completed(pending):
            package_id, cache, version = pending[future]
            try:
                import yaml
                response = json.loads(future.result())
                if response.get("encoding") != "base64":
                    raise ValueError("Expected base64 GitHub content")
                document = yaml.safe_load(base64.b64decode(response["content"]))
                if document.get("PackageIdentifier") != package_id or str(document.get("PackageVersion")) != version or not document.get("Installers"):
                    raise ValueError("Manifest identity, version or installers do not match the reviewed selection")
                atomic_write(cache, yaml.safe_dump(document, sort_keys=False, allow_unicode=True).encode())
            except Exception as error:
                failures.append(package_id)
                print(f"winget manifest {package_id}: FAILED: {error}")
    if failures:
        raise RuntimeError("Winget manifest downloads failed: " + ", ".join(sorted(failures)))
    return f"{len(jobs)} winget manifests downloaded; reviewed cache complete"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--github-files", type=Path, help="JSON list of repository, path, optional ref and output")
    args = parser.parse_args()
    jobs = {
        "homebrew-formula": lambda: json_document("https://formulae.brew.sh/api/formula.json", CACHE / "homebrew-formula.json"),
        "homebrew-cask": lambda: json_document("https://formulae.brew.sh/api/cask.json", CACHE / "homebrew-cask.json"),
        "flathub-appstream": lambda: json_document("https://flathub.org/api/v2/appstream", CACHE / "flathub-appstream.json"),
        "winget": winget_index,
        "winget-manifests": winget_manifests,
    }
    if args.github_files:
        print(github_files(args.github_files))
        return
    selected = args.only or list(jobs)
    failures = []
    with ThreadPoolExecutor(max_workers=3) as executor:
        pending = {executor.submit(jobs[name]): name for name in selected}
        for future in as_completed(pending):
            name = pending[future]
            try:
                print(f"{name}: {future.result()}")
            except Exception as error:
                failures.append(name)
                print(f"{name}: FAILED: {error}")
    if failures:
        raise SystemExit("Refresh failed (old caches retained): " + ", ".join(sorted(failures)))


if __name__ == "__main__":
    main()
