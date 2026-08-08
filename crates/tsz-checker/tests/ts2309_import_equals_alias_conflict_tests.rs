//! TS2309 ("An export assignment cannot be used in a module with other
//! exported elements") for `export import a = <entity-name>;` mixed with
//! `export = x;`.
//!
//! Structural rule (verified against `typescript@7.0.2`): an `export import`
//! alias counts as an "other exported element" for the TS2309 conflict only
//! when the aliased entity has value meaning. An alias that resolves purely
//! to a type (an interface, or a namespace with no value members) carries no
//! runtime export and does not conflict with `export =` — exactly like a
//! directly-exported `interface`/`type` alias is excluded from this same
//! check, while still counting when routed through a named `export { X }`
//! clause elsewhere in the checker. `check_export_assignment`
//! (`crates/tsz-checker/src/declarations/import/core/module_exports.rs`) is
//! the owning check for both the ambient `declare module` body and the
//! ordinary top-level module path (it is called from both
//! `check_module_body` and the source-file checker).
//!
//! Binder names are varied across rows so the check keys off structure, not a
//! fixed identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// Negative: a type-only `export import` alias does not count.
// ---------------------------------------------------------------------------

#[test]
fn export_import_alias_of_interface_in_ambient_module_does_not_emit_ts2309() {
    let src = concat!(
        "declare module \"m\" {\n",
        "    namespace x {\n",
        "        interface c {}\n",
        "    }\n",
        "    export import a = x.c;\n",
        "    export = x;\n",
        "}\n",
    );
    let got = codes(src);
    assert!(
        !got.contains(&2309),
        "a type-only `export import` alias of an interface must NOT emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_import_alias_of_interface_at_top_level_does_not_emit_ts2309() {
    let src = concat!(
        "namespace outerNs {\n",
        "    export interface Member {}\n",
        "}\n",
        "export import aliasName = outerNs.Member;\n",
        "export = outerNs;\n",
    );
    let got = codes(src);
    assert!(
        !got.contains(&2309),
        "a type-only `export import` alias at top level must NOT emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_import_alias_of_interface_with_renamed_binders_does_not_emit_ts2309() {
    // Same structure as the ambient case, with every binder renamed, to prove
    // the check is structural and not keyed on a specific identifier.
    let src = concat!(
        "declare module \"pkg\" {\n",
        "    namespace zeta {\n",
        "        interface Shape {}\n",
        "    }\n",
        "    export import shapeAlias = zeta.Shape;\n",
        "    export = zeta;\n",
        "}\n",
    );
    let got = codes(src);
    assert!(
        !got.contains(&2309),
        "a renamed type-only `export import` alias must NOT emit TS2309, got: {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive controls: a value-meaning `export import` alias still conflicts.
// ---------------------------------------------------------------------------

#[test]
fn export_import_alias_of_class_in_ambient_module_emits_ts2309() {
    let src = concat!(
        "declare module \"m\" {\n",
        "    namespace x {\n",
        "        class c {}\n",
        "    }\n",
        "    export import a = x.c;\n",
        "    export = x;\n",
        "}\n",
    );
    let got = codes(src);
    assert!(
        got.contains(&2309),
        "an `export import` alias of a class (a value) must still emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_import_alias_of_class_at_top_level_emits_ts2309() {
    let src = concat!(
        "namespace outerNs {\n",
        "    export class Member {}\n",
        "}\n",
        "export import aliasName = outerNs.Member;\n",
        "export = outerNs;\n",
    );
    let got = codes(src);
    assert!(
        got.contains(&2309),
        "an `export import` alias of a class at top level must still emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_import_alias_of_namespace_with_value_member_emits_ts2309() {
    // A namespace that itself has a value member (not just types) is not
    // type-only, so aliasing it still conflicts with `export =`.
    let src = concat!(
        "namespace outerNs {\n",
        "    export namespace inner {\n",
        "        export const v = 1;\n",
        "    }\n",
        "}\n",
        "export import aliasName = outerNs.inner;\n",
        "export = outerNs;\n",
    );
    let got = codes(src);
    assert!(
        got.contains(&2309),
        "an `export import` alias of a value-bearing namespace must still emit TS2309, got: {got:?}"
    );
}
