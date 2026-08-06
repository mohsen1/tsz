//! A qualified type name whose root has no *namespace* meaning.
//!
//! `tsc`'s `SymbolFlags.Namespace` is `ValueModule | NamespaceModule | Enum`.
//! A **class** is not in it: a class declaration alone cannot serve as the
//! left-hand side of a qualified type name, and `resolveEntityName` never asks
//! it for a member. So `C.m` in type position is one of two errors, chosen by
//! whether the type `C` *denotes in type space* — its instance side — carries
//! the property:
//!
//! - it does → `TS2713` "Cannot access 'C.m' because 'C' is a type, but not a
//!   namespace. Did you mean to retrieve the type of the property 'm' in 'C'
//!   with `C["m"]`?"
//! - it does not → `TS2702` "'C' only refers to a type, but is being used as a
//!   namespace here."
//!
//! tsz treated a class as namespace-like, which sent the whole family down the
//! member-lookup path (`TS2694` "Namespace 'C' has no exported member 'm'.") or,
//! once a member did resolve, the value-in-type-position path (`TS2749`). A
//! class *merged* with a namespace declaration keeps working through the merged
//! symbol's module flags, so the merge rows below are unchanged.
//!
//! Every row is measured against `typescript@7.0.2` with
//! `--noEmit --strict --target es2015`; the residual rows at the bottom are
//! pinned as they behave today rather than left to drift.

use crate::test_utils::{
    check_multi_file, check_multi_file_with_global_index, check_source_diagnostics,
};
use tsz_common::CheckerOptions;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.message_text)
        .collect()
}

