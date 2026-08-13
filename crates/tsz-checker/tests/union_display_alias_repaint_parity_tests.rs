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
//! structurally identical type written longhand somewhere else — that was
//! `store_display_alias`'s repaint divergence, fixed by #16645 (originally
//! filed as #16610).
//!
//! Every expectation here was verified against the pinned oracle
//! (`typescript@7.0.2`, `--noEmit --strict --lib es2022 --target es2022`), one
//! file per row so no row can be perturbed by another row's declarations —
//! which matters especially here, since the defect is precisely about one
//! declaration reaching another site.
//!
//! All rows above the "second mechanism" section are a regression floor for
//! the repaint fix. `#[ignore]`d rows below record a separate, still-open
//! divergence (an alias whose RHS collapses to a pre-existing type). Run them
//! with `cargo test -p tsz-checker --test union_display_alias_repaint_parity_tests -- --ignored`.

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
// Regression floor for the repaint fix (#16610, #16645).
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
/// printer. `get_type_from_type_operator` now does the same for a bare
/// `any`/`unknown`/`never` operand.
///
/// Kept in this file because the two interact — once `keyof any` resolves to
/// the union, it lands on exactly the `TypeId` the rows above show is (no
/// longer) repainted, so this row only stays green once the repaint rows above
/// are also fixed.
#[test]
fn keyof_any_renders_as_its_resolved_member_union() {
    let source = "declare const value: keyof any;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// Sibling degenerate operand: `keyof never` resolves the same way as
/// `keyof any` (`tsc`'s `getIndexType` treats both eagerly), with an
/// unreferenced primitive-union alias in scope so the row also exercises the
/// repaint guard above.
#[test]
fn keyof_never_renders_as_its_resolved_member_union() {
    let source = "type Zed = string | number | symbol;\n\
                  declare const value: keyof never;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// Renamed binder: a differently-named unused alias in scope must not change
/// the outcome — rules out anything keyed on the specific `Zed`/`PropertyKey`
/// spelling.
#[test]
fn keyof_any_renders_as_its_resolved_member_union_with_renamed_alias_in_scope() {
    let source = "type Zorb = string | number | symbol;\n\
                  declare const value: keyof any;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// A non-generic type alias whose body IS `keyof any`, referenced by name at
/// the use site (`Foo`), should still render structurally — `tsc`'s
/// `getIndexType` resolves the operand before the alias machinery can attach
/// an `aliasSymbol` to the result, so even a genuinely-written-through alias
/// carries none. tsz's separate "written through the alias" display path
/// (for `declare const value: Foo`, `value`'s annotation is the `TYPE_REFERENCE`
/// `Foo`, not `keyof any` itself, so the degenerate-operand guard above does
/// not see it) still prints `Foo`. A real, adjacent divergence surfaced while
/// building the guard above, but a distinct mechanism and out of this fix's
/// scope: fixing it needs the alias-body check to propagate through the
/// written-through-alias display path, not the annotation-node guard.
#[test]
#[ignore = "known divergence: a named alias whose body is `keyof any` keeps the alias name instead of resolving structurally"]
fn keyof_any_type_alias_body_renders_structurally_even_written_through() {
    let source = "type Foo = keyof any;\n\
                  declare const value: Foo;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string | number | symbol");
}

/// Negative control: `keyof <named interface>` is not a degenerate operand —
/// it keeps its own `keyof Name` display (existing, unrelated mechanism),
/// which the new guard must not disturb. Needs a literal-sensitive target
/// (`tsc` widens a `keyof` source to `string` against any other target,
/// independent of this fix).
#[test]
fn keyof_named_interface_still_renders_keyof_name() {
    let source = "interface Widget { a: number; b: string }\n\
                  declare const value: keyof Widget;\n\
                  const target: \"z\" = value;\n";
    assert_eq!(rendered_source_type(source), "keyof Widget");
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
/// to the pre-existing `string[]` and tsc loses the alias. Fixed: the
/// `INTERSECTION_TYPE` body arm of `alias_declaration_body_is_computed` now marks
/// an intersection that collapses to a single array/tuple as a computed body.
#[test]
fn collapsing_intersection_alias_renders_the_collapsed_type() {
    let source = "type Coll = string[] & Array<string>;\n\
                  declare const value: Coll;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "string[]");
}

/// Same mechanism reached through a homomorphic mapped type over an array: the
/// mapped type reduces to the array itself, so the alias is dropped by tsc.
/// Renamed binders relative to the row above, and no intersection involved.
///
/// The display collapse is now fixed (the `TYPE_REFERENCE` application arm drops
/// the `Mapped` name and renders structurally); this row stays `#[ignore]`d for a
/// *separate*, still-open divergence: tsz widens the element of the homomorphic
/// mapped result `Copy<1[]>` from the literal `1` to `number`, so it renders
/// `number[]` where tsc keeps `1[]`. That is a literal-widening bug in the mapped
/// evaluator, not a display-alias attribution one.
#[test]
#[ignore = "separate open divergence: homomorphic mapped over `1[]` widens the element to `number` (renders `number[]`, not `1[]`); the alias-name collapse itself is fixed"]
fn mapped_over_array_alias_renders_the_collapsed_array() {
    let source = "type Copy<T> = { [K in keyof T]: T[K] };\n\
                  type Mapped = Copy<1[]>;\n\
                  declare const value: Mapped;\n\
                  const target: number = value;\n";
    assert_eq!(rendered_source_type(source), "1[]");
}

/// And through a conditional with a variadic `infer` tail, whose result is an
/// ordinary tuple that already exists. Three different evaluation routes to one
/// collapse, so a fix cannot be keyed on any single type constructor. Fixed by
/// the `TYPE_REFERENCE` application arm: the alias body `Tail<[1, 2, 3]>` is a
/// bare generic application whose evaluated result is the pre-existing tuple
/// `[2, 3]`, so the `Rest` name is dropped.
#[test]
fn variadic_infer_tail_alias_renders_the_collapsed_tuple() {
    let source = "type Tail<T> = T extends [infer _H, ...infer R] ? R : never;\n\
                  type Rest = Tail<[1, 2, 3]>;\n\
                  declare const value: Rest;\n\
                  const target: boolean = value;\n";
    assert_eq!(rendered_source_type(source), "[2, 3]");
}

// ---------------------------------------------------------------------------
// Adjacent cases: the collapse rule is structural, not name-keyed. Renamed
// binders and different element types must not change the outcome — the fix
// reads no identifier, alias, or type-parameter string.
// ---------------------------------------------------------------------------

/// Renamed binders and a different tuple for the variadic-`infer`-tail row: a
/// bare generic application collapsing to a pre-existing tuple drops its name
/// regardless of the `Tail`/`Rest` spelling or the concrete elements.
#[test]
fn renamed_binders_variadic_infer_tail_renders_the_collapsed_tuple() {
    let source = "type DropHead<L> = L extends [infer _First, ...infer Remainder] ? Remainder : never;\n\
                  type Leftover = DropHead<[9, 8, 7]>;\n\
                  declare const held: Leftover;\n\
                  const dest: boolean = held;\n";
    assert_eq!(rendered_source_type(source), "[8, 7]");
}
