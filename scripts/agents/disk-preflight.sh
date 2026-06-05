#!/usr/bin/env bash
#
# Fast preflight for multi-agent work. It reports disk guard state, reusable
# worktrees, TypeScript submodule linkage, and cache-preserving cleanup advice.

set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/agents/disk-preflight.sh [--json-report PATH] [AgentName]

Runs compact checks only. It does not delete files or create worktrees.

With --json-report PATH, also write a machine-readable preflight report.
USAGE
}

AGENT="unknown"
JSON_REPORT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --json-report)
      shift
      if [[ $# -eq 0 ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      JSON_REPORT="$1"
      ;;
    --json-report=*)
      JSON_REPORT="${1#--json-report=}"
      if [[ -z "$JSON_REPORT" ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      ;;
    --*)
      echo "Unknown option: $1 (try --help)" >&2
      exit 2
      ;;
    *)
      if [[ "$AGENT" != "unknown" ]]; then
        echo "Unknown option: $1 (try --help)" >&2
        exit 2
      fi
      AGENT="$1"
      ;;
  esac
  shift
done

case "$AGENT" in
  unknown|M1-A|M1-B|M1-D|M1-Opus|M4-A|M4-B|M4-C|M4-Opus|Studio-A|Studio-B|Studio-C|Studio-Opus|Studio-manager) ;;
  *) echo "unknown AgentName: $AGENT" >&2; exit 1 ;;
esac

ROOT="$(git rev-parse --show-toplevel)"
GIT_HEAD="$(git -C "$ROOT" rev-parse HEAD)"
GIT_HEAD_SHORT="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
GIT_BRANCH="$(git -C "$ROOT" branch --show-current)"
GIT_DETACHED=false
if [[ -z "$GIT_BRANCH" ]]; then
  GIT_BRANCH="detached:$GIT_HEAD_SHORT"
  GIT_DETACHED=true
fi
GIT_UPSTREAM="$(git -C "$ROOT" rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)"
GIT_STATUS_OUTPUT="$(git -C "$ROOT" status --porcelain --untracked-files=normal)"
GIT_DIRTY=false
GIT_DIRTY_FILES=0
GIT_UNTRACKED_FILES=0
if [[ -n "$GIT_STATUS_OUTPUT" ]]; then
  GIT_DIRTY=true
  GIT_DIRTY_FILES="$(printf '%s\n' "$GIT_STATUS_OUTPUT" | wc -l | awk '{ print $1 }')"
  GIT_UNTRACKED_FILES="$(printf '%s\n' "$GIT_STATUS_OUTPUT" | awk 'substr($0, 1, 2) == "??" { count++ } END { print count + 0 }')"
fi

echo "agent=$AGENT"
echo "repo=$ROOT"
echo ""
echo "== current git state =="
echo "git_head=$GIT_HEAD"
echo "git_branch=$GIT_BRANCH"
echo "git_detached=$GIT_DETACHED"
if [[ -n "$GIT_UPSTREAM" ]]; then
  echo "git_upstream=$GIT_UPSTREAM"
else
  echo "git_upstream=none"
fi
echo "git_dirty=$GIT_DIRTY"
echo "git_dirty_files=$GIT_DIRTY_FILES"
echo "git_untracked_files=$GIT_UNTRACKED_FILES"
echo ""
echo "== disk guard =="
GUARD_OUTPUT="$("$ROOT/scripts/setup/disk-worktree-guard.sh")"
echo "$GUARD_OUTPUT"

echo ""
echo "== current TypeScript state =="
if [[ -L "$ROOT/TypeScript" ]]; then
  TYPESCRIPT_STATE="symlink"
  TYPESCRIPT_TARGET="$(readlink "$ROOT/TypeScript")"
  echo "typescript=symlink target=$TYPESCRIPT_TARGET"
elif [[ -d "$ROOT/TypeScript/tests/cases" ]]; then
  TYPESCRIPT_STATE="populated-local-submodule"
  TYPESCRIPT_TARGET=""
  echo "typescript=populated-local-submodule"
elif [[ -d "$ROOT/TypeScript" ]]; then
  TYPESCRIPT_STATE="present-but-not-populated"
  TYPESCRIPT_TARGET=""
  echo "typescript=present-but-not-populated"
else
  TYPESCRIPT_STATE="missing"
  TYPESCRIPT_TARGET=""
  echo "typescript=missing"
fi

