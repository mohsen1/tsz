should_run_application_project() {
    if [ -n "$FILTER" ]; then
        return 0
    fi
    [ "${TSZ_BENCH_INCLUDE_APPLICATIONS:-0}" = "1" ]
}

tsz_application_benchmark_metadata() {
    local row_name="$1"
    TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" \
    TSZ_PROJECT_ROW_NAME="$row_name" \
    node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";

const rowsPath = process.env.TSZ_PROJECT_ROWS_MJS;
const rowName = process.env.TSZ_PROJECT_ROW_NAME;
const { PROJECT_ROWS_BY_NAME } = await import(pathToFileURL(rowsPath));
const row = PROJECT_ROWS_BY_NAME[rowName];

if (!row || row.category !== "application" || row.perf_timed !== true) {
  process.exit(1);
}

function shellQuote(value) {
  return `'${String(value ?? "").replace(/'/g, `'\\''`)}'`;
}

const fields = {
  app_name: row.name,
  app_label: row.label || row.name,
  app_fixture_dir: row.fixture_dir,
  app_repo: row.repo,
  app_ref: row.ref,
  app_install_cmd: row.install_cmd,
  app_install_root: row.install_root || ".",
  app_tsconfig: row.app_tsconfig,
  app_source_dir: row.source_dir || ".",
};

for (const [key, value] of Object.entries(fields)) {
  if (!value) process.exit(1);
  console.log(`${key}=${shellQuote(value)}`);
}
NODE
}

run_application_project_benchmarks() {
    local row_name="$1"
    should_run_application_project || return 0
    if ! is_benchmark_selected "$row_name"; then
        return
    fi

    eval "$(tsz_application_benchmark_metadata "$row_name")"

    print_header "Real-world Application Project - $app_label"
    local root="$EXTERNAL_BENCH_DIR/$app_fixture_dir"
    tsz_ensure_git_fixture "$app_fixture_dir" "$app_repo" "$app_ref" "$root" 1 || return 1
    echo -e "${GREEN}✓${NC} $app_label pinned at $(git -C "$root" rev-parse --short HEAD)"

    if ! (cd "$root/$app_install_root" && eval "$app_install_cmd"); then
        find "$root" -type d -name node_modules -prune -exec rm -rf {} + 2>/dev/null || true
        return 1
    fi

    local tsconfig="$root/$app_tsconfig"
    local src_dir="$root/$app_source_dir"
    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        find "$root" -type d -name node_modules -prune -exec rm -rf {} + 2>/dev/null || true
        return 1
    fi

    local rc=0
    run_project_benchmark "$app_name" "$tsconfig" "$src_dir" || rc=$?
    find "$root" -type d -name node_modules -prune -exec rm -rf {} + 2>/dev/null || true
    echo
    return "$rc"
}
