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

// ---------------------------------------------------------------------------
// The TARGET mirror of the repaint rows above.
//
// The source side was fixed by #16610/#16645 via an annotation-node gate
// (`longhand_primitive_union_source_display`); the target side kept no such
// gate, so `let a: string | number = someBoolean` rendered the *target* as an
// unrelated alias's name whenever one of that shape was declared anywhere in
// the program. Same defect, same owner, opposite position.
//
// Every expectation below was verified against the pinned oracle
// (`typescript@7.0.2` via `scripts/conformance/oracle.sh`, `--strict`).
// ---------------------------------------------------------------------------

/// The rendered target-type name from the single `TS2322` a row produces.
///
/// Every fixture below assigns a `boolean` to the annotation under test, so the
/// message is always `Type 'boolean' is not assignable to type 'X'.` and `X` is
/// the display surface.
fn rendered_target_type(source: &str) -> String {
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
        .strip_prefix("Type 'boolean' is not assignable to type '")
        .unwrap_or_else(|| panic!("unexpected TS2322 shape: {message}"));
    let end = rest
        .rfind("'.")
        .unwrap_or_else(|| panic!("unexpected TS2322 shape: {message}"));
    rest[..end].to_string()
}

/// A longhand primitive union **target** is not repainted by a user alias of
/// the same shape declared in the same file and never referenced.
#[test]
fn longhand_primitive_union_target_is_not_repainted_by_an_unreferenced_user_alias() {
    let source = "type Zed = string | number;\n\
                  declare const flag: boolean;\n\
                  const dest: string | number = flag;\n";
    assert_eq!(rendered_target_type(source), "string | number");
}

/// The positive control: an annotation written *through* the alias keeps the
/// alias name on the target side too, so the gate suppresses repaints without
/// erasing genuine alias spellings.
#[test]
fn union_target_written_through_its_alias_renders_the_alias() {
    let source = "type Zed = string | number;\n\
                  declare const flag: boolean;\n\
                  const dest: Zed = flag;\n";
    assert_eq!(rendered_target_type(source), "Zed");
}

/// Renamed binders and a three-member union, so no row can be satisfied by
/// anything keyed on the `Zed` spelling or the two-member shape.
#[test]
fn renamed_binders_longhand_three_member_union_target_is_not_repainted() {
    let source = "type Trio = string | number | symbol;\n\
                  declare const held: boolean;\n\
                  const sink: string | number | symbol = held;\n";
    assert_eq!(rendered_target_type(source), "string | number | symbol");
}

/// The widest-reach target row: repainted by the **lib** alias `PropertyKey`,
/// which the annotation never mentions.
#[test]
fn longhand_primitive_union_target_is_not_repainted_by_an_unreferenced_lib_alias() {
    let source = "declare const flag: boolean;\n\
                  const dest: string | number | symbol = flag;\n";
    assert_eq!(rendered_target_type(source), "string | number | symbol");
}

/// Positive control for the lib alias: written through, it keeps its name.
#[test]
fn union_target_written_through_a_lib_alias_renders_the_lib_alias() {
    let source = "declare const flag: boolean;\n\
                  const dest: PropertyKey = flag;\n";
    assert_eq!(rendered_target_type(source), "PropertyKey");
}

/// The alias declared *after* the use site repaints just the same on main, so
/// the outcome is not a declaration-order artifact.
#[test]
fn an_alias_declared_after_the_use_site_does_not_repaint_the_longhand_union_target() {
    let source = "declare const flag: boolean;\n\
                  const dest: string | number = flag;\n\
                  type Later = string | number;\n";
    assert_eq!(rendered_target_type(source), "string | number");
}

/// Two distinct aliases of the *same* interned union: each written spelling
/// keeps its own name rather than collapsing to whichever registered first.
///
/// `register_type_to_def` is first-writer-wins on the interned `TypeId`, so a
/// global reverse lookup answers `First` for both spellings. Fixed by the
/// per-occurrence gate `written_alias_reference_target_display`, which resolves
/// the annotation's own written reference to its alias definition and requires
/// the alias body to be identity-equal to the displayed target.
#[test]
fn two_aliases_of_one_union_each_keep_their_own_written_target_spelling() {
    let source = "type First = string | number;\n\
                  type Second = string | number;\n\
                  declare const flag: boolean;\n\
                  const a: Second = flag;\n";
    assert_eq!(rendered_target_type(source), "Second");
}

