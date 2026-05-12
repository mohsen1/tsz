#!/usr/bin/env python3
"""Audit recent merged PRs for potentially missed important review comments.

This script scans merged pull requests via GitHub GraphQL and extracts
unresolved, non-outdated review threads. It then filters out likely nitpicks
and emits a ranked report of potentially important missed feedback.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

GRAPHQL_QUERY = r"""
query($owner: String!, $repo: String!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequests(
      first: 20
      after: $cursor
      states: MERGED
      orderBy: {field: UPDATED_AT, direction: DESC}
    ) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number
        title
        url
        mergedAt
        updatedAt
        baseRefName
        author { login }
        reviewThreads(first: 100) {  # NOTE: max 100 threads per PR; pagination not implemented
          pageInfo { hasNextPage }
          nodes {
            id
            isResolved
            isOutdated
            path
            line
            comments(first: 30) {
              nodes {
                body
                createdAt
                author { login }
              }
            }
          }
        }
      }
    }
  }
}
""".strip()

NIT_RE = re.compile(
    r"\b("
    r"nit|nitpick|nits|optional|style|format(?:ting)?|typo|wording|grammar|"
    r"spelling|rename|can we rename|minor|tiny|small cleanup|docs?\b|comment\b"
    r")\b",
    re.IGNORECASE,
)

IMPORTANT_HINT_RE = re.compile(
    r"\b("
    r"bug|regression|incorrect|wrong|broken|breaks?|crash|panic|unsafe|security|"
    r"race|deadlock|leak|memory|overflow|null|none\b|edge case|failing|flaky|"
    r"test(?:s)? fail|ci fail|assert|off by one|data loss|corrupt|infinite|hang"
    r")\b",
    re.IGNORECASE,
)

ACTION_RE = re.compile(
    r"\b("
    r"must|need to|needs to|please fix|should fix|blocker|request(?:ed)? changes?|"
    r"before merge|cannot merge|can't merge|won't merge|required"
    r")\b",
    re.IGNORECASE,
)

FOLLOWUP_RE = re.compile(
    r"review comments left on #(\d+)|unaddressed\s+copilot\s+review\s+comments\s+on\s+#(\d+)",
    re.IGNORECASE,
)


@dataclass
class Candidate:
    pr_number: int
    pr_title: str
    pr_url: str
    merged_at: str
    base_ref: str
    thread_id: str
    path: str
    line: int | None
    reviewer: str
    body: str
    score: int
    reasons: list[str]


def _run(cmd: list[str], stdin: str | None = None) -> str:
    p = subprocess.run(cmd, input=stdin, text=True, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(
            f"command failed ({p.returncode}): {' '.join(cmd)}\n"
            f"stdout:\n{p.stdout}\n"
            f"stderr:\n{p.stderr}"
        )
    return p.stdout


def _run_json(cmd: list[str], stdin: str | None = None) -> Any:
    raw = _run(cmd, stdin=stdin)
    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"failed to parse JSON from {' '.join(cmd)}:\n{raw}") from exc


def ensure_gh_auth() -> None:
    _run(["gh", "auth", "status"])


def resolve_repo(repo_flag: str | None) -> tuple[str, str]:
    if repo_flag:
        if "/" not in repo_flag:
            raise ValueError(f"--repo must be owner/name, got: {repo_flag}")
        owner, repo = repo_flag.split("/", 1)
        return owner, repo
    info = _run_json(["gh", "repo", "view", "--json", "nameWithOwner"])
    name_with_owner = info["nameWithOwner"]
    owner, repo = name_with_owner.split("/", 1)
    return owner, repo


def load_followed_up_prs(root: Path) -> set[int]:
    claims_root = root / "docs" / "plan" / "claims"
    followed: set[int] = set()
    if not claims_root.exists():
        return followed

    for md in claims_root.rglob("*.md"):
        try:
            text = md.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in FOLLOWUP_RE.finditer(text):
            for group in m.groups():
                if group:
                    followed.add(int(group))
    return followed


def graphql_page(owner: str, repo: str, cursor: str | None) -> dict[str, Any]:
    cmd = [
        "gh",
        "api",
        "graphql",
        "-F",
        "query=@-",
        "-F",
        f"owner={owner}",
        "-F",
        f"repo={repo}",
    ]
    if cursor:
        cmd.extend(["-F", f"cursor={cursor}"])
    return _run_json(cmd, stdin=GRAPHQL_QUERY)


def score_comment(comment: str) -> tuple[int, list[str]]:
    body = comment.strip()
    score = 0
    reasons: list[str] = []

    if IMPORTANT_HINT_RE.search(body):
        score += 3
        reasons.append("important-keyword")
    if ACTION_RE.search(body):
        score += 2
        reasons.append("action-language")
    if "?" in body and not NIT_RE.search(body):
        score += 1
        reasons.append("open-question")
    if len(body) > 260:
        score += 1
        reasons.append("detailed-thread")
    if NIT_RE.search(body) and not IMPORTANT_HINT_RE.search(body):
        score -= 3
        reasons.append("nit-likely")

    return score, reasons


def first_reviewer_comment(comments: list[dict[str, Any]], pr_author: str | None) -> tuple[str, str]:
    for c in comments:
        author = ((c.get("author") or {}).get("login")) or "unknown"
        if pr_author and author.lower() == pr_author.lower():
            continue
        body = (c.get("body") or "").strip()
        if body:
            return author, body

    if comments:
        c0 = comments[0]
        author = ((c0.get("author") or {}).get("login")) or "unknown"
        return author, (c0.get("body") or "").strip()
    return "unknown", ""


def subsystem_from_path(path: str) -> str:
    if not path:
        return "unknown"
    parts = path.split("/")
    if len(parts) < 2:
        return parts[0]
    if parts[0] == "crates":
        return "/".join(parts[:2])
    return parts[0]


def markdown_text(text: str) -> str:
    return text.replace("\\", "\\\\").replace("`", "\\`")


def collect_candidates(owner: str, repo: str, limit: int, followed_up: set[int]) -> tuple[list[Candidate], dict[str, Any]]:
    cursor: str | None = None
    scanned_prs = 0
    candidates: list[Candidate] = []
    truncated_thread_prs: list[int] = []

    while scanned_prs < limit:
        payload = graphql_page(owner, repo, cursor)
        if payload.get("errors"):
            raise RuntimeError(f"graphql error: {json.dumps(payload['errors'], indent=2)}")

        data = payload["data"]["repository"]["pullRequests"]
        nodes = data.get("nodes") or []
        if not nodes:
            break

        for pr in nodes:
            scanned_prs += 1
            if scanned_prs > limit:
                break

            number = int(pr["number"])
            if number in followed_up:
                continue

            pr_author = ((pr.get("author") or {}).get("login")) or None
            review_threads = pr.get("reviewThreads") or {}
            if (review_threads.get("pageInfo") or {}).get("hasNextPage"):
                truncated_thread_prs.append(number)
            threads = review_threads.get("nodes") or []
            for thread in threads:
                if thread.get("isResolved") or thread.get("isOutdated"):
                    continue
                comments = (thread.get("comments") or {}).get("nodes") or []
                reviewer, body = first_reviewer_comment(comments, pr_author)
                if not body:
                    continue
                score, reasons = score_comment(body)
                if score <= 0:
                    continue
                if "nit-likely" in reasons and score < 3:
                    continue

                candidates.append(
                    Candidate(
                        pr_number=number,
                        pr_title=pr.get("title") or "",
                        pr_url=pr.get("url") or "",
                        merged_at=pr.get("mergedAt") or "",
                        base_ref=pr.get("baseRefName") or "",
                        thread_id=thread.get("id") or "",
                        path=thread.get("path") or "",
                        line=thread.get("line"),
                        reviewer=reviewer,
                        body=body,
                        score=score,
                        reasons=reasons,
                    )
                )

        page_info = data.get("pageInfo") or {}
        if not page_info.get("hasNextPage"):
            break
        cursor = page_info.get("endCursor")
        if not cursor:
            break

    summary = {
        "scanned_prs": scanned_prs,
        "truncated_thread_prs": sorted(set(truncated_thread_prs)),
    }
    return candidates, summary


def emit_markdown(
    out_path: Path,
    owner: str,
    repo: str,
    limit: int,
    followed_up_count: int,
    summary: dict[str, Any],
    candidates: list[Candidate],
) -> None:
    by_subsystem: Counter[str] = Counter(subsystem_from_path(c.path) for c in candidates)
    by_pr: Counter[int] = Counter(c.pr_number for c in candidates)
    grouped: dict[int, list[Candidate]] = defaultdict(list)
    for c in candidates:
        grouped[c.pr_number].append(c)

    lines: list[str] = []
    lines.append(f"# Missed Important Review Comment Audit ({owner}/{repo})")
    lines.append("")
    lines.append(f"- Scan scope: last {limit} merged PRs")
    lines.append(f"- PRs scanned: {summary['scanned_prs']}")
    lines.append(f"- PRs excluded as already followed-up: {followed_up_count}")
    lines.append(f"- Potential important unresolved threads: {len(candidates)}")
    truncated = summary.get("truncated_thread_prs") or []
    if truncated:
        lines.append(
            f"- PRs with more than 100 review threads, audit may be incomplete: "
            + ", ".join(f"#{pr}" for pr in truncated)
        )
    lines.append("")

    if by_subsystem:
        lines.append("## Top Subsystems")
        lines.append("")
        for subsystem, count in by_subsystem.most_common(10):
            lines.append(f"- `{subsystem}`: {count}")
        lines.append("")

    if by_pr:
        lines.append("## Top PRs By Candidate Count")
        lines.append("")
        for pr_number, count in by_pr.most_common(20):
            sample = grouped[pr_number][0]
            lines.append(f"- [#{pr_number}]({sample.pr_url}) {markdown_text(sample.pr_title)}: {count}")
        lines.append("")

    lines.append("## Candidate Threads (Top 100 by score)")
    lines.append("")
    for c in sorted(candidates, key=lambda x: (-x.score, x.pr_number))[:100]:
        loc = f"{c.path}:{c.line}" if c.path and c.line else (c.path or "n/a")
        body = " ".join(c.body.split())
        if len(body) > 220:
            body = f"{body[:217]}..."
        lines.append(f"- [#{c.pr_number}]({c.pr_url}) `{loc}` score={c.score} reviewer=`{c.reviewer}` reasons={','.join(c.reasons)}")
        lines.append(f"  - {body}")

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="owner/name (default: current gh repo)")
    parser.add_argument("--limit", type=int, default=500, help="number of merged PRs to scan (default: 500)")
    parser.add_argument(
        "--json-out",
        default="docs/plan/review-comment-audit-latest.json",
        help="output JSON path",
    )
    parser.add_argument(
        "--md-out",
        default="docs/plan/review-comment-audit-latest.md",
        help="output markdown summary path",
    )
    args = parser.parse_args()

    ensure_gh_auth()
    owner, repo = resolve_repo(args.repo)

    root = Path.cwd()
    followed = load_followed_up_prs(root)
    candidates, summary = collect_candidates(owner, repo, args.limit, followed)
    candidates_sorted = sorted(candidates, key=lambda x: (-x.score, x.pr_number))

    json_path = Path(args.json_out)
    md_path = Path(args.md_out)

    json_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "repo": f"{owner}/{repo}",
        "limit": args.limit,
        "summary": summary,
        "excluded_followed_up_prs": sorted(followed),
        "candidate_count": len(candidates_sorted),
        "candidates": [
            {
                "pr_number": c.pr_number,
                "pr_title": c.pr_title,
                "pr_url": c.pr_url,
                "merged_at": c.merged_at,
                "base_ref": c.base_ref,
                "thread_id": c.thread_id,
                "path": c.path,
                "line": c.line,
                "reviewer": c.reviewer,
                "body": c.body,
                "score": c.score,
                "reasons": c.reasons,
            }
            for c in candidates_sorted
        ],
    }
    json_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    emit_markdown(
        out_path=md_path,
        owner=owner,
        repo=repo,
        limit=args.limit,
        followed_up_count=len(followed),
        summary=summary,
        candidates=candidates_sorted,
    )

    print(
        json.dumps(
            {
                "repo": f"{owner}/{repo}",
                "limit": args.limit,
                "scanned_prs": summary["scanned_prs"],
                "candidate_count": len(candidates_sorted),
                "json_out": str(json_path),
                "md_out": str(md_path),
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # noqa: BLE001
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
