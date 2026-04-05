#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run acceptance-v1 for codex-worker-rs.")
    parser.add_argument(
        "--manifest",
        default="tests/acceptance/manifest.json",
        help="Path to acceptance manifest.",
    )
    parser.add_argument(
        "--doc",
        default="docs/parity/acceptance-v1.md",
        help="Path to parity documentation.",
    )
    parser.add_argument(
        "--report",
        default="target/acceptance-v1/report.json",
        help="Path to JSON report output.",
    )
    return parser.parse_args()


def load_json(path: pathlib.Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:  # pragma: no cover - exercised in CI/runtime
        raise SystemExit(f"manifest parse error: {exc}") from exc
    if not isinstance(data, dict):
        raise SystemExit("manifest root must be an object")
    return data


def validate_manifest(data: dict[str, Any]) -> list[dict[str, Any]]:
    cases = data.get("cases")
    if not isinstance(cases, list) or not cases:
        raise SystemExit("manifest.cases must be a non-empty array")

    required_fields = {"id", "title", "priority", "covers", "rust_stage", "status", "commands"}
    seen_ids: set[str] = set()
    normalized_cases: list[dict[str, Any]] = []

    for idx, case in enumerate(cases):
        if not isinstance(case, dict):
            raise SystemExit(f"case[{idx}] must be an object")

        missing = sorted(required_fields - set(case.keys()))
        if missing:
            raise SystemExit(f"case[{idx}] missing fields: {', '.join(missing)}")

        case_id = case["id"]
        if not isinstance(case_id, str) or not re.fullmatch(r"ACPT-\d{3}", case_id):
            raise SystemExit(f"invalid case id at index {idx}: {case_id!r}")
        if case_id in seen_ids:
            raise SystemExit(f"duplicate case id in manifest: {case_id}")
        seen_ids.add(case_id)

        if case.get("priority") != "required-for-parity":
            raise SystemExit(f"case {case_id} must have priority=required-for-parity")

        covers = case.get("covers")
        if not isinstance(covers, list) or not covers or not all(
            isinstance(item, str) and item.strip() for item in covers
        ):
            raise SystemExit(f"case {case_id} must have non-empty covers[] of strings")

        rust_stage = case.get("rust_stage")
        if not isinstance(rust_stage, list) or not rust_stage or not all(
            isinstance(item, str) and item.strip() for item in rust_stage
        ):
            raise SystemExit(f"case {case_id} must have non-empty rust_stage[] of strings")

        status = case.get("status")
        if not isinstance(status, str) or not status.strip():
            raise SystemExit(f"case {case_id} must have non-empty status")

        commands = case.get("commands")
        if not isinstance(commands, list) or not commands:
            raise SystemExit(f"case {case_id} must have non-empty commands[]")
        for cmd_idx, command in enumerate(commands):
            if not isinstance(command, list) or not command:
                raise SystemExit(f"case {case_id} command[{cmd_idx}] must be a non-empty array")
            if not all(isinstance(part, str) and part for part in command):
                raise SystemExit(
                    f"case {case_id} command[{cmd_idx}] must contain only non-empty strings"
                )

        normalized_cases.append(case)

    return normalized_cases


def validate_doc_parity(doc_text: str, manifest_ids: list[str]) -> None:
    doc_ids = re.findall(r"ACPT-\d{3}", doc_text)
    if not doc_ids:
        raise SystemExit("no ACPT-* ids found in acceptance-v1.md")

    doc_set = set(doc_ids)
    manifest_set = set(manifest_ids)
    missing_in_doc = sorted(manifest_set - doc_set)
    extra_in_doc = sorted(doc_set - manifest_set)

    if missing_in_doc:
        raise SystemExit(
            "ids missing in docs/parity/acceptance-v1.md: " + ", ".join(missing_in_doc)
        )
    if extra_in_doc:
        raise SystemExit(
            "ids present in docs but missing in manifest: " + ", ".join(extra_in_doc)
        )


def summarize_output(text: str) -> str:
    if not text:
        return ""
    lines = text.strip().splitlines()
    tail = lines[-20:]
    summary = "\n".join(tail)
    if len(summary) > 4000:
        summary = summary[-4000:]
    return summary


def run_command(command: list[str], cwd: pathlib.Path) -> dict[str, Any]:
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    duration = round(time.perf_counter() - started, 3)
    return {
        "command": command,
        "exit_code": completed.returncode,
        "duration_seconds": duration,
        "stdout_tail": summarize_output(completed.stdout),
        "stderr_tail": summarize_output(completed.stderr),
    }


def run_cases(cases: list[dict[str, Any]], cwd: pathlib.Path) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []

    for case in cases:
        case_id = case["id"]
        title = case["title"]
        print(f"[acceptance-v1] {case_id} {title}")

        command_results = []
        case_passed = True
        for command in case["commands"]:
            print("+", " ".join(command))
            result = run_command(command, cwd)
            command_results.append(result)
            if result["exit_code"] != 0:
                case_passed = False
                print(f"  -> FAIL ({result['exit_code']})")
                break
            print("  -> PASS")

        results.append(
            {
                "id": case_id,
                "title": title,
                "status": "PASS" if case_passed else "FAIL",
                "covers": case["covers"],
                "commands": command_results,
            }
        )

    return results


def write_report(
    report_path: pathlib.Path,
    manifest_path: pathlib.Path,
    doc_path: pathlib.Path,
    suite: str,
    results: list[dict[str, Any]],
) -> None:
    passed = sum(1 for result in results if result["status"] == "PASS")
    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "suite": suite,
        "manifest_path": manifest_path.as_posix(),
        "doc_path": doc_path.as_posix(),
        "case_count": len(results),
        "passed_count": passed,
        "failed_count": len(results) - passed,
        "all_passed": passed == len(results),
        "cases": results,
    }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    manifest_path = (repo_root / args.manifest).resolve()
    doc_path = (repo_root / args.doc).resolve()
    report_path = (repo_root / args.report).resolve()

    if not manifest_path.is_file():
        raise SystemExit(f"manifest file not found: {manifest_path}")
    if not doc_path.is_file():
        raise SystemExit(f"parity doc not found: {doc_path}")

    manifest = load_json(manifest_path)
    cases = validate_manifest(manifest)
    validate_doc_parity(doc_path.read_text(encoding="utf-8"), [case["id"] for case in cases])

    results = run_cases(cases, repo_root)
    write_report(report_path, manifest_path, doc_path, manifest.get("suite", "acceptance-v1"), results)

    passed = sum(1 for result in results if result["status"] == "PASS")
    print(f"[acceptance-v1] report written to {report_path}")
    print(f"[acceptance-v1] passed {passed}/{len(results)} cases")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
