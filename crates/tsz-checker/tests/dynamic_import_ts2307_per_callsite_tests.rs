//! Tests verifying that dynamic `import()` calls emit TS2307 per call-site,
//! independently of static imports and other dynamic imports to the same module.
//!
//! tsc behavior: each `import("unresolved")` call site independently reports TS2307.
//! There is no cross-site deduplication for dynamic imports.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Set up a checker with `ESNext` module/target so that:
/// - `import()` doesn't trigger TS1323 ("dynamic imports need --module esnext ...")
/// - module-not-found uses TS2307 (not TS2792 which applies to Classic resolution)
/// - no TS2712 for missing Promise constructor
fn check_esnext(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// A single dynamic import of a non-existent module emits TS2307.
#[test]
fn single_dynamic_import_unresolved_emits_ts2307() {
    let diags = check_esnext(r#"import("./does-not-exist");"#);
    assert_eq!(
        count_code(&diags, 2307),
        1,
        "Expected exactly one TS2307, got: {diags:?}"
    );
}

/// Multiple dynamic imports to the same non-existent module each independently emit TS2307.
/// tsc does not deduplicate per-module for dynamic import call sites.
#[test]
fn multiple_dynamic_imports_same_module_each_emit_ts2307() {
    let source = r#"
import("./does-not-exist");
import("./does-not-exist");
import("./does-not-exist");
"#;
    let diags = check_esnext(source);
    assert_eq!(
        count_code(&diags, 2307),
        3,
        "Expected three TS2307 (one per call site), got: {diags:?}"
    );
}

/// A static import followed by dynamic imports to the same non-existent module:
/// tsc emits TS2307 for the static import AND for each dynamic import call site.
#[test]
fn static_import_then_dynamic_imports_all_emit_ts2307() {
    let source = r#"
import {} from "./does-not-exist";
import("./does-not-exist");
import("./does-not-exist");
"#;
    let diags = check_esnext(source);
    // Static import: 1 TS2307; each dynamic import: 1 TS2307 each.
    assert_eq!(
        count_code(&diags, 2307),
        3,
        "Expected three TS2307 (static + 2 dynamic), got: {diags:?}"
    );
}

/// Dynamic import with a non-literal specifier (variable / concatenation) must NOT emit TS2307
/// because the specifier cannot be statically resolved.
#[test]
fn dynamic_import_non_literal_specifier_no_ts2307() {
    let source = r#"
declare const path: string;
import(path);
import("" + "./does-not-exist");
"#;
    let diags = check_esnext(source);
    assert_eq!(
        count_code(&diags, 2307),
        0,
        "Expected no TS2307 for non-literal specifiers, got: {diags:?}"
    );
}

/// Resolved module (via `declare module`) must not emit TS2307.
#[test]
fn dynamic_import_resolved_via_ambient_module_no_ts2307() {
    let source = r#"
declare module "./resolved" { export const x: number; }
import("./resolved");
"#;
    let diags = check_esnext(source);
    assert_eq!(
        count_code(&diags, 2307),
        0,
        "Expected no TS2307 for resolved ambient module, got: {diags:?}"
    );
}