fn multi_file_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file(files, entry, options)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// Like `multi_file_codes`, but wires the production `global_symbol_file_index`
/// before checking, matching the real CLI driver instead of the plain
/// in-memory `check_multi_file` pipeline's dynamic overlay. #16465/#16486's
/// own test suite (`qualified_name_default_import_namespace_meaning_tests.rs`,
/// #16479) established this as the harness for a default-import qualifier's
/// namespace-meaning gate — the no-lib `check_multi_file` overlay does not
/// reliably surface the downstream missing-member `TS2694` in this shape, so
/// the load-bearing assertion for the regression is "does not report
/// `TS2702`", not "reports `TS2694`" (see the comment on
/// `export_default_namespace_keeps_namespace_meaning` in that suite).
fn multi_file_codes_global_index(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    multi_file_diags_global_index(files, entry)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// Like `multi_file_codes_global_index`, but keeps `(code, message)` so the
/// namespace name in a `TS2694` can be asserted — the load-bearing detail for
/// #16503, where the missing-member message must name the *target* namespace,
/// not a colliding local read from the checking file's binder.
fn multi_file_diags_global_index(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_global_index(files, entry, options)
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
        .collect()
}

// ---------------------------------------------------------------------------
// A class has no namespace meaning: the missing-member rows
// ---------------------------------------------------------------------------

/// `class Local {}` + `Local.X`: no such property on the instance side, so the
/// qualifier itself is the error. tsz reported `TS2694` here, treating the class
/// as a namespace with an export list.
#[test]
fn class_qualifier_with_no_such_member_reports_type_used_as_namespace() {
    assert_eq!(codes("class Local {}\nvar a: Local.X;\n"), vec![2702]);
}

/// Anti-hardcoding: the rule keys on the qualifier symbol's flags, not on any
/// identifier. Renamed binders behave identically.
#[test]
fn class_qualifier_rule_is_binder_name_independent() {
    let rendered = messages("class Widget {}\nvar registry: Widget.Registry;\n");
    assert_eq!(rendered.len(), 1, "exactly one diagnostic: {rendered:?}");
    assert_eq!(
        rendered[0],
        "'Widget' only refers to a type, but is being used as a namespace here."
    );
}

/// A *static*-only member is still not on the instance side, so `tsc` reports
/// the plain qualifier error rather than the indexed-access suggestion.
#[test]
fn class_qualifier_with_static_only_member_reports_type_used_as_namespace() {
    assert_eq!(
        codes("class C2 { static S: number = 0 }\nvar a: C2.S;\n"),
        vec![2702]
    );
}

/// A type-*query* through the static side is legal and must stay clean —
/// `typeof C.S` is not a qualified type name and never reaches this rule.
#[test]
fn static_member_type_query_stays_clean() {
    assert_eq!(
        codes("class C3 { static S: number = 0 }\nlet a: typeof C3.S;\n"),
        Vec::<u32>::new()
    );
}

// ---------------------------------------------------------------------------
// A class has no namespace meaning: the TS2713 indexed-access rows
// ---------------------------------------------------------------------------

/// The instance side *does* carry `m`, so `tsc` suggests the indexed access.
/// tsz reported `TS2749` ("refers to a value, but is being used as a type").
#[test]
fn class_qualifier_with_instance_property_suggests_indexed_access() {
    let rendered = messages("class C { m: number = 0 }\nvar a: C.m;\n");
    assert_eq!(rendered.len(), 1, "exactly one diagnostic: {rendered:?}");
    assert_eq!(
        rendered[0],
        "Cannot access 'C.m' because 'C' is a type, but not a namespace. \
         Did you mean to retrieve the type of the property 'm' in 'C' with 'C[\"m\"]'?"
    );
}

/// An ambient class declaration takes the same path as a concrete one.
#[test]
fn ambient_class_qualifier_with_instance_property_suggests_indexed_access() {
    assert_eq!(
        codes("declare class Amb { p: number }\nvar a: Amb.p;\n"),
        vec![2713]
    );
}

/// A *generic* class resolves its instance side for the probe without needing
/// type arguments — `tsc` reports `TS2713` here and no missing-type-argument
/// error, so the probe must not synthesize an arity diagnostic either.
#[test]
fn generic_class_qualifier_with_instance_property_suggests_indexed_access() {
    assert_eq!(
        codes("class G<T> { g: T | undefined }\nvar a: G.g;\n"),
        vec![2713]
    );
}

// ---------------------------------------------------------------------------
// Qualifiers that DO have namespace meaning stay on the member-lookup path
// ---------------------------------------------------------------------------

/// A class merged with a namespace declaration keeps namespace meaning through
/// the merged symbol's module flags: the exported member resolves.
#[test]
fn class_merged_with_namespace_resolves_its_exported_member() {
    assert_eq!(
        codes("class M {}\nnamespace M { export interface I { k: number } }\nvar a: M.I;\n"),
        Vec::<u32>::new()
    );
}

/// ...and a genuinely absent export on that merge is still `TS2694`, not the
/// qualifier error — this is the row that would break if the fix keyed on the
/// class-ness of the declaration instead of the merged symbol's flags.
#[test]
fn class_merged_with_namespace_keeps_ts2694_for_a_missing_export() {
    assert_eq!(
        codes("class M2 {}\nnamespace M2 { export interface I { k: number } }\nvar a: M2.Nope;\n"),
        vec![2694]
    );
}

/// Enums, enum members and namespaces are unaffected: they are in `tsc`'s
/// `SymbolFlags.Namespace` and keep the member-lookup path.
#[test]
fn enum_and_namespace_qualifiers_keep_the_member_lookup_path() {
    assert_eq!(codes("enum E0 { A }\nvar a: E0.A;\n"), Vec::<u32>::new());
    assert_eq!(codes("enum E { A }\nvar b: E.Missing;\n"), vec![2694]);
    assert_eq!(
        codes("namespace NS { export interface I { k: number } }\nvar c: NS.Missing;\n"),
        vec![2694]
    );
}

/// Interfaces and type aliases were already on the correct path in both
/// directions; they are pinned here because the fix re-points the property
/// probe at the type-reference type, and an interface's type-reference type is
/// a semantic ref that carries no member surface until it is evaluated.
#[test]
fn interface_and_alias_qualifiers_are_unchanged_in_both_directions() {
    assert_eq!(
        codes("interface I2 { k: number }\nvar a: I2.k;\n"),
        vec![2713]
    );
    assert_eq!(
        codes("type A2 = { k: number };\nvar a: A2.k;\n"),
        vec![2713]
    );
    assert_eq!(
        codes("interface Iface { k: number }\nvar d: Iface.Missing;\n"),
        vec![2702]
    );
    assert_eq!(
        codes("type Alias = { k: number };\nvar e: Alias.Missing;\n"),
        vec![2702]
    );
    assert_eq!(
        codes("type Fn = (x: number) => void;\nvar a: Fn.x;\n"),
        vec![2702]
    );
}

// ---------------------------------------------------------------------------
// Cross-file: a named-imported class takes the same two branches
// ---------------------------------------------------------------------------

/// An imported class is still a class: `Plain.X` is the qualifier error and
/// `Plain.m` the indexed-access suggestion. The second row was `TS2702` before,
/// because the probe read the class's static side and found no `m` there.
#[test]
fn named_imported_class_qualifier_takes_both_branches() {
    assert_eq!(
        multi_file_codes(
            &[
                ("/q1.ts", "export class Plain { m: number = 0 }\n"),
                (
                    "/q2.ts",
                    "import { Plain } from \"./q1\";\nvar a: Plain.X;\nvar b: Plain.m;\n"
                ),
            ],
            "/q2.ts",
        ),
        vec![2702, 2713]
    );
}

// ---------------------------------------------------------------------------
// Cross-file: an import alias resolves to its target's namespace meaning,
// not the local binding's ALIAS flag (closes #16465)
// ---------------------------------------------------------------------------

/// A *default*-imported class used as a namespace qualifier. `export default`
/// carries only the class's type/value meaning — never a merged namespace's —
/// so `tsc` reports `TS2702` on the qualifier for every row here.
#[test]
fn default_imported_class_used_as_namespace_reports_type_used_as_namespace() {
    for (files, entry) in [
        (
            vec![
                ("/t1.ts", "export default class D {}\n"),
                ("/t2.ts", "import D from \"./t1\";\nvar a: D.X;\n"),
            ],
            "/t2.ts",
        ),
        (
            vec![
                (
                    "/m1.ts",
                    "export default class Decl {}\nexport namespace Decl { export interface I { q: number } }\n",
                ),
                ("/m2.ts", "import Entity from \"./m1\";\nvar y: Entity.I;\n"),
            ],
            "/m2.ts",
        ),
        (
            vec![
                (
                    "/u1.ts",
                    "export default interface OnlyType { k: number }\n",
                ),
                (
                    "/u2.ts",
                    "import OnlyType from \"./u1\";\nvar a: OnlyType.X;\n",
                ),
            ],
            "/u2.ts",
        ),
    ] {
        assert_eq!(
            multi_file_codes(&files, entry),
            vec![2702],
            "tsc reports TS2702 for {entry}"
        );
    }
}

/// A named-imported class *merged* with a namespace keeps its namespace
/// meaning across the file boundary: `tsc` accepts `Named.I` and reports
/// `TS2694` for `Named.Missing`.
#[test]
fn cross_file_class_namespace_merge_keeps_the_member_lookup_path() {
    assert_eq!(
        multi_file_codes(
            &[
                (
                    "/n1.ts",
                    "export class Named {}\nexport namespace Named { export interface I { q: number } }\n"
                ),
                (
                    "/n2.ts",
                    "import { Named } from \"./n1\";\nvar y: Named.I;\nvar z: Named.Missing;\n"
                ),
            ],
            "/n2.ts",
        ),
        vec![2694],
        "tsc accepts `Named.I` and reports TS2694 for `Named.Missing`"
    );
}

/// A namespace import is a module namespace and keeps the member-lookup path.
#[test]
fn namespace_import_qualifier_keeps_the_member_lookup_path() {
    assert_eq!(
        multi_file_codes(
            &[
                ("/p1.ts", "export default class D {}\n"),
                ("/p2.ts", "import * as NsD from \"./p1\";\nvar b: NsD.X;\n"),
            ],
            "/p2.ts",
        ),
        vec![2694]
    );
}

// ---------------------------------------------------------------------------
// `export default <identifier>` re-exporting a namespace/enum keeps namespace
// meaning through the default import (closes #16486, a regression from
// #16480: that PR's alias-target check stopped at the synthesized `default`
// wrapper symbol's own flags, which are always bare `ALIAS` regardless of
// what the identifier denotes). #16503 completes it: the member-lookup path
// now hops through the `default` slot too, so a present member resolves and a
// missing one is `TS2694` — the strict assertions live in the #16503 section
// below (`default_exported_namespace_member_resolves_and_missing_is_ts2694`
// and its enum / renamed twins), which supersede #16486's weaker
// "must not be TS2702" checks.
// ---------------------------------------------------------------------------

/// Negative control, restated from `default_imported_class_used_as_namespace_reports_type_used_as_namespace`:
/// a default-exported class *merged* with a same-named namespace still
/// reports `TS2702` through the `default` slot specifically — tsc only folds
/// the merge into the *named* binding (`Decl`), never the synthetic `default`
/// wrapper. This must NOT regress when the identifier-hop above is added: the
/// hop only fires when `default`'s own `value_declaration` is a bare
/// `Identifier`, and `export default class Decl {}` makes it the class
/// declaration node itself.
#[test]
fn default_exported_class_merged_with_namespace_still_reports_ts2702() {
    assert_eq!(
        multi_file_codes(
            &[
                (
                    "/dep3.ts",
                    "export default class Decl {}\nexport namespace Decl { export interface I { q: number } }\n",
                ),
                (
                    "/main3.ts",
                    "import Entity from \"./dep3\";\nvar y: Entity.I;\n"
                ),
            ],
            "/main3.ts",
        ),
        vec![2702]
    );
}

/// The bare-identifier-reference twin of the control above (#16501): a class
/// merged with a same-named namespace, default-exported by referencing the
/// already-declared, already-merged name rather than declaring the class
/// inline. The identifier-hop above *does* fire here (`export default Decl2;`
/// makes `default`'s `value_declaration` a bare `Identifier`), so without the
/// `Class`-bit override it would wrongly resolve `Entity3.I` as a member
/// lookup — oracle-verified (`typescript@7.0.2`, three ways: `oracle.sh`, a
/// bare `tsc` invocation, and with `export` dropped from the namespace) that
/// `tsc` reports `TS2702` here exactly as it does for the inline form.
#[test]
fn default_exported_bare_ref_class_merged_with_namespace_still_reports_ts2702() {
    assert_eq!(
        multi_file_codes(
            &[
                (
                    "/dep4.ts",
                    "class Decl2 {}\nexport namespace Decl2 { export interface I { q: number } }\nexport default Decl2;\n",
                ),
                (
                    "/main4.ts",
                    "import Entity3 from \"./dep4\";\nvar y: Entity3.I;\n"
                ),
            ],
            "/main4.ts",
        ),
        vec![2702]
    );
}

// ---------------------------------------------------------------------------
// #16503: a default-exported namespace/enum's real members are reachable
// through the default import. #16486 only stopped the false `TS2702` at the
// diagnostic gate; the member-lookup path still routed the synthetic `default`
// alias, so a present member resolved only by error-masking coincidence and a
// missing member emitted *nothing* (namespace) or `TS2503` (enum) instead of
// tsc's `TS2694`. The anchor now hops to the real namespace/enum symbol, read
// from its declaring binder, so both a present member and a missing one behave
// like tsc.
//
// All rows oracled against the pinned `tsc` (`--noEmit --strict --pretty false
// --target es2022 --lib es2022`). The `TS2694` names the *target* namespace
// (`m`/`Color`), not the local import alias — tsc's `getFullyQualifiedName` of
// the resolved namespace symbol, confirmed against the oracle:
//   main2.ts(2,10): error TS2694: Namespace 'm2' has no exported member 'Missing'.
//   maine.ts(2,12): error TS2694: Namespace 'Color' has no exported member 'Nope'.
// ---------------------------------------------------------------------------

/// A present member of a default-exported namespace resolves cleanly, and a
/// missing one is `TS2694` naming the target namespace — not silence, and not
/// `TS2702`.
#[test]
fn default_exported_namespace_member_resolves_and_missing_is_ts2694() {
    const DEP: &str = "namespace m { export interface foo { a: number } }\nexport default m;\n";

    let present = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("main.ts", "import D from './dep';\nvar q: D.foo;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        present,
        Vec::<(u32, String)>::new(),
        "a present member resolves with no diagnostic"
    );

    let missing = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("main.ts", "import D from './dep';\nvar bad: D.Missing;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'm' has no exported member 'Missing'.".to_string()
        )],
        "a missing member is TS2694 naming the target namespace"
    );
}