/// The first-declared spelling of the same pair keeps *its* name too — the fix
/// renders the written reference, not "the other alias".
#[test]
fn two_aliases_of_one_union_first_written_target_keeps_the_first_alias() {
    let source = "type First = string | number;\n\
                  type Second = string | number;\n\
                  declare const flag: boolean;\n\
                  const a: First = flag;\n";
    assert_eq!(rendered_target_type(source), "First");
}

/// Renamed binders and a three-member union, so no row of this family can be
/// satisfied by anything keyed on the `First`/`Second` spelling or the
/// two-member shape.
#[test]
fn renamed_binder_alias_pair_three_member_union_target_keeps_the_written_alias() {
    let source = "type AlphaKeys = string | number | symbol;\n\
                  type BetaKeys = string | number | symbol;\n\
                  declare const held: boolean;\n\
                  const sink: AlphaKeys = held;\n";
    assert_eq!(rendered_target_type(source), "AlphaKeys");
}

/// The written alias declared *after* the use site (and after an unreferenced
/// twin) still names the diagnostic, ruling out a declaration-order artifact.
#[test]
fn written_alias_declared_after_the_use_site_still_names_the_target() {
    let source = "declare const flag: boolean;\n\
                  const a: Late = flag;\n\
                  type Early = string | number;\n\
                  type Late = string | number;\n";
    assert_eq!(rendered_target_type(source), "Late");
}

/// The same collapse for **object**-bodied alias pairs — the defect is alias
/// reference identity, not a union-shaped special case.
#[test]
fn two_object_aliases_of_one_shape_keep_their_own_written_target_spelling() {
    let source = "type ObjA = { p: number };\n\
                  type ObjB = { p: number };\n\
                  declare const flag: boolean;\n\
                  const o: ObjB = flag;\n";
    assert_eq!(rendered_target_type(source), "ObjB");
}

/// Negative control: two aliases of the same *computed* body (a reduced
/// conditional) both render the underlying type — tsc attaches no
/// `aliasSymbol` to a reducing operator's shared result, so the
/// per-occurrence gate must decline via `type_alias_displayed_as_underlying`.
#[test]
fn two_computed_body_aliases_still_render_the_underlying_type() {
    let source = "type CondA = true extends true ? string : number;\n\
                  type CondB = true extends true ? string : number;\n\
                  declare const flag: boolean;\n\
                  const c: CondB = flag;\n";
    assert_eq!(rendered_target_type(source), "string");
}

/// Negative control on the forwarding chain: `type Outer = Inner` written at
/// the use site renders `Inner` — tsc resolves the bare alias-to-alias
/// reference through the chain and stamps the inner alias (oracle-pinned on
/// 7.0.2), so the per-occurrence gate declines a forwarding body and leaves
/// the chain-following display path in charge.
///
/// Red both with and without the per-occurrence gate: tsz renders `Outer`
/// through the established display path (the gate correctly declines, so it
/// neither causes nor can fix this). Recorded rather than fixed — the
/// chain-resolution display is the alias-underlying family's own owner
/// (`type_alias_displayed_as_underlying` currently keeps a forwarding alias's
/// declared name where tsc re-stamps the inner alias).
#[test]
#[ignore = "separate open divergence: a bare alias-to-alias forwarding target renders the outer alias name; tsc resolves the chain and shows the inner alias"]
fn bare_alias_to_alias_forwarding_target_renders_the_inner_alias() {
    let source = "type Inner = string | number;\n\
                  type Outer = Inner;\n\
                  declare const flag: boolean;\n\
                  const c: Outer = flag;\n";
    assert_eq!(rendered_target_type(source), "Inner");
}

/// Negative control on application-bodied aliases, half one: a plain
/// generic-application body keeps the written alias (`BoxNum`), which the
/// established application display path already produced — the
/// per-occurrence gate declines application bodies and must not disturb it.
#[test]
fn plain_application_bodied_alias_target_keeps_the_written_alias() {
    let source = "type Box<T> = { v: T };\n\
                  type BoxNum = Box<number>;\n\
                  declare const flag: boolean;\n\
                  const b: BoxNum = flag;\n";
    assert_eq!(rendered_target_type(source), "BoxNum");
}

