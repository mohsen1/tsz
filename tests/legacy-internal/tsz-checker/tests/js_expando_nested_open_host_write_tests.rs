//! A write onto a NESTED expando property whose own base link is an OPEN,
//! non-callable expando host (`a.d = {}`, an empty object literal — not a
//! function or class expression) is an ordinary new-member declaration, not
//! a class-shape declaration. Every member name is eligible, including
//! `prototype`: an empty object has no intrinsic `.prototype`, so writing
//! one is just adding a fresh member to an open container, exactly like any
//! other undeclared member.
//!
//! Structural rule (oracle-pinned against tsc 6.0.2, `--checkJs --allowJs
//! --noImplicitAny`):
//!
//! > `a.d.member = e` is accepted (no `TS2339`) when every visible `a.d =
//! > rhs` declaring assignment is host-shaped (empty object literal,
//! > function, or class expression) — matching the same rule an ordinary
//! > `a.member = e` on `a` itself already follows. This holds cross-file:
//! > `a.d = {}` in one file, `a.d.member = e` in another. A closed-shape RHS
//! > (`a.d = { x: 1 }`) keeps `TS2339`.
//!
//! `d` is a binder-tracked expando chain-key entry on `a`, never a real
//! declaration `resolve_identifier_symbol`/`resolve_qualified_symbol` can
//! find, so the direct-root write path
//! (`root_symbol_supports_js_direct_expando_write`) never reaches it. This
//! exercises the nested-chain fallback added alongside it
//! (`expando_base_link_host_verdict`, shared with
//! `nested_expando_base_link_is_declared`). Distinct from
//! `js_expando_nested_prototype_write_callable_tests`, which covers the
//! narrower case where the nested base is itself callable (function/class
//! RHS) and so carries an intrinsic `.prototype`.

use crate::CheckerOptions;
use crate::test_utils::check_multi_file_with_global_index;
use crate::test_utils::check_source;

fn same_file_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "writer.js",
        CheckerOptions {
            no_implicit_any: true,
            check_js: true,
            allow_js: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

fn cross_file_codes(host_source: &str, writer_source: &str) -> Vec<u32> {
    check_multi_file_with_global_index(
        &[("host.js", host_source), ("writer.js", writer_source)],
        "writer.js",
        CheckerOptions {
            no_implicit_any: true,
            check_js: true,
            allow_js: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

/// The motivating fixture (`conformance/salsa/jsContainerMergeJsContainer.ts`
/// family, follow-up to the callable-only nested `.prototype` fix): an
/// ordinary new member write onto an open (non-callable) nested expando host
/// is clean cross-file.
#[test]
fn nested_open_host_new_member_write_is_clean() {
    let codes = cross_file_codes("const a = {};\na.d = {};\n", "a.d.newMember = 5;\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "an ordinary new member on an open nested expando host must be clean; got {codes:?}"
    );
}

/// `prototype` is not special here: an open (non-callable) host has no
/// intrinsic `.prototype`, so writing one is just another ordinary member.
#[test]
fn nested_open_host_prototype_write_is_clean() {
    let codes = cross_file_codes("const a = {};\na.d = {};\n", "a.d.prototype = {};\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "a `.prototype` write on an open nested expando host must be clean, matching any other new member; got {codes:?}"
    );
}

/// Renamed binders: the rule is structural (RHS shape), not keyed on the
/// identifier or member names `a`/`d`.
#[test]
fn nested_open_host_new_member_write_is_clean_renamed_binders() {
    let codes = cross_file_codes(
        "const registry = {};\nregistry.widgets = {};\n",
        "registry.widgets.count = 5;\n",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "renamed-binder variant must also accept the new member write; got {codes:?}"
    );
}

/// Same-file variant of the open-host new-member write: this already worked
/// before the fix (the binder records same-file declaring writes directly),
/// so this pins the pre-existing behavior against a regression.
#[test]
fn nested_open_host_new_member_write_is_clean_same_file() {
    let codes = same_file_codes("const a = {};\na.d = {};\na.d.newMember = 5;\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "same-file open-host new member write must stay clean; got {codes:?}"
    );
}

/// Negative control: a closed-shape RHS (`{ x: 1 }`) is an ordinary object,
/// not an open host — a later nested member write must keep `TS2339`.
#[test]
fn nested_closed_host_new_member_write_stays_ts2339() {
    let codes = cross_file_codes("const a = {};\na.d = { x: 1 };\n", "a.d.newMember = 5;\n");
    assert_eq!(
        codes,
        vec![2339],
        "a closed-shape nested host must keep TS2339 on a new member; got {codes:?}"
    );
}

/// Negative control: a closed-shape RHS also keeps `TS2339` for a
/// `.prototype` write (already covered by
/// `js_expando_nested_prototype_write_callable_tests`, pinned again here for
/// this file's own coverage).
#[test]
fn nested_closed_host_prototype_write_stays_ts2339() {
    let codes = cross_file_codes("const a = {};\na.d = { x: 1 };\n", "a.d.prototype = {};\n");
    assert_eq!(
        codes,
        vec![2339],
        "a closed-shape nested host must keep TS2339 on `.prototype`; got {codes:?}"
    );
}

/// Regression control (`salsa/typeFromPropertyAssignment23.ts`): a write on
/// a REAL class's `.prototype` for a member the class never declares must
/// stay `TS2339`. `Module.prototype` is a nested chain whose base link's
/// own member name happens to be `prototype`, which
/// `nested_expando_base_link_is_declared` treats as a vacuous pass for its
/// own narrower question (validating a chain base before a further recorded-
/// assignment lookup) — a fix that reused that carve-out to grant this write
/// outright would wrongly silence every undeclared member on any class's
/// prototype.
#[test]
fn nested_open_host_write_does_not_bypass_real_class_prototype_member_check() {
    let codes = same_file_codes(
        "class Module {}\nModule.prototype.identifier = undefined;\nModule.prototype.size = null;\n",
    );
    assert_eq!(
        codes,
        vec![2339, 2339],
        "an undeclared member write on a real class's `.prototype` must still be TS2339; got {codes:?}"
    );
}
