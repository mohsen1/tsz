//! TS2530/TS2634 for a source property checked against a target index
//! signature, and the underlying false negative this closes.
//!
//! `check_properties_against_index_signatures`
//! (`crates/tsz-solver/src/relations/subtype/rules/objects.rs`) used to
//! `continue` past any source property whose name the target also declares as
//! a named member, on the theory that named-property rules already cover it.
//! But `tsc`'s `membersRelatedToIndexInfo` checks *every* source property
//! against the index, including one that name-matches a target member — so a
//! target whose own declared property conflicts with its index signature (the
//! TS2411 shape, e.g. `{ [k: string]: number; flag: boolean }`) still rejects
//! an assignment whose property violates the index. tsz silently accepted it.
//!
//! Independently, every property-vs-index failure was labeled TS2634 (`'{kind}'
//! index signatures are incompatible.`), which `tsc` reserves for a source
//! *index signature* vs the target index; a source *property* vs the target
//! index is TS2530 (`Property '{name}' is incompatible with index
//! signature.`).

use tsz_checker::test_utils::{check_source_strict, check_source_strict_codes};

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

/// Flatten each diagnostic's own `(code, message)` plus every
/// `related_information` elaboration line's `(code, message)` — TS2530/TS2634
/// are emitted as elaboration lines under the top-level TS2322/TS2345, not as
/// their own top-level diagnostics.
fn codes_and_messages_with_elaborations(source: &str) -> Vec<(u32, String)> {
    check_source_strict(source)
        .into_iter()
        .flat_map(|d| {
            let mut all = vec![(d.code, d.message_text.clone())];
            all.extend(
                d.related_information
                    .iter()
                    .map(|r| (r.code, r.message_text.clone())),
            );
            all
        })
        .collect()
}

#[test]
fn named_matched_property_violating_index_now_reports_on_variable_assignment() {
    // `Box.flag` (`boolean`) conflicts with `Box`'s own `[k: string]: number`
    // index (TS2411), so a source whose `flag` is `boolean` must also fail the
    // index check — tsc: TS2411 + TS2322/TS2530.
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare const s: { flag: boolean };
const c: Box = s;
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2411),
        "TS2411 on the Box declaration, got: {cs:?}"
    );
    assert!(
        cs.contains(&2322),
        "assigning a `flag: boolean` source must now also fail via the index signature, got: {cs:?}"
    );
}

#[test]
fn named_matched_property_violating_index_reports_on_return() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare const s: { flag: boolean };
function f(): Box {
    return s;
}
"#;
    let cs = codes(source);
    assert!(cs.contains(&2411), "got: {cs:?}");
    assert!(cs.contains(&2322), "got: {cs:?}");
}

#[test]
fn named_matched_property_violating_index_reports_on_call_argument() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare const s: { flag: boolean };
declare function take(b: Box): void;
take(s);
"#;
    let cs = codes(source);
    assert!(cs.contains(&2411), "got: {cs:?}");
    assert!(
        cs.contains(&2345),
        "call-argument context reports TS2345, got: {cs:?}"
    );
}

#[test]
fn named_matched_property_satisfying_index_stays_clean() {
    // Negative control: when the target's own named property *does* satisfy
    // its index (no TS2411 shape), removing the skip must not introduce a new
    // failure — the property check already established `source.prop <:
    // target.namedProp <: index`, so the index check passes by transitivity.
    let source = r#"
type Ok = { [k: string]: number; flag: number };
declare const s: { flag: number };
const c: Ok = s;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "a source matching a target property that already satisfies its own index must stay clean"
    );
}

#[test]
fn incompatible_source_and_target_stays_clean_when_target_lacks_index() {
    // Negative control: no index signature at all on the target — unrelated
    // to this fix, must stay unaffected.
    let source = r#"
type Plain = { flag: boolean };
declare const s: { flag: boolean };
const c: Plain = s;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "got unexpected diagnostics"
    );
}

#[test]
fn renamed_binders_still_trigger_the_named_property_index_check() {
    // Anti-hardcoding: different type/property/binder names, same shape.
    let source = r#"
type Widget = { [key: string]: number; enabled: boolean };
declare const w: { enabled: boolean };
const result: Widget = w;
"#;
    let cs = codes(source);
    assert!(cs.contains(&2411), "got: {cs:?}");
    assert!(cs.contains(&2322), "got: {cs:?}");
}

#[test]
fn property_vs_index_failure_is_labeled_ts2530_not_ts2634() {
    let source = r#"
type Dict = { [k: string]: number };
declare const s: { label: string };
const d: Dict = s;
"#;
    let messages = codes_and_messages_with_elaborations(source);
    let hit = messages.iter().find(|(code, _)| *code == 2530);
    assert!(
        hit.is_some_and(|(_, m)| m.contains("'label'")),
        "a source property vs target index failure must be TS2530 naming the property, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|(code, _)| *code == 2634),
        "must not also emit TS2634 for the same failure, got: {messages:?}"
    );
}

#[test]
fn index_vs_index_failure_stays_labeled_ts2634() {
    // Control: when *both* sides are index signatures (not a named property),
    // the failure stays TS2634, not TS2530.
    let source = r#"
type Dict = { [k: string]: number };
interface HasStringIndex {
    [k: string]: string;
}
declare const s: HasStringIndex;
const d: Dict = s;
"#;
    let messages = codes_and_messages_with_elaborations(source);
    let hit = messages.iter().find(|(code, _)| *code == 2634);
    assert!(
        hit.is_some_and(|(_, m)| m.contains("'string' index signatures are incompatible")),
        "an index-signature-to-index-signature failure must stay TS2634, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|(code, _)| *code == 2530),
        "must not emit TS2530 when no named property is involved, got: {messages:?}"
    );
}

#[test]
fn number_index_property_vs_index_failure_is_also_ts2530() {
    let source = r#"
type NumDict = { [k: number]: string };
declare const s: { 1: number };
const d: NumDict = s;
"#;
    let messages = codes_and_messages_with_elaborations(source);
    assert!(
        messages
            .iter()
            .any(|(code, m)| *code == 2530 && m.contains("'1'")),
        "a number-index property mismatch must also report TS2530, got: {messages:?}"
    );
}
