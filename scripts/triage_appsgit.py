#!/usr/bin/env python3
"""Build evidence-backed dispositions for unresolved AppsGit discovery rows.

Default mode is a dry run: it writes a proposed reviewed ledger and accounting
file under temp/pakkit/audit. --apply updates scripts/appsgit_reviewed.json only
after all pending repositories have complete repository/license evidence or a
stable repository 404 exclusion, and the cache contains no transport errors.
Existing manual decisions are never overwritten.
"""
from __future__ import annotations

import argparse
import collections
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "temp" / "pakkit"
EVIDENCE = CACHE / "appsgit-github-evidence"
LEDGER = CACHE / "audit" / "ledger.json"
REVIEWED = ROOT / "scripts" / "appsgit_reviewed.json"
SPDX = CACHE / "spdx-licenses.json"
PROPOSED = CACHE / "audit" / "proposed-appsgit-reviewed.json"
ACCOUNTING = CACHE / "audit" / "proposed-accounting.json"
GENERIC_SPDX = {
    "GPL-2.0",
    "GPL-3.0",
    "LGPL-2.1",
    "LGPL-3.0",
    "AGPL-3.0",
    "Apache-1.0",
    "Apache-1.1",
    "BSD-2-Clause-FreeBSD",
    "BSD-3-Clause-LBNL",
}
LOCALES = ("zh-CN", "zh-TW", "en", "ja", "ko")


def unique(values: list[str]) -> list[str]:
    return list(dict.fromkeys(value for value in values if value))


def expression_components(expression: str) -> list[str]:
    return [
        token
        for token in re.split(r"\s+", expression.replace("(", " ").replace(")", " "))
        if token and token not in {"AND", "OR", "WITH"} and not token.endswith("-exception")
    ]


def load_evidence() -> dict[str, dict]:
    result: dict[str, dict] = {}
    for directory in EVIDENCE.glob("*"):
        if not directory.is_dir():
            continue
        repository = directory.name.replace("__", "/").lower()
        item: dict = {"repository": repository, "directory": directory}
        if (directory / "repository.json").exists():
            item["repositoryDocument"] = json.loads((directory / "repository.json").read_text())
        if (directory / "license.json").exists():
            item["licenseDocument"] = json.loads((directory / "license.json").read_text())
        if (directory / "license-missing.json").exists():
            item["licenseMissing"] = json.loads((directory / "license-missing.json").read_text())
        if (directory / "repository-missing.json").exists():
            item["repositoryMissing"] = json.loads((directory / "repository-missing.json").read_text())
        if (directory / "error.json").exists():
            item["error"] = json.loads((directory / "error.json").read_text())
        result[repository] = item
    return result


def formula_license(row: dict, formulas: dict[str, dict], spdx: dict[str, dict]) -> tuple[str | None, list[str], list[str]]:
    expressions = []
    for name in row.get("formulaCandidates", []):
        package = formulas.get(name)
        if package and not package["deprecated"] and not package["disabled"] and package.get("license"):
            expressions.append(package["license"])
    if not expressions:
        return None, [], []
    expression = sorted(set(expressions))[0]
    components = expression_components(expression)
    non_osi = [component for component in components if spdx.get(component, {}).get("isOsiApproved") is not True]
    deprecated = [component for component in components if spdx.get(component, {}).get("isDeprecatedLicenseId") is True]
    return expression, non_osi, deprecated


def package_evidence(row: dict) -> list[str]:
    return unique([
        *[f"https://formulae.brew.sh/api/formula/{name}.json" for name in row.get("formulaCandidates", [])],
        *[f"https://formulae.brew.sh/api/cask/{name}.json" for name in row.get("caskCandidates", [])],
        *[f"https://flathub.org/apps/{app_id}" for app_id in row.get("flathubDiscoveryCandidates", [])],
    ])


