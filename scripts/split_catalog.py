#!/usr/bin/env python3
"""Split catalog.json into catalog/index.json and per-category app files."""

import argparse
import json
import os
from pathlib import Path


def write_json(path: Path, value: object) -> None:
    temporary = path.with_suffix(".json.tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as output:
        json.dump(value, output, ensure_ascii=False, indent=2)
        output.write("\n")
    os.replace(temporary, path)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, default=Path("catalog.json"))
    parser.add_argument("destination", nargs="?", type=Path, default=Path("catalog"))
    args = parser.parse_args()

    with args.source.open(encoding="utf-8") as source:
        catalog = json.load(source)

    categories = catalog["categories"]
    category_ids = [category["id"] for category in categories]
    if len(category_ids) != len(set(category_ids)):
        raise ValueError("Category IDs must be unique")
    if any(
        not category_id
        or any(
            not (
                character.isascii()
                and (character.islower() or character.isdigit() or character == "-")
            )
            for character in category_id
        )
        for category_id in category_ids
    ):
        raise ValueError(
            "Category IDs must contain only lowercase ASCII letters, digits, and hyphens"
        )

    known_categories = set(category_ids)
    unknown = sorted({app["category"] for app in catalog["apps"]} - known_categories)
    if unknown:
        raise ValueError(f"Apps reference unknown categories: {', '.join(unknown)}")

    index = {key: value for key, value in catalog.items() if key != "apps"}
    category_files = {
        category_id: {"apps": [app for app in catalog["apps"] if app["category"] == category_id]}
        for category_id in category_ids
    }

    expected_files = {"index.json", *(f"{category_id}.json" for category_id in category_ids)}
    stale_files = [
        path for path in args.destination.glob("*.json") if path.name not in expected_files
    ]
    if stale_files:
        raise ValueError(f"Refusing to delete unlisted files: {', '.join(str(path) for path in stale_files)}")

    args.destination.mkdir(parents=True, exist_ok=True)

    write_json(args.destination / "index.json", index)
    for category_id, contents in category_files.items():
        write_json(args.destination / f"{category_id}.json", contents)

    with (args.destination / "index.json").open(encoding="utf-8") as source:
        written_index = json.load(source)
    merged = dict(written_index)
    merged["apps"] = []
    for category_id in category_ids:
        with (args.destination / f"{category_id}.json").open(encoding="utf-8") as source:
            contents = json.load(source)
        if set(contents) != {"apps"}:
            raise ValueError(f"Unexpected fields in {category_id}.json")
        if any(app["category"] != category_id for app in contents["apps"]):
            raise ValueError(f"Wrong category in {category_id}.json")
        merged["apps"].extend(contents["apps"])

    original_order = {
        app["id"]: position for position, app in enumerate(catalog["apps"])
    }
    merged["apps"].sort(key=lambda app: original_order[app["id"]])
    if merged != catalog:
        raise ValueError("Split catalog does not match the source")

    print(
        f"Split {len(catalog['apps'])} apps into {len(category_ids)} category files "
        f"under {args.destination}"
    )


if __name__ == "__main__":
    main()
