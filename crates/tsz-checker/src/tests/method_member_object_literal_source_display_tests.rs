//! Source-side display of a fresh object literal that carries a method or
//! accessor member under a written (non-computed) property name.
//!
//! Structural rule, oracled against `typescript@7.0.2` (`--strict`): the
//! head/source render of a `TS2322`/`TS2345`/`TS2741` keeps every sibling
//! property's contextual literal preservation even when a method or accessor
//! member is present — `Type '{ kind: "a"; f(): number; }' ...` — with the
//! method rendered shorthand-style from the literal's own checked type and a
//! get-only accessor rendered `readonly`. An accessor's value type shows the
//! inference-widened form (`readonly q: number` for `get q() { return 1 }`,
//! `b: boolean` for a `true`-returning getter beside a `boolean` setter),
//! because tsc widens accessor-inferred literals at the accessor itself.
//!
//! tsz previously bailed the whole syntax-driven renderer on any
//! non-computed-key method/accessor member, so the fallback widened the
//! sibling literals too (`{ kind: string; f(): number; }`).
//!
//! Owner: `object_literal_source_type_display`
//! (`error_reporter/core/diagnostic_source/object_literal_source_display.rs`)
//! with the member itself rendered by the solver printer
//! (`TypeFormatter::format_object_type_property`). Binder names vary across
//! cases so no identifier string is load-bearing.

use crate::test_utils::check_source_strict_messages;

fn message_for(source: &str, code: u32) -> Option<String> {
    check_source_strict_messages(source)
        .into_iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message)
}

// ---------------------------------------------------------------------------
// Method member beside a literal sibling: concrete target, TS2741 head.
// ---------------------------------------------------------------------------

#[test]
fn method_member_keeps_sibling_literal_in_missing_property_head() {
    let source = r#"
type Payload = { kind: "a"; f(): number; z: string };
const payload: Payload = { kind: "a", f() { return 1; } };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains(r#"'{ kind: "a"; f(): number; }'"#),
        "sibling literal must stay preserved beside a method member: {message}"
    );
}

#[test]
fn renamed_binders_method_member_keeps_sibling_literal() {
    let source = r#"
type Envelope = { stamp: 7; deliver(): string; route: string };
const parcel: Envelope = { stamp: 7, deliver() { return "x"; } };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains("'{ stamp: 7; deliver(): string; }'"),
        "numeric sibling literal must stay preserved beside a method member: {message}"
    );
}

// ---------------------------------------------------------------------------
// Method member in a fresh union source: TS2322 head keeps both renders.
// ---------------------------------------------------------------------------

#[test]
fn union_target_head_keeps_literal_and_method_shorthand() {
    let source = r#"
type Msg = { tag: "go"; run(): number } | { tag: "stop"; run(): string };
const msg: Msg = { tag: "go", run() { return "oops"; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the union mismatch");
    assert!(
        message.contains(r#"'{ tag: "go"; run(): string; }'"#),
        "union head must keep the discriminant literal and the method's own checked return: {message}"
    );
}

#[test]
fn shorthand_sibling_beside_method_keeps_boolean_literal() {
    let source = r#"
type Toggle = { on: true; poke(): number } | { on: false; poke(): string };
declare const on: true;
const t: Toggle = { on, poke() { return "s"; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the union mismatch");
    assert!(
        message.contains("'{ on: true; poke(): string; }'"),
        "shorthand boolean literal must stay preserved beside a method member: {message}"
    );
}

// ---------------------------------------------------------------------------
// Accessors: get-only renders readonly + inference-widened value type;
// setter-only renders plain; a boolean-literal getter widens to boolean.
// ---------------------------------------------------------------------------

#[test]
fn get_only_accessor_renders_readonly_with_widened_value() {
    let source = r#"
type Cfg = { mode: "fast"; level: number; label: string };
const cfg: Cfg = { mode: "fast", get level() { return 3; } };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains(r#"'{ mode: "fast"; readonly level: number; }'"#),
        "get-only accessor must render readonly with the widened value type: {message}"
    );
}

#[test]
fn setter_only_accessor_renders_plain_property() {
    let source = r#"
type Sink = { name: "w"; depth: number; owner: string };
const sink: Sink = { name: "w", set depth(next: number) {} };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains(r#"'{ name: "w"; depth: number; }'"#),
        "setter-only accessor must render as a plain property: {message}"
    );
}

#[test]
fn boolean_getter_beside_boolean_setter_widens_to_boolean() {
    let source = r#"
type Flagged = { key: "on"; live: boolean; note: string };
const flagged: Flagged = { key: "on", get live() { return true; }, set live(v: boolean) {} };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains(r#"'{ key: "on"; live: boolean; }'"#),
        "a true-returning getter beside a boolean setter must widen to boolean: {message}"
    );
}

#[test]
fn union_target_getter_head_keeps_discriminant_literal() {
    let source = r#"
type Pair = { axis: "x"; v: number; z: string } | { axis: "y"; v: string };
const pair: Pair = { axis: "x", get v() { return 3; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the union mismatch");
    assert!(
        message.contains(r#"'{ axis: "x"; readonly v: number; }'"#),
        "union head must keep the discriminant literal beside a getter member: {message}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback cases: what must NOT change.
// ---------------------------------------------------------------------------

#[test]
fn sibling_literal_still_widens_when_target_property_is_wide() {
    // The contextual acceptance test still governs siblings: a numeric literal
    // against a plain `number` target property widens in the head (tsc
    // renders `n: number` there — bool_rule D3 in the oracle set).
    let source = r#"
type Loose = { n: number; m(): number; z: string };
const loose: Loose = { n: 5, m() { return 1; } };
"#;
    let message = message_for(source, 2741).expect("TS2741 for the missing property");
    assert!(
        message.contains("'{ n: number; m(): number; }'"),
        "a literal no target property pins must still widen beside a method: {message}"
    );
}

#[test]
fn wide_computed_key_method_group_still_folds_into_index_clause() {
    // The pre-existing #16662/#16721 path: a wide, non-entity computed key
    // still takes the synthesized index-signature clause, not the new named
    // member render.
    let source = r#"
declare const wsKey: string;
interface Table { [slot: string]: string }
const table: Table = { [wsKey.toUpperCase()]() { return 1; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [x: string]: () => number; }"),
        "non-entity wide-key methods must keep the synthesized index clause: {message}"
    );
}
