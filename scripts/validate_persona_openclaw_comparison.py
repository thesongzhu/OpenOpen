#!/usr/bin/env python3
"""Validate the source-level OpenOpen/OpenClaw 50-case comparison."""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OPENOPEN = ROOT / "evidence/validation/persona-draft-03-human-stress-50.jsonl"
COMPARISON = (
    ROOT
    / "evidence/validation/persona-draft-03-openclaw-v2026.7.1-comparison.jsonl"
)
REPORT = ROOT / "evidence/validation/PERSONA_DRAFT_03_OPENCLAW_COMPARISON.md"
VALID_COVERAGE = {"explicit", "partial", "unspecified", "different-default"}
VALID_FINDINGS = {"aligned-contract", "openopen-more-explicit", "different-default"}
VALID_BASIS = {
    "AGENTS_DEFAULT",
    "CHARACTER_EVAL",
    "PERSONAL_PACK",
    "SOUL",
    "SYSTEM_PROMPT",
    "USER_CONTEXT",
}
PINNED_SOURCE_SHA256 = {
    "docs/concepts/personal-agent-benchmark-pack.md": (
        "35da45e4b22b1044a777fa8d6bce87f9ace377950dd0af3f2419b40cfe4d9be6"
    ),
    "docs/concepts/qa-e2e-automation.md": (
        "602dcdd6743d63a4e11c40096165b95aa8c96aeee0c526cff0af365c717c2076"
    ),
    "docs/concepts/system-prompt.md": (
        "1aabd41b5d4b51ed139d47b506017322c240bb1002bae901886d5f7991c0dc5e"
    ),
    "docs/reference/AGENTS.default.md": (
        "645342f8c6e2805135817cf4bbc2c8bd1d57066054ed671eda93876b2762ffb1"
    ),
    "docs/reference/templates/SOUL.md": (
        "79c61aaee618c787c164c5d767053a83c6e218191b3b6ebb66fd07320b071ce8"
    ),
    "docs/reference/templates/USER.md": (
        "599bd4d663c852bca679a341d53605c1a48b7cd7601bd7d102ee5407828dbacb"
    ),
    "extensions/qa-lab/src/character-eval.ts": (
        "ed6c6d72d5e3fd0d1f240a7acebf5929af7f98bfee7fa82a1de181988e6a7dc6"
    ),
    "qa/scenarios/character/character-vibes-c3po.yaml": (
        "74864f2d3152181112e9f6f2b7bda7adecbd2ff80994223b1314d9fc8cc36368"
    ),
    "qa/scenarios/character/character-vibes-gollum.yaml": (
        "cd4dfd9e2f0830e21bb9c8ffbb5e7439ead15e76983035105a2538e70020507d"
    ),
}


def load_jsonl(path: Path) -> list[dict[str, object]]:
    records: list[dict[str, object]] = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        assert line and line == line.strip(), f"{path.name}:{number}: non-canonical line"
        records.append(json.loads(line))
    return records


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--openclaw-source-root",
        type=Path,
        help="Optional official OpenClaw v2026.7.1 checkout to hash in full.",
    )
    return parser.parse_args()


def verify_source_bindings(source_root: Path | None) -> int:
    report = REPORT.read_text(encoding="utf-8")
    for relative_path, expected_digest in PINNED_SOURCE_SHA256.items():
        binding = f"- `{relative_path}`, SHA-256\n  `{expected_digest}`"
        assert binding in report, f"report binding mismatch: {relative_path}"
    if source_root is None:
        return 0
    assert source_root.is_dir(), f"not a source directory: {source_root}"
    for relative_path, expected_digest in PINNED_SOURCE_SHA256.items():
        source_path = source_root / relative_path
        assert source_path.is_file(), f"source missing: {source_path}"
        actual_digest = hashlib.sha256(source_path.read_bytes()).hexdigest()
        assert actual_digest == expected_digest, f"source digest mismatch: {relative_path}"
    return len(PINNED_SOURCE_SHA256)


def main() -> None:
    args = parse_args()
    openopen = load_jsonl(OPENOPEN)
    comparison = load_jsonl(COMPARISON)
    expected_ids = [str(case["id"]) for case in openopen]
    actual_ids = [str(case["id"]) for case in comparison]

    assert len(openopen) == 50
    assert len(comparison) == 50
    assert actual_ids == expected_ids, "comparison must preserve the canonical case order"
    assert len(set(actual_ids)) == 50
    assert len(PINNED_SOURCE_SHA256) == 9
    assert all(
        len(digest) == 64 and set(digest) <= set("0123456789abcdef")
        for digest in PINNED_SOURCE_SHA256.values()
    )
    verified_source_files = verify_source_bindings(args.openclaw_source_root)

    for record in comparison:
        case_id = str(record["id"])
        coverage = str(record["coverage"])
        finding = str(record["finding"])
        basis = record["basis"]
        note = str(record["note"])
        assert coverage in VALID_COVERAGE, f"{case_id}: invalid coverage"
        assert finding in VALID_FINDINGS, f"{case_id}: invalid finding"
        assert isinstance(basis, list) and basis, f"{case_id}: missing basis"
        assert set(map(str, basis)) <= VALID_BASIS, f"{case_id}: invalid basis"
        assert note == note.strip() and note.endswith("."), f"{case_id}: bad note"
        if coverage == "explicit":
            assert finding == "aligned-contract", f"{case_id}: explicit must be aligned"
        if coverage == "different-default":
            assert finding == "different-default", f"{case_id}: default difference mismatch"

    coverage_counts = Counter(str(record["coverage"]) for record in comparison)
    finding_counts = Counter(str(record["finding"]) for record in comparison)
    assert coverage_counts == {
        "explicit": 6,
        "partial": 20,
        "unspecified": 23,
        "different-default": 1,
    }
    print(
        json.dumps(
            {
                "baseline": "openclaw/openclaw v2026.7.1",
                "cases": len(comparison),
                "coverage": dict(sorted(coverage_counts.items())),
                "findings": dict(sorted(finding_counts.items())),
                "status": "PASS",
                "sourceFilesVerified": verified_source_files,
                "testLevel": "source-contract",
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
