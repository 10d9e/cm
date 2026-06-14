#!/usr/bin/env python3
"""Generate docs/data/leaderboard.json from the authoritative ledger.

Parses the RESULTS.md leaderboard table and enriches each row with the full
"Approach" narrative from its history/entries/ file. The static GitHub Pages
site (docs/) renders this JSON. Run from the repo root; safe to run anywhere.
"""
from __future__ import annotations

import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "RESULTS.md"
OUT = ROOT / "docs" / "data" / "leaderboard.json"

ROW_RE = re.compile(r"^\|\s*\d{4}\s*\|")
LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
FIRST_INT_RE = re.compile(r"-?\d+")


def cells(line: str) -> list[str]:
    return [c.strip() for c in line.strip().strip("|").split("|")]


def approach_text(entry_rel: str) -> str:
    """Return the full '## Approach' section of a history entry, if present."""
    if not entry_rel:
        return ""
    path = ROOT / entry_rel
    if not path.is_file():
        return ""
    text = path.read_text(encoding="utf-8")
    out: list[str] = []
    capturing = False
    for line in text.splitlines():
        if line.startswith("## "):
            if line.strip().lower() == "## approach":
                capturing = True
                continue
            if capturing:
                break
        elif capturing:
            out.append(line)
    return "\n".join(out).strip()


def first_int(s: str) -> int | None:
    m = FIRST_INT_RE.search(s)
    return int(m.group()) if m else None


def main() -> int:
    repo = os.environ.get("GITHUB_REPOSITORY", "10d9e/cm")
    rows: list[dict] = []

    for raw in RESULTS.read_text(encoding="utf-8").splitlines():
        if not ROW_RE.match(raw):
            continue
        c = cells(raw)
        if len(c) < 9:
            continue
        entry_id, date, author, score, delta, vs_zstd, commit, entry, note = c[:9]
        link_m = LINK_RE.search(entry)
        entry_rel = link_m.group(1) if link_m else ""
        full = approach_text(entry_rel)
        rows.append(
            {
                "id": entry_id,
                "date": date,
                "author": author,
                "score": first_int(score),
                "delta": delta,
                "deltaValue": first_int(delta) if "baseline" not in delta else None,
                "vsZstd": vs_zstd,
                "commit": commit.strip("`"),
                "entryPath": entry_rel,
                "note": full or note,
                "isRecord": "record" in delta.lower(),
            }
        )

    scored = [r for r in rows if r["score"] is not None]
    baseline = scored[0]["score"] if scored else None
    record_row = min(scored, key=lambda r: r["score"]) if scored else None

    data = {
        "repo": repo,
        "generatedAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "baseline": baseline,
        "record": (
            {
                "id": record_row["id"],
                "score": record_row["score"],
                "author": record_row["author"],
            }
            if record_row
            else None
        ),
        "entries": rows,
    }

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(rows)} entries)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