/// The enum twin: `enum Color` carries `SymbolFlags.Namespace` (`Enum` is in
/// the set), so `C.Red` resolves and `C.Nope` is `TS2694` naming `Color`.
/// Before the fix the enum default import produced `TS2503` for both.
#[test]
fn default_exported_enum_member_resolves_and_missing_is_ts2694() {
    const DEP: &str = "enum Color { Red, Green }\nexport default Color;\n";

    let present = multi_file_diags_global_index(
        &[
            ("e.ts", DEP),
            ("main.ts", "import C from './e';\nvar ok: C.Red;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        present,
        Vec::<(u32, String)>::new(),
        "a present enum member resolves with no diagnostic"
    );

    let missing = multi_file_diags_global_index(
        &[
            ("e.ts", DEP),
            ("main.ts", "import C from './e';\nvar bad: C.Nope;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Color' has no exported member 'Nope'.".to_string()
        )],
        "a missing enum member is TS2694 naming the enum"
    );
}

/// Anti-hardcoding: the hop keys on the `default` slot's target flags, not on
/// any spelling. Renaming both the namespace binder and the local import
/// binding leaves the behaviour — and the target-relative message name —
/// unchanged.
#[test]
fn default_exported_namespace_member_rule_is_binder_name_independent() {
    let missing = multi_file_diags_global_index(
        &[
            (
                "dep.ts",
                "namespace Payload { export interface foo { a: number } }\nexport default Payload;\n",
            ),
            (
                "main.ts",
                "import Renamed from './dep';\nvar bad: Renamed.Missing;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Payload' has no exported member 'Missing'.".to_string()
        )],
        "the message names the renamed target namespace, not the local alias"
    );
}

/// The same raw-`SymbolId`-collision fix restores the correct namespace name
/// for a *named* cross-file import too: before it, the missing-member `TS2694`
/// read the anchor from the checking file's binder and named a nearby local
/// (`var z`). This is the pre-existing collision #16503's member-lookup read
/// also fixed, pinned so it cannot silently regress.
#[test]
fn named_cross_file_namespace_missing_member_names_the_target_namespace() {
    let missing = multi_file_diags_global_index(
        &[
            (
                "n1.ts",
                "export namespace Named { export interface I { q: number } }\n",
            ),
            (
                "n2.ts",
                "import { Named } from './n1';\nvar z: Named.Missing;\n",
            ),
        ],
        "n2.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Named' has no exported member 'Missing'.".to_string()
        )],
    );
}

