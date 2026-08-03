//! Regression tests for #16284: an object-literal `set` accessor's return-type
//! annotation was parsed (to consume the tokens and report TS1095) and then
//! thrown away — `parse_object_set_accessor`
//! (`crates/tsz-parser/src/parser/state_expressions_literals/object_members.rs`)
//! hard-coded `type_annotation: NodeIndex::NONE` on the built node regardless of
//! whether a `: Type` was actually present.
//!
//! The class and interface/type-literal setter arms already store the parsed
//! type in `AccessorData::type_annotation` (`state_statements_class_members.rs`,
//! `state_declarations.rs`); this brings the object-literal arm in line with
//! both, mirroring `parse_object_get_accessor` right above it in the same file,
//! which already stored its own return type correctly.
//!
//! Every position below was recorded from the pinned `typescript@7.0.2` oracle
//! (`--noEmit --strict --pretty false --lib es2022 --target es2022`), not
//! derived from the rule. Binder names vary per row so no check can key on an
//! identifier string.

use crate::parser::NodeIndex;
use crate::parser::node::AccessorData;
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;

/// `AccessorData` for the first accessor node of `kind` in the parse tree.
fn first_accessor_of_kind(parser: &crate::parser::ParserState, kind: u16) -> AccessorData {
    let arena = parser.get_arena();
    let node = arena
        .nodes
        .iter()
        .find(|node| node.kind == kind)
        .unwrap_or_else(|| panic!("expected an accessor node of kind {kind}"));
    arena
        .get_accessor(node)
        .unwrap_or_else(|| panic!("node of kind {kind} should carry AccessorData"))
        .clone()
}

// ---------------------------------------------------------------------------
// The reported witness: the annotation is stored, not discarded.
// ---------------------------------------------------------------------------

#[test]
fn object_literal_set_accessor_return_type_annotation_is_stored() {
    let source = "const oa20 = {\n  set pa20(va20: string): void {}\n};";
    let (parser, _root) = parse_source(source);

    let accessor = first_accessor_of_kind(&parser, syntax_kind_ext::SET_ACCESSOR);
    assert!(
        accessor.type_annotation.is_some(),
        "object-literal setter's `: void` must be stored on AccessorData, not discarded"
    );
}

/// Control: an object-literal setter without a return type annotation still
/// stores `NodeIndex::NONE` — the fix must not fabricate an annotation.
#[test]
fn object_literal_set_accessor_without_return_type_stores_none() {
    let source = "const ob21 = {\n  set pb21(vb21: string) {}\n};";
    let (parser, _root) = parse_source(source);

    let accessor = first_accessor_of_kind(&parser, syntax_kind_ext::SET_ACCESSOR);
    assert_eq!(
        accessor.type_annotation,
        NodeIndex::NONE,
        "a setter with no `: Type` must keep type_annotation as NONE"
    );
}

/// Control: the paired `get` accessor in the same object literal keeps storing
/// its own return type correctly (this arm was never broken) — guards against
/// a future edit accidentally coupling the two arms incorrectly.
#[test]
fn object_literal_get_accessor_return_type_annotation_still_stored() {
    let source =
        "const oc22 = {\n  get pc22(): number { return 1; },\n  set pc22(vc22: number) {}\n};";
    let (parser, _root) = parse_source(source);

    let getter = first_accessor_of_kind(&parser, syntax_kind_ext::GET_ACCESSOR);
    assert!(
        getter.type_annotation.is_some(),
        "paired getter's `: number` must remain stored"
    );
}

// ---------------------------------------------------------------------------
// Node end position: the accessor's span now extends through the return type
// when a body is missing, exactly like `parse_object_get_accessor` already
// does — `signature_end` mirrors that sibling's existing `self.token_pos()`
// computation verbatim.
//
// That computation is itself an *existing*, shared quirk: `token_pos()`
// returns the start of the next real token, skipping trivia, so when
// whitespace/a newline separates the return type from the next token the
// reported position lands one past the type's true last character rather
// than on it (oracle for the analogous object-literal `get`, `case_get.ts`:
// `void` ends at 0-based offset 28, tsc reports TS1005 at 28 — tsz's
// existing `parse_object_get_accessor` reports at 29). This test pins the
// *fixed* behavior — the position now depends on the return type at all,
// rather than always landing on the `)` — without also fixing that
// pre-existing off-by-one, which predates this change, already affects the
// getter arm, and is out of scope for #16284 (`type_annotation` being
// discarded, not diagnostic positioning).
// ---------------------------------------------------------------------------

#[test]
fn missing_body_after_return_type_reports_open_brace_near_end_of_type_not_close_paren() {
    let source = "const od23 = {\n  set pd23(vd23: string): void\n};";
    let (parser, _root) = parse_source(source);

    // `signature_end - 1` where `signature_end` is `token_pos()` of the next
    // real token after the return type — the character immediately before
    // `}` (whatever trivia occupies it), not the `)`.
    let close_brace_pos = source.find('}').expect("`}` in source") as u32;
    let expected_pos = close_brace_pos - 1;
    let close_paren_pos = source.find(')').expect("`)` in source") as u32;
    assert_ne!(
        expected_pos, close_paren_pos,
        "test setup sanity: the two candidate positions must differ"
    );

    let diagnostics = parser.get_diagnostics();
    use tsz_common::diagnostics::diagnostic_codes;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.start == expected_pos),
        "expected TS1005 `'{{' expected.` at {expected_pos} (mirroring \
         `parse_object_get_accessor`'s existing computation), got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.start == close_paren_pos),
        "TS1005 must not land on the `)` once a return type annotation follows it — that was \
         the pre-fix behavior (signature_end always fell back to close_paren_end), \
         got {diagnostics:?}"
    );
}

/// Adjacent case: the same missing-body shape with no return type annotation
/// keeps reporting at the `)`, matching the pre-existing (already-correct)
/// behavior for that shape.
#[test]
fn missing_body_without_return_type_reports_open_brace_at_close_paren() {
    let source = "const oe24 = {\n  set pe24(ve24: string)\n};";
    let (parser, _root) = parse_source(source);

    let close_paren_pos = source.find(')').expect("`)` in source") as u32;

    let diagnostics = parser.get_diagnostics();
    use tsz_common::diagnostics::diagnostic_codes;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.start == close_paren_pos),
        "expected TS1005 `'{{' expected.` at the `)` ({close_paren_pos}) when there is no \
         return type, got {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent container matrix: the class and interface/type-literal arms were
// never broken (they already stored the annotation before this fix) — pinned
// here as controls so a future shared-helper refactor can't silently regress
// this container while leaving the others green.
// ---------------------------------------------------------------------------

#[test]
fn class_set_accessor_return_type_annotation_still_stored() {
    let source = "class Cf25 {\n  set pf25(vf25: string): void {}\n}";
    let (parser, _root) = parse_source(source);

    let accessor = first_accessor_of_kind(&parser, syntax_kind_ext::SET_ACCESSOR);
    assert!(
        accessor.type_annotation.is_some(),
        "class setter's `: void` must remain stored (control, unrelated container)"
    );
}

#[test]
fn interface_set_accessor_signature_return_type_annotation_still_stored() {
    let source = "interface Ig26 { set pg26(vg26: string): void; }";
    let (parser, _root) = parse_source(source);

    let accessor = first_accessor_of_kind(&parser, syntax_kind_ext::SET_ACCESSOR);
    assert!(
        accessor.type_annotation.is_some(),
        "interface setter signature's `: void` must remain stored (control, unrelated container)"
    );
}
