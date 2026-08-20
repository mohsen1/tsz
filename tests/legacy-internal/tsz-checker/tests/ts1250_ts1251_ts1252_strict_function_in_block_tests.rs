//! Regression tests for the TS1250/TS1251/TS1252 family — a function
//! declaration nested inside a block (if/while/for/etc.) when targeting
//! ES3/ES5 in strict mode.
//!
//! `tsc` picks one of three messages depending on *why* the enclosing code is
//! strict, oracle-confirmed (`typescript@5.6.2`, since the pinned
//! `typescript@7.0.2` native compiler has removed `--target es5` entirely and
//! can no longer be invoked to observe this legacy family directly):
//!
//! - inside a class body -> TS1251 ("Class definitions are automatically in
//!   strict mode.")
//! - else inside an external module (has top-level `import`/`export`) ->
//!   TS1252 ("Modules are automatically in strict mode.")
//! - else (an explicit `"use strict"` directive, or `alwaysStrict`) -> TS1250
//!
//! Class beats module beats explicit directive when more than one reason
//! applies (`export class C { m() { if (true) function f() {} } }` still
//! reports TS1251, and a module file with its own `"use strict"` prologue
//! still reports TS1252, not TS1250).
//!
//! `crates/tsz-checker/src/declarations/declarations.rs`'s
//! `check_strict_mode_function_in_block` used to only distinguish class vs.
//! not-class, so a module-hosted, non-class occurrence fell through to the
//! generic TS1250 message instead of TS1252 — and, since module-ness was not
//! itself treated as a strict-mode reason there, some module-hosted cases
//! reported no diagnostic at all.

use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

/// This whole family is target-gated: `tsc` only reports it when targeting
/// ES3 or ES5 (downlevel `function` hoisting semantics differ from ES2015+
/// block-scoped `function` declarations). Tests below target ES5 unless
/// checking that gate itself.
fn diag_codes(source: &str) -> Vec<u32> {
    diag_codes_with_target(source, ScriptTarget::ES5)
}

fn diag_codes_with_target(source: &str, target: ScriptTarget) -> Vec<u32> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// `CheckerOptions::default()` sets `always_strict: true` (tsc's own default
/// when not explicitly configured), which alone is a strict-mode reason. The
/// sloppy-mode negative controls below need it off to isolate "no reason at
/// all to be strict".
fn diag_codes_not_always_strict(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES5,
            always_strict: false,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Plain script, explicit `"use strict"` prologue, no class, no module -> TS1250.
#[test]
fn ts1250_plain_script_with_use_strict_directive() {
    let codes = diag_codes(
        r#"
"use strict";
if (true) {
    function f1() {}
}
"#,
    );
    assert!(codes.contains(&1250), "Expected TS1250. Got: {codes:?}");
    assert!(
        !codes.contains(&1251) && !codes.contains(&1252),
        "Got: {codes:?}"
    );
}

/// Function declaration in a block inside a class method -> TS1251 (class wins).
#[test]
fn ts1251_inside_class_method() {
    let codes = diag_codes(
        r#"
class C {
    m() {
        if (true) {
            function f2() {}
        }
    }
}
"#,
    );
    assert!(codes.contains(&1251), "Expected TS1251. Got: {codes:?}");
    assert!(
        !codes.contains(&1250) && !codes.contains(&1252),
        "Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: renamed class/method/function names.
#[test]
fn ts1251_inside_class_method_renamed() {
    let codes = diag_codes(
        r#"
class WrappedThing {
    invoke() {
        if (true) {
            function innerHelper() {}
        }
    }
}
"#,
    );
    assert!(
        codes.contains(&1251),
        "Renamed variant: expected TS1251. Got: {codes:?}"
    );
}

/// Module top-level (file has `export`), no class, no explicit directive -> TS1252.
#[test]
fn ts1252_inside_external_module_no_directive() {
    let codes = diag_codes(
        r#"
export {};
if (true) {
    function f3() {}
}
"#,
    );
    assert!(codes.contains(&1252), "Expected TS1252. Got: {codes:?}");
    assert!(
        !codes.contains(&1250) && !codes.contains(&1251),
        "Got: {codes:?}"
    );
}

/// A module made external via `import` rather than `export` still gets TS1252.
#[test]
fn ts1252_inside_external_module_via_import() {
    let codes = diag_codes(
        r#"
import "some-module";
if (true) {
    function f4() {}
}
"#,
    );
    assert!(codes.contains(&1252), "Expected TS1252. Got: {codes:?}");
}

/// Module *and* an explicit `"use strict"` prologue -> still TS1252, not TS1250:
/// module-ness outranks the explicit directive as the stated reason.
#[test]
fn ts1252_module_outranks_explicit_use_strict_directive() {
    let codes = diag_codes(
        r#"
export {};
"use strict";
if (true) {
    function f5() {}
}
"#,
    );
    assert!(codes.contains(&1252), "Expected TS1252. Got: {codes:?}");
    assert!(!codes.contains(&1250), "Got: {codes:?}");
}

/// Class *and* module (an exported class) -> TS1251: class outranks module.
#[test]
fn ts1251_class_outranks_module() {
    let codes = diag_codes(
        r#"
export class C {
    m() {
        if (true) {
            function f6() {}
        }
    }
}
"#,
    );
    assert!(codes.contains(&1251), "Expected TS1251. Got: {codes:?}");
    assert!(!codes.contains(&1252), "Got: {codes:?}");
}

/// Negative control: a plain script (no class, no module, no directive, no
/// `alwaysStrict`) is sloppy mode — a function declaration in a block is
/// legal ES3/ES5 sloppy-mode behavior, no TS1250/1251/1252.
#[test]
fn no_diagnostic_in_sloppy_mode_script() {
    let codes = diag_codes_not_always_strict(
        r#"
if (true) {
    function f7() {}
}
"#,
    );
    assert!(
        !codes.contains(&1250) && !codes.contains(&1251) && !codes.contains(&1252),
        "Sloppy-mode script should not report the strict-mode family. Got: {codes:?}"
    );
}

/// Negative control: the whole family is gated on ES3/ES5; an otherwise
/// identical strict-mode shape at ES2015+ reports nothing from this family.
#[test]
fn no_diagnostic_at_es2015_target() {
    let codes = diag_codes_with_target(
        r#"
"use strict";
if (true) {
    function f9() {}
}
"#,
        ScriptTarget::ES2015,
    );
    assert!(
        !codes.contains(&1250) && !codes.contains(&1251) && !codes.contains(&1252),
        "ES2015+ target should not report the strict-mode family. Got: {codes:?}"
    );
}

/// Negative control: a namespace body is not itself strict and has no
/// `import`/`export`, so a nested function-in-block stays legal.
#[test]
fn no_diagnostic_inside_plain_namespace() {
    let codes = diag_codes_not_always_strict(
        r#"
namespace M {
    if (true) {
        function f8() {}
    }
}
"#,
    );
    assert!(
        !codes.contains(&1250) && !codes.contains(&1251) && !codes.contains(&1252),
        "Namespace alone should not induce strict mode. Got: {codes:?}"
    );
}
