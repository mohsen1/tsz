#!/usr/bin/env python3
"""Guard: the repo's shell scripts must run under macOS system Bash 3.2.

Bash 4+ builtins and parameter expansions silently degrade under the
`/bin/bash` (3.2) that ships with macOS -- e.g. `mapfile: command not found`,
which collapsed package discovery and ran Cargo with empty arguments (issue
#15440). A developer running the local CI gates on a Mac executes many of these
scripts (`safe-run.sh`, `full-ci.sh`, and everything they source/exec), so the
policy is simply: every shell script under `scripts/` stays 3.2-safe. This
scanner fails if a Bash-4-only construct is reintroduced so the regression
surfaces in `run_lint` on Linux CI rather than only on a Mac.

Usage: bash32_compat_guard.py [<root> ...]   (defaults to scanning `scripts`)
"""

from __future__ import annotations

import os
import re
import sys

# Each rule is (compiled regex, human-readable reason). Patterns match against a
# comment-stripped copy of each line so prose that merely names a construct does
# not trip the guard.
RULES = [
    (re.compile(r"(?<![\w.-])(mapfile|readarray)(?![\w.-])"),
     "mapfile/readarray builtin (Bash 4+); use a `while IFS= read -r` loop"),
    (re.compile(r"(?<![\w-])(declare|local|typeset)\s+-[A-Za-z]*A"),
     "associative array declaration `-A` (Bash 4+)"),
    (re.compile(r"(?<![\w-])(declare|local|typeset)\s+-[A-Za-z]*n"),
     "nameref declaration `-n` (Bash 4.3+)"),
    (re.compile(r"\$\{[A-Za-z_][A-Za-z0-9_]*(\[[^\]]*\])?(,,?|\^\^?)"),
     "case-modification parameter expansion ${x^^}/${x,,} (Bash 4+)"),
    (re.compile(r"\$\{[A-Za-z_][A-Za-z0-9_]*(\[[^\]]*\])?@[QEPAKakLU]"),
     "${x@...} parameter transformation (Bash 4.4+)"),
    (re.compile(r"(?<![\w-])coproc(?![\w-])"), "coproc (Bash 4+)"),
    (re.compile(r"\|&"), "|& pipe-both shorthand (Bash 4+); use `2>&1 |`"),
    (re.compile(r"(?<![\w-])wait\s+-[A-Za-z]*n"), "wait -n (Bash 4.3+)"),
    (re.compile(r"shopt\s+-s\s+globstar"), "globstar/** (Bash 4+)"),
]

# Drop a trailing `#` comment (at start of line or after whitespace) so prose
# naming a construct does not self-trip. Heuristic: a `#` inside a quoted string
# on the same line as a real construct is not a pattern any rule realistically
# hits, so full shell-quote parsing is unnecessary here.
_COMMENT = re.compile(r"(?:^|\s)#.*$")

# (path, lineno, reason, code snippet)
Violation = tuple[str, int, str, str]


def scan(root: str) -> list[Violation]:
    violations: list[Violation] = []
    for dirpath, _dirs, files in os.walk(root):
        for name in sorted(files):
            if not name.endswith(".sh"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8") as handle:
                for lineno, raw in enumerate(handle, start=1):
                    code = _COMMENT.sub("", raw).strip()
                    if not code:
                        continue
                    for pattern, reason in RULES:
                        if pattern.search(code):
                            violations.append((path, lineno, reason, code))
    return violations


def main(argv: list[str]) -> int:
    roots = argv[1:] or ["scripts"]
    violations: list[Violation] = []
    for root in roots:
        violations.extend(scan(root))

    if violations:
        sys.stderr.write(
            "error: Bash 4+ construct found in a shell script; these must run "
            "under macOS system Bash 3.2 (see issue #15440):\n"
        )
        for path, lineno, reason, snippet in violations:
            sys.stderr.write(f"  {path}:{lineno}: {reason}\n      {snippet}\n")
        return 1

    print(f"bash32-compat: {', '.join(roots)} clean of Bash 4+ constructs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
