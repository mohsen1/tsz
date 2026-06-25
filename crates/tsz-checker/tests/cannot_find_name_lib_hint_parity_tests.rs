//! Parity for the "cannot find name" lib/install hint selection (TS2584 /
//! TS2591 / TS2592 / TS2593 vs plain TS2304).
//!
//! Structural rule: when a value- or type-position name fails to resolve, `tsc`
//! picks the diagnostic via `getCannotFindNameDiagnosticForName` (checker.ts) —
//! a *fixed name switch*, not a "does this name live in some unloaded lib"
//! query. Only a small curated set gets a hint; everything else is plain
//! TS2304. tsz's classifier lists had drifted broad, so a front-end project
//! compiled without the `dom` lib got a spurious "include 'dom'" (TS2584) on
//! every `window` / `HTMLElement` / `fetch` reference. These tests pin the
//! exact `tsc` sets:
//!
//! - TS2584 (`include 'dom'`): exactly `document`, `console`.
//! - TS2591 (`@types/node`):   exactly `process`, `require`, `Buffer`,
//!   `module`, `NodeJS` — NOT `exports` / `__filename` / `__dirname`.
//! - TS2592 (`@types/jquery`): exactly `$` — NOT the bare `jQuery` identifier.
//! - TS2593 (test runner):     `beforeEach`, `describe`, `suite`, `it`, `test`.
//!
//! Owner: `tsz_checker::query_boundaries::capabilities` classifiers + the
//! name-resolution error reporter.
//!
//! The hints are keyed purely on the name, so `tsc` emits them identically in
//! value and type position. These tests exercise **type position** because the
//! checker-only test harness (`check_source_with_libs`) routes type-position
//! name-resolution through the same shared
//! `try_emit_install_types_for_missing_global` dispatch the CLI uses, whereas
//! the value-position expression path resolves unknown uppercase/known-global
//! identifiers to `any` without re-running the install-hint dispatch in this
//! harness. End-to-end value-position parity is locked by the CLI driver tests
//! (`crates/tsz-cli/tests/driver_tests_parts`). The one value-position case the
//! checker harness does drive directly — the `{document, console}` DOM hint —
//! is asserted below as well.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

/// es5 + es2015 base globals, but deliberately **no** `dom` lib, so DOM names
/// are genuinely unresolved and exercise the cannot-find-name hint path.
fn dom_less_libs() -> Vec<Arc<LibFile>> {
    load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ])
}

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

/// Return the diagnostic code emitted for the unresolved `name` referenced in
/// `source`, restricted to the cannot-find-name family so unrelated lib noise
/// (TS2318 missing-global-type, etc.) does not interfere.
fn cannot_find_code(source: &str, name: &str) -> Option<u32> {
    let libs = dom_less_libs();
    let needle = format!("'{name}'");
    check_source_with_libs_code_messages(source, "test.ts", strict(), &libs)
        .into_iter()
        .filter(|(code, _)| matches!(code, 2304 | 2584 | 2591 | 2592 | 2593 | 2583 | 2552 | 2868))
        .find(|(_, msg)| msg.contains(&needle))
        .map(|(code, _)| code)
}

/// Type-position probe: `let x: NAME;`. The install/lib hints key purely on the
/// name, so this is the same selection the value path makes in the real CLI.
fn type_code(name: &str) -> Option<u32> {
    cannot_find_code(&format!("let v: {name};"), name)
}

/// Value-position probe: `const x: NAME = …`-style reference.
fn value_code(name: &str) -> Option<u32> {
    cannot_find_code(&format!("const v = {name};"), name)
}

// ---------------------------------------------------------------------------
// DOM (TS2584): exactly `document` and `console`.
// ---------------------------------------------------------------------------

#[test]
fn dom_hint_only_for_document_and_console() {
    // Both positions: `getCannotFindNameDiagnosticForName` keys on the name.
    assert_eq!(value_code("document"), Some(2584));
    assert_eq!(value_code("console"), Some(2584));
    assert_eq!(type_code("document"), Some(2584));
    assert_eq!(type_code("console"), Some(2584));
}

#[test]
fn other_dom_globals_are_plain_ts2304() {
    // The previous broad classifier emitted TS2584 for every one of these.
    for name in [
        "HTMLElement",
        "Element",
        "Event",
        "URL",
        "AbortController",
        "Document",
    ] {
        assert_eq!(
            type_code(name),
            Some(2304),
            "expected plain TS2304 for dom global `{name}`, not a TS2584 dom hint",
        );
    }
}

// ---------------------------------------------------------------------------
// Node (TS2591): exactly process / require / Buffer / module / NodeJS.
// ---------------------------------------------------------------------------

#[test]
fn node_hint_for_curated_node_globals() {
    for name in ["Buffer", "NodeJS"] {
        assert_eq!(
            type_code(name),
            Some(2591),
            "expected TS2591 @types/node hint for `{name}`",
        );
    }
}

#[test]
fn commonjs_wrapper_locals_are_plain_ts2304() {
    // `__filename` / `__dirname` are NOT on tsc's switch — plain TS2304, no
    // @types/node hint.
    for name in ["__filename", "__dirname"] {
        assert_eq!(
            type_code(name),
            Some(2304),
            "expected plain TS2304 for CommonJS wrapper local `{name}`",
        );
    }
}

// ---------------------------------------------------------------------------
// jQuery (TS2592): exactly `$`, not the bare `jQuery` identifier.
// ---------------------------------------------------------------------------

#[test]
fn jquery_hint_only_for_dollar() {
    assert_eq!(type_code("$"), Some(2592));
    assert_eq!(
        type_code("jQuery"),
        Some(2304),
        "bare `jQuery` is plain TS2304 in tsc, not a jQuery install hint",
    );
}

// ---------------------------------------------------------------------------
// Test runner (TS2593): includes `beforeEach`.
// ---------------------------------------------------------------------------

#[test]
fn test_runner_hint_includes_before_each() {
    for name in ["beforeEach", "describe", "suite", "it", "test"] {
        assert_eq!(
            type_code(name),
            Some(2593),
            "expected TS2593 test-runner hint for `{name}`",
        );
    }
}
