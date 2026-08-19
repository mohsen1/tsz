//! Union display order for the nullish intrinsics (#17661 residual 3).
//!
//! Structural rule: `tsc`'s `formatUnionTypes` filters `TypeFlags.Nullable`
//! constituents out of the printed member walk and appends `nullType` then
//! `undefinedType` after it, so a rendered union always shows
//! `... | null | undefined` — regardless of the union's internal (type-id)
//! member order or the annotation's as-written order. The elaborated failing
//! member below the head line is unaffected (the relation walk still visits
//! `undefined`/`null` first). Owners:
//!
//! * Solver (`diagnostics/format/mod.rs`):
//!   `reorder_union_members_nullish_last`, the shared member-list reorder for
//!   checker-side reconstructions (`format_union` already applied the rule
//!   internally).
//! * Checker enum-union display (`assignability_enum_display.rs`): the
//!   collapsed-enum render walk iterates the reordered list instead of the
//!   interner's canonical order (which puts the small-id nullish intrinsics
//!   first: `undefined | Duo`).
//! * Checker annotation repaint (`core/type_display.rs`): a repainted
//!   annotation's top-level `null`/`undefined` union parts move to the tail
//!   instead of keeping the written order (`undefined | (() => void)`).
//!
//! Every expectation below is oracle-pinned against `tsc` 6.0.2 (`--strict`),
//! byte-for-byte. Binder names vary across cases so a fix keyed to a
//! particular spelling cannot satisfy the suite.

use tsz_checker::test_utils::{check_with_options, strict_checker_options};
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, strict_checker_options())
}

/// The full chain of the single diagnostic with `code`: the primary message
/// at depth 0 prepended to its related-information `(depth + 1, text)` pairs,
/// asserted exactly.
fn assert_exact_chain(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = diagnostics(source);
    let matching: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut chain = vec![(0u8, matching[0].message_text.clone())];
    chain.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    let rendered: Vec<(u8, &str)> = chain.iter().map(|(d, m)| (*d, m.as_str())).collect();
    assert_eq!(rendered, expected, "chain mismatch for:\n{source}");
}

// --- Enum-union display site (interner canonical order leaked) ------------

#[test]
fn enum_member_source_undefined_written_last_keeps_member_first() {
    assert_exact_chain(
        r#"
enum Gear { Low, High }
declare const g: Gear.Low | undefined;
const t: Gear.High = g;
"#,
        2322,
        &[
            (
                0,
                "Type 'Gear.Low | undefined' is not assignable to type 'Gear.High'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'Gear.High'."),
        ],
    );
}

#[test]
fn enum_member_source_undefined_written_first_still_renders_last() {
    assert_exact_chain(
        r#"
enum Dial { Off, On }
declare const d: undefined | Dial.Off;
const t: Dial.On = d;
"#,
        2322,
        &[
            (
                0,
                "Type 'Dial.Off | undefined' is not assignable to type 'Dial.On'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'Dial.On'."),
        ],
    );
}

#[test]
fn whole_enum_source_generalizing_target_renders_enum_before_undefined() {
    assert_exact_chain(
        r#"
enum Cadence { One, Two }
declare const c: undefined | Cadence;
const s: string = c;
"#,
        2322,
        &[
            (
                0,
                "Type 'Cadence | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn enum_with_null_and_undefined_renders_null_before_undefined_at_tail() {
    assert_exact_chain(
        r#"
enum Pulse { A, B }
declare const p: undefined | Pulse | null;
const s: string = p;
"#,
        2322,
        &[
            (
                0,
                "Type 'Pulse | null | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn enum_with_null_only_renders_null_at_tail() {
    assert_exact_chain(
        r#"
enum Phase { Solid, Liquid }
declare const p: null | Phase;
const s: string = p;
"#,
        2322,
        &[
            (0, "Type 'Phase | null' is not assignable to type 'string'."),
            (1, "Type 'null' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn single_member_enum_member_source_renders_before_undefined() {
    assert_exact_chain(
        r#"
enum Lone { Just }
declare const q: Lone.Just | undefined;
const s: string = q;
"#,
        2322,
        &[
            (
                0,
                "Type 'Lone | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn enum_union_argument_renders_enum_before_undefined_in_ts2345() {
    assert_exact_chain(
        r#"
enum Cog { A, B }
declare function take(s: string): void;
declare const e1: undefined | Cog;
take(e1);
"#,
        2345,
        &[
            (
                0,
                "Argument of type 'Cog | undefined' is not assignable to parameter of type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn optional_enum_property_read_renders_enum_before_undefined() {
    assert_exact_chain(
        r#"
enum Trio { P, Q, R }
declare const oe: { b?: Trio };
const x: string = oe.b;
"#,
        2322,
        &[
            (
                0,
                "Type 'Trio | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

// --- Annotation-repaint site (written order leaked) -----------------------

#[test]
fn function_union_annotation_written_undefined_first_renders_undefined_last() {
    assert_exact_chain(
        r#"
declare const cb: undefined | (() => void);
const s: string = cb;
"#,
        2322,
        &[
            (
                0,
                "Type '(() => void) | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn function_union_annotation_with_null_and_undefined_renders_canonical_tail() {
    assert_exact_chain(
        r#"
declare const mix: undefined | null | (() => number);
const s: string = mix;
"#,
        2322,
        &[
            (
                0,
                "Type '(() => number) | null | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

// --- Negative controls (already-canonical renders stay byte-identical) ----

#[test]
fn function_union_annotation_written_canonical_stays_unchanged() {
    assert_exact_chain(
        r#"
declare const ok: (() => void) | undefined;
const s: string = ok;
"#,
        2322,
        &[
            (
                0,
                "Type '(() => void) | undefined' is not assignable to type 'string'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn primitive_union_written_undefined_first_renders_undefined_last() {
    assert_exact_chain(
        r#"
declare const pn: undefined | string;
const n: number = pn;
"#,
        2322,
        &[
            (
                0,
                "Type 'string | undefined' is not assignable to type 'number'.",
            ),
            (1, "Type 'undefined' is not assignable to type 'number'."),
        ],
    );
}

#[test]
fn nested_property_union_keeps_canonical_tail_in_elaboration() {
    assert_exact_chain(
        r#"
enum Cam { M, N }
declare const holder: { m: undefined | Cam };
const t: { m: string } = holder;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: Cam | undefined; }' is not assignable to type '{ m: string; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'Cam | undefined' is not assignable to type 'string'.",
            ),
            (3, "Type 'undefined' is not assignable to type 'string'."),
        ],
    );
}
