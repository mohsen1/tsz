//! A reference to a multi-declaration (merged) `interface` — above all the
//! lib's generic collections (`Map<K, V>`, `Set<T>`, `WeakMap<K, V>`,
//! `Promise<T>`), declared across `lib.es2015.collection` /
//! `lib.es2015.iterable` / `lib.es2015.symbol.wellknown` — structurally
//! evaluates in tsz to an intersection of its per-declaration shapes. `tsc`
//! never relates or reports such a target as an intersection: a failed
//! assignment reports ONE flat missing-property diagnostic
//! (TS2740/TS2739/TS2741) whose list spans ALL declarations' members and whose
//! target display is the named interface reference:
//!
//! ```text
//! Type '{}' is missing the following properties from type
//! 'Map<string, number>': clear, delete, forEach, get, and 8 more.
//! ```
//!
//! tsz previously emitted a TS2322 headline plus a per-constituent
//! elaboration computed against only the *first failing declaration's* shape
//! (`Type '{}' is missing the following properties from type
//! '{ [Symbol.iterator](): …; entries(): …; } & { readonly
//! [Symbol.toStringTag]: string; }': [Symbol.iterator], …`), dropping the
//! other declarations' members from the list entirely.
//!
//! Structural rule (verified against `tsc` 6.0.2, `--strict`): when the
//! target of a failed assignability relation is a reference to an `interface`
//! definition (`Lazy(DefId)` or `Application(Lazy(DefId), …)` with
//! `DefKind::Interface`), the missing-property reason is computed over the
//! full merged member surface and rendered flat against the named reference;
//! the written-intersection framing (TS2322 + first-failing-constituent
//! elaboration) applies only to intersection-*typed* targets (`A & B`
//! annotations and aliases). Owners: the solver explain pass
//! (`explain_failure_inner`'s intersection-target arm), the assignability
//! gateway's constituent wrap (`target_intersection_constituents`), and the
//! renderer's intersection-target downgrade
//! (`resolve_intersection_target_for_display_kind`).
//!
//! List order (fixed by #17351): tsc lists declarations in lib load order
//! (`clear, delete, forEach, get` first for `Map`, `es2015.collection` before
//! `es2015.iterable` before `es2015.symbol.wellknown`), members in declaration
//! order. The generic multi-declaration lib merge
//! (`resolve_lib_type_with_params`) previously flattened to a single ordered
//! object only under `--declaration`; a plain check took an `intersection2`
//! fallback that restarted each declaration's `declaration_order` from 1, so
//! the missing-list sort interleaved per-declaration ranks. Merging through
//! `merge_interface_types` unconditionally (matching the sibling
//! `resolve_lib_type_by_name`) rebases each declaration's members after the
//! previous, reproducing tsc's order. These tests now pin the exact tsc string
//! (order included) for the whole family.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_diagnostics, check_source_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check_with_libs(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

