#!/usr/bin/env python3
"""Guard: keep shell scripts within macOS system `/bin/bash` (3.2) features.

CI and contributor tooling must run under the Bash that ships with macOS,
which is 3.2 (frozen at that release for licensing reasons). Reaching for a
Bash 4+ builtin or expansion silently breaks local runs there -- the failure
mode that motivated issue #15440, where `mapfile` collapsed unit-test package
discovery to an empty package name.

This guard scans every shell script under `scripts/` (files ending in `.sh`
plus any file with a Bash shebang, e.g. the githooks) for the Bash 4+ constructs
that come up in practice and reports each with a portable alternative. It runs
in the PR `clippy` job and in `run_lint` so the whole script surface stays
3.2-safe.

Run standalone to check the tree:

    python3 scripts/lib/check-sh-portability.py

Exits non-zero (and prints each violation) when anything is found.
"""

from __future__ import annotations

import pathlib
import re
import sys
from dataclasses import dataclass
from typing import Iterable, List, Optional


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS_DIR = ROOT / "scripts"


@dataclass(frozen=True)
class Rule:
    name: str
    pattern: "re.Pattern[str]"
    hint: str


# Each rule matches a construct introduced after Bash 3.2. Patterns run against
# a comment-stripped copy of each line so that documentation mentions do not
# trip the guard.
RULES: List[Rule] = [
    Rule(
        "mapfile/readarray builtin",
        re.compile(r"(?:^|[\s;&|(){}])(?:mapfile|readarray)\b"),
        "use portable_read_lines from scripts/lib/sh-portability.sh "
        "(Bash 3.2 has no mapfile/readarray)",
    ),
    Rule(
        "associative array (declare -A)",
        re.compile(r"\b(?:declare|local|typeset|readonly|export)\s+-[A-Za-z]*A[A-Za-z]*\b"),
        "associative arrays require Bash 4; use parallel indexed arrays, a "
        "case statement, or a temp file",
    ),
    Rule(
        "nameref (declare -n)",
        re.compile(r"\b(?:declare|local|typeset)\s+-[A-Za-z]*n[A-Za-z]*\b"),
        "namerefs require Bash 4.3; pass the variable name and use eval-based "
        "indirection (see portable_read_lines)",
    ),
    Rule(
        "case-modifying expansion (${v^^}/${v,,})",
        re.compile(r"\$\{[#!]?[A-Za-z_][A-Za-z0-9_]*(?:\[[^\]]*\])?[\^,]"),
        "case modification requires Bash 4; pipe through "
        "tr '[:lower:]' '[:upper:]' (or the inverse)",
    ),
    Rule(
        "wait -n",
        re.compile(r"\bwait\s+-n\b"),
        "wait -n requires Bash 4.3; wait on explicit PIDs collected in an array",
    ),
    Rule(
        "coproc",
        re.compile(r"(?:^|[\s;&|(){}])coproc\b"),
        "coproc requires Bash 4; use an explicit FIFO or temp file",
    ),
    Rule(
        "&>> redirect",
        re.compile(r"&>>"),
        "&>> requires Bash 4; use >>file 2>&1",
    ),
    Rule(
        "|& pipe",
        re.compile(r"\|&"),
        "|& requires Bash 4; use 2>&1 | instead",
    ),
    Rule(
        "negative array index",
        re.compile(r"\$\{[A-Za-z_][A-Za-z0-9_]*\[\s*-"),
        "negative array subscripts require Bash 4.3; index from "
        "${#arr[@]} - N instead",
    ),
]


@dataclass(frozen=True)
class Violation:
    path: pathlib.Path
    lineno: int
    rule: Rule
    text: str


def strip_comment(line: str) -> str:
    """Remove comment content so doc mentions do not trip the guard.

    A `#` starts a comment only at the start of a word (line start or after
    whitespace) and only outside quotes, so a `#` inside a string or a `${#x}`
    expansion is preserved. Lines with no `#` at all skip the scan entirely.
    """
    if "#" not in line:
        return line
    out: List[str] = []
    in_single = in_double = False
    prev = ""
    for ch in line:
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double and (prev == "" or prev.isspace()):
            break
        out.append(ch)
        prev = ch
    return "".join(out)


def read_if_shell(path: pathlib.Path) -> Optional[str]:
    """Return the file text when `path` is a shell script to scan, else None.

    `.sh` files are always in scope. Extensionless files are in scope only when
    they carry a Bash shebang (e.g. the githooks). Non-shell suffixes are
    rejected without reading the file.
    """
    if not path.is_file():
        return None
    if path.suffix and path.suffix != ".sh":
        return None
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    if path.suffix == ".sh":
        return text
    first_line = text.split("\n", 1)[0]
    if first_line.startswith("#!") and "bash" in first_line:
        return text
    return None


def scan_text(path: pathlib.Path, text: str) -> Iterable[Violation]:
    for lineno, raw in enumerate(text.splitlines(), start=1):
        code = strip_comment(raw)
        if not code.strip():
            continue
        for rule in RULES:
            if rule.pattern.search(code):
                yield Violation(path, lineno, rule, raw.rstrip())


def find_violations(root: pathlib.Path = SCRIPTS_DIR) -> List[Violation]:
    violations: List[Violation] = []
    for path in sorted(root.rglob("*")):
        text = read_if_shell(path)
        if text is not None:
            violations.extend(scan_text(path, text))
    return violations


def main(argv: List[str]) -> int:
    root = pathlib.Path(argv[1]).resolve() if len(argv) > 1 else SCRIPTS_DIR
    violations = find_violations(root)
    if not violations:
        return 0
    print("Bash 4+ constructs found (must stay macOS /bin/bash 3.2 compatible):")
    print()
    for v in violations:
        rel = v.path.relative_to(ROOT) if v.path.is_relative_to(ROOT) else v.path
        print(f"  {rel}:{v.lineno}: {v.rule.name}")
        print(f"      {v.text.strip()}")
        print(f"      -> {v.rule.hint}")
    print()
    print(f"{len(violations)} violation(s). See scripts/lib/sh-portability.sh for helpers.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
