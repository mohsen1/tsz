//! ES5 downlevel parity with tsc for variable-declaration destructuring whose
//! element target is a nested binding pattern that collapses to a single
//! non-rest identifier.
//!
//! tsc's `flattenObjectBindingOrAssignmentPattern` /
//! `flattenArrayBindingOrAssignmentPattern` only introduce an intermediate temp
//! for a nested element's source value when the nested pattern has more than one
//! element (`numElements !== 1`). A single-element nested pattern reuses the
//! source expression directly, inlining the member-access path into the leaf
//! binding (`{ m: { n } }` -> `n = _a.m.n`) instead of capturing `_a.m` in a
//! fresh temp. Parameter patterns already did this; these cases pin the same
//! behaviour for variable declarations across the object/array, temp/identifier
//! source, and defaulted-leaf paths.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es5_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

#[test]
fn object_nested_single_binding_inlines_path_within_multi_element_pattern() {
    let source = "const { a, m: { n }, z } = { a: 1, m: { n: 2 }, z: 3 };\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains("a = _a.a, n = _a.m.n, z = _a.z"),
        "Single-element nested pattern `m: {{ n }}` must inline `_a.m.n` with no \
         intermediate temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= _a.m,"),
        "No intermediate `_b = _a.m` temp should be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn object_nested_single_binding_inlines_against_identifier_source() {
    let source =
        "declare const oo: { m: { n: number }, p: number };\nconst { m: { n }, p } = oo;\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains("var n = oo.m.n, p = oo.p;"),
        "An identifier source must be reused inline as `oo.m.n` for a single \
         nested binding.\nOutput:\n{output}"
    );
}

#[test]
fn object_nested_single_array_binding_inlines_index_path() {
    let source = "const { d: [e], f } = { d: [1], f: 2 };\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains("e = _a.d[0], f = _a.f"),
        "A single-element nested array pattern `d: [e]` must inline `_a.d[0]`.\n\
         Output:\n{output}"
    );
}

#[test]
fn object_deeply_nested_single_binding_inlines_full_path() {
    let source = "const { s: { t: { u } } } = { s: { t: { u: 1 } } };\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".s.t.u;"),
        "A deep single-element chain must inline the full `.s.t.u` path with no \
         temps.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches(" = ").count(),
        1,
        "Only the single `u = ...` binding should be emitted; no intermediate \
         temps.\nOutput:\n{output}"
    );
}

#[test]
fn object_nested_single_binding_with_leaf_default_uses_one_value_temp() {
    let source = "const { a, k: { l = 5 }, z } = { a: 1, k: { l: 2 }, z: 3 };\n";
    let output = parse_lower_emit(source, es5_opts());

    // The access path is inlined into a single value temp before the default is
    // applied: `_b = _a.k.l, l = _b === void 0 ? 5 : _b`.
    assert!(
        output.contains("_b = _a.k.l, l = _b === void 0 ? 5 : _b"),
        "A defaulted leaf must inline the access path into one value temp.\n\
         Output:\n{output}"
    );
    assert!(
        !output.contains("= _a.k,"),
        "No intermediate `_b = _a.k` temp should be emitted for the defaulted \
         leaf.\nOutput:\n{output}"
    );
}

#[test]
fn array_nested_single_binding_inlines_alongside_object_rest_sibling() {
    // The object-rest sibling forces the deferred (two-phase) array path; the
    // single-element nested sibling must still inline its read+decomposition in
    // source order rather than capturing a temp.
    let source = "declare const arr2: [{ p: number, q: number }, { m: { n: number } }];\n\
         const [{ p, ...pr }, { m: { n } }] = arr2;\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains("n = arr2[1].m.n"),
        "The single nested sibling must inline `arr2[1].m.n` even on the \
         deferred object-rest path.\nOutput:\n{output}"
    );
    assert!(
        output.contains("pr = __rest(_a"),
        "The object-rest sibling still decomposes through its own temp.\n\
         Output:\n{output}"
    );
}

#[test]
fn object_nested_single_with_intermediate_default_captures_leaf_default_once() {
    // Repro for #14766: a single top-level nested element with an intermediate
    // default (`{ ... } = {}`) and a defaulted leaf, sourced from a
    // NON-identifier expression (a call). tsc captures the leaf member access in
    // one temp before applying the default; tsz used to re-read `.bee` twice, so
    // a getter with side effects would fire twice.
    let source = "declare function get(): { a?: { bee?: number } };\n\
         const { a: { bee = 1 } = {} } = get();\n";
    let output = parse_lower_emit(source, es5_opts());

    // The leaf `.bee` member access must be read exactly once (into a temp), and
    // the `=== void 0` default test must reference that temp, never the member
    // access itself (the double-eval signature `X.bee === void 0 ? 1 : X.bee`).
    assert!(
        !output.contains(".bee === void 0"),
        "The defaulted leaf must test a captured temp, not re-read `.bee`.\n\
         Output:\n{output}"
    );
    assert_eq!(
        output.matches(".bee").count(),
        1,
        "The leaf member access `.bee` must be emitted exactly once.\n\
         Output:\n{output}"
    );
}

#[test]
fn object_nested_single_with_intermediate_default_object_literal_source_captures_once() {
    // Same shape as above but sourced from an object literal (also a
    // non-identifier expression). The leaf default must still capture once.
    let source = "const { a: { bee = 1 } = {} } = { a: { bee: 5 } };\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        !output.contains(".bee === void 0"),
        "Object-literal source: defaulted leaf must test a captured temp.\n\
         Output:\n{output}"
    );
    assert_eq!(
        output.matches(".bee").count(),
        1,
        "Object-literal source: `.bee` must be emitted exactly once.\n\
         Output:\n{output}"
    );
}

#[test]
fn object_nested_single_with_intermediate_default_no_leaf_default_reads_once() {
    // Regression guard for the no-default branch of the same inline path: the
    // leaf without a default must still be a single inline read (no temp, no
    // `=== void 0` test on the member access).
    let source = "declare function get(): { a?: { bee?: number } };\n\
         const { a: { bee } = {} } = get();\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        !output.contains(".bee === void 0"),
        "A non-defaulted leaf must not gain a `=== void 0` test.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches(".bee").count(),
        1,
        "A non-defaulted leaf must read `.bee` exactly once.\nOutput:\n{output}"
    );
}

#[test]
fn multi_element_nested_pattern_keeps_intermediate_temp() {
    // Sanity: a nested pattern with more than one binding still materializes the
    // source once into a temp (`numElements !== 1`) — unchanged behaviour.
    let source = "const { p: { q, r } } = { p: { q: 1, r: 2 } };\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".p, q = _a.q, r = _a.r"),
        "A 2-element nested pattern must keep its `_a = ....p` source temp.\n\
         Output:\n{output}"
    );
}
