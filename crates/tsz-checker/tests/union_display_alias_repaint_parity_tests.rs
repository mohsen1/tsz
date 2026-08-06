//! Parity pins for the display alias a rendered type picks up in a diagnostic.
//!
//! `tsc` decides the displayed name of a type from the **type node that was
//! written at the site**: an annotation spelled `Zed` renders as `Zed`, and an
//! annotation spelled `string | number | symbol` renders structurally, even
//! when a `type Zed = string | number | symbol` declaration elsewhere in the
//! program describes exactly that type. `getUnionType` keys its cache on the
//! member list *plus* the alias identity, so the aliased and the longhand
//! spelling are two distinct `Type` objects and neither can repaint the other.
//!
//! tsz interns one `TypeId` per content and carries the alias in a global
//! `TypeId -> alias` side table (`TypeInterner::store_display_alias` /
//! `get_display_alias`), so an alias declared anywhere can repaint a
//! structurally identical type written longhand somewhere else. That is the
//! divergence the `#[ignore]`d rows below record.
//!
//! Every expectation here was verified against the pinned oracle
//! (`typescript@7.0.2`, `--noEmit --strict --lib es2022 --target es2022`), one
//! file per row so no row can be perturbed by another row's declarations —
//! which matters especially here, since the defect is precisely about one
//! declaration reaching another site.
//!
//! Live rows are a regression floor for the spellings that already match.
//! `#[ignore]`d rows are tripwires: they assert `tsc`'s answer and are expected
//! to fail until the repaint is fixed. Run them with
//! `cargo test -p tsz-checker --test union_display_alias_repaint_parity_tests -- --ignored`.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The rendered source-type name from the single `TS2322` a row produces.
///
/// Every fixture below assigns some value to a `boolean` annotation, so the
/// message is always `Type 'X' is not assignable to type 'boolean'.` and `X` is
/// the display surface under test.
fn rendered_source_type(source: &str) -> String {
    let diagnostics = check_source_with_libs_code_messages(
        source,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    );
    let assignability: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        assignability.len(),
        1,
        "expected exactly one TS2322 for this fixture, got {diagnostics:?}"
    );
    let message = &assignability[0].1;
    let rest = message
        .strip_prefix("Type '")
        .unwrap_or_else(|| panic!("unexpected TS2322 shape: {message}"));
    let end = rest
        .find("' is not assignable")
        .unwrap_or_else(|| panic!("unexpected TS2322 shape: {message}"));
    rest[..end].to_string()
}

// ---------------------------------------------------------------------------
// Live rows: spellings whose display already matches tsc. Regression floor.
// ---------------------------------------------------------------------------

/// An annotation written *through* a user alias renders as that alias.
#[test]
fn union_annotation_written_through_its_alias_renders_the_alias() {
    let source = "type Zed = string | number | symbol;\n\
                  declare const value: Zed;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "Zed");
}

/// Same rule for a lib-declared alias the source actually references.
#[test]
fn union_annotation_written_through_a_lib_alias_renders_the_lib_alias() {
    let source = "declare const value: PropertyKey;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "PropertyKey");
}

/// An object alias declared but not used does not repaint a longhand object
/// literal of the same shape. This is the control that separates the defect
/// family from "aliases repaint everything": objects are already correct.
#[test]
fn object_alias_declared_elsewhere_does_not_repaint_a_longhand_object() {
    let source = "type Obj = { p: number };\n\
                  declare const value: { p: number };\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "{ p: number; }");
}

/// The positive half of the same object pair.
#[test]
fn object_annotation_written_through_its_alias_renders_the_alias() {
    let source = "type Obj = { p: number };\n\
                  declare const value: Obj;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "Obj");
}

/// A union whose members are *object* types is likewise not repainted — the
/// sharpest available discriminator on the defect's boundary. The failing rows
/// below differ from this one only in that their union members are primitives.
#[test]
fn object_member_union_alias_declared_elsewhere_does_not_repaint_the_longhand_union() {
    let source = "type Qux = { p: number } | { q: string };\n\
                  declare const value: { p: number } | { q: string };\n\
                  const target: boolean = value;\n";
    assert_eq!(
        rendered_source_type(source),
        "{ p: number; } | { q: string; }"
    );
}

/// An interface (rather than an alias) never lends its name to a structurally
/// identical anonymous object.
#[test]
fn interface_declared_elsewhere_does_not_repaint_a_longhand_object() {
    let source = "interface Iface { p: number }\n\
                  declare const value: { p: number };\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "{ p: number; }");
}

// ---------------------------------------------------------------------------
// Tripwires: oracle-verified divergences. Expected to fail until fixed.
// ---------------------------------------------------------------------------

