# shellcheck shell=bash
# Exact provenance helpers for project compatibility rows. Sourced by the
# compile guard after project-compile-fingerprint.sh.

TSZ_COMPAT_SOURCE_COMMIT=""
TSZ_COMPAT_SOURCE_DIRTY=""
TSZ_COMPAT_SOURCE_TREE_FINGERPRINT=""
TSZ_COMPAT_SOURCE_STABLE=""
TSZ_COMPAT_INITIAL_SOURCE_COMMIT=""
TSZ_COMPAT_INITIAL_SOURCE_DIRTY=""
TSZ_COMPAT_INITIAL_SOURCE_TREE_FINGERPRINT=""
TSZ_COMPAT_BUILD_MANIFEST_SHA256=""
TSZ_COMPAT_BUILD_INPUTS_SHA256=""
TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256=""
TSZ_COMPAT_EVIDENCE_PROTOCOL_FINGERPRINT=""
LAST_COMPILE_INPUT_FINGERPRINT=""
LAST_COMPILE_INPUT_STABLE=""

tsz_checkout_tree_digest_once() {
  local checkout="$1" head="$2" untracked_list="" untracked_digest="" digest=""
  untracked_list="$(mktemp)" || return 1
  if ! git -C "$checkout" ls-files --others --exclude-standard -z > "$untracked_list"; then
    rm -f "$untracked_list"
    return 1
  fi
  untracked_digest="$(node "$_TSZ_COMPILE_FINGERPRINT_BATCH_HASHER" \
    "$checkout" "$untracked_list" --untracked 2>/dev/null || true)"
  [[ "$untracked_digest" =~ ^[0-9a-f]{64}$ ]] || {
    rm -f "$untracked_list"
    return 1
  }
  digest="$({
    printf 'source-commit\0%s\0staged-diff\0' "$head"
    git -c core.quotePath=true -C "$checkout" diff --no-color --binary --no-ext-diff --cached -- || exit 1
    printf '\0unstaged-diff\0'
    git -c core.quotePath=true -C "$checkout" diff --no-color --binary --no-ext-diff -- || exit 1
    printf '\0untracked-tree\0%s\0' "$untracked_digest"
  } | sha256_of_stdin)" || {
    rm -f "$untracked_list"
    return 1
  }
  rm -f "$untracked_list"
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s\n' "$digest"
}

