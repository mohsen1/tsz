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
        diags.iter().any(|(code, _)| *code == 2322),
        "expected TS2322 — `toggle` keeps non-fresh type '\"on\"'. Got: {diags:?}"
    );
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
// Enum members widen at mutable-binding observation points even though the
// initializer is not an AST literal. Consts keep the member type.
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
        diags.iter().any(|(code, _)| *code == 2322),
        "expected TS2322 — `typeof pinned` is `Signal.Go`. Got: {diags:?}"
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
        diags.iter().any(|(code, _)| *code == 2322),
        "expected exactly the string-to-number TS2322. Got: {diags:?}"
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
