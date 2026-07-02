#!/usr/bin/env python3
"""PreToolUse gate: answer slow cargo invocations from warm rust-analyzer.

Reads the Claude Code / Codex PreToolUse hook JSON on stdin. If the Bash
command is a cargo check/clippy/nextest/test/build (bare or wrapped in
scripts/safe-run.sh) and the tszd daemon already sees compile errors, the call
is DENIED with the numbered diagnostics as the reason -- the same information
cargo would print, seconds-to-minutes sooner (and it saves the nextest
compile). In every other situation the call is ALLOWED:

  * daemon cold / unreachable / slow (>5s)  -> allow (fail-open)
  * rust-analyzer reports no errors         -> allow (native diagnostics miss a
    small fraction of error classes, so a clean state must NEVER block)
  * RA_SKIP=1 anywhere in the command       -> allow (false-positive escape)
  * TSZ_FAST_LOOP=0 in the environment      -> allow (kill switch)

The gate never counts or budgets checks. Every decision is appended to
.tsz-ra/events.jsonl (see `ra stats`).
"""

from __future__ import annotations

import json
import os
import re
import socket
import subprocess
import sys
import time
from pathlib import Path

STATE = ".tsz-ra"
REASON_BUDGET = 8000  # hook feedback is capped at 10k chars; stay well under
CARGO_RE = re.compile(
    r"^(?:\S*/)?safe-run\.sh(?:\s+\S+)*?\s+(?:--\s+)?cargo\s+(check|clippy|nextest|test|build)\b"
    r"|^cargo\s+(check|clippy|nextest|test|build)\b"
)


def out_allow() -> int:
    return 0


def out_deny(reason: str) -> int:
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": reason[:REASON_BUDGET],
    }}))
    return 0


def log_event(ws: Path, event: dict) -> None:
    try:
        (ws / STATE).mkdir(exist_ok=True)
        with open(ws / STATE / "events.jsonl", "a", encoding="utf-8") as fh:
            fh.write(json.dumps(event) + "\n")
    except OSError:
        pass


def query_daemon(ws: Path, timeout: float = 5.0) -> list[dict] | None:
    addr = ws / STATE / "daemon.json"
    if not addr.exists():
        return None
    try:
        meta = json.loads(addr.read_text())
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(timeout)
        s.connect(meta["socket"])
        s.sendall(b'{"cmd": "diag"}\n')
        buf = b""
        while b"\n" not in buf:
            chunk = s.recv(1 << 16)
            if not chunk:
                break
            buf += chunk
        s.close()
        resp = json.loads(buf.split(b"\n", 1)[0]) if buf else {}
        return resp.get("diagnostics") if resp.get("ok") else None
    except (OSError, json.JSONDecodeError, KeyError):
        return None


def warm_in_background(ws: Path) -> None:
    ra = Path(__file__).resolve().parent / "ra"
    try:
        subprocess.Popen([sys.executable, str(ra), "up"], cwd=ws,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        pass


def main() -> int:
    if os.environ.get("TSZ_FAST_LOOP", "1") == "0":
        return out_allow()
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return out_allow()
    command = ((payload.get("tool_input") or {}).get("command") or "").strip()
    if not command or "RA_SKIP" in command:
        return out_allow()
    if not CARGO_RE.search(command):
        return out_allow()

    cwd = Path(payload.get("cwd") or os.getcwd())
    top = subprocess.run(["git", "-C", str(cwd), "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True)
    if top.returncode != 0:
        return out_allow()
    ws = Path(top.stdout.strip())
    if not (ws / "crates").is_dir():  # only gate tsz-shaped worktrees
        return out_allow()

    t0 = time.monotonic()
    diags = query_daemon(ws)
    ms = round((time.monotonic() - t0) * 1000)
    if diags is None:
        warm_in_background(ws)  # cold now; warm for the NEXT check
        log_event(ws, {"ts": time.strftime("%H:%M:%S"), "decision": "allow",
                       "why": "daemon_cold", "ms": ms, "cmd": command[:120]})
        return out_allow()

    errors = [d for d in diags if d.get("severity") == "error"
              and d.get("file", "").startswith(("crates/", "src/", "tests/"))]
    if not errors:
        log_event(ws, {"ts": time.strftime("%H:%M:%S"), "decision": "allow",
                       "why": "clean", "ms": ms, "cmd": command[:120]})
        return out_allow()

    lines = [f"rust-analyzer already sees {len(errors)} compile error(s) -- this "
             "cargo run would fail with the same errors after a much longer wait. "
             "Fix these first (this IS your cargo feedback, delivered early):", ""]
    for i, d in enumerate(errors[:25]):
        code = f" {d['code']}" if d.get("code") else ""
        lines.append(f"[{i}] {d['file']}:{d['line']}:{d['col']}{code}: {d['message']}")
    if len(errors) > 25:
        lines.append(f"... and {len(errors) - 25} more -- run `./tools/tszd/ra diag`")
    lines += ["", "After editing, re-run your cargo command -- it is allowed the "
              "moment rust-analyzer is clean.",
              "False positive? Re-run with RA_SKIP=1 prefixed to bypass this gate."]
    log_event(ws, {"ts": time.strftime("%H:%M:%S"), "decision": "deny",
                   "errors": len(errors), "ms": ms, "cmd": command[:120]})
    return out_deny("\n".join(lines))


if __name__ == "__main__":
    raise SystemExit(main())