COMMON_DIR="$(git -C "$ROOT" rev-parse --git-common-dir)"
GIT_DIR="$(git -C "$ROOT" rev-parse --git-dir)"
COMMON_REAL="$(cd "$COMMON_DIR" && pwd -P)"
GIT_REAL="$(cd "$GIT_DIR" && pwd -P)"
PRIMARY_REPO="$(cd "$COMMON_REAL/.." && pwd -P)"
PRIMARY_TS="$PRIMARY_REPO/TypeScript"

echo ""
echo "== local cargo cache presence =="
cache_size_kb() {
  if [[ -d "$1" ]]; then
    du -sk "$1" 2>/dev/null | awk '{ print $1 }'
  else
    echo 0
  fi
}

CARGO_CACHE_STUB_MAX_KB="${TSZ_CARGO_CACHE_STUB_MAX_KB:-1024}"
LOCAL_CARGO_CACHE_DIR_COUNT=0
LOCAL_CARGO_CACHE_PRESENT_COUNT=0
LOCAL_CARGO_CACHE_TOTAL_KB=0
CARGO_DOT_TARGET=false
CARGO_DOT_TARGET_BENCH=false
CARGO_TARGET=false
CARGO_DOT_TARGET_STATUS=missing
CARGO_DOT_TARGET_BENCH_STATUS=missing
CARGO_TARGET_STATUS=missing
CARGO_DOT_TARGET_SIZE_KB=0
CARGO_DOT_TARGET_BENCH_SIZE_KB=0
CARGO_TARGET_SIZE_KB=0
for dir in .target .target-bench target; do
  if [[ -d "$ROOT/$dir" ]]; then
    size_kb="$(cache_size_kb "$ROOT/$dir")"
    LOCAL_CARGO_CACHE_DIR_COUNT=$((LOCAL_CARGO_CACHE_DIR_COUNT + 1))
    LOCAL_CARGO_CACHE_TOTAL_KB=$((LOCAL_CARGO_CACHE_TOTAL_KB + size_kb))
    if (( size_kb > CARGO_CACHE_STUB_MAX_KB )); then
      status=present
      LOCAL_CARGO_CACHE_PRESENT_COUNT=$((LOCAL_CARGO_CACHE_PRESENT_COUNT + 1))
    else
      status=stub
    fi
    echo "$dir=$status size_kb=$size_kb"
    case "$dir" in
      .target)
        CARGO_DOT_TARGET=true
        CARGO_DOT_TARGET_STATUS="$status"
        CARGO_DOT_TARGET_SIZE_KB="$size_kb"
        ;;
      .target-bench)
        CARGO_DOT_TARGET_BENCH=true
        CARGO_DOT_TARGET_BENCH_STATUS="$status"
        CARGO_DOT_TARGET_BENCH_SIZE_KB="$size_kb"
        ;;
      target)
        CARGO_TARGET=true
        CARGO_TARGET_STATUS="$status"
        CARGO_TARGET_SIZE_KB="$size_kb"
        ;;
    esac
  else
    echo "$dir=missing"
  fi
done
if (( LOCAL_CARGO_CACHE_PRESENT_COUNT > 0 )); then
  CARGO_CACHE_STATUS="present"
  echo "cargo_cache_status=present"
elif (( LOCAL_CARGO_CACHE_DIR_COUNT > 0 )); then
  CARGO_CACHE_STATUS="stub"
  echo "cargo_cache_status=stub"
else
  CARGO_CACHE_STATUS="missing"
  echo "cargo_cache_status=missing"
fi
echo "cargo_cache_total_kb=$LOCAL_CARGO_CACHE_TOTAL_KB"

echo ""
echo "== TypeScript reuse sources =="
TYPESCRIPT_REUSE_OUTPUT=""
if [[ -d "$ROOT/TypeScript/tests/cases" ]]; then
  line="current=$ROOT ts-populated"
  TYPESCRIPT_REUSE_OUTPUT+="$line"$'\n'
  echo "$line"
fi
if [[ -d "$PRIMARY_TS/tests/cases" ]]; then
  PRIMARY_TYPESCRIPT_STATE="ts-populated"
  line="primary=$PRIMARY_REPO ts-populated"
  TYPESCRIPT_REUSE_OUTPUT+="$line"$'\n'
  echo "$line"
