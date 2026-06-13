#!/usr/bin/env python3
"""Create a hierarchy of GitHub issues from an audit spec, with native sub-issues.

Reads a specs JSON of the shape produced by hygiene-audit-workflow.mjs:
  { "epics":    [ { "key","title", "body_md"? } ],
    "children": [ { "epic_key","title","labels"[],"size","body_md" } ] }

Creates each epic, then each child, links children as native GitHub sub-issues,
and appends a "## Child issue map" to every epic as a durable fallback.

Dry-run by default; pass --apply to write. Idempotent-ish: re-linking an
existing sub-issue is harmless.

Usage:
  create_issues.py --repo owner/name --specs result.json [--apply]
                   [--epic-labels tech-debt,Project Direction] [--footer FILE]
"""
import argparse, json, subprocess, sys, time

def gh(args, body=None, retries=4):
    last = ""
    for i in range(retries):
        try:
            r = subprocess.run(["gh"] + args, input=body, capture_output=True, text=True, timeout=90)
            if r.returncode == 0:
                return r.stdout.strip(), True
            last = (r.stderr or r.stdout).strip()
            if "already exists" in last.lower():
                return last, True
        except subprocess.TimeoutExpired:
            last = "timeout"
        time.sleep(3 * (i + 1))
    print(f"  gh failed: {' '.join(args)} :: {last[:160]}", file=sys.stderr)
    return last, False

def valid_labels(repo):
    out, ok = gh(["label", "list", "-R", repo, "--limit", "200", "--json", "name", "-q", ".[].name"])
    return set(out.splitlines()) if ok else set()

def create_issue(repo, title, body, labels, valid, dry):
    labels = [l for l in labels if l in valid]
    if dry:
        print(f"[dry] create {title[:70]!r} labels={labels}")
        return {"number": 0, "id": 0}
    args = ["issue", "create", "-R", repo, "-t", title, "-F", "-"]
    for l in labels:
        args += ["-l", l]
    url, ok = gh(args, body=body)
    if not ok:
        return None
    num = int(url.rstrip("/").split("/")[-1])
    dbid, _ = gh(["api", f"repos/{repo}/issues/{num}", "-q", ".id"])
    print(f"  created #{num} {title[:60]}")
    return {"number": num, "id": int(dbid)}

def link_sub_issue(repo, parent_num, child_id, dry):
    if dry:
        return True
    _, ok = gh(["api", "-X", "POST", f"repos/{repo}/issues/{parent_num}/sub_issues",
                "-F", f"sub_issue_id={child_id}"], retries=3)  # -F integer, db id
    return ok

def epic_fallback_body(epic, kids):
    return (f"## Summary\n\nTracking epic: {epic['title']}\n\n"
            f"Child issues carry the verified evidence and proposed fixes.")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--specs", required=True)
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--epic-labels", default="tech-debt,Project Direction")
    ap.add_argument("--footer", help="path to a markdown footer appended to every issue body")
    a = ap.parse_args()
    dry = not a.apply
    data = json.load(open(a.specs))
    valid = valid_labels(a.repo) if a.apply else set(["tech-debt", "Project Direction"])
    footer = ("\n\n" + open(a.footer).read()) if a.footer else ""
    epic_labels = [x for x in a.epic_labels.split(",") if x]

    epic_rec, child_by_epic = {}, {e["key"]: [] for e in data["epics"]}
    print("=== EPICS ===")
    for e in data["epics"]:
        kids = [c for c in data["children"] if c["epic_key"] == e["key"]]
        body = (e.get("body_md") or epic_fallback_body(e, kids)) + footer
        rec = create_issue(a.repo, e["title"], body, epic_labels, valid, dry)
        epic_rec[e["key"]] = rec

    print("\n=== CHILDREN ===")
    for c in data["children"]:
        rec = create_issue(a.repo, c["title"], (c["body_md"] + footer),
                           c.get("labels", ["tech-debt"]), valid, dry)
        if not rec:
            continue
        rec.update(title=c["title"], size=c.get("size", "?"))
        rec["linked"] = link_sub_issue(a.repo, epic_rec[c["epic_key"]]["number"], rec["id"], dry)
        child_by_epic[c["epic_key"]].append(rec)

    print("\n=== CHILD MAPS ===")
    for e in data["epics"]:
        kids = child_by_epic[e["key"]]
        lines = ["\n\n## Child issue map\n"] + [
            f"- [ ] {('#'+str(k['number'])) if not dry else '#?'} ({k.get('size','?')}) {k['title']}"
            + ("" if k.get("linked") else " *(see map)*") for k in kids]
        if dry:
            print(f"[dry] epic {e['key']}: {len(kids)} children")
            continue
        cur, _ = gh(["issue", "view", str(epic_rec[e["key"]]["number"]), "-R", a.repo, "--json", "body", "-q", ".body"])
        gh(["issue", "edit", str(epic_rec[e["key"]]["number"]), "-R", a.repo, "-F", "-"], body=cur + "\n".join(lines))
        print(f"  updated epic #{epic_rec[e['key']]['number']} ({len(kids)} children)")

    print("\n=== TREE ===")
    for e in data["epics"]:
        er = epic_rec[e["key"]]
        print(f"EPIC #{er['number']} {e['title']}")
        for k in child_by_epic[e["key"]]:
            print(f"   #{k['number']} ({k.get('size','?')}) {'sub' if k.get('linked') else 'map'} {k['title'][:60]}")

if __name__ == "__main__":
    main()