/// Negative control for the hop's class exclusion, restated at message level:
/// a default-exported *class* has no namespace meaning, so `D.X` stays
/// `TS2702` and never becomes a member lookup. (Codes-level twins live in
/// `default_imported_class_used_as_namespace_reports_type_used_as_namespace`.)
#[test]
fn default_exported_class_member_access_stays_ts2702_not_ts2694() {
    let codes = multi_file_codes_global_index(
        &[
            ("t1.ts", "export default class D { m: number = 0 }\n"),
            ("t2.ts", "import D from './t1';\nvar a: D.X;\n"),
        ],
        "t2.ts",
    );
    assert_eq!(
        codes,
        vec![2702],
        "a default-exported class stays TS2702, never TS2694"
    );
}

// ---------------------------------------------------------------------------
// #16503 (re-export-hub residual): a namespace/enum member reached *through a
// re-export hub* (`export { default } from './dep'` or `export { Foo } from
// './dep'`, chained) now resolves like `tsc`. The qualified-name type anchor
// follows the re-export chain to the terminal declaration — a hub specifier is
// itself an `ALIAS` with an `import_module`, so alias resolution used to stop
// on it (it owns no member surface) and the qualifier fell to the `TS2702`
// "used as a namespace" gate. Following the chain restores the member-lookup
// path: a present member resolves and a missing one is `TS2694`.
//
// Oracled against `tsc` 6.0.2 (`--noEmit --strict --pretty false --target
// es2022 --lib es2022`); `tsc` follows both the default and the named hub:
//   main.ts(2,12): error TS2694: Namespace 'm' has no exported member 'Missing'.
//   emain.ts(2,12): error TS2694: Namespace 'Color' has no exported member 'Nope'.
// The message names the *target* namespace (bare, as everywhere else in this
// suite — tsz does not reproduce `tsc`'s `getFullyQualifiedName` module prefix,
// a separately-tracked display divergence, not this fix's concern).
// ---------------------------------------------------------------------------