else
  PRIMARY_TYPESCRIPT_STATE="ts-missing-or-unpopulated"
  line="primary=$PRIMARY_REPO ts-missing-or-unpopulated"
  TYPESCRIPT_REUSE_OUTPUT+="$line"$'\n'
  echo "$line"
fi

TS_SOURCE_COUNT=0
while IFS= read -r wt; do
  [[ -n "$wt" ]] || continue
  [[ "$wt" != "$ROOT" ]] || continue
  if [[ -d "$wt/TypeScript/tests/cases" ]]; then
    TS_SOURCE_COUNT=$((TS_SOURCE_COUNT + 1))
    line="source=$wt ts-populated"
    TYPESCRIPT_REUSE_OUTPUT+="$line"$'\n'
    echo "$line"
  fi
done < <(git -C "$ROOT" worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }')

if [[ "$COMMON_REAL" != "$GIT_REAL" && ! -e "$ROOT/TypeScript/tests/cases" ]]; then
  if [[ -d "$PRIMARY_TS/tests/cases" ]]; then
    echo "hint=run scripts/setup/link-ts-submodule.sh"
  elif (( TS_SOURCE_COUNT > 0 )); then
    echo "hint=run scripts/setup/link-ts-submodule.sh --source <source-path-above>"
  else
    echo "hint=no populated TypeScript source found; run scripts/setup/setup-ts-submodule.sh in the primary checkout first"
  fi
fi

CARGO_CACHE_SOURCE_COUNT=0
while IFS= read -r wt; do
  [[ -n "$wt" ]] || continue
  [[ "$wt" != "$ROOT" ]] || continue
  cache_kb=0
  for dir in .target .target-bench target; do
    if [[ -d "$wt/$dir" ]]; then
      size_kb="$(cache_size_kb "$wt/$dir")"
      cache_kb=$((cache_kb + size_kb))
    fi
  done
  if (( cache_kb > CARGO_CACHE_STUB_MAX_KB )); then
    CARGO_CACHE_SOURCE_COUNT=$((CARGO_CACHE_SOURCE_COUNT + 1))
  fi
done < <(git -C "$ROOT" worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }')

echo ""
echo "== cargo cache reuse summary =="
echo "cargo_cache_reuse_sources=$CARGO_CACHE_SOURCE_COUNT"
if (( LOCAL_CARGO_CACHE_PRESENT_COUNT == 0 && CARGO_CACHE_SOURCE_COUNT > 0 )); then
  echo "hint=reuse an existing cached worktree before creating a new build cache"
fi

