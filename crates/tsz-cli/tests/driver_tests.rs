use super::args::CliArgs;
use super::driver::{
    CompilationCache, compile, compile_with_cache, compile_with_cache_and_changes,
};
use clap::Parser;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tsz_binder::BinderState;
use tsz_binder::SymbolId;
use tsz_binder::state::BinderStateScopeInputs;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_solver::construction::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "tsz_cli_driver_test_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn with_types_versions_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
    super::driver::with_types_versions_env(value, f)
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn default_args() -> CliArgs {
    // Use clap's parser to create default args - this handles all the many fields automatically
    CliArgs::try_parse_from(["tsz"]).expect("default args should parse")
}

fn parse_args(args: &[&str]) -> CliArgs {
    CliArgs::try_parse_from(args).expect("test args should parse")
}

fn assert_cli_option_validation_reports(args: &[&str], file_name: &str, source: &str, code: u32) {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join(file_name), source);

    let args = parse_args(args);
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code == code),
        "expected TS{code} for args {args:?}, got diagnostics: {:#?}",
        result.diagnostics
    );
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a semantically contiguous slice of CLI driver tests.
include!("driver_tests_parts/part_00.rs");
include!("driver_tests_parts/part_01.rs");
include!("driver_tests_parts/part_02.rs");
include!("driver_tests_parts/part_03.rs");
include!("driver_tests_parts/part_04.rs");
include!("driver_tests_parts/part_05.rs");
include!("driver_tests_parts/part_06.rs");
include!("driver_tests_parts/part_07.rs");
include!("driver_tests_parts/part_08.rs");
include!("driver_tests_parts/part_09.rs");
include!("driver_tests_parts/part_10.rs");
include!("driver_tests_parts/part_11.rs");
include!("driver_tests_parts/part_12.rs");
