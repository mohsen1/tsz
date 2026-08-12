//! #17186: a TS-syntax qualified import type (`import("./mod").A.B[.C…]`)
//! requires every segment but the last to resolve in *namespace* meaning.
//!
//! Structural rule: `tsc`'s import-type qualifier walk resolves each non-last
//! segment with `SymbolFlags.Namespace` meaning (`ValueModule |
//! NamespaceModule | Enum`). A class/interface/type-alias head has no such
//! meaning, so the reference fails **at that segment**, blamed against the
//! namespace formed by the segments that did resolve (the bare module
//! namespace for a head failure) and anchored at the failing segment's own
//! identifier — even when the tail member would exist in the class's static
//! side. An enum qualifier passes the gate (`Shade.Umber` is a legal enum
//! member type reference), and a class merged with a namespace passes through
//! the namespace side. Oracle: typescript@7.0.2 on this exact matrix.
//!
//! The JSDoc-path mirror of this rule landed in #17188
//! (`resolve_jsdoc_import_type_qualified_chain`); this file pins the
//! TS-syntax resolver (`state/type_resolution/import_type.rs`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::diagnostics::Diagnostic;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
}

fn ts2694(files: &[(&str, &str)], entry: &str) -> Vec<(String, u32)> {
    check(files, entry)
        .into_iter()
        .filter(|d| d.code == 2694)
        .map(|d| (d.message_text, d.start))
        .collect()
}

fn all_codes(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check(files, entry)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

const KLASS_MOD: (&str, &str) = (
    "modc.ts",
    "export class Klass {\n    s(): void {}\n    static Stat = 1;\n    static Nested = class {};\n}\n",
);

/// Class head, missing member: fails at `Klass` (offset of the head segment),
/// blamed against the unqualified module namespace — never a
/// `"modc".Klass`-qualified failure on `Inner`.
#[test]
fn class_head_missing_member_fails_at_head() {
    let entry = "type A = import('./modc').Klass.Inner;\ndeclare const a: A;\n";
    let msgs = ts2694(&[KLASS_MOD, ("main.ts", entry)], "main.ts");
    let klass_offset = entry.find("Klass").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modc\"' has no exported member 'Klass'.".to_string(),
            klass_offset,
        )],
    );
}

/// Class head, tail member EXISTS as a static property: `tsc` still fails at
/// `Klass` — the class never gains namespace meaning from having statics.
/// Guards against the false-negative shape (tsz resolving `Stat` through the
/// class's export table and reporting nothing).
#[test]
fn class_head_existing_static_still_fails_at_head() {
    let entry = "type B = import('./modc').Klass.Stat;\ndeclare const b: B;\n";
    let msgs = ts2694(&[KLASS_MOD, ("main.ts", entry)], "main.ts");
    let klass_offset = entry.find("Klass").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modc\"' has no exported member 'Klass'.".to_string(),
            klass_offset,
        )],
    );
}

/// Class head, tail member exists as a static *class expression*: same rule —
/// the nested class is reachable only in value meaning, not as a type
/// qualifier target.
#[test]
fn class_head_existing_static_class_still_fails_at_head() {
    let entry = "type L = import('./modc').Klass.Nested;\ndeclare const l: L;\n";
    let msgs = ts2694(&[KLASS_MOD, ("main.ts", entry)], "main.ts");
    let klass_offset = entry.find("Klass").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modc\"' has no exported member 'Klass'.".to_string(),
            klass_offset,
        )],
    );
}

/// Interface head: not namespace-eligible, fails at the head.
#[test]
fn interface_head_fails_at_head() {
    let entry = "type C = import('./modi').Iface.x;\ndeclare const c: C;\n";
    let msgs = ts2694(
        &[
            ("modi.ts", "export interface Iface { x: number }\n"),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let head_offset = entry.find("Iface").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modi\"' has no exported member 'Iface'.".to_string(),
            head_offset,
        )],
    );
}