/// A `export { default } from './dep'` hub over `export default <namespace>`:
/// the present member resolves and the missing one is `TS2694` naming the
/// target namespace — no longer the `TS2702` qualifier error.
#[test]
fn reexport_default_hub_namespace_member_resolves_and_missing_is_ts2694() {
    const DEP: &str = "namespace m { export interface foo { a: number } }\nexport default m;\n";
    const HUB: &str = "export { default } from './dep';\n";

    let present = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("hub.ts", HUB),
            ("main.ts", "import D from './hub';\nvar ok: D.foo;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        present,
        Vec::<(u32, String)>::new(),
        "a present member resolves through the default hub"
    );

    let missing = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("hub.ts", HUB),
            ("main.ts", "import D from './hub';\nvar bad: D.Missing;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'm' has no exported member 'Missing'.".to_string()
        )],
        "a missing member through the default hub is TS2694 naming the target namespace"
    );
}

/// The named-hub twin (`export { Foo } from './dep'`): `tsc` follows it the same
/// way, proving the fix is a general re-export-chain resolution, not a
/// default-export special case.
#[test]
fn reexport_named_hub_namespace_member_resolves_and_missing_is_ts2694() {
    const DEP: &str = "export namespace Foo { export interface bar { a: number } }\n";
    const HUB: &str = "export { Foo } from './dep';\n";

    let present = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("hub.ts", HUB),
            (
                "main.ts",
                "import { Foo } from './hub';\nvar ok: Foo.bar;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        present,
        Vec::<(u32, String)>::new(),
        "a present member resolves through the named hub"
    );

    let missing = multi_file_diags_global_index(
        &[
            ("dep.ts", DEP),
            ("hub.ts", HUB),
            (
                "main.ts",
                "import { Foo } from './hub';\nvar bad: Foo.Missing;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Foo' has no exported member 'Missing'.".to_string()
        )],
        "a missing member through the named hub is TS2694"
    );
}

