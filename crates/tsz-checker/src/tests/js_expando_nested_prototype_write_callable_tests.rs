//! A cross-file `.prototype` write onto a NESTED expando property
//! (`a.d.prototype = e`, where `d` is itself an expando member of `a`, not a
//! directly-declared symbol) is a legitimate class-shape declaration exactly
//! when `d`'s own declaring assignment is a function-or-class expression —
//! the only RHS shapes that carry an intrinsic `.prototype`.
//!
//! Structural rule (oracle-pinned against tsc 6.0.2, `--checkJs --allowJs`):
//!
//! > `a.d.prototype = e` is accepted (no `TS2339`) when every visible
//! > `a.d = rhs` declaring assignment has a function-expression, arrow, or
//! > class-expression RHS — matching the intrinsic `.prototype` every
//! > function/class value carries. A closed-shape RHS (`a.d = { x: 1 }`)
//! > keeps `TS2339`: an ordinary object has no `.prototype` member.
//!
//! `d` is a binder-tracked expando chain-key entry on `a`, never a real
//! declaration `resolve_identifier_symbol`/`resolve_qualified_symbol` can
//! find — unlike a plain identifier root (`function C() {}` / `C.prototype`),
//! which already had dedicated handling
//! (`js_prototype_write_root_is_callable_or_constructible`). This exercises
//! the nested-chain fallback added alongside it
//! (`nested_expando_base_link_rhs_is_callable`).

use crate::CheckerOptions;
use crate::test_utils::check_multi_file_with_global_index;

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
/// shape): a nested expando property assigned a plain function expression
/// gets its intrinsic `.prototype` recognized cross-file.
#[test]
fn nested_expando_function_rhs_prototype_write_is_clean() {
    let codes = cross_file_codes(
        "const a = {};\na.d = function() {};\n",
        "a.d.prototype = {};\n",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "function-hosted nested expando member must accept a `.prototype` write; got {codes:?}"
    );
}

/// Renamed binders: the rule is structural (RHS shape), not keyed on the
/// identifier or member names `a`/`d`.
#[test]
fn nested_expando_function_rhs_prototype_write_is_clean_renamed_binders() {
    let codes = cross_file_codes(
        "const registry = {};\nregistry.Widget = function() {};\n",
        "registry.Widget.prototype = {};\n",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "renamed-binder variant must also accept the `.prototype` write; got {codes:?}"
    );
}

/// A class-expression RHS carries the same intrinsic `.prototype`.
#[test]
fn nested_expando_class_expression_rhs_prototype_write_is_clean() {
    let codes = cross_file_codes("const a = {};\na.d = class {};\n", "a.d.prototype = {};\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "class-expression-hosted nested expando member must accept a `.prototype` write; got {codes:?}"
    );
}

/// Negative control: a closed-shape RHS (`{ x: 1 }`) is an ordinary object,
/// which has no `.prototype` member — TS2339 must survive.
#[test]
fn nested_expando_closed_object_rhs_prototype_write_stays_ts2339() {
    let codes = cross_file_codes("const a = {};\na.d = { x: 1 };\n", "a.d.prototype = {};\n");
    assert_eq!(
        codes,
        vec![2339],
        "an ordinary object-shaped nested member must keep TS2339 on `.prototype`; got {codes:?}"
    );
}

/// Negative control: a genuinely-absent member on the function-hosted nested
/// expando property must still report TS2339 — the fix must not open every
/// member, only `.prototype`.
#[test]
fn nested_expando_function_rhs_absent_member_stays_ts2339() {
    let codes = cross_file_codes(
        "const a = {};\na.d = function() {};\n",
        "a.d.reallyMissing;\n",
    );
    assert_eq!(
        codes,
        vec![2339],
        "a genuinely-absent member must still be TS2339; got {codes:?}"
    );
}

/// An empty-object-literal-hosted nested expando member (`a.d = {}`, not
/// callable) is still an OPEN container: `.prototype` is an ordinary new
/// member on it, same as any other undeclared member would be — oracle-clean
/// on tsc, distinct from the callable-RHS rule above (#17482's documented
/// follow-up gap).
#[test]
fn nested_expando_empty_object_rhs_prototype_write_is_clean() {
    let codes = cross_file_codes("const a = {};\na.d = {};\n", "a.d.prototype = {};\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "an empty-object-hosted (open, non-callable) nested expando member must still accept an ordinary `.prototype` write; got {codes:?}"
    );
}

/// The same empty-object-host open container also accepts an ordinary
/// (non-`prototype`) new member write — the general open-container rule
/// `.prototype` must fall under, not a special case.
#[test]
fn nested_expando_empty_object_rhs_ordinary_member_write_is_clean() {
    let codes = cross_file_codes("const a = {};\na.d = {};\n", "a.d.foo = 1;\n");
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "an empty-object-hosted nested expando member must accept an ordinary new member write; got {codes:?}"
    );
}
