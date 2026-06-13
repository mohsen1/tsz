#!/usr/bin/env python3
"""Measure clippy pedantic+nursery warnings and triage them.

Runs `cargo clippy --workspace --lib -- -W clippy::pedantic -W clippy::nursery`
without modifying any files, aggregates warnings per lint, and prints a
promote-vs-defer triage. Writes <out>/clippy.json (raw), <out>/clippy-triage.md.

Usage:
  measure_clippy.py --worktree . --out /tmp/hygiene [--no-build]
"""
import argparse, json, os, subprocess, sys, collections

# Senior triage for a *compiler* codebase. These are defaults, not gospel —
# re-judge per repo. The reasoning lives in references/lint-triage.md.
PROMOTE = {  # high signal-to-churn: idiom, ownership, dead surface, complexity
 "manual_let_else","explicit_iter_loop","redundant_closure_for_method_calls","implicit_clone",
 "map_unwrap_or","single_match_else","unnested_or_patterns","needless_pass_by_ref_mut",
 "needless_pass_by_value","default_trait_access","assigning_clones","branches_sharing_code",
 "useless_let_if_seq","needless_continue","elidable_lifetime_names","unused_self",
 "wildcard_imports","struct_excessive_bools","format_push_string","redundant_pub_crate",
 "case_sensitive_file_extension_comparisons","too_many_lines"}
DEFER = {  # mostly intentional in a compiler (numeric casts, must_use, doc pedantry)
 "cast_possible_truncation","cast_lossless","cast_precision_loss","cast_sign_loss",
 "must_use_candidate","return_self_not_must_use","missing_panics_doc","missing_errors_doc",
 "too_long_first_doc_paragraph","items_after_statements","if_not_else","option_if_let_else"}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worktree", default=".")
    ap.add_argument("--out", default="/tmp/hygiene")
    ap.add_argument("--no-build", action="store_true", help="skip clippy, only re-triage an existing clippy.json")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    raw = os.path.join(a.out, "clippy.json")

    if not a.no_build:
        print("Running clippy (pedantic+nursery, libs only)...", file=sys.stderr)
        cmd = ["cargo", "clippy", "--workspace", "--lib", "--message-format=json",
               "--", "-W", "clippy::pedantic", "-W", "clippy::nursery"]
        with open(raw, "w") as f:
            subprocess.run(cmd, cwd=a.worktree, stdout=f, stderr=subprocess.DEVNULL)

    counts = collections.Counter()
    with open(raw) as f:
        for line in f:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                m = json.loads(line)
            except Exception:
                continue
            if m.get("reason") != "compiler-message":
                continue
            msg = m.get("message", {})
            code = (msg.get("code") or {}).get("code") or ""
            if code.startswith("clippy::") and msg.get("level") == "warning":
                counts[code] += 1

    total = sum(counts.values())
    out = [f"# Clippy pedantic+nursery measurement", "", f"Total warnings: {total}", ""]
    out.append("## Recommend PROMOTE (staged, signal > churn)")
    for l, n in counts.most_common():
        if l.replace("clippy::", "") in PROMOTE:
            out.append(f"- {n:6d}  {l}")
    out.append("\n## Recommend ALLOW/DEFER (mostly intentional in a compiler)")
    for l, n in counts.most_common():
        if l.replace("clippy::", "") in DEFER:
            out.append(f"- {n:6d}  {l}")
    out.append("\n## Uncategorized (judge per repo)")
    for l, n in counts.most_common():
        k = l.replace("clippy::", "")
        if k not in PROMOTE and k not in DEFER:
            out.append(f"- {n:6d}  {l}")
    text = "\n".join(out)
    with open(os.path.join(a.out, "clippy-triage.md"), "w") as f:
        f.write(text + "\n")
    print(text)

if __name__ == "__main__":
    main()