# Changes to evidence parsers, graph normalizers, fixture declaration writers,
# or cache-key code must invalidate result-cache entries even when the measured
# compiler binary and project fixture are unchanged.
tsz_evidence_protocol_fingerprint() {
  local checkout="$1"
  shift
  local relative file_hash
  local fields=(protocol tsz-project-evidence-v4)
  local protocol_files=(
    scripts/ci/project-compile-guard.sh
    scripts/ci/project-compatibility.mjs
    scripts/ci/project-compile-stats.mjs
    scripts/ci/lib/project-compat-evidence.sh
    scripts/ci/lib/project-compile-fingerprint.sh
    scripts/ci/lib/project-source-tree-hash.mjs
    scripts/ci/lib/project-tsc-oracle.sh
    scripts/bench/project-fixtures.sh
    scripts/bench/lib/project-fixture-stubs.sh
    scripts/bench/lib/project-fixture-stubs-canary.sh
    scripts/bench/lib/fixture-stub-inventory.mjs
  )
  # Product-specific adapters participate without making the compile guard
  # depend on benchmark-only implementation details. The benchmark runner
  # passes its evidence producer/serializer here; the guard needs no extras.
  protocol_files+=("$@")
  for relative in "${protocol_files[@]}"; do
    [[ -f "$checkout/$relative" ]] || return 1
    file_hash="$(sha256_of_file "$checkout/$relative")"
    [[ "$file_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
    fields+=(file "$relative" content "$file_hash")
  done
  tsz_hash_framed_values "${fields[@]}"
}

tsz_pin_checkout_evidence() {
  tsz_capture_checkout_evidence "$1" || return 1
  TSZ_COMPAT_INITIAL_SOURCE_COMMIT="$TSZ_COMPAT_SOURCE_COMMIT"
  TSZ_COMPAT_INITIAL_SOURCE_DIRTY="$TSZ_COMPAT_SOURCE_DIRTY"
  TSZ_COMPAT_INITIAL_SOURCE_TREE_FINGERPRINT="$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"
  TSZ_COMPAT_SOURCE_STABLE=true
}

tsz_refresh_checkout_evidence() {
  local checkout="$1"
  if ! tsz_capture_checkout_evidence "$checkout" \
    || [[ "$TSZ_COMPAT_SOURCE_COMMIT" != "$TSZ_COMPAT_INITIAL_SOURCE_COMMIT" \
      || "$TSZ_COMPAT_SOURCE_DIRTY" != "$TSZ_COMPAT_INITIAL_SOURCE_DIRTY" \
      || "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT" != "$TSZ_COMPAT_INITIAL_SOURCE_TREE_FINGERPRINT" ]]; then
    TSZ_COMPAT_SOURCE_STABLE=false
    return 1
  fi
  TSZ_COMPAT_SOURCE_STABLE=true
}

# Verify a supplied/adjacent conformance build manifest against every binary it
# names, then bind its `tsz` entry to the immutable snapshot that the guard
# actually runs. Missing or stale manifests leave the row runnable but gray.
tsz_verify_build_manifest() {
  local checkout="$1" manifest="$2" run_binary_sha="$3"
  local specs=()
  local manifest_fields=""
  local manifest_hash_before="" manifest_hash_after="" manifest_hash_final=""
  TSZ_COMPAT_BUILD_MANIFEST_SHA256=""
  TSZ_COMPAT_BUILD_INPUTS_SHA256=""
  TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256=""
  [[ -f "$manifest" && "$run_binary_sha" =~ ^[0-9a-f]{64}$ ]] || return 1
  while IFS= read -r spec; do
    [[ -n "$spec" ]] && specs+=(--binary "$spec")
  done < <(node --input-type=module - "$checkout" "$manifest" <<'NODE'
import fs from "node:fs";
import path from "node:path";
const [root, manifestPath] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
for (const [name, binary] of Object.entries(manifest.binaries || {})) {
  if (!/^[A-Za-z0-9._-]+$/.test(name) || typeof binary?.path !== "string") process.exit(1);
  process.stdout.write(`${name}=${path.resolve(root, binary.path)}\n`);
}
NODE
  )
  [[ "${#specs[@]}" -gt 0 ]] || return 1
  manifest_hash_before="$(sha256_of_file "$manifest")"
  [[ "$manifest_hash_before" =~ ^[0-9a-f]{64}$ ]] || return 1
  PYTHONDONTWRITEBYTECODE=1 python3 "$checkout/scripts/conformance/build-manifest.py" verify \
    --repo "$checkout" --manifest "$manifest" "${specs[@]}" >/dev/null 2>&1 || return 1
  manifest_hash_after="$(sha256_of_file "$manifest")"
  [[ "$manifest_hash_after" == "$manifest_hash_before" ]] || return 1
  manifest_fields="$(node --input-type=module - "$manifest" <<'NODE'
import fs from "node:fs";
const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
process.stdout.write(`${manifest.inputs?.sha256 || ""}\t${manifest.binaries?.tsz?.sha256 || ""}\n`);
NODE
  )" || return 1
  manifest_hash_final="$(sha256_of_file "$manifest")"
  [[ "$manifest_hash_final" == "$manifest_hash_before" ]] || return 1
  IFS=$'\t' read -r TSZ_COMPAT_BUILD_INPUTS_SHA256 \
    TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256 <<< "$manifest_fields"
  TSZ_COMPAT_BUILD_MANIFEST_SHA256="$manifest_hash_before"
  [[ "$TSZ_COMPAT_BUILD_MANIFEST_SHA256" =~ ^[0-9a-f]{64}$ \
    && "$TSZ_COMPAT_BUILD_INPUTS_SHA256" =~ ^[0-9a-f]{64}$ \
    && "$TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256" == "$run_binary_sha" ]]
}

# Capture two identical observations so a checkout changing while it is hashed
# cannot be attributed to a compiler result. Dirty trees are valid evidence,
# but their staged, unstaged, and untracked contents are bound explicitly.
tsz_capture_checkout_evidence() {
  local checkout="$1" attempt head_before head_after status_before status_after
  local status_before_file status_after_file dirty_before dirty_after
  local digest_before digest_after
  TSZ_COMPAT_SOURCE_COMMIT=""
  TSZ_COMPAT_SOURCE_DIRTY=""
  TSZ_COMPAT_SOURCE_TREE_FINGERPRINT=""
  for attempt in 1 2 3; do
    : "$attempt"
    status_before_file="$(mktemp)" || return 1
    status_after_file="$(mktemp)" || {
      rm -f "$status_before_file"
      return 1
    }
    head_before="$(git -C "$checkout" rev-parse HEAD 2>/dev/null || true)"
    if git -C "$checkout" status --porcelain=v1 -z --untracked-files=all \
      > "$status_before_file" 2>/dev/null; then
      status_before="$(sha256_of_file "$status_before_file")"
      [[ -s "$status_before_file" ]] && dirty_before=true || dirty_before=false
    else
      status_before=""; dirty_before=""
    fi
    digest_before="$(tsz_checkout_tree_digest_once "$checkout" "$head_before" 2>/dev/null || true)"
    head_after="$(git -C "$checkout" rev-parse HEAD 2>/dev/null || true)"
    if git -C "$checkout" status --porcelain=v1 -z --untracked-files=all \
      > "$status_after_file" 2>/dev/null; then
      status_after="$(sha256_of_file "$status_after_file")"
      [[ -s "$status_after_file" ]] && dirty_after=true || dirty_after=false
    else
      status_after=""; dirty_after=""
    fi
    digest_after="$(tsz_checkout_tree_digest_once "$checkout" "$head_after" 2>/dev/null || true)"
    rm -f "$status_before_file" "$status_after_file"
    if [[ "$head_before" =~ ^[0-9a-f]{40}$ && "$head_before" == "$head_after" \
      && "$status_before" =~ ^[0-9a-f]{64}$ && "$status_before" == "$status_after" \
      && "$dirty_before" == "$dirty_after" \
      && "$digest_before" =~ ^[0-9a-f]{64}$ && "$digest_before" == "$digest_after" ]]; then
      TSZ_COMPAT_SOURCE_COMMIT="$head_before"
      TSZ_COMPAT_SOURCE_DIRTY="$dirty_before"
      TSZ_COMPAT_SOURCE_TREE_FINGERPRINT="$digest_before"
      return 0
    fi
  done
  return 1
}

tsz_compile_input_fingerprint() {
  local name="$1" tsconfig="$2" source_root="$3" identity="" fingerprint=""
  identity="$(compute_compile_fingerprint "$name" "$tsconfig" "$source_root" 2>/dev/null || true)"
  [[ -n "$identity" ]] || return 1
  fingerprint="$(printf '%s' "$identity" | sha256_of_stdin)"
  [[ "$fingerprint" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s\n' "$fingerprint"
}

# A row is bound to its initial compile-input observation. At record time take
# two fresh observations and require all three to agree. This detects ordinary
# mid-run fixture/config mutations and fail-closes schema 3. There is no
# portable way to lock arbitrary project trees against a change-and-revert
# between observations; immutable fixture snapshots are the remaining stronger
# option for callers that need protection against an adversarial writer.
tsz_refresh_compile_input_evidence() {
  local name="$1" tsconfig="$2" source_root="$3" observed_once="" observed_twice=""
  LAST_COMPILE_INPUT_STABLE=false
  [[ "$LAST_COMPILE_INPUT_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]] || return 1
  observed_once="$(tsz_compile_input_fingerprint "$name" "$tsconfig" "$source_root" 2>/dev/null || true)"
  observed_twice="$(tsz_compile_input_fingerprint "$name" "$tsconfig" "$source_root" 2>/dev/null || true)"
  if [[ "$observed_once" =~ ^[0-9a-f]{64}$ \
    && "$observed_once" == "$observed_twice" \
    && "$observed_once" == "$LAST_COMPILE_INPUT_FINGERPRINT" ]]; then
    LAST_COMPILE_INPUT_STABLE=true
    return 0
  fi
  return 1
}

record_project_compatibility() {
  local name="$1" exit_class="$2" phase="$3" diagnostic_status="$4"
  local diagnostic_delta="${5:-}" files_reached="${6-}" peak_memory_bytes="${7:-}"
  local tsz_exit_codes="${8:-}" tsconfig_path="${9:-}" source_root="${10:-}"
  local tsc_exit_codes="${11:-}" files_reached_reason="${12:-}"
  local fixture_sources stub_evidence="" stub_schema="" stub_modules=""
  local stub_any="" stub_fingerprint="" stub_owners="[]"
  fixture_sources="$(tsz_project_fixture_sources "$name")"
  if stub_evidence="$(node "$ROOT_DIR/scripts/bench/lib/fixture-stub-inventory.mjs" \
      row-evidence "$ROOT_DIR" "$name" 2>/dev/null)"; then
    IFS=$'\t' read -r stub_schema stub_modules stub_any stub_fingerprint stub_owners \
      <<< "$stub_evidence"
  fi
  # Keep the final source/input fence immediately adjacent to artifact
  # publication; source/model inventory work above must not widen its race.
  tsz_refresh_checkout_evidence "$ROOT_DIR" || true
  tsz_refresh_compile_input_evidence "$name" "$tsconfig_path" "$source_root" || true

  local evidence_schema=""
  if [[ "${LAST_SEMANTIC_COMPLETION:-}" == "complete" \
    && "$exit_class" == "exit success" && "$diagnostic_status" == "none" ]]; then
    evidence_schema=3
  fi
  local peak_memory_bytes_reason=""
  if [[ -z "$peak_memory_bytes" ]]; then
    peak_memory_bytes_reason="$(peak_rss_unavailable_reason)"
    [[ -n "$peak_memory_bytes_reason" ]] || peak_memory_bytes_reason="process exited before sampling"
  fi

  COMPAT_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  COMPAT_OUTPUT_ROOT="$FIXTURE_ROOT" \
  COMPAT_NAME="$name" COMPAT_EXIT_CLASS="$exit_class" COMPAT_PHASE="$phase" \
  COMPAT_DIAGNOSTIC_STATUS="$diagnostic_status" COMPAT_EVIDENCE_SCHEMA="$evidence_schema" \
  COMPAT_SEMANTIC_COMPLETION="${LAST_SEMANTIC_COMPLETION:-}" \
  COMPAT_SOURCE_COMMIT="$TSZ_COMPAT_SOURCE_COMMIT" \
  COMPAT_SOURCE_DIRTY="$TSZ_COMPAT_SOURCE_DIRTY" \
  COMPAT_SOURCE_STABLE="$TSZ_COMPAT_SOURCE_STABLE" \
  COMPAT_SOURCE_TREE_FINGERPRINT="$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT" \
  COMPAT_EVIDENCE_PROTOCOL_FINGERPRINT="$TSZ_COMPAT_EVIDENCE_PROTOCOL_FINGERPRINT" \
  COMPAT_TSZ_BINARY_SHA256="${_TSZ_BINARY_HASH:-}" \
  COMPAT_BUILD_MANIFEST_SHA256="$TSZ_COMPAT_BUILD_MANIFEST_SHA256" \
  COMPAT_BUILD_INPUTS_SHA256="$TSZ_COMPAT_BUILD_INPUTS_SHA256" \
  COMPAT_BUILD_MANIFEST_BINARY_SHA256="$TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256" \
  COMPAT_COMPILE_INPUT_FINGERPRINT="${LAST_COMPILE_INPUT_FINGERPRINT:-}" \
  COMPAT_COMPILE_INPUT_STABLE="${LAST_COMPILE_INPUT_STABLE:-}" \
  COMPAT_ORACLE_FINGERPRINT="${TSC_ORACLE_CMD_HASH:-}" \
  COMPAT_ROOT_FILES="${LAST_ROOT_FILES:-}" COMPAT_SOURCE_FILES="${LAST_SOURCE_FILES:-}" \
  COMPAT_ROOT_FILE_FINGERPRINT="${LAST_ROOT_FILE_FINGERPRINT:-}" \
  COMPAT_SOURCE_FILE_FINGERPRINT="${LAST_SOURCE_FILE_FINGERPRINT:-}" \
  COMPAT_ORACLE_ROOT_FILES="${LAST_TSC_ROOT_FILES:-}" \
  COMPAT_ORACLE_SOURCE_FILES="${LAST_TSC_SOURCE_FILES:-}" \
  COMPAT_ORACLE_ROOT_FILE_FINGERPRINT="${LAST_TSC_ROOT_FINGERPRINT:-}" \
  COMPAT_ORACLE_SOURCE_FILE_FINGERPRINT="${LAST_TSC_SOURCE_FINGERPRINT:-}" \
  COMPAT_DIAGNOSTIC_RECORDS="${LAST_DIAGNOSTIC_RECORDS:-}" \
  COMPAT_DIAGNOSTIC_FINGERPRINT="${LAST_DIAGNOSTIC_FINGERPRINT:-}" \
  COMPAT_ORACLE_DIAGNOSTIC_RECORDS="${LAST_ORACLE_DIAGNOSTIC_RECORDS:-}" \
  COMPAT_ORACLE_DIAGNOSTIC_FINGERPRINT="${LAST_ORACLE_DIAGNOSTIC_FINGERPRINT:-}" \
  COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" COMPAT_FILES_REACHED="$files_reached" \
  COMPAT_FILES_REACHED_REASON="$files_reached_reason" \
  COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
  COMPAT_PEAK_MEMORY_BYTES_REASON="$peak_memory_bytes_reason" \
  COMPAT_TSZ_EXIT_CODES="$tsz_exit_codes" COMPAT_TSC_EXIT_CODES="$tsc_exit_codes" \
  COMPAT_TSCONFIG_PATH="$tsconfig_path" COMPAT_SOURCE_ROOT="$source_root" \
  COMPAT_FIXTURE_ROOT="$FIXTURE_ROOT" COMPAT_FIXTURE_SOURCES="$fixture_sources" \
  COMPAT_STUB_INVENTORY_SCHEMA="$stub_schema" COMPAT_STUBBED_MODULES="$stub_modules" \
  COMPAT_STUBBED_ANY_MEMBERS="$stub_any" COMPAT_STUB_INVENTORY_FINGERPRINT="$stub_fingerprint" \
  COMPAT_STUB_INVENTORY_OWNERS="$stub_owners" \
  COMPAT_TSZ_COMMAND_ENV_PREFIX="TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=${TSZ_RUST_MIN_STACK:-536870912}" \
  node "$ROOT_DIR/scripts/ci/project-compatibility.mjs" record
}