/// The enum twin of the default hub: `enum` carries `SymbolFlags.Namespace`, so
/// `C.Red` resolves and `C.Nope` is `TS2694` naming the enum.
#[test]
fn reexport_default_hub_enum_member_resolves_and_missing_is_ts2694() {
    const DEP: &str = "enum Color { Red, Green }\nexport default Color;\n";
    const HUB: &str = "export { default } from './edep';\n";

    let present = multi_file_diags_global_index(
        &[
            ("edep.ts", DEP),
            ("ehub.ts", HUB),
            ("main.ts", "import C from './ehub';\nvar ok: C.Red;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        present,
        Vec::<(u32, String)>::new(),
        "a present enum member resolves through the default hub"
    );

    let missing = multi_file_diags_global_index(
        &[
            ("edep.ts", DEP),
            ("ehub.ts", HUB),
            ("main.ts", "import C from './ehub';\nvar bad: C.Nope;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Color' has no exported member 'Nope'.".to_string()
        )],
        "a missing enum member through the default hub is TS2694 naming the enum"
    );
}

/// A *chained* hub (`hub2` re-exports the `default` of `hub`, which re-exports
/// the `default` of `dep`): each hop is an `import_module` re-export alias, so
/// the walk must compose across all of them to the terminal namespace.
#[test]
fn reexport_chained_default_hub_missing_member_is_ts2694() {
    let missing = multi_file_diags_global_index(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("hub.ts", "export { default } from './dep';\n"),
            ("hub2.ts", "export { default } from './hub';\n"),
            ("main.ts", "import D from './hub2';\nvar bad: D.Missing;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'm' has no exported member 'Missing'.".to_string()
        )],
        "a chained default hub composes to the terminal namespace"
    );
}

/// Anti-hardcoding: the chain-follow keys on the re-export edge's flags, not on
/// any spelling. Renaming the namespace binder, the hub's export, and the local
/// import leaves the behaviour and the target-relative message unchanged.
#[test]
fn reexport_hub_rule_is_binder_name_independent() {
    let missing = multi_file_diags_global_index(
        &[
            (
                "dep.ts",
                "export namespace Payload { export interface bar { a: number } }\n",
            ),
            ("hub.ts", "export { Payload } from './dep';\n"),
            (
                "main.ts",
                "import { Payload as Renamed } from './hub';\nvar bad: Renamed.Missing;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        missing,
        vec![(
            2694,
            "Namespace 'Payload' has no exported member 'Missing'.".to_string()
        )],
        "the message names the renamed target namespace, not the local alias"
    );
}

/// Negative control: a default-exported **class** reached through a hub has no
/// namespace meaning, so `D.X` stays `TS2702` and never becomes a member
/// lookup — the terminal is judged by the same namespace-meaning gate the
/// direct default-import path uses. Oracle-verified: `tsc` reports `TS2702`.
#[test]
fn reexport_default_hub_class_stays_ts2702_not_ts2694() {
    let codes = multi_file_codes_global_index(
        &[
            ("dep.ts", "export default class D { m: number = 0 }\n"),
            ("hub.ts", "export { default } from './dep';\n"),
            ("main.ts", "import D from './hub';\nvar a: D.X;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        codes,
        vec![2702],
        "a class reached through a default hub stays TS2702, never TS2694"
    );
}
