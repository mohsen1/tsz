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

use crate::test_utils::{check_multi_file, check_source_diagnostics};
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
// Cross-file: an ALIAS qualifier is resolved to its target, not skipped
// ---------------------------------------------------------------------------

/// A *default*-imported class used as a namespace qualifier. `export default`
/// carries only the class's type/value meaning — never a merged namespace's —
/// so `tsc` reports `TS2702` on the qualifier for every row here. The default
/// export is bound as a synthetic alias distinct from any same-named local
/// declaration, so a merge partner (`namespace Decl {}`, row 2) never
/// contributes namespace meaning to it either.
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
/// `TS2694` for `Named.Missing`. The alias resolves to the *merged* export
/// symbol directly (no synthetic default-export wrapper in the way), so its
/// flags already carry the module meaning the merge produced.
#[test]
fn cross_file_class_namespace_merge_keeps_its_namespace_meaning() {
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