def propose(row: dict, evidence_map: dict[str, dict], formulas: dict[str, dict], spdx: dict[str, dict], spdx_version: str) -> dict:
    repository = row["sourceRepository"].removeprefix("https://github.com/").lower()
    evidence = evidence_map.get(repository)
    package_urls = package_evidence(row)

    if evidence is None or "error" in evidence:
        return {
            "status": "held",
            "reason": "Canonical GitHub repository and license evidence is incomplete; transport/API failure is not treated as an exclusion.",
            "licenseEvidence": [row["sourceRepository"]],
            "packageEvidence": package_urls,
        }

    if "repositoryMissing" in evidence:
        missing = evidence["repositoryMissing"]
        provenance = missing.get("provenance", {})
        return {
            "status": "excluded",
            "reason": "The canonical GitHub repository endpoint returns a stable 404, so repository identity, source availability and an upstream open-source license grant cannot be established.",
            "licenseEvidence": [provenance.get("url", row["sourceRepository"])],
            "packageEvidence": package_urls,
        }

    if "repositoryDocument" not in evidence:
        return {
            "status": "held",
            "reason": "Canonical GitHub repository and license evidence is incomplete; transport/API failure is not treated as an exclusion.",
            "licenseEvidence": [row["sourceRepository"]],
            "packageEvidence": package_urls,
        }

    repo_document = evidence["repositoryDocument"]
    license_evidence = [repo_document["html_url"]]
    if "licenseDocument" in evidence:
        license_document = evidence["licenseDocument"]
        license_evidence.extend([license_document["html_url"], license_document["download_url"]])
    elif "licenseMissing" in evidence:
        missing = evidence["licenseMissing"]
        provenance = missing.get("provenance", {})
        license_evidence.append(provenance.get("url", row["sourceRepository"] + "/license"))
    else:
        return {
            "status": "held",
            "reason": "The repository evidence is cached, but a license-file response or explicit GitHub 404 is not yet available.",
            "licenseEvidence": license_evidence,
            "packageEvidence": package_urls,
        }

    formula_expression, non_osi_formula, deprecated_formula = formula_license(row, formulas, spdx)
    if formula_expression is not None:
        if non_osi_formula:
            return {
                "status": "excluded",
                "verifiedLicense": formula_expression,
                "reason": f"The Homebrew formula license expression contains a non-OSI-approved component: {', '.join(non_osi_formula)}.",
                "licenseEvidence": unique(license_evidence),
                "packageEvidence": package_urls,
            }
        if deprecated_formula:
            return {
                "status": "held",
                "reason": f"The Homebrew formula license expression contains deprecated SPDX identifier(s): {', '.join(deprecated_formula)}; current exact licensing remains under review.",
                "licenseEvidence": unique(license_evidence),
                "packageEvidence": package_urls,
            }

    if "licenseMissing" in evidence:
        return {
            "status": "held",
            "reason": "GitHub's license endpoint returns 404 for the canonical repository, so no current OSI-approved SPDX license grant has been established.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }

    license_document = evidence["licenseDocument"]
    spdx_id = license_document.get("license", {}).get("spdx_id")
    if spdx_id == "NOASSERTION":
        return {
            "status": "held",
            "reason": "GitHub cannot determine an SPDX license from the canonical license file; the complete product license and any bundled components require manual review.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }
    if spdx_id in GENERIC_SPDX:
        return {
            "status": "held",
            "reason": f"GitHub reports the generic SPDX identifier {spdx_id}; the exact only/or-later or exception terms must be established before acceptance.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }
    spdx_entry = spdx.get(spdx_id)
    if spdx_entry is None:
        return {
            "status": "held",
            "reason": f"The reported SPDX identifier {spdx_id} is not present in the local SPDX {spdx_version} list; license validity remains unverified.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }
    if spdx_entry.get("isOsiApproved") is not True:
        return {
            "status": "excluded",
            "verifiedLicense": spdx_id,
            "reason": f"The canonical repository's detected SPDX license {spdx_id} is not OSI-approved.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }
    if spdx_entry.get("isDeprecatedLicenseId") is True:
        return {
            "status": "held",
            "reason": f"The canonical repository uses deprecated SPDX identifier {spdx_id}; a current valid expression remains under review.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }

    routes = []
    if row.get("formulaCandidates"):
        routes.append("Homebrew formula bottle architecture and executable scope")
    if row.get("caskCandidates"):
        routes.append("Homebrew cask stability, permissions and actual Mach-O architecture")
    if row.get("flathubDiscoveryCandidates"):
        routes.append("Flathub identity, AppStream metadata and architecture")
    if not routes:
        return {
            "status": "held",
            "reason": "An OSI-approved license is evidenced, but no supported stable winget, Homebrew, Flathub, npm-safe, script or GitHub-release installation route has yet been verified.",
            "licenseEvidence": unique(license_evidence),
            "packageEvidence": package_urls,
        }
    return {
        "status": "held",
        "reason": "An OSI-approved license is evidenced, but acceptance still requires verification of: " + "; ".join(routes) + ".",
        "licenseEvidence": unique(license_evidence),
        "packageEvidence": package_urls,
    }


def validate_complete_evidence(ledger: list[dict], evidence_map: dict[str, dict]) -> list[str]:
    errors = []
    repositories = {
        row["sourceRepository"].removeprefix("https://github.com/").lower()
        for row in ledger
        if row["status"] == "pending-license-and-package-review"
    }
    for repository in sorted(repositories):
        evidence = evidence_map.get(repository)
        if evidence is None:
            errors.append(f"missing repository evidence: {repository}")
            continue
        if "error" in evidence:
            errors.append(f"transport/service error: {repository}")
            continue
        if "repositoryMissing" in evidence:
            continue
        if "repositoryDocument" not in evidence:
            errors.append(f"missing repository evidence: {repository}")
            continue
        if "licenseDocument" not in evidence and "licenseMissing" not in evidence:
            errors.append(f"missing license response or stable 404: {repository}")
    return errors


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true", help="update scripts/appsgit_reviewed.json after completeness checks")
    args = parser.parse_args()

    ledger = json.loads(LEDGER.read_text())
    reviewed = json.loads(REVIEWED.read_text())
    spdx_document = json.loads(SPDX.read_text())
    spdx = {item["licenseId"]: item for item in spdx_document["licenses"]}
    formulas = {item["name"]: item for item in json.loads((CACHE / "homebrew-formula.json").read_text())}
    evidence_map = load_evidence()
    proposed = dict(reviewed)
    counts = collections.Counter()
    reason_counts: dict[str, int] = {}
    new_decisions: dict[str, dict] = {}

    for row in ledger:
        slug = row["slug"]
        if row["status"] == "existing":
            counts["existing"] += 1
            continue
        if slug in reviewed:
            counts[f"reviewed_{reviewed[slug]['status']}"] += 1
            continue
        decision = propose(row, evidence_map, formulas, spdx, spdx_document["licenseListVersion"])
        proposed[slug] = decision
        new_decisions[slug] = decision
        counts[f"new_{decision['status']}"] += 1
        reason_counts[decision["reason"]] = reason_counts.get(decision["reason"], 0) + 1

    summary = {
        "totalSlugs": len(ledger),
        "uniqueSlugs": len({row["slug"] for row in ledger}),
        "counts": dict(counts),
        "proposedReviewedRows": len(proposed),
        "newDecisionRows": len(new_decisions),
        "reasonCounts": dict(sorted(reason_counts.items(), key=lambda item: (-item[1], item[0]))),
        "evidenceCache": {
            "repositories": len(evidence_map),
            "licenseDocuments": sum("licenseDocument" in item for item in evidence_map.values()),
            "licenseMissing404": sum("licenseMissing" in item for item in evidence_map.values()),
            "repositoryMissing404": sum("repositoryMissing" in item for item in evidence_map.values()),
            "errors": sum("error" in item for item in evidence_map.values()),
        },
    }

    PROPOSED.write_text(json.dumps(proposed, ensure_ascii=False, indent=2) + "\n")
    ACCOUNTING.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")

    if args.apply:
        completeness_errors = validate_complete_evidence(ledger, evidence_map)
        if completeness_errors:
            raise SystemExit("Refusing to apply incomplete triage:\n" + "\n".join(completeness_errors[:50]))
        if set(reviewed).intersection(new_decisions):
            raise SystemExit("Refusing to overwrite an existing reviewed decision")
        merged = dict(reviewed)
        for slug in sorted(new_decisions):
            merged[slug] = new_decisions[slug]
        REVIEWED.write_text(json.dumps(merged, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(summary, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
