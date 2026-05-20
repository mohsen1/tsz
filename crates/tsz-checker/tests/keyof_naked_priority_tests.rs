//! Regression tests for the priority of "reverse keyof" inference.
//!
//! When a generic function takes both a naked `obj: T` parameter and a
//! `key: keyof T` parameter, tsc gives the naked argument inference site the
//! highest priority (`NakedTypeVariable`). The synthetic shape that reverse
//! keyof inference would build from a string-literal `key` argument
//! (`{ [literalKey]: any }`) is added at `LiteralKeyof` priority and must
//! never override the naked candidate.
//!
//! Without that priority gating, calls like `f({ x: 1 }, "z")` would pick the
//! synthetic `{ z: any }` shape for T and report TS2345 against the *first*
//! argument instead of the second. tsc anchors the diagnostic on the second
//! argument with the message `Argument of type '"z"' is not assignable to
//! parameter of type '"x"'.`

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

/// Repro derived from `literalTypeNameAssertionNotTriggered.ts` —
/// minimal form: `obj: T` plus `key: keyof T`, where the key literal does not
/// match any property of the inferred object. The diagnostic must point at
/// the `key` argument, not the `obj` argument.
#[test]
fn naked_obj_t_outranks_reverse_keyof_when_key_is_unknown_literal() {
    let source = r#"
declare function f<T>(obj: T, key: keyof T): void;
declare const a: { p: number; q: string };
f(a, "z");
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2345: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2345).collect();

    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345; got: {diagnostics:#?}"
    );
    let (_, msg) = ts2345[0];
    assert!(
        msg.contains("'\"z\"'") && msg.contains("'\"p\" | \"q\"'"),
        "TS2345 should report \"z\" vs \"p\" | \"q\" (the key argument). Actual: {msg}"
    );
    assert!(
        !msg.contains("{ z: any"),
        "Diagnostic should not synthesise `{{ z: any }}` from reverse keyof. Actual: {msg}"
    );
}

/// Single-property variant of the same pattern. Mirrors the conformance
/// failure reproducer where the namespace `a` from `import a = require("./a")`
/// has a single export `x`, and the call site passes the empty string as a
/// key. The diagnostic must report `"x"`, not `{ '': any }`.
#[test]
fn naked_obj_t_outranks_reverse_keyof_single_property_object() {
    let source = r#"
declare function f<T>(obj: T, key: keyof T): void;
declare const a: { x: number };
f(a, "");
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2345: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2345).collect();

    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345; got: {diagnostics:#?}"
    );
    let (_, msg) = ts2345[0];
    assert!(
        msg.contains("'\"\"'") && msg.contains("'\"x\"'"),
        "TS2345 should report \"\" vs \"x\". Actual: {msg}"
    );
}

/// The matching-key path must remain a clean pass — no diagnostics. Guards
/// against an over-eager priority filter accidentally rejecting valid calls.
#[test]
fn naked_obj_t_with_matching_key_has_no_diagnostics() {
    let source = r#"
declare function f<T>(obj: T, key: keyof T): void;
declare const a: { x: number };
f(a, "x");
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "matching key call should produce no diagnostics; got: {diagnostics:#?}"
    );
}

/// When `T` only appears under `keyof T` (no naked usage), the reverse keyof
/// inference still has to drive T from the supplied keys. tsc accepts both
/// calls below — combining the per-arg synthetic shapes via intersection.
/// Locks in that the new `LiteralKeyof` priority does not regress this case.
#[test]
fn reverse_keyof_only_still_infers_from_string_literals() {
    let source = r#"
declare function bar<T>(x: keyof T, y: keyof T): T;
const r = bar("a", "b");
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "keyof-only inference must still succeed; got: {diagnostics:#?}"
    );
}

/// Variant from the original conformance reproducer with three properties.
/// Confirms the diagnostic reports the union of all keys, not just the first.
#[test]
fn naked_obj_t_picks_union_of_keys_for_keyof_diagnostic() {
    let source = r#"
declare function f<T>(obj: T, key: keyof T): void;
declare const a: { p: number; q: string; r: boolean };
f(a, "missing");
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2345: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2345).collect();

    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345; got: {diagnostics:#?}"
    );
    let (_, msg) = ts2345[0];
    // The exact display order may vary across printer changes; just require
    // the literal was reported and all three property names appear.
    assert!(msg.contains("'\"missing\"'"), "missing the literal: {msg}");
    for key in ["\"p\"", "\"q\"", "\"r\""] {
        assert!(
            msg.contains(key),
            "expected `{key}` in TS2345 union message: {msg}"
        );
    }
}
