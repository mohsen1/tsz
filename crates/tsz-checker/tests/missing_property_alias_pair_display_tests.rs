//! Parity pins for the alias name TS2739/TS2740/TS2741 heads (and the TS2322
//! source slot) pick when **two type aliases lower to one interned `TypeId`**.
//!
//! `tsc` keys a type's display identity on the alias reference written at the
//! use site: the target's annotation and the source's declaration each carry
//! their own `aliasSymbol`, so `type PairA = { x; y }; type PairB = { x; y };
//! const p: PairB = ...` says `PairB` even though `PairA` describes the same
//! shape. tsz interns one `TypeId` per content and the reverse `type_to_def`
//! table is earliest-declaration-wins, so before the per-occurrence
//! written-alias gate reached these heads every occurrence rendered the
//! first-registered alias — including a *different alias family entirely*
//! (`ReqB` rendered `PairA`).
//!
//! The target half of the gate landed for TS2322 in #17756
//! (`written_alias_reference_target_display`); these rows pin its extension to
//! the missing-property heads and the new source-side counterpart
//! (`written_alias_reference_source_display`) in the shared assignment-source
//! formatter.
//!
//! Every expectation was verified against the pinned oracle
//! (`typescript@7.0.2`, `--strict --noEmit --pretty false`), one probe file
//! per row, on 2026-08-20. Rows in the "decline" section pin the negative
//! space: spellings where tsc does NOT repaint with the written alias and the
//! gate must stand down.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// All diagnostics of `code` a fixture produces, as rendered message strings.
fn messages_for_code(source: &str, code: u32) -> Vec<String> {
    let diagnostics = check_source_with_libs_code_messages(
        source,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    );
    diagnostics
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, message)| message.clone())
        .collect()
}

fn sole_message_for_code(source: &str, code: u32) -> String {
    let messages = messages_for_code(source, code);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS{code} for this fixture, got {messages:?}"
    );
    messages.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------------
// TS2739 / TS2740: multi-property missing list, non-fresh alias-pair source.
// ---------------------------------------------------------------------------

/// Both head slots render the alias written at their own occurrence, not the
/// first-registered alias of the shape (`SlimA` / `WideA`).
#[test]
fn ts2739_head_renders_the_written_alias_pair() {
    let source = "type WideA = { a: string; b: number; c: boolean };\n\
                  type WideB = { a: string; b: number; c: boolean };\n\
                  type SlimA = { a: string };\n\
                  type SlimB = { a: string };\n\
                  declare const s: SlimB;\n\
                  const w: WideB = s;\n";
    assert_eq!(
        sole_message_for_code(source, 2739),
        "Type 'SlimB' is missing the following properties from type 'WideB': b, c"
    );
}

/// The truncated `and N more` form keeps the same per-occurrence rule.
#[test]
fn ts2740_truncated_head_renders_the_written_alias_pair() {
    let source = "type BigA = { a: 1; b: 1; c: 1; d: 1; e: 1; f: 1; g: 1 };\n\
                  type BigB = { a: 1; b: 1; c: 1; d: 1; e: 1; f: 1; g: 1 };\n\
                  type TinyA = { a: 1 };\n\
                  type TinyB = { a: 1 };\n\
                  declare const tb: TinyB;\n\
                  const big: BigB = tb;\n";
    assert_eq!(
        sole_message_for_code(source, 2740),
        "Type 'TinyB' is missing the following properties from type 'BigB': b, c, d, e, and 2 more."
    );
}

// ---------------------------------------------------------------------------
// TS2741: single missing property.
// ---------------------------------------------------------------------------

/// A fresh object-literal source keeps its structural display; the target
/// renders the annotation's own alias, not the first-registered `PairA`.
#[test]
fn ts2741_fresh_literal_source_target_renders_the_written_alias() {
    let source = "type PairA = { x: number; y: number };\n\
                  type PairB = { x: number; y: number };\n\
                  const p: PairB = { x: 1 };\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'y' is missing in type '{ x: number; }' but required in type 'PairB'."
    );
}

/// Non-fresh source: both slots follow their own written reference. `ReqB`'s
/// shape interns to the same `TypeId` as the unrelated `PairA`/`PairB` family,
/// so before the gate the head rendered `PairA` for a target written `ReqB` —
/// a different alias family entirely, not just the wrong twin.
#[test]
fn ts2741_non_fresh_source_both_slots_render_their_written_alias() {
    let source = "type PairA = { x: number; y: number };\n\
                  type PairB = { x: number; y: number };\n\
                  type SoloA = { x: number };\n\
                  type SoloB = { x: number };\n\
                  type ReqA = { x: number; y: number };\n\
                  type ReqB = { x: number; y: number };\n\
                  declare const solo: SoloB;\n\
                  const q: ReqB = solo;\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'y' is missing in type 'SoloB' but required in type 'ReqB'."
    );
}

