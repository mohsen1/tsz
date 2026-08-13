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
//! Name order *within* the flat list now matches tsc: the merged declarations
//! are lowered in canonical lib load order, so the list groups a declaration's
//! members together in declaration order (`clear, delete, forEach, get` first
//! for `Map`, then the es2015.iterable members, then the well-known symbol) —
//! see `resolve_lib_type_with_params`'s `lib_file_load_rank` sort (issue
//! #17344 follow-up). The order is pinned below.

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
    assert!(
        diag.message_text
            .contains("from type 'Map<string, number>'"),
        "target must render as the named reference, got: {}",
        diag.message_text
    );
    assert!(
        diag.message_text.contains("and 8 more"),
        "count must span all three declarations (12 members), got: {}",
        diag.message_text
    );
    // Order parity: tsc lists es2015.collection's members first, in
    // declaration order, then truncates. The canonical lib-load-order merge
    // reproduces `clear, delete, forEach, get` — NOT the es2015.iterable
    // members (`[Symbol.iterator]`, `entries`, …) that the resolution-incidental
    // context order surfaced first before the #17344 follow-up.
    assert!(
        diag.message_text
            .ends_with("'Map<string, number>': clear, delete, forEach, get, and 8 more."),
        "missing list must be in tsc lib-load order, got: {}",
        diag.message_text
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
    assert!(
        diag.message_text
            .contains("from type 'WeakMap<object, number>'"),
        "got: {}",
        diag.message_text
    );
    for member in ["delete", "get", "has", "set", "[Symbol.toStringTag]"] {
        assert!(
            diag.message_text.contains(member),
            "missing list must span both declarations; `{member}` absent \
             from: {}",
            diag.message_text
        );
    }
    assert!(
        !diag.message_text.contains("more"),
        "exactly five missing members list in full, got: {}",
        diag.message_text
    );
    // Order parity: es2015.collection's members (declaration order) precede the
    // es2015.symbol.wellknown member — the canonical lib-load-order merge.
    assert!(
        diag.message_text
            .ends_with("'WeakMap<object, number>': delete, get, has, set, [Symbol.toStringTag]"),
        "missing list must be in tsc lib-load order, got: {}",
        diag.message_text
    );
}

/// `Set<T>` has the same three-declaration shape as `Map` (es2015.collection +
/// es2015.iterable + es2015.symbol.wellknown). The flat list leads with the
/// collection declaration's members in declaration order — the same lib-load
/// ordering, exercised on a different member set so the guard isn't `Map`-only.
#[test]
fn empty_object_to_set_leads_with_collection_declaration_in_load_order() {
    let diags = check_with_libs("const s: Set<number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    assert!(
        diag.message_text
            .ends_with("'Set<number>': add, clear, delete, forEach, and 7 more."),
        "missing list must be in tsc lib-load order, got: {}",
        diag.message_text
    );
}

/// `Promise<T>` is the cross-*major-version* merge in the family: `then`/`catch`
/// come from `es2015.promise`, `finally` from `es2018.promise`, and
/// `[Symbol.toStringTag]` from `es2015.symbol.wellknown`. The canonical
/// lib-load-order sort must place `finally` (es2018) after `then`/`catch`
/// (es2015) and before/after the well-known member exactly as tsc does — a
/// case the same-version Map/Set/WeakMap rows do not cover.
#[test]
fn empty_object_to_promise_lists_all_declarations_in_load_order() {
    let diags = check_with_libs("const p: Promise<number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2739, "message: {}", diag.message_text);
    assert!(
        diag.message_text
            .ends_with("'Promise<number>': then, catch, finally, [Symbol.toStringTag]"),
        "missing list must span every declaration in tsc lib-load order, got: {}",
        diag.message_text
    );
}

/// `ReadonlyMap<K, V>` is the read-side variant: `es2015.collection`
/// (`forEach, get, has, size`) + `es2015.iterable` (`entries, keys, values,
/// [Symbol.iterator]`), and — unlike `Map` — no `es2015.symbol.wellknown`
/// member. The collection declaration still leads in declaration order.
#[test]
fn empty_object_to_readonlymap_leads_with_collection_declaration_in_load_order() {
    let diags = check_with_libs("const rm: ReadonlyMap<string, number> = {};");
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, 2740, "message: {}", diag.message_text);
    assert!(
        diag.message_text
            .ends_with("'ReadonlyMap<string, number>': forEach, get, has, size, and 4 more."),
        "missing list must be in tsc lib-load order, got: {}",
        diag.message_text
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