echo ""
echo "== reusable worktree signals =="
REUSABLE_WORKTREE_OUTPUT="$(
  git -C "$ROOT" worktree list --porcelain \
  | awk '
      /^worktree / { if (path) print path "\t" branch; path=substr($0, 10); branch=""; head="" }
      /^HEAD / { head=substr($0, 6) }
      /^branch / { branch=substr($0, 8) }
      /^detached/ {
        rev=substr($0, 10)
        if (rev == "") rev=substr(head, 1, 12)
        branch="detached:" rev
      }
      END { if (path) print path "\t" branch }
    ' \
  | while IFS=$'\t' read -r wt branch; do
      [[ -n "$wt" ]] || continue
      flags=()
      [[ -L "$wt/TypeScript" ]] && flags+=("ts-link")
      [[ -d "$wt/TypeScript/tests/cases" ]] && flags+=("ts-populated")
      for dir in .target .target-bench target; do
        if [[ -d "$wt/$dir" ]]; then
          size_kb="$(cache_size_kb "$wt/$dir")"
          if (( size_kb > CARGO_CACHE_STUB_MAX_KB )); then
            flags+=("$dir")
          else
            flags+=("$dir:stub")
          fi
        fi
      done
      [[ ${#flags[@]} -eq 0 ]] && flags+=("no-local-cache-signal")
      printf "%s branch=%s %s\n" "$wt" "${branch:-unknown}" "${flags[*]}"
    done
)"
echo "$REUSABLE_WORKTREE_OUTPUT"

if echo "$GUARD_OUTPUT" | grep -q 'disk_status=low'; then
  cat <<'LOWDISK'

== low disk cleanup ladder ==
1. Reuse an existing worktree with TypeScript/cache state.
2. Run scripts/setup/disk-worktree-guard.sh --auto-prune.
3. Run scripts/setup/clean.sh --quiet to preserve .target, .target-bench, and target.
4. Delete only abandoned worktrees whose branch/PR owner is understood.
5. Use scripts/setup/clean.sh --full only as a deliberate last resort.
LOWDISK
fi

if [[ -n "$JSON_REPORT" ]]; then
  AGENT="$AGENT" \
  ROOT="$ROOT" \
  GIT_HEAD="$GIT_HEAD" \
  GIT_BRANCH="$GIT_BRANCH" \
  GIT_DETACHED="$GIT_DETACHED" \
  GIT_UPSTREAM="$GIT_UPSTREAM" \
  GIT_DIRTY="$GIT_DIRTY" \
  GIT_DIRTY_FILES="$GIT_DIRTY_FILES" \
  GIT_UNTRACKED_FILES="$GIT_UNTRACKED_FILES" \
  GUARD_OUTPUT="$GUARD_OUTPUT" \
  TYPESCRIPT_STATE="$TYPESCRIPT_STATE" \
  TYPESCRIPT_TARGET="$TYPESCRIPT_TARGET" \
  PRIMARY_REPO="$PRIMARY_REPO" \
  PRIMARY_TYPESCRIPT_STATE="$PRIMARY_TYPESCRIPT_STATE" \
  TYPESCRIPT_REUSE_OUTPUT="$TYPESCRIPT_REUSE_OUTPUT" \
  CARGO_DOT_TARGET="$CARGO_DOT_TARGET" \
  CARGO_DOT_TARGET_BENCH="$CARGO_DOT_TARGET_BENCH" \
  CARGO_TARGET="$CARGO_TARGET" \
  CARGO_DOT_TARGET_STATUS="$CARGO_DOT_TARGET_STATUS" \
  CARGO_DOT_TARGET_BENCH_STATUS="$CARGO_DOT_TARGET_BENCH_STATUS" \
  CARGO_TARGET_STATUS="$CARGO_TARGET_STATUS" \
  CARGO_DOT_TARGET_SIZE_KB="$CARGO_DOT_TARGET_SIZE_KB" \
  CARGO_DOT_TARGET_BENCH_SIZE_KB="$CARGO_DOT_TARGET_BENCH_SIZE_KB" \
  CARGO_TARGET_SIZE_KB="$CARGO_TARGET_SIZE_KB" \
  CARGO_CACHE_STATUS="$CARGO_CACHE_STATUS" \
  CARGO_CACHE_TOTAL_KB="$LOCAL_CARGO_CACHE_TOTAL_KB" \
  CARGO_CACHE_REUSE_SOURCES="$CARGO_CACHE_SOURCE_COUNT" \
  REUSABLE_WORKTREE_OUTPUT="$REUSABLE_WORKTREE_OUTPUT" \
  JSON_REPORT="$JSON_REPORT" \
  node <<'NODE'
const fs = require("fs");
const path = require("path");

function parseKeyValueLines(text) {
  const values = {};
  for (const line of text.split(/\n/)) {
    if (/^\s/.test(line)) continue;
    for (const token of line.trim().split(/\s+/)) {
      const match = /^([A-Za-z0-9_]+)=(.*)$/.exec(token);
      if (match) values[match[1]] = match[2];
    }
  }
  return values;
}

function parseTypeScriptReuse(text) {
  return text
    .split(/\n/)
    .filter(Boolean)
    .map((line) => {
      const match = /^(current|primary|source)=(.*?)(?: (.*))?$/.exec(line);
      if (!match) return { kind: "unknown", path: line, state: null };
      return {
        kind: match[1],
        path: match[2],
        state: match[3] ?? null,
      };
    });
}

function parseWorktreeSignals(text) {
  return text
    .split(/\n/)
    .filter(Boolean)
    .map((line) => {
      const [worktree, rest = ""] = line.split(" branch=");
      const parts = rest.split(/\s+/).filter(Boolean);
      const branch = parts.shift() ?? "unknown";
      return { worktree, branch, signals: parts };
    });
}

function parseGuardReuseCandidates(text) {
  const lines = text.split(/\n/);
  const candidates = [];
  let inSection = false;
  for (const line of lines) {
    if (line === "sister_worktree_reuse_candidates:") {
      inSection = true;
      continue;
    }
    if (!inSection) continue;
    if (!line.startsWith("  ")) break;

    const trimmed = line.trim();
    if (!trimmed || trimmed === "none") continue;

    const match = /^(.*?) branch=(.*?) inactive_hours>=(\d+)$/.exec(trimmed);
    if (!match) {
      candidates.push({
        path: trimmed,
        branch: null,
        inactive_hours_min: null,
      });
      continue;
    }
    candidates.push({
      path: match[1],
      branch: match[2],
      inactive_hours_min: Number(match[3]),
    });
  }
  return candidates;
}

const guard = parseKeyValueLines(process.env.GUARD_OUTPUT ?? "");
const bool = (value) => value === "true";
const diskOk = guard.disk_status === "ok";
const toNumber = (value) => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
};
const diskFreeMb = toNumber(guard.disk_free_mb);
const minFreeGb = toNumber(guard.min_free_gb);
const minFreeMb = minFreeGb * 1024;
const sisterReuseCandidates = parseGuardReuseCandidates(
  process.env.GUARD_OUTPUT ?? "",
);
const cleanupLadder = [
  "Reuse an existing worktree with TypeScript/cache state.",
  "Run scripts/setup/disk-worktree-guard.sh --auto-prune.",
  "Run scripts/setup/clean.sh --quiet to preserve .target, .target-bench, and target.",
  "Delete only abandoned worktrees whose branch/PR owner is understood.",
  "Use scripts/setup/clean.sh --full only as a deliberate last resort.",
];
const report = {
  ok: diskOk,
  status: diskOk ? "pass" : "fail",
  disk_preflight_status: diskOk ? "pass" : "fail",
  agent: process.env.AGENT,
  repo: process.env.ROOT,
  git_context: {
    head: process.env.GIT_HEAD,
    branch: process.env.GIT_BRANCH,
    detached: bool(process.env.GIT_DETACHED),
    upstream: process.env.GIT_UPSTREAM || null,
    dirty: bool(process.env.GIT_DIRTY),
    dirty_files: Number(process.env.GIT_DIRTY_FILES ?? 0),
    untracked_files: Number(process.env.GIT_UNTRACKED_FILES ?? 0),
  },
  disk_guard: {
    ...guard,
    ok: diskOk,
  },
  disk_pressure: {
    ok: diskOk,
    status: guard.disk_status ?? "unknown",
    free_mb: diskFreeMb,
    min_free_mb: minFreeMb,
    shortfall_mb: Math.max(0, minFreeMb - diskFreeMb),
    sister_reuse_candidate_count: sisterReuseCandidates.length,
    sister_reuse_candidates: sisterReuseCandidates,
    cleanup_ladder: diskOk ? [] : cleanupLadder,
  },
  typescript: {
    state: process.env.TYPESCRIPT_STATE,
    target: process.env.TYPESCRIPT_TARGET || null,
    primary: {
      path: process.env.PRIMARY_REPO,
      state: process.env.PRIMARY_TYPESCRIPT_STATE,
    },
    reuse_sources: parseTypeScriptReuse(process.env.TYPESCRIPT_REUSE_OUTPUT ?? ""),
  },
  cargo_cache: {
    status: process.env.CARGO_CACHE_STATUS,
    total_size_kb: Number(process.env.CARGO_CACHE_TOTAL_KB ?? 0),
    local: {
      ".target": bool(process.env.CARGO_DOT_TARGET),
      ".target-bench": bool(process.env.CARGO_DOT_TARGET_BENCH),
      target: bool(process.env.CARGO_TARGET),
    },
    local_status: {
      ".target": process.env.CARGO_DOT_TARGET_STATUS,
      ".target-bench": process.env.CARGO_DOT_TARGET_BENCH_STATUS,
      target: process.env.CARGO_TARGET_STATUS,
    },
    local_size_kb: {
      ".target": Number(process.env.CARGO_DOT_TARGET_SIZE_KB ?? 0),
      ".target-bench": Number(process.env.CARGO_DOT_TARGET_BENCH_SIZE_KB ?? 0),
      target: Number(process.env.CARGO_TARGET_SIZE_KB ?? 0),
    },
    reuse_sources: Number(process.env.CARGO_CACHE_REUSE_SOURCES ?? 0),
  },
  reusable_worktrees: parseWorktreeSignals(process.env.REUSABLE_WORKTREE_OUTPUT ?? ""),
};

fs.mkdirSync(path.dirname(process.env.JSON_REPORT), { recursive: true });
fs.writeFileSync(process.env.JSON_REPORT, `${JSON.stringify(report, null, 2)}\n`);
NODE
fi