/// Renamed binders, declaration order reversed (the later-declared alias is
/// the one written at both occurrences' partners).
#[test]
fn ts2741_renamed_binders_reversed_declaration_order() {
    let source = "type Zulu = { p: string; q: number };\n\
                  type Alpha = { p: string; q: number };\n\
                  type Yankee = { p: string };\n\
                  type Bravo = { p: string };\n\
                  declare const src: Bravo;\n\
                  const dst: Alpha = src;\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'q' is missing in type 'Bravo' but required in type 'Alpha'."
    );
}

/// A parameter-declared source identifier resolves its annotation the same
/// way a variable declaration does.
#[test]
fn ts2741_parameter_declared_source_renders_its_written_alias() {
    let source = "type ArgA = { u: string; v: number };\n\
                  type ArgB = { u: string; v: number };\n\
                  function f(x: ArgB) {\n\
                    const y: { u: string; v: number; w: boolean } = x;\n\
                  }\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'w' is missing in type 'ArgB' but required in type '{ u: string; v: number; w: boolean; }'."
    );
}

// ---------------------------------------------------------------------------
// TS2322: the shared assignment-source formatter serves TS2322 too.
// ---------------------------------------------------------------------------

/// The source slot of a plain TS2322 renders the declaration's own alias.
#[test]
fn ts2322_source_slot_renders_the_written_alias() {
    let source = "type SrcA = { x: number };\n\
                  type SrcB = { x: number };\n\
                  declare const sb: SrcB;\n\
                  const n: number = sb;\n";
    assert_eq!(
        sole_message_for_code(source, 2322),
        "Type 'SrcB' is not assignable to type 'number'."
    );
}

// ---------------------------------------------------------------------------
// Decline rows: spellings tsc does NOT repaint with the written alias.
// ---------------------------------------------------------------------------

/// A bare alias-to-alias forwarding annotation renders the alias the chain
/// resolves to (`Inner`), on the source side exactly as on the target side.
#[test]
fn forwarding_alias_source_renders_the_chain_resolved_inner_alias() {
    let source = "type Inner = { a: string; b: number };\n\
                  type Outer = Inner;\n\
                  declare const o: Outer;\n\
                  const t: { a: string; b: number; c: boolean } = o;\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'c' is missing in type 'Inner' but required in type '{ a: string; b: number; c: boolean; }'."
    );
}

/// A flow-narrowed source renders the narrowed checked type; the identity
/// guard declines because the narrowed type no longer equals the annotation's
/// lowered body.
#[test]
fn flow_narrowed_source_keeps_the_narrowed_display() {
    let source = "type MaybeNum = string | number;\n\
                  type AlsoMaybe = string | number;\n\
                  declare const m: AlsoMaybe;\n\
                  if (typeof m === \"string\") {\n\
                    const n: number = m;\n\
                  }\n";
    assert_eq!(
        sole_message_for_code(source, 2322),
        "Type 'string' is not assignable to type 'number'."
    );
}

/// Generic alias applications keep the established application-aware display
/// on both sides; the gate declines references with type arguments.
#[test]
fn generic_application_pair_keeps_the_application_display() {
    let source = "type Box<T> = { v: T; w: T };\n\
                  type Crate<T> = { v: T; w: T };\n\
                  declare const b: Box<number>;\n\
                  const c: Crate<string> = b;\n";
    let message = sole_message_for_code(source, 2322);
    assert!(
        message.starts_with("Type 'Box<number>' is not assignable to type 'Crate<string>'."),
        "unexpected TS2322 head: {message}"
    );
}

/// A single alias with no like-shaped twin renders exactly as before the gate
/// (the gate answers the same name the reverse lookup already found).
#[test]
fn single_alias_source_display_is_unchanged() {
    let source = "type Solo = { m: string; n: number };\n\
                  declare const s: Solo;\n\
                  const t: { m: string; n: number; o: boolean } = s;\n";
    assert_eq!(
        sole_message_for_code(source, 2741),
        "Property 'o' is missing in type 'Solo' but required in type '{ m: string; n: number; o: boolean; }'."
    );
}
