//! Regression suite for the literal-freshness query boundary (#15390).
//!
//! `tsc` widens a fresh literal only at mutable-binding observation points and
//! keeps the literal everywhere else (non-fresh sources, `readonly`/`const`
//! bindings, diagnostic display). tsz owns those timing rules in
//! `crates/tsz-checker/src/types/utilities/fresh_literal.rs`; this suite pins
//! the rules across the migrated consumers with renamed binders, enum and
//! primitive forms, and positive/negative pairs so per-site drift (the bug
//! class behind #15366/#15373) is caught structurally.

use crate::test_utils::check_source_strict_messages as check_strict;

// ---------------------------------------------------------------------------
// Mutable variable bindings: fresh literal initializers widen.
// ---------------------------------------------------------------------------

/// `let` with a fresh string literal widens to `string`.
#[test]
fn let_fresh_string_literal_widens() {
    let diags = check_strict(
        r#"
let greeting = "hello";
greeting = "goodbye";
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// `let` initialized from an *annotated* const keeps the non-fresh literal.
#[test]
fn let_from_annotated_const_stays_literal() {
    let diags = check_strict(
        r#"
const fixed: "on" = "on";
let toggle = fixed;
toggle = "off";
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("\"on\"")),
        "expected TS2322 against '\"on\"' — `toggle` keeps the non-fresh literal. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// An unannotated const literal is a widening literal type: copying it into a
/// mutable binding widens (`tsc`'s fresh-by-reference rule).
#[test]
fn let_from_unannotated_const_widens() {
    let diags = check_strict(
        r#"
const tag = "start";
let phase = tag;
phase = "stop";
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

// ---------------------------------------------------------------------------
// Enum members observe the same freshness rules as primitive literals
// (#15445): a *direct* member access mints a fresh enum literal that widens
// to the parent enum at mutable observation points; non-fresh sources —
// annotated const references, property reads, call results — keep the
// member type. Consts keep the member type either way.
// ---------------------------------------------------------------------------

/// `let m = E.A` widens to `E`, so assigning another member is fine.
#[test]
fn let_enum_member_initializer_widens_to_enum() {
    let diags = check_strict(
        r#"
enum Signal { Go, Stop }
let current = Signal.Go;
current = Signal.Stop;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// `const c = E.A` keeps the member type: `typeof c` rejects other members.
#[test]
fn const_enum_member_initializer_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Signal { Go, Stop }
const pinned = Signal.Go;
let other: typeof pinned = Signal.Stop;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Signal.Go")),
        "expected TS2322 against `Signal.Go` — `typeof pinned` keeps the member type. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// An *annotated* enum-member const reference is non-fresh: copying it into
/// a `let` keeps `E.A`, and assigning another member errors (#15445; tsc
/// gates the enum arm on freshness exactly like primitive literals).
#[test]
fn annotated_enum_const_reference_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Level { Low, High }
const pinnedLevel: Level.Low = Level.Low;
let cursor = pinnedLevel;
cursor = Level.High;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Level.Low")),
        "expected TS2322 against `Level.Low` — `cursor` keeps the non-fresh member type. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// An *unannotated* const enum-member reference is fresh-by-reference, like
/// the primitive const-chain rule: copying it into a `let` widens to the
/// parent enum.
#[test]
fn unannotated_const_enum_member_chain_widens() {
    let diags = check_strict(
        r#"
enum Gear { First, Second }
const seed = Gear.First;
let active = seed;
active = Gear.Second;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// A property read from a value merely *typed* as an enum member is
/// non-fresh: the binding keeps `E.A` and assigning another member errors.
#[test]
fn property_read_of_enum_member_typed_object_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Mode { Idle, Busy }
declare const box: { current: Mode.Idle };
let snapshot = box.current;
snapshot = Mode.Busy;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Mode.Idle")),
        "expected TS2322 against `Mode.Idle` — a property read is non-fresh. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// A call result typed as an enum member is non-fresh: the binding keeps
/// `E.A`.
#[test]
fn call_result_enum_member_keeps_member_type() {
    let diags = check_strict(
        r#"
enum State { On, Off }
declare function fetchState(): State.On;
let latest = fetchState();
latest = State.Off;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("State.On")),
        "expected TS2322 against `State.On` — a call result is non-fresh. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// String-keyed element access (`E["A"]`) is a direct member access and
/// widens exactly like `E.A`.
#[test]
fn element_access_enum_member_initializer_widens() {
    let diags = check_strict(
        r#"
enum Tone { Soft, Loud }
let volume = Tone["Soft"];
volume = Tone.Loud;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// A namespace-qualified member access (`ns.E.A`) resolves through the
/// export chain and widens like a top-level `E.A`.
#[test]
fn namespace_qualified_enum_member_initializer_widens() {
    let diags = check_strict(
        r#"
namespace audio { export enum Level { Min, Max } }
let gain = audio.Level.Min;
gain = audio.Level.Max;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// `const enum` member accesses observe the same freshness rules as regular
/// enums: a direct access widens to the parent enum.
#[test]
fn const_enum_member_initializer_widens_to_enum() {
    let diags = check_strict(
        r#"
const enum Speedo { Still, Moving }
let pace = Speedo.Still;
pace = Speedo.Moving;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// A conditional over direct member accesses is fresh through both arms and
/// widens at the binding.
#[test]
fn conditional_enum_member_initializer_widens() {
    let diags = check_strict(
        r#"
enum Route { North, South }
declare const forked: boolean;
let heading = forked ? Route.North : Route.South;
heading = Route.North;
heading = Route.South;
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// String enums follow the same freshness gate: direct access widens,
/// annotated const references keep the member type.
#[test]
fn string_enum_freshness_gate_matches_numeric() {
    let diags = check_strict(
        r#"
enum Chan { Email = "email", Sms = "sms" }
let route = Chan.Email;
route = Chan.Sms;
const fixedChan: Chan.Email = Chan.Email;
let bound = fixedChan;
bound = Chan.Sms;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Chan.Email")),
        "expected TS2322 against `Chan.Email` on the non-fresh write. Got: {diags:?}"
    );
    assert_eq!(
        diags.len(),
        1,
        "the fresh binding must widen (no error on `route`): {diags:?}"
    );
}

/// A type assertion strips freshness (tsc's `isTypeAssertion` carve-out):
/// `E.A as E.A` keeps the member type at a mutable binding.
#[test]
fn asserted_enum_member_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Phase { Boot, Ready }
let stage = Phase.Boot as Phase.Boot;
stage = Phase.Ready;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Phase.Boot")),
        "expected TS2322 against `Phase.Boot` — assertions are non-fresh. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// A non-null assertion preserves the operand's freshness: `E.A!` widens
/// like a bare `E.A`, and a fresh string literal behind `!` widens to
/// `string`.
#[test]
fn non_null_assertion_preserves_freshness() {
    let diags = check_strict(
        r#"
enum Slot { Head, Tail }
let chosen = Slot.Head!;
chosen = Slot.Tail;
let word = "start"!;
word = "stop";
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

// ---------------------------------------------------------------------------
// Return-position widening observes the same enum freshness gate.
// ---------------------------------------------------------------------------

/// A returned direct member access widens the inferred return type to the
/// parent enum.
#[test]
fn return_direct_enum_member_widens_to_enum() {
    let diags = check_strict(
        r#"
enum Flag { No, Yes }
function grab() {
    return Flag.No;
}
declare function wantNo(value: Flag.No): void;
wantNo(grab());
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "expected TS2345 — `grab` returns the widened `Flag`, not `Flag.No`. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

/// A returned annotated const reference is non-fresh: the inferred return
/// type keeps the member type (#15445's second consumer).
#[test]
fn return_annotated_const_reference_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Kind { Alpha, Beta }
function pick() {
    const chosen: Kind.Alpha = Kind.Alpha;
    return chosen;
}
let narrow: Kind.Alpha = pick();
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics — `pick` keeps the `Kind.Alpha` return type: {diags:?}"
    );
}

/// A parameter default of an enum member widens the parameter to the enum.
#[test]
fn parameter_enum_member_default_widens_to_enum() {
    let diags = check_strict(
        r#"
enum Hue { Red, Green }
function paint(shade = Hue.Red) {
    shade = Hue.Green;
}
paint();
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// A parameter default of a fresh literal widens; a later narrower write is
/// accepted, a cross-primitive write is not.
#[test]
fn parameter_fresh_literal_default_widens() {
    let diags = check_strict(
        r#"
function step(count = 1) {
    count = 2;
    count = "two";
}
step();
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("'number'")),
        "expected the string-to-number TS2322 on the '\"two\"' write. Got: {diags:?}"
    );
    assert_eq!(
        diags.len(),
        1,
        "the numeric re-assignment must be accepted (widened to number): {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Class properties: fresh initializers widen unless readonly; non-fresh
// identifier references keep their declared type.
// ---------------------------------------------------------------------------

/// Mutable class property with a fresh literal widens.
#[test]
fn class_property_fresh_literal_widens() {
    let diags = check_strict(
        r#"
class Panel {
    label = "a";
}
new Panel().label = "b";
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// `readonly` class property keeps the fresh literal unwidened.
#[test]
fn readonly_class_property_keeps_literal() {
    let diags = check_strict(
        r#"
class Panel {
    readonly kind = "panel";
}
declare function expectPanelKind(value: "panel"): void;
expectPanelKind(new Panel().kind);
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// Class property initialized from an annotated const is non-fresh: the
/// declared union survives instead of widening to the primitive.
#[test]
fn class_property_from_annotated_const_keeps_declared_type() {
    let diags = check_strict(
        r#"
type Mode = "on" | "off";
const DEFAULT_MODE: Mode = "on";
class Toggle {
    mode = DEFAULT_MODE;
}
const t = new Toggle();
t.mode = "off";
t.mode = "auto";
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "expected TS2322 for the out-of-union write. Got: {diags:?}"
    );
    assert_eq!(
        diags.len(),
        1,
        "the in-union write must be accepted (`mode` stays `Mode`): {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Display recovery: diagnostics show the operand's own unwidened literal.
// ---------------------------------------------------------------------------

/// A binding-element default keeps its literal in the TS2322 message.
#[test]
fn binding_default_display_uses_unwidened_literal() {
    let diags = check_strict(
        r#"
type Label = "alpha" | "beta";
function pick({ label = "gamma" }: { label?: Label }) {
    return label;
}
pick({});
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("\"gamma\"")),
        "expected TS2322 mentioning the unwidened literal '\"gamma\"'. Got: {diags:?}"
    );
}

/// An argument-mismatch message keeps a literal argument unwidened.
#[test]
fn argument_display_uses_unwidened_literal() {
    let diags = check_strict(
        r#"
declare function acceptsUnion(flag: 1 | 2): void;
acceptsUnion(3);
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2345 && msg.contains("'3'")),
        "expected TS2345 mentioning the literal '3'. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Parameter defaults observe the enum freshness gate like variable bindings.
// ---------------------------------------------------------------------------

/// A parameter default referencing an annotated enum-member const is
/// non-fresh: the parameter keeps `E.A` and a cross-member write errors.
#[test]
fn parameter_default_annotated_const_reference_keeps_member_type() {
    let diags = check_strict(
        r#"
enum Speed { Slow, Fast }
const baseline: Speed.Slow = Speed.Slow;
function throttle(rate = baseline) {
    rate = Speed.Fast;
}
throttle();
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("Speed.Slow")),
        "expected TS2322 against `Speed.Slow` — the default is non-fresh. Got: {diags:?}"
    );
    assert_eq!(diags.len(), 1, "no other diagnostics expected: {diags:?}");
}

// ---------------------------------------------------------------------------
// Imported enums resolve through the alias to the same freshness rules.
// ---------------------------------------------------------------------------

/// A member access on an imported enum is a direct access and widens.
#[test]
fn imported_enum_member_initializer_widens() {
    let diags = crate::test_utils::check_multi_file(
        &[
            (
                "remote.ts",
                r#"
export enum Remote { Zero, One }
"#,
            ),
            (
                "main.ts",
                r#"
import { Remote } from "./remote";
let picked = Remote.Zero;
picked = Remote.One;
"#,
            ),
        ],
        "main.ts",
        crate::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected no TS2322 — the imported enum member widens to `Remote`: {diags:?}"
    );
}
