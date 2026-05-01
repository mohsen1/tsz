//! Locks in the JSX-spread missing-required-property suppression: when the
//! solver reports the spread expression is structurally assignable to the
//! whole props type, the property-by-property TS2741 missing-prop check must
//! NOT fire.
//!
//! Without this gate, a JSX spread like `{...{}}` into an intrinsic element
//! whose props type only requires members inherited from `Object.prototype`
//! (e.g. `toString`) emits a false-positive TS2741 — the missing-prop check
//! walks the spread's declared property shape (empty for `{}`) and never
//! consults the apparent shape that includes inherited Object members.
//!
//! Regression target: TypeScript's `tsxAttributeResolution5.tsx`, the
//! `<test2 {...{}} />` line.
//!
//! These tests load the standard lib so `{}` carries Object's inherited
//! members, matching the conformance harness and the `tsz` CLI.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                let lib_file = LibFile::from_source(file_name.to_string(), content);
                lib_files.push(Arc::new(lib_file));
                break;
            }
        }
    }

    lib_files
}

fn check_jsx_with_libs(source: &str) -> Vec<u32> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let options = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// Empty spread into a target whose only required member is inherited from
/// `Object.prototype` (here `toString`) must NOT emit TS2741 — `{}` is
/// structurally assignable to `{ toString(): string }` because every object
/// has `toString` via Object inheritance. The fix in
/// `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` consults the
/// solver's whole-spread assignability via `is_assignable_to(spread, props)`
/// before invoking the property-by-property missing check; if assignability
/// holds, `spread_covers_all` is set and the missing check is skipped.
///
/// Mirrors the `<test2 {...{}} />` line of TypeScript's
/// `tsxAttributeResolution5.tsx`.
#[test]
fn empty_spread_into_object_prototype_only_target_does_not_emit_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test2: Attribs2;
    }
}
interface Attribs2 {
    toString(): string;
}
<test2 {...{}} />;
"#;
    let codes = check_jsx_with_libs(source);
    assert!(
        !codes.contains(&2741),
        "Empty spread into target requiring only inherited `toString` must not emit TS2741; got: {codes:?}"
    );
}

/// Sibling lock: when the spread is genuinely missing a non-inherited required
/// property (`x: string`), TS2741 MUST still fire. The assignability gate only
/// suppresses missing-prop errors when the solver agrees the spread fully
/// satisfies the props type.
///
/// Mirrors the `<test1 {...{}} />` line of `tsxAttributeResolution5.tsx`.
#[test]
fn empty_spread_missing_non_inherited_required_property_still_emits_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x: string;
}
<test1 {...{}} />;
"#;
    let codes = check_jsx_with_libs(source);
    assert!(
        codes.contains(&2741),
        "Empty spread into target requiring `x: string` must still emit TS2741; got: {codes:?}"
    );
}

/// Generic spread whose constraint mismatches the target keeps TS2322 on the
/// element tag. This guards the assignability gate from masking real type
/// mismatches: when `T extends { x: number }` is spread into a target wanting
/// `{ x: string }`, the solver's `is_assignable_to(T, target)` returns false,
/// so `spread_covers_all` stays unset and the deferred per-spread check
/// emits TS2322.
///
/// Mirrors the `make2`/`make3` lines of `tsxAttributeResolution5.tsx`.
#[test]
fn generic_spread_with_incompatible_constraint_still_emits_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x: string;
}
function make2<T extends { x: number }>(obj: T) {
    return <test1 {...obj} />;
}
function make3<T extends { y: string }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let codes = check_jsx_with_libs(source);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert!(
        ts2322_count >= 2,
        "Both make2 and make3 must emit TS2322 (generic spread incompatible with target); got codes: {codes:?}"
    );
}