/// `{}` against `Map<K, V>`: one flat TS2740 naming the reference, whose
/// count spans all three declarations (7 collection + 4 iterable + 1
/// well-known = 12 members ⇒ 4 listed + "and 8 more"), with no TS2322
/// headline and no per-constituent elaboration.
#[test]
fn empty_object_to_map_reports_flat_ts2740_over_all_declarations() {
    let diags = check_with_libs("const m: Map<string, number> = {};");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    // Exact tsc 6.0.2 string: named reference, lib-load-order list
    // (es2015.collection members first), and the "and 8 more" tail.
    assert_eq!(
        diag.message_text,
        "Type '{}' is missing the following properties from type \
         'Map<string, number>': clear, delete, forEach, get, and 8 more.",
    );
    assert!(
        diag.related_information.is_empty(),
        "tsc emits no per-constituent elaboration for a merged interface \
         reference, got: {:?}",
        diag.related_information
            .iter()
            .map(|info| info.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// `WeakMap<K, V>` merges `es2015.collection` (4 members) with
/// `es2015.symbol.wellknown` (1 member): exactly five missing ⇒ TS2739 with
/// the full list (no "and N more"), including members from BOTH declarations.
#[test]
fn empty_object_to_weakmap_lists_both_declarations_members() {
    let diags = check_with_libs("const wm: WeakMap<object, number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2739, "message: {}", diag.message_text);
    // Exact tsc order: es2015.collection (delete, get, has, set) before
    // es2015.symbol.wellknown ([Symbol.toStringTag]).
    assert_eq!(
        diag.message_text,
        "Type '{}' is missing the following properties from type \
         'WeakMap<object, number>': delete, get, has, set, [Symbol.toStringTag]",
    );
}

/// `Set<T>` merges three declarations (`es2015.collection` +
/// `es2015.iterable` + `es2015.symbol.wellknown`); the listed prefix is the
/// collection declaration's members in declaration order, matching tsc.
#[test]
fn empty_object_to_set_lists_collection_members_first() {
    let diags = check_with_libs("const s: Set<number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    assert_eq!(
        diag.message_text,
        "Type '{}' is missing the following properties from type \
         'Set<number>': add, clear, delete, forEach, and 7 more.",
    );
}

/// `Promise<T>` merges `es2015.promise` (`then`, `catch`), `es2018.promise`
/// (`finally`), and `es2015.symbol.wellknown` (`[Symbol.toStringTag]`) — the
/// list preserves that cross-declaration order (`then, catch` before
/// `finally`), not a per-declaration-rank interleave.
#[test]
fn empty_object_to_promise_preserves_cross_declaration_order() {
    let diags = check_with_libs("const p: Promise<number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2739, "message: {}", diag.message_text);
    assert_eq!(
        diag.message_text,
        "Type '{}' is missing the following properties from type \
         'Promise<number>': then, catch, finally, [Symbol.toStringTag]",
    );
}

/// A source already satisfying the iterable + well-known declarations still
/// gets the collection declaration's members reported — previously those were
/// only reachable through the constituent elaboration, never the flat list.
#[test]
fn partial_source_reports_remaining_declarations_members() {
    let diags = check_with_libs(
        r#"
declare const halfway: {
  [Symbol.iterator](): MapIterator<[string, number]>;
  entries(): MapIterator<[string, number]>;
  keys(): MapIterator<string>;
  values(): MapIterator<number>;
  readonly [Symbol.toStringTag]: string;
};
const m: Map<string, number> = halfway;
"#,
    );
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    assert!(
        diag.message_text
            .contains("from type 'Map<string, number>'"),
        "got: {}",
        diag.message_text
    );
    // 7 collection members missing ⇒ 4 listed + "and 3 more" (tsc-exact
    // count; the four listed are collection members in every order).
    assert!(
        diag.message_text.contains("and 3 more"),
        "got: {}",
        diag.message_text
    );
}

/// A primitive source keeps the flat TS2322 — and must NOT regain the
/// per-constituent "is missing" elaboration the interface-reference gate
/// removed from the wrap path.
#[test]
fn primitive_source_to_map_stays_flat_ts2322() {
    let diags = check_with_libs("const m: Map<string, number> = 42;");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2322, "message: {}", diag.message_text);
    assert_eq!(
        diag.message_text,
        "Type 'number' is not assignable to type 'Map<string, number>'.",
    );
    assert!(
        diag.related_information.is_empty(),
        "no constituent frame for a merged interface reference, got: {:?}",
        diag.related_information
            .iter()
            .map(|info| info.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Same-file declaration merging (the single-arena path) was already flat and
/// stays byte-identical to tsc — including cross-declaration list order,
/// which the single-arena lowering preserves.
#[test]
fn same_file_merged_interface_stays_flat_and_ordered() {
    let diags = check_source_diagnostics(
        r#"
interface Duo { one: number; }
interface Duo { two: string; }
const d: Duo = {};
"#,
    );
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2739);
    assert_eq!(
        diag.message_text,
        "Type '{}' is missing the following properties from type 'Duo': one, two",
    );
}

/// Renamed-binder variant of the same-file merge: the rule keys on the
/// definition kind, not any particular interface name.
#[test]
fn same_file_merged_interface_renamed_binders() {
    let diags = check_source_diagnostics(
        r#"
interface Blob9 { alpha: number; }
interface Blob9 { beta: string; }
const value9: Blob9 = {};
"#,
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(
        diags[0].message_text,
        "Type '{}' is missing the following properties from type 'Blob9': alpha, beta",
    );
}

/// Negative control: a written intersection target (alias spelling) keeps the
/// TS2322 headline + first-failing-constituent elaboration — the
/// interface-reference gate must not leak onto genuine intersections.
#[test]
fn written_intersection_alias_target_keeps_constituent_elaboration() {
    let diags = check_source_diagnostics(
        r#"
interface FirstHalf { left: number; }
interface SecondHalf { right: string; }
type Whole = FirstHalf & SecondHalf;
declare const given: { left: number };
const w: Whole = given;
"#,
    );
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2322, "message: {}", diag.message_text);
    let elaboration: Vec<&str> = diag
        .related_information
        .iter()
        .map(|info| info.message_text.as_str())
        .collect();
    assert!(
        elaboration
            .iter()
            .any(|line| line.contains("required in type 'SecondHalf'")),
        "written intersections keep the per-constituent drill, got: {elaboration:?}"
    );
}

/// A generic interface extending a merged lib interface reports the inherited
/// merged surface flat against ITS name, exactly like tsc.
#[test]
fn interface_extending_map_reports_flat_against_its_own_name() {
    let diags = check_with_libs(
        r#"
interface Extended9<K, V> extends Map<K, V> { extra: boolean; }
declare const src9: { extra: boolean };
const e: Extended9<string, number> = src9;
"#,
    );
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    assert!(
        diag.message_text
            .contains("from type 'Extended9<string, number>'"),
        "got: {}",
        diag.message_text
    );
    assert!(
        diag.message_text.contains("and 8 more"),
        "inherited merged surface spans all Map declarations, got: {}",
        diag.message_text
    );
}
