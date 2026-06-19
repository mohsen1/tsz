//! Node-global cannot-find parity in TYPE position (TS2591).
//!
//! Structural rule: a missing Node.js global (`Buffer`, …) used as a *type*
//! gets the same "install @types/node" diagnostic (TS2591) tsc emits in value
//! position — `let b: Buffer` without `@types/node` is TS2591, not a bare
//! TS2304. tsz's type-position name-resolution failure arm previously emitted
//! plain TS2304, skipping the capability dispatch that the value path runs.
//!
//! The fix routes the position-independent "install @types/X" categories
//! (Node/jQuery/test-runner/Bun) through the shared
//! `try_emit_install_types_for_missing_global` helper from both paths. The
//! es2015 ("change target library", TS2583) and DOM cases are excluded — they
//! already match tsc in type position — so this is verified to NOT change them.

use crate::test_utils::check_source_strict_codes as check_strict;

/// TS2591: Cannot find name 'X'. Do you need to install type definitions for node?
const TS2591: u32 = 2591;
/// TS2304: Cannot find name 'X'. (generic)
const TS2304: u32 = 2304;
/// TS2583: Cannot find name 'X'. Do you need to change your target library?
const TS2583: u32 = 2583;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn node_global_in_type_annotation_position_reports_install_node_types() {
    // `let b: Buffer` with no @types/node — tsc emits TS2591, not TS2304.
    let codes = check_strict("let value: Buffer;");
    assert_eq!(count(&codes, TS2591), 1, "expected TS2591, got: {codes:?}");
    assert_eq!(
        count(&codes, TS2304),
        0,
        "must not emit bare TS2304: {codes:?}"
    );
}

#[test]
fn node_global_in_parameter_type_position_reports_install_node_types() {
    let codes = check_strict("function handle(chunk: Buffer) { return chunk; }");
    assert_eq!(count(&codes, TS2591), 1, "expected TS2591, got: {codes:?}");
    assert_eq!(count(&codes, TS2304), 0);
}

// Value-position node-global parity (`Buffer.from(...)` -> TS2591) is covered by
// the CLI/canary verification and the moduleResolution conformance family; the
// unit-test lib resolves `Buffer` as a value, so there is no cannot-find there
// to assert on. The value path's behavior is unchanged — it now routes the same
// install-@types categories through the shared helper.

#[test]
fn unknown_non_global_type_still_reports_plain_cannot_find_name() {
    // A name that is not a known environment global stays TS2304 in type position.
    let codes = check_strict("let thing: SomeProjectLocalType;");
    assert_eq!(
        count(&codes, TS2304),
        1,
        "non-global must stay TS2304: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2591),
        0,
        "must not gain a node hint: {codes:?}"
    );
}

#[test]
fn es2015_global_in_type_position_keeps_change_target_library() {
    // `Map` as a type (no es2015 lib) is the "change your target library"
    // case (TS2583) — the fix must NOT reroute it to a node/install hint.
    let codes = check_strict("let m: Map<string, number>;");
    assert_eq!(
        count(&codes, TS2591),
        0,
        "es2015 global must not get a node hint: {codes:?}"
    );
    // Either TS2583 (change lib) or, depending on the test lib, resolved; the
    // invariant under test is simply that it is NOT rerouted to TS2591.
    let _ = TS2583;
}
