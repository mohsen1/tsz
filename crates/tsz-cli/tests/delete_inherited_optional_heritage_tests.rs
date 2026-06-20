//! #14112 regression: `delete obj.prop` must stay legal when `prop` is declared
//! optional in a *base* interface and inherited (without redeclaration) through
//! the heritage chain, even when — under the project pipeline's fresh per-file
//! parallel checking — the receiver is a deferred cross-file
//! `Application`/intersection at the delete site.
//!
//! Witness: ofetch `src/fetch.ts` — `delete context.options.query`, where
//! `query?: Record<string, any>` is declared on `FetchOptions`
//! (`extends Omit<RequestInit, ...>`) and inherited by
//! `ResolvedFetchOptions extends FetchOptions`. The `Omit<RequestInit, ...>`
//! heritage layer keeps the cross-file merged receiver from collapsing to a
//! flat object shape, so the deleted property is absent from the receiver's own
//! `ObjectShape`; tsz's optionality lookup read only that shape and emitted a
//! false `TS2790`. `tsc`/`tsgo` accept the delete.
//!
//! This drives the real binary (DOM lib via the explicit `lib` set) with the
//! consumer file listed before the declaring module and the genuine
//! per-file `par_iter` fresh-checker path forced
//! (`TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK[_TINY]`), reproducing the project-row
//! schedule under which the receiver stays deferred. A negative case pins that
//! a genuinely required inherited member still reports `TS2790`.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_delete_heritage_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

// `include` (glob discovery) rather than `files`: it drives the module-graph
// discovery / check order under which the consumer is checked before the
// declaring module's interface flattens — the schedule that leaves the
// receiver a deferred cross-file form at the delete site (the witness does not
// reproduce under an explicit `files` list).
const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2020", "DOM"],
    "noEmit": true
  },
  "include": ["fetch.ts", "types.ts"]
}
"#;

/// Run the project on the genuine concurrent per-file checker path and return
/// stdout (diagnostics, one per line). Both force-parallel flags are set so a
/// sub-floor (2-file) witness actually runs through `par_iter`.
fn run_tiny_parallel(files: &[(&str, &str)]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new("witness").expect("temp dir");
    for (name, source) in files {
        std::fs::write(temp.path.join(name), source).expect("write source");
    }
    std::fs::write(temp.path.join("tsconfig.json"), TSCONFIG).expect("write tsconfig");

    let output = Command::new(&tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(&temp.path)
        .env("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK", "1")
        .env("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY", "1")
        .output()
        .expect("run tsz on witness");
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn count_ts2790(stdout: &str) -> usize {
    stdout
        .lines()
        .filter(|line| line.contains("error TS2790"))
        .count()
}

/// Build the declaring module. `fetch_options_members` is spliced into the
/// generic `FetchOptions<R, T>` body (which `extends Omit<RequestInit, ...>`);
/// `ResolvedFetchOptions<R, T> extends FetchOptions<R, T>` inherits them, and
/// `FetchContext.options: ResolvedFetchOptions<R, T>` keeps the delete-site
/// receiver a generic (deferred) application.
fn types_src(fetch_options_members: &str) -> String {
    format!(
        r#"export type ResponseType = "json" | "text" | "blob";

export interface FetchContext<T = any, R extends ResponseType = ResponseType> {{
  request: string;
  options: ResolvedFetchOptions<R, T>;
}}

export interface FetchOptions<R extends ResponseType = ResponseType, T = any>
  extends Omit<RequestInit, "body"> {{
{fetch_options_members}
}}

export interface ResolvedFetchOptions<
  R extends ResponseType = ResponseType,
  T = any
> extends FetchOptions<R, T> {{
  extra: number;
}}
"#
    )
}

/// The reduced ofetch witness: deleting a base-inherited optional through an
/// `Omit<...>` heritage layer (guarded + bare) must not emit TS2790.
#[test]
fn delete_base_inherited_optional_through_omit_heritage_is_legal() {
    // Consumer first: `fetch.ts` is checked before `types.ts` has flattened
    // `ResolvedFetchOptions`, so the receiver stays a deferred cross-file form.
    let types = types_src(
        "  baseURL?: string;\n  params?: Record<string, any>;\n  query?: Record<string, any>;\n  responseType?: R;",
    );
    let files = &[
        (
            "fetch.ts",
            r#"import type { FetchContext } from "./types";
export function handle(context: FetchContext) {
  if (context.options.query) {
    delete context.options.query;
  }
  delete context.options.params;
}
"#,
        ),
        ("types.ts", types.as_str()),
    ];
    let Some(stdout) = run_tiny_parallel(files) else {
        println!("skipping #14112 witness: tsz binary not found");
        return;
    };
    assert_eq!(
        count_ts2790(&stdout),
        0,
        "base-inherited optional through Omit heritage must not emit TS2790.\nstdout:\n{stdout}"
    );
}

/// Negative control: deleting a genuinely *required* base-inherited property
/// still reports TS2790 — the heritage-aware optionality lookup must not
/// blanket-suppress the error.
#[test]
fn delete_base_inherited_required_property_still_errors() {
    let types = types_src("  anchor: number;");
    let files = &[
        (
            "fetch.ts",
            r#"import type { FetchContext } from "./types";
export function handle(context: FetchContext) {
  delete context.options.anchor;
}
"#,
        ),
        ("types.ts", types.as_str()),
    ];
    let Some(stdout) = run_tiny_parallel(files) else {
        println!("skipping #14112 negative control: tsz binary not found");
        return;
    };
    assert_eq!(
        count_ts2790(&stdout),
        1,
        "deleting a required base-inherited property must still report TS2790.\nstdout:\n{stdout}"
    );
}
