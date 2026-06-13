#!/usr/bin/env python3
"""Assign Priority and Effort native issue Fields across all open issues.

Discovers the repo's single-select issue Fields named Priority and Effort by
name (so it is not hardcoded to one repo's field ids), then assigns:

  Priority  — correctness > speed > tech-debt, by label:
              urgent/panic -> Urgent; correctness labels -> High;
              performance/bench -> Medium; everything else -> Low.
  Effort    — from the audit `size` for issues we created (S/M/L), else a
              label/title heuristic. Mapped onto whatever Effort options exist
              (top option for epic-scale, descending for L/M/S).

setIssueFieldValue works with `repo` scope. Changing field *options* needs
admin:org — this script only sets values, never edits field definitions.

Usage:
  assign_fields.py --repo owner/name [--specs result.json] [--apply]
"""
import argparse, json, subprocess, sys, time
from collections import Counter

CORRECTNESS = {"bug","false-positive","false-negative","conformance","class-inheritance","type-inference",
 "narrowing","overload-resolution","strict-mode","generic-inference","contextual-typing","recursive-types",
 "conditional-types","variadic-tuples","index-signatures","module-resolution","diagnostic-chain","error-position",
 "fingerprint-only","type-display","type-challenges","parity(solver)","declaration-emit","emit","emitter",
 "diagnostics","diagnostic","jsdoc"}
SPEED = {"performance","bench"}
VHIGH_CUES = ("pay down","strip the","strip tsz","god-facade","unify the three","three parallel",
 "three hand-rolled","three independent","campaign","boundary debt","master plan","family map")
HIGH_CUES = ("unify","collapse","consolidate","retire","migrate","re-land","replace the","god-files",
 "megafunction","table-drive","centralize")
LOW_CUES = ("remove ","drop ","delete ","dedupe the","trim","fixture-scoped","hack","typo","rename")

def gh(args, retries=4):
    last = ""
    for i in range(retries):
        r = subprocess.run(["gh"] + args, capture_output=True, text=True, timeout=90)
        if r.returncode == 0 and '"errors"' not in r.stdout:
            return r.stdout, True
        last = r.stdout or r.stderr
        time.sleep(2 * (i + 1))
    return last, False

def discover_fields(repo):
    owner, name = repo.split("/")
    q = ('{ repository(owner:"%s",name:"%s"){ issueFields(first:30){ nodes{ '
         '__typename ... on IssueFieldSingleSelect { id name options{ id name } } } } } }' % (owner, name))
    out, ok = gh(["api", "graphql", "-f", f"query={q}"])
    if not ok:
        sys.exit("could not read issueFields (needs the issue-fields preview)")
    fields = {}
    for n in json.loads(out)["data"]["repository"]["issueFields"]["nodes"]:
        if n.get("__typename") == "IssueFieldSingleSelect":
            fields[n["name"]] = {"id": n["id"], "options": {o["name"]: o["id"] for o in n["options"]}}
    return fields

def priority(labels):
    L = set(labels)
    if "urgent" in L or "panic" in L: return "Urgent"
    if L & CORRECTNESS: return "High"
    if L & SPEED: return "Medium"
    return "Low"

def effort_rank(num, labels, title, epic_nums, size_by_title):
    L, t = set(labels), title.lower()
    if num in epic_nums: return 4
    if title in size_by_title: return {"S": 1, "M": 2, "L": 3}[size_by_title[title]]
    if "Project Direction" in L or any(c in t for c in VHIGH_CUES): return 4
    if any(c in t for c in LOW_CUES) or "fingerprint-only" in L: return 1
    if any(c in t for c in HIGH_CUES) or "performance" in L: return 3
    return 2

def effort_option(rank, opts):
    """Map rank 1..4 onto the available Effort options (ordered by name heuristic)."""
    order = ["Very High", "High", "Medium", "Low"]  # preferred 4-level naming
    present = [o for o in order if o in opts]
    if not present:                       # unknown naming: use options as-is
        present = list(opts.keys())
    # rank 4=top .. 1=bottom; clamp to available
    idx_from_top = max(0, len(present) - rank)
    return present[min(idx_from_top, len(present) - 1)]

def set_fields(repo, node_id, field_pairs):
    inner = ",".join(f'{{fieldId:"{fid}", singleSelectOptionId:"{oid}"}}' for fid, oid in field_pairs)
    q = f'mutation {{ setIssueFieldValue(input:{{issueId:"{node_id}", issueFields:[{inner}]}}){{ issue{{number}} }} }}'
    return gh(["api", "graphql", "-f", f"query={q}"])

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--specs", help="result.json to read created sizes/epics from")
    ap.add_argument("--apply", action="store_true")
    a = ap.parse_args()
    dry = not a.apply

    fields = discover_fields(a.repo)
    pri = fields.get("Priority"); eff = fields.get("Effort")
    if not pri:
        sys.exit("no 'Priority' single-select field found")
    print(f"Priority options: {list(pri['options'])}")
    if eff: print(f"Effort options:   {list(eff['options'])}")

    epic_nums, size_by_title = set(), {}
    if a.specs:
        d = json.load(open(a.specs))
        size_by_title = {c["title"]: c.get("size", "M") for c in d.get("children", [])}
        # epics are matched after creation by title; if numbers unknown, title cue handles it

    out, _ = gh(["issue", "list", "-R", a.repo, "--state", "open", "--limit", "300",
                 "--json", "id,number,title,labels"])
    issues = json.loads(out)
    pc, ec, ok, fail = Counter(), Counter(), 0, 0
    for it in issues:
        labels = [l["name"] for l in it["labels"]]
        p = priority(labels)
        rank = effort_rank(it["number"], labels, it["title"], epic_nums, size_by_title)
        e_name = effort_option(rank, eff["options"]) if eff else None
        pc[p] += 1; ec[e_name] += 1
        if dry:
            continue
        pairs = [(pri["id"], pri["options"][p])]
        if eff and e_name in eff["options"]:
            pairs.append((eff["id"], eff["options"][e_name]))
        _, good = set_fields(a.repo, it["id"], pairs)
        ok += good; fail += (not good)
    print(f"Priority dist: {dict(pc)}")
    print(f"Effort dist:   {dict(ec)}")
    if not dry:
        print(f"set ok={ok} fail={fail}")

if __name__ == "__main__":
    main()