/// Negative control on application-bodied aliases, half two: a *recursive
/// mapped* application body renders the substituted application, not the
/// written alias — tsc splits the application-bodied family this way
/// (oracle-pinned; the deep form is conformance `deeplyNestedMappedTypes.ts`,
/// which regressed when the per-occurrence gate repainted it and is why the
/// gate declines every application body).
#[test]
fn recursive_mapped_application_alias_target_renders_the_application() {
    let source = "type Id<T> = { [K in keyof T]: Id<T[K]> };\n\
                  type FooA = Id<{ x: { c: number } }>;\n\
                  type FooB = Id<{ x: { c: string } }>;\n\
                  declare const fa: FooA;\n\
                  const fb: FooB = fa;\n";
    // The source here is not `boolean`, so this row asserts the whole head
    // line rather than going through `rendered_target_type`.
    let diagnostics = check_source_with_libs_code_messages(
        source,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    );
    let heads: Vec<&String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect();
    assert_eq!(
        heads.len(),
        1,
        "expected exactly one TS2322: {diagnostics:?}"
    );
    assert!(
        heads[0].starts_with(
            "Type 'Id<{ x: { c: number; }; }>' is not assignable to type 'Id<{ x: { c: string; }; }>'."
        ),
        "unexpected head: {}",
        heads[0]
    );
}

/// The source-side dual of the two-alias pair: the declared annotation of the
/// *source* identifier keeps its own written spelling too.
#[test]
fn two_aliases_of_one_union_source_keeps_its_own_written_spelling() {
    let source = "type First = string | number;\n\
                  type Second = string | number;\n\
                  declare const s: Second;\n\
                  const t: boolean = s;\n";
    assert_eq!(rendered_source_type(source), "Second");
}

/// Baseline with no alias anywhere: the structural render must be unchanged, so
/// the gate is not what produces the member spelling in the ordinary case.
#[test]
fn longhand_primitive_union_target_with_no_alias_in_scope_renders_structurally() {
    let source = "declare const flag: boolean;\n\
                  const dest: string | number = flag;\n";
    assert_eq!(rendered_target_type(source), "string | number");
}

/// Written in the reverse order, the target still renders in tsc's canonical
/// member order — the gate renders the *type*, not the annotation's text, so it
/// cannot resurrect written order (which #17715 established tsc ignores).
#[test]
fn reverse_written_longhand_union_target_renders_in_canonical_member_order() {
    let source = "type Zed = string | number;\n\
                  declare const flag: boolean;\n\
                  const dest: number | string = flag;\n";
    assert_eq!(rendered_target_type(source), "string | number");
}

/// Negative control on the nullish-collapse boundary: a longhand
/// `string | undefined` target against a non-nullish source still collapses to
/// `string` (#17714's single-survivor rule). The gate is ordered after the
/// nullish strip precisely so it cannot override that.
#[test]
fn longhand_nullable_union_target_still_collapses_to_its_single_survivor() {
    let source = "type Maybe = string | undefined;\n\
                  declare const flag: boolean;\n\
                  const dest: string | undefined = flag;\n";
    assert_eq!(rendered_target_type(source), "string");
}

/// The alias-written half of the same nullish pair should keep the alias name
/// (`Maybe`), as tsc does — its collapse lives in the structural elaboration
/// path, which an annotation carrying an `aliasSymbol` never enters.
///
/// Red both with and without the target gate above: tsz renders `string`
/// because `strip_nullish_for_assignability_display` runs unconditionally,
/// before any annotation is consulted. Recorded here rather than fixed, since
/// making the strip alias-aware is the nullable-display family's own owner
/// (#17714 / `nullable_union_assignability_target_display_tests.rs`), not this
/// repaint gate's.
#[test]
#[ignore = "separate open divergence: the nullish strip runs before the annotation is consulted, so an alias-written `string | undefined` target collapses to `string` instead of keeping its name"]
fn nullable_union_target_written_through_its_alias_renders_the_alias() {
    let source = "type Maybe = string | undefined;\n\
                  declare const flag: boolean;\n\
                  const dest: Maybe = flag;\n";
    assert_eq!(rendered_target_type(source), "Maybe");
}

/// Object-member union targets are outside the gate's admitted shape (it takes
/// primitive keywords only) and were already correct; this row pins that the
/// established anonymous-composite path still owns them.
#[test]
fn object_member_union_target_still_renders_structurally() {
    let source = "type Qux = { p: number } | { q: string };\n\
                  declare const flag: boolean;\n\
                  const dest: { p: number } | { q: string } = flag;\n";
    assert_eq!(
        rendered_target_type(source),
        "{ p: number; } | { q: string; }"
    );
}
