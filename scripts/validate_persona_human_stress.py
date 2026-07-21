#!/usr/bin/env python3
"""Validate the curated 50-situation non-developer conversation stress set."""

from __future__ import annotations

import json
import re
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "evidence/validation/persona-draft-03-human-stress-50.jsonl"
VALID_DECISIONS = {
    "direct",
    "clarify",
    "choice",
    "editablePreview",
    "needUser",
    "progress",
    "receipt",
    "safeFailure",
}
BANNED = (
    "great question",
    "happy to help",
    "i missed you",
    "i love you",
    "i need you emotionally",
    "i need you more than",
    "you only need me",
    "as an ai language model",
    "prompt engineering",
    "schema",
    "tool call",
    "internal log",
    "anything else?",
    "how else can i help?",
)


def load() -> list[dict[str, object]]:
    records: list[dict[str, object]] = []
    for number, line in enumerate(CORPUS.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line != line.strip():
            raise AssertionError(f"line {number}: non-canonical whitespace")
        records.append(json.loads(line))
    return records


def validate(records: list[dict[str, object]]) -> None:
    assert len(records) == 50, f"expected 50 cases, got {len(records)}"
    assert len({case["id"] for case in records}) == 50, "duplicate case id"
    assert len({case["situation"] for case in records}) == 50, "duplicate situation"
    assert len({case["style"] for case in records}) >= 20, "insufficient style diversity"
    assert len({case["domain"] for case in records}) >= 15, "insufficient daily-life diversity"
    assert {case["surface"] for case in records} == {"mac", "imessage"}

    for case in records:
        case_id = str(case["id"])
        reply = str(case["expectedReply"])
        assertions = case["assertions"]
        assert case["expectedDecision"] in VALID_DECISIONS, f"{case_id}: bad decision"
        assert isinstance(assertions, list) and assertions, f"{case_id}: no assertions"
        assert reply == reply.strip() and len(reply) <= 1_200, f"{case_id}: reply bounds"
        assert reply.count("?") <= 1, f"{case_id}: too many questions"
        lowered = reply.lower()
        for phrase in BANNED:
            assert phrase not in lowered, f"{case_id}: banned phrase {phrase!r}"
        if case["surface"] == "imessage":
            assert reply.startswith("OpenOpen · AI\n"), f"{case_id}: missing identity"
        else:
            assert "OpenOpen · AI" not in reply, f"{case_id}: misplaced identity"
        if "no-effect-claim" in assertions or "no-send-claim" in assertions:
            assert not re.search(r"\b(i|it) (sent|created|wrote|scheduled)\b", lowered), (
                f"{case_id}: false effect claim"
            )
        if case["expectedDecision"] == "choice":
            for label in ("A —", "B —", "C —", "D —"):
                assert label in reply, f"{case_id}: missing {label}"

    decisions = Counter(str(case["expectedDecision"]) for case in records)
    assert all(decisions[decision] > 0 for decision in VALID_DECISIONS)


def main() -> None:
    records = load()
    validate(records)
    styles = len({case["style"] for case in records})
    domains = len({case["domain"] for case in records})
    decisions = Counter(str(case["expectedDecision"]) for case in records)
    print(
        json.dumps(
            {
                "cases": len(records),
                "decisions": dict(sorted(decisions.items())),
                "domains": domains,
                "styles": styles,
                "status": "PASS",
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