/// A longhand primitive union is repainted by a **lib** alias the source never
/// mentions. tsz renders `PropertyKey`; tsc renders the union.
///
/// This is the widest-reach row: every `string | number | symbol` written by
/// any user anywhere renders as `PropertyKey`, because `lib.es5.d.ts` declares
/// that alias and the display-alias table is keyed on the interned `TypeId`.
#[test]
fn longhand_primitive_union_is_not_repainted_by_an_unreferenced_lib_alias() {
    let source = "declare const value: string | number | symbol;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// The same repaint from a **user** alias declared in the same file and never
/// referenced. No lib involvement, so this row rules out "the lib is stamped
/// specially" as the cause.
#[test]
fn longhand_primitive_union_is_not_repainted_by_an_unreferenced_user_alias() {
    let source = "type Zed = string | number | symbol;\n\
                  declare const value: string | number | symbol;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// Renamed binders, and a two-member union, so the row cannot be satisfied by
/// anything keyed on the specific alias name or on the three-member shape that
/// `PropertyKey` happens to have.
#[test]
fn renamed_binders_longhand_two_member_union_is_not_repainted_by_its_alias() {
    let source = "type Pair = string | number;\n\
                  declare const other: string | number;\n\
                  const sink: boolean = other;\n";
    assert_eq!(rendered_source_type(source), "string | number");
}

/// The alias declared *after* the use site repaints it just the same, so the
/// behaviour is not a declaration-order artifact that a source-order rule could
/// explain away.
#[test]
fn an_alias_declared_after_the_use_site_does_not_repaint_the_longhand_union() {
    let source = "declare const value: string | number;\n\
                  const target: boolean = value;\n\
                  type Later = string | number;\n";
    assert_eq!(rendered_source_type(source), "string | number");
}

/// A separate mechanism in the same display family: tsc resolves `keyof any` to
/// `string | number | symbol` eagerly, so the operator never reaches the
/// printer. tsz keeps the `KeyOf` node and renders it verbatim.
///
/// Kept in this file because the two interact — once `keyof any` resolves to
/// the union, it lands on exactly the `TypeId` the rows above show is
/// repainted, so fixing this one alone would render `PropertyKey` here.
#[test]
#[ignore = "known divergence: `keyof any` is not resolved to its member union for display"]
fn keyof_any_renders_as_its_resolved_member_union() {
    let source = "declare const value: keyof any;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

// ---------------------------------------------------------------------------
// Second mechanism: an alias whose RHS COLLAPSES to a pre-existing type.
//
// Opposite polarity to the rows above, and a different owner. There, the alias
// is *not* written at the site and tsz adds it. Here, the alias *is* written at
// the site and tsc drops it: when an alias's RHS reduces to a type that already
// exists, `getIntersectionType`/`getTypeAliasInstantiation` return that existing
// type object, which never carried the alias, so `typeToString` has no alias to
// print. tsz keeps the written spelling.
//
// The distinction matters because the obvious unifying rule — "an alias whose
// RHS interns to the same `TypeId` as the value being rendered" — is refuted by
// `longhand_collapsing_intersection_renders_structurally` below: there the RHS
// does intern to the rendered value's `TypeId` and tsz is already correct.
// ---------------------------------------------------------------------------

/// The control that separates the two mechanisms, and the reason the rules
/// cannot be merged: with the *same* alias in scope, the longhand spelling of a
/// collapsing intersection already renders structurally. So the global
/// display-alias table is not what drives the failing row below — the alias
/// being written at the site is.
#[test]
fn longhand_collapsing_intersection_renders_structurally() {
    let source = "type Coll = string[] & Array<string>;\n\
                  declare const value: string[] & Array<string>;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string[]");
}

/// A non-collapsing intersection keeps its alias on both sides. Bounds the
/// mechanism to the collapse, rather than to intersections generally.
#[test]
fn non_collapsing_intersection_alias_renders_the_alias() {
    let source = "type Both = { p: number } & { q: string };\n\
                  declare const value: Both;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "Both");
}

/// `string[] & Array<string>` are the same type, so the intersection collapses
/// to the pre-existing `string[]` and tsc loses the alias. tsz renders `Coll`.
#[test]
#[ignore = "known divergence: an alias whose RHS collapses to a pre-existing type keeps its name"]
fn collapsing_intersection_alias_renders_the_collapsed_type() {
    let source = "type Coll = string[] & Array<string>;\n\
                  declare const value: Coll;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string[]");
}

/// Same mechanism reached through a homomorphic mapped type over an array: the
/// mapped type reduces to the array itself, so the alias is dropped by tsc.
/// Renamed binders relative to the row above, and no intersection involved.
#[test]
#[ignore = "known divergence: an alias whose RHS collapses to a pre-existing type keeps its name"]
fn mapped_over_array_alias_renders_the_collapsed_array() {
    let source = "type Copy<T> = { [K in keyof T]: T[K] };\n\
                  type Mapped = Copy<1[]>;\n\
                  declare const value: Mapped;\n\
                  const target: number = value;\n";
    assert_eq!(rendered_source_type(source), "1[]");
}

/// And through a conditional with a variadic `infer` tail, whose result is an
/// ordinary tuple that already exists. Three different evaluation routes to one
/// collapse, so a fix cannot be keyed on any single type constructor.
#[test]
#[ignore = "known divergence: an alias whose RHS collapses to a pre-existing type keeps its name"]
fn variadic_infer_tail_alias_renders_the_collapsed_tuple() {
    let source = "type Tail<T> = T extends [infer _H, ...infer R] ? R : never;\n\
                  type Rest = Tail<[1, 2, 3]>;\n\
                  declare const value: Rest;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "[2, 3]");
}
