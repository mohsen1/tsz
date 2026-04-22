#!/usr/bin/env python3
"""Write a sanitized markdown summary for one GCP CI suite."""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path
from typing import Any

MAX_SUMMARY_CHARS = 60000
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


def strip_ansi(text: str) -> str:
    return ANSI_RE.sub("", text)


def load_json(path: Path) -> Any | None:
    try:
        with path.open(encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return None
    except json.JSONDecodeError:
        return None


def read_lines(path: Path, limit: int = 80) -> list[str]:
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except FileNotFoundError:
        return []
    return lines[-limit:]


def rel(path: str) -> str:
    return path.removeprefix("/workspace/").removeprefix("./")


def clip(value: object, limit: int = 240) -> str:
    text = strip_ansi(str(value))
    text = text.replace("\r", " ").replace("\n", " ")
    if len(text) <= limit:
        return text
    return text[: limit - 3] + "..."


def md_escape(value: object) -> str:
    text = clip(value, 2000)
    return text.replace("|", "\\|").replace("<", "&lt;").replace(">", "&gt;")


def code(value: object, limit: int = 240) -> str:
    text = md_escape(clip(value, limit)).replace("`", "'")
    return f"`{text}`"


def metric(data: dict[str, Any], key: str, default: object = 0) -> object:
    return data.get(key, default)


def append_env(lines: list[str], suite: str, exit_code: int) -> None:
    trigger = os.environ.get("TRIGGER_NAME", "")
    build_id = os.environ.get("BUILD_ID", "")
    short_sha = os.environ.get("SHORT_SHA", "")
    branch = os.environ.get("BRANCH_NAME") or os.environ.get("_HEAD_BRANCH", "")
    pr_number = os.environ.get("_PR_NUMBER", "")

    title_bits = [code(suite)]
    if trigger:
        title_bits.append(f"via {code(trigger)}")
    if short_sha:
        title_bits.append(f"on {code(short_sha)}")
    lines.append("### TSZ CI Summary")
    lines.append("")
    lines.append(" ".join(title_bits))
    lines.append("")
    lines.append("| Field | Value |")
    lines.append("| --- | --- |")
    lines.append(f"| Result | {code('success' if exit_code == 0 else 'failure')} |")
    lines.append(f"| Exit code | {code(exit_code)} |")
    if branch:
        lines.append(f"| Branch | {code(branch)} |")
    if pr_number:
        lines.append(f"| PR | {code('#' + md_escape(pr_number))} |")
    if build_id:
        lines.append(f"| Cloud Build | {code(build_id)} |")
    lines.append("")


def emit_summary(metrics_dir: Path, logs_dir: Path, lines: list[str]) -> None:
    aggregate = load_json(metrics_dir / "emit.json") or {}
    if aggregate:
        lines.append("#### Emit Aggregate")
        lines.append("")
        lines.append("| Kind | Passed | Total | Skipped | Timeouts | Pass rate |")
        lines.append("| --- | ---: | ---: | ---: | ---: | ---: |")
        lines.append(
            f"| JS | {md_escape(metric(aggregate, 'js_passed'))} | "
            f"{md_escape(metric(aggregate, 'js_total'))} | "
            f"{md_escape(metric(aggregate, 'js_skipped'))} | "
            f"{md_escape(metric(aggregate, 'js_timeouts'))} | "
            f"{md_escape(metric(aggregate, 'js_pass_rate', '0.0'))}% |"
        )
        lines.append(
            f"| DTS | {md_escape(metric(aggregate, 'dts_passed'))} | "
            f"{md_escape(metric(aggregate, 'dts_total'))} | "
            f"{md_escape(metric(aggregate, 'dts_skipped'))} |  | "
            f"{md_escape(metric(aggregate, 'dts_pass_rate', '0.0'))}% |"
        )
        lines.append("")

    failures: list[dict[str, Any]] = []
    timeouts: list[dict[str, Any]] = []
    for detail_path in sorted(metrics_dir.glob("emit-detail-*.json")):
        detail = load_json(detail_path) or {}
        for result in detail.get("results", []):
            js_status = result.get("jsStatus")
            dts_status = result.get("dtsStatus")
            if js_status == "timeout" or dts_status == "timeout":
                timeouts.append(result)
            elif js_status not in (None, "pass", "skip") or dts_status not in (None, "pass", "skip"):
                failures.append(result)

    if timeouts:
        lines.append("#### Emit Timeouts")
        lines.append("")
        for result in timeouts[:20]:
            detail = (
                f"{rel(result.get('testPath', ''))}; "
                f"js={result.get('jsStatus')}, dts={result.get('dtsStatus')}"
            )
            lines.append(f"- {code(result.get('name', '<unknown>'))} ({md_escape(detail)})")
        if len(timeouts) > 20:
            lines.append(f"- ... {len(timeouts) - 20} more")
        lines.append("")

    if failures:
        lines.append("#### Emit Failures")
        lines.append("")
        for result in failures[:30]:
            detail = (
                f"{rel(result.get('testPath', ''))}; "
                f"js={result.get('jsStatus')}, dts={result.get('dtsStatus')}"
            )
            error = result.get("jsError") or result.get("dtsError")
            if error:
                detail += f"; {clip(error, 160)}"
            lines.append(f"- {code(result.get('name', '<unknown>'))} ({md_escape(detail)})")
        if len(failures) > 30:
            lines.append(f"- ... {len(failures) - 30} more")
        lines.append("")

    for log_path in sorted((logs_dir / "emit").glob("*.log")):
        tail = [
            line
            for line in read_lines(log_path, 40)
            if line.startswith("Timeouts")
            or line.startswith("  T ")
            or line.startswith("JSON results written")
        ]
        if tail:
            lines.append(f"#### {code(rel(str(log_path)))} timeout tail")
            lines.append("")
            lines.append("```text")
            lines.extend(strip_ansi(line) for line in tail[:40])
            lines.append("```")
            lines.append("")


def fourslash_summary(metrics_dir: Path, logs_dir: Path, lines: list[str]) -> None:
    shard_metrics = []
    for path in sorted(metrics_dir.glob("fourslash-shard-*.json")):
        data = load_json(path)
        if data:
            shard_metrics.append(data)

    if shard_metrics:
        passed = sum(int(item.get("passed") or 0) for item in shard_metrics)
        total = sum(int(item.get("total") or 0) for item in shard_metrics)
        lines.append("#### Fourslash Aggregate")
        lines.append("")
        lines.append(f"- Passed `{passed}` of `{total}` tests across `{len(shard_metrics)}` shards.")
        lines.append("")

    detail_files = sorted(metrics_dir.glob("fourslash-detail-*.json"))
    details = [load_json(path) or {} for path in detail_files]
    if not details:
        details = [load_json(Path("scripts/fourslash/fourslash-detail.json")) or {}]

    failures = [
        result
        for detail in details
        for result in detail.get("results", [])
        if result.get("status") != "pass" or result.get("timedOut")
    ]
    if failures:
        lines.append("#### Fourslash Failures")
        lines.append("")
        for result in failures[:30]:
            status = "timeout" if result.get("timedOut") else result.get("status")
            detail = f"{result.get('file', '')}; status={status}"
            if result.get("firstFailure"):
                detail += f"; {clip(result.get('firstFailure'), 160)}"
            lines.append(f"- {code(result.get('name', '<unknown>'))} ({md_escape(detail)})")
        if len(failures) > 30:
            lines.append(f"- ... {len(failures) - 30} more")
        lines.append("")

    for log_path in sorted((logs_dir / "fourslash").glob("*.log")):
        tail = [
            strip_ansi(line)
            for line in read_lines(log_path, 80)
            if "Worker " in line
            or line.startswith("Results:")
            or "Cannot find module" in line
            or "TIMEOUT" in line
        ]
        if tail:
            lines.append(f"#### {code(rel(str(log_path)))} tail")
            lines.append("")
            lines.append("```text")
            lines.extend(strip_ansi(line) for line in tail[:60])
            lines.append("```")
            lines.append("")


def conformance_summary(metrics_dir: Path, logs_dir: Path, lines: list[str], exit_code: int) -> None:
    metrics = load_json(metrics_dir / "conformance.json") or {}
    if metrics:
        lines.append("#### Conformance Aggregate")
        lines.append("")
        lines.append(
            f"- Passed {code(metrics.get('passed', 0))} of {code(metrics.get('total', 0))} tests "
            f"with {code(metrics.get('workers', '?'))} workers."
        )
        lines.append(f"- Wrapper exit: {code(metrics.get('rc', '?'))}")
        lines.append("")

    detail = load_json(Path("scripts/conformance/conformance-detail.json")) or {}
    aggregates = detail.get("aggregates", {})
    categories = aggregates.get("categories", {})
    if categories:
        lines.append("#### Conformance Buckets")
        lines.append("")
        lines.append("| Bucket | Count |")
        lines.append("| --- | ---: |")
        for key, count in sorted(categories.items(), key=lambda item: (-int(item[1]), item[0])):
            lines.append(f"| {md_escape(key)} | {md_escape(count)} |")
        lines.append("")

    for label, key in [
        ("Top missing diagnostic codes", "top_missing_codes"),
        ("Top extra diagnostic codes", "top_extra_codes"),
    ]:
        items = aggregates.get(key, [])
        if items:
            lines.append(f"#### {label}")
            lines.append("")
            lines.append(", ".join(f"{code(i.get('code'))} x{i.get('count')}" for i in items[:12]))
            lines.append("")
            lines.append("")

    interesting_log_lines: list[str] = []
    for log_path in [logs_dir / "conformance" / "full.log", logs_dir / "full-ci.log"]:
        for raw in read_lines(log_path, 400):
            line = strip_ansi(raw)
            lower = line.lower()
            if (
                line.startswith("FINAL RESULTS:")
                or "regression" in lower
                or "incomplete" in lower
                or lower.startswith("error:")
                or "wrapper failed" in lower
            ):
                interesting_log_lines.append(line)

    if interesting_log_lines:
        lines.append("#### Conformance Signals")
        lines.append("")
        lines.append("```text")
        lines.extend(interesting_log_lines[-40:])
        lines.append("```")
        lines.append("")

    last_run = Path("scripts/conformance/conformance-last-run.txt")
    failed_rows: list[str] = []
    xfail_rows: list[str] = []
    for line in read_lines(last_run, 20000):
        if line.startswith(("FAIL ", "CRASH ", "TIMEOUT ")):
            failed_rows.append(line)
        elif line.startswith("XFAIL "):
            xfail_rows.append(line)
    if failed_rows:
        lines.append("#### Conformance Failed Cases")
        lines.append("")
        for row in failed_rows[:40]:
            lines.append(f"- {code(row, 500)}")
        if len(failed_rows) > 40:
            lines.append(f"- ... {len(failed_rows) - 40} more")
        lines.append("")
    elif exit_code != 0 and xfail_rows:
        lines.append("#### Conformance Expected-Failure Sample")
        lines.append("")
        for row in xfail_rows[:20]:
            lines.append(f"- {code(row, 500)}")
        if len(xfail_rows) > 20:
            lines.append(f"- ... {len(xfail_rows) - 20} more")
        lines.append("")


def fallback_tail(logs_dir: Path, lines: list[str]) -> None:
    log_paths = sorted(logs_dir.glob("**/*.log"))
    if not log_paths:
        return
    lines.append("#### Log Tail")
    lines.append("")
    for path in log_paths[:4]:
        tail = [strip_ansi(line) for line in read_lines(path, 40)]
        if not tail:
            continue
        lines.append(f"##### {code(rel(str(path)))}")
        lines.append("")
        lines.append("```text")
        lines.extend(tail)
        lines.append("```")
        lines.append("")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", required=True)
    parser.add_argument("--exit-code", type=int, default=0)
    parser.add_argument("--metrics-dir", default=".ci-metrics")
    parser.add_argument("--logs-dir", default=".ci-logs")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    metrics_dir = Path(args.metrics_dir)
    logs_dir = Path(args.logs_dir)
    lines: list[str] = []
    append_env(lines, args.suite, args.exit_code)

    if args.suite == "emit":
        emit_summary(metrics_dir, logs_dir, lines)
    elif args.suite == "fourslash":
        fourslash_summary(metrics_dir, logs_dir, lines)
    elif args.suite == "conformance":
        conformance_summary(metrics_dir, logs_dir, lines, args.exit_code)
    else:
        fallback_tail(logs_dir, lines)

    if len(lines) < 20:
        lines.append("No structured suite details were produced.")
        lines.append("")

    summary = "\n".join(lines).rstrip() + "\n"
    if len(summary) > MAX_SUMMARY_CHARS:
        summary = summary[:MAX_SUMMARY_CHARS].rstrip() + "\n\n... summary truncated ...\n"

    output = Path(args.out)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(summary, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