/// Type-alias head: not namespace-eligible, fails at the head.
#[test]
fn type_alias_head_fails_at_head() {
    let entry = "type D = import('./moda').Alias.y;\ndeclare const d: D;\n";
    let msgs = ts2694(
        &[
            ("moda.ts", "export type Alias = { y: string };\n"),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let head_offset = entry.find("Alias").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"moda\"' has no exported member 'Alias'.".to_string(),
            head_offset,
        )],
    );
}

/// Enum head with an existing member: `SymbolFlags.Namespace` includes Enum,
/// so `Shade.Umber` is a legal enum-member type reference — no diagnostics.
#[test]
fn enum_head_existing_member_resolves() {
    let codes = all_codes(
        &[
            ("mode.ts", "export enum Shade { Umber, Ochre }\n"),
            (
                "main.ts",
                "type E = import('./mode').Shade.Umber;\ndeclare const e: E;\nconst ok: E = 0 as unknown as E;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(codes, Vec::<(u32, String)>::new());
}

/// Const-enum head with an existing member also resolves.
#[test]
fn const_enum_head_existing_member_resolves() {
    let codes = all_codes(
        &[
            ("mode.ts", "export const enum Cshade { Bright }\n"),
            (
                "main.ts",
                "type K = import('./mode').Cshade.Bright;\ndeclare const k: K;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(codes, Vec::<(u32, String)>::new());
}

/// Enum head with a missing member: the enum passes the qualifier gate, so
/// the failure is the ordinary qualified miss — blamed on `Missing` under
/// `"mode".Shade`, anchored at `Missing`.
#[test]
fn enum_head_missing_member_reports_qualified() {
    let entry = "type F = import('./mode').Shade.Missing;\ndeclare const f: F;\n";
    let msgs = ts2694(
        &[
            ("mode.ts", "export enum Shade { Umber, Ochre }\n"),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let missing_offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"mode\".Shade' has no exported member 'Missing'.".to_string(),
            missing_offset,
        )],
    );
}

/// Positive control: a genuine namespace head with a real nested member keeps
/// resolving.
#[test]
fn namespace_head_existing_member_resolves() {
    let codes = all_codes(
        &[
            (
                "modn.ts",
                "export namespace Wing {\n    export interface Feather { q: number }\n}\n",
            ),
            (
                "main.ts",
                "type G = import('./modn').Wing.Feather;\ndeclare const g: G;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(codes, Vec::<(u32, String)>::new());
}

/// Namespace head with a missing member: message stays qualified and anchors
/// at the missing (rightmost) segment — unchanged by the qualifier gate.
#[test]
fn namespace_head_missing_member_reports_qualified() {
    let entry = "type H = import('./modn').Wing.Missing;\ndeclare const h: H;\n";
    let msgs = ts2694(
        &[
            (
                "modn.ts",
                "export namespace Wing {\n    export interface Feather { q: number }\n}\n",
            ),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let missing_offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modn\".Wing' has no exported member 'Missing'.".to_string(),
            missing_offset,
        )],
    );
}

/// A class merged with a namespace has namespace meaning: the qualifier walk
/// proceeds through the namespace side.
#[test]
fn class_namespace_merge_head_resolves() {
    let codes = all_codes(
        &[
            (
                "modm.ts",
                "export class Merged {}\nexport namespace Merged {\n    export interface Deep { z: 1 }\n}\n",
            ),
            (
                "main.ts",
                "type I = import('./modm').Merged.Deep;\ndeclare const i: I;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(codes, Vec::<(u32, String)>::new());
}

/// Middle-segment gate: `Wing.Feather.q` fails at `Feather` (an interface is
/// not namespace-eligible even when it resolved as a namespace member),
/// blamed under the prefix that did resolve (`"modn".Wing`) and anchored at
/// the failing middle segment — not at the rightmost `q`.
#[test]
fn interface_middle_segment_fails_at_middle() {
    let entry = "type J = import('./modn').Wing.Feather.q;\ndeclare const j: J;\n";
    let msgs = ts2694(
        &[
            (
                "modn.ts",
                "export namespace Wing {\n    export interface Feather { q: number }\n}\n",
            ),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let feather_offset = entry.find("Feather").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modn\".Wing' has no exported member 'Feather'.".to_string(),
            feather_offset,
        )],
    );
}

/// Mid-chain member miss: `Wing.Missing.q` stops at `Missing` (not found in
/// `Wing`'s exports), blamed under `"modn".Wing` and anchored at `Missing`.
#[test]
fn mid_chain_member_miss_fails_at_missing_segment() {
    let entry = "type M = import('./modn').Wing.Missing.q;\ndeclare const m: M;\n";
    let msgs = ts2694(
        &[
            (
                "modn.ts",
                "export namespace Wing {\n    export interface Feather { q: number }\n}\n",
            ),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let missing_offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modn\".Wing' has no exported member 'Missing'.".to_string(),
            missing_offset,
        )],
    );
}

/// An enum *member* used as a qualifier: `Shade.Umber.x` fails at `Umber` —
/// an enum member passes the last-segment type meaning but has no namespace
/// meaning, so tsc reports it as missing under `"mode".Shade`.
#[test]
fn enum_member_as_qualifier_fails_at_member() {
    let entry = "type N = import('./mode').Shade.Umber.x;\ndeclare const n: N;\n";
    let msgs = ts2694(
        &[
            ("mode.ts", "export enum Shade { Umber, Ochre }\n"),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let umber_offset = entry.find("Umber").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"mode\".Shade' has no exported member 'Umber'.".to_string(),
            umber_offset,
        )],
    );
}

/// A value-only LAST segment: `Wing.Vee` where `Vee` is a `const` inside the
/// namespace. tsc requires type meaning of the final segment and reports the
/// qualified miss at `Vee`.
#[test]
fn value_only_last_segment_fails_at_last() {
    let entry = "type P = import('./modv').Wing.Vee;\ndeclare const p: P;\n";
    let msgs = ts2694(
        &[
            (
                "modv.ts",
                "export namespace Wing {\n    export const Vee = 1;\n    export interface Feather { q: number }\n}\n",
            ),
            ("main.ts", entry),
        ],
        "main.ts",
    );
    let vee_offset = entry.find("Vee").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"modv\".Wing' has no exported member 'Vee'.".to_string(),
            vee_offset,
        )],
    );
}

// KNOWN RESIDUAL GAP (out of scope here, oracle-verified so the next session
// can pick it up cold): a CROSS-FILE `export import Al = Wing` alias inside a
// namespace should be a valid qualifier (`import('./modv').Outer.Al.Feather`
// resolves in tsc, and a miss past it renders the WRITTEN path:
// `Namespace '"modv".Outer.Al' has no exported member 'Missing'`). tsz's
// `resolve_alias_symbol` reads the consuming file's binder, so a namespace
// `export import` declared in another file never resolves, and the walk now
// stops at `Al` (`Namespace '"modv".Outer' has no exported member 'Al'`).
// Before the meaning gate this shape was also wrong, just differently
// (`Namespace '"modv".Outer.Al' has no exported member 'Feather'`). Fixing it
// needs cross-arena alias delegation, not a walk-level change.

/// Binder-name independence: the gate follows symbol meaning, not any
/// identifier spelling — renamed module, class, and member reproduce the
/// head-failure shape verbatim.
#[test]
fn renamed_binders_class_head_fails_at_head() {
    let entry = "type Z = import('./widgets').Gadget.Cog;\ndeclare const z: Z;\n";
    let msgs = ts2694(
        &[
            (
                "widgets.ts",
                "export class Gadget {\n    static Cog = 2;\n}\n",
            ),
            ("consumer.ts", entry),
        ],
        "consumer.ts",
    );
    let head_offset = entry.find("Gadget").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"widgets\"' has no exported member 'Gadget'.".to_string(),
            head_offset,
        )],
    );
}
