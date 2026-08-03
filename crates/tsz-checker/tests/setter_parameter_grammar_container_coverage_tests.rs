//! The `set`-accessor *parameter grammar* family (`TS1052`/`TS1053`) for every
//! container a `set` accessor can be written in.
//!
//! Structural rule, one sentence: a `set` accessor's parameter may not carry an
//! initializer (`TS1052`) and may not be a rest parameter (`TS1053`), and `tsc`
//! decides both from the parameter node alone — so the rule holds identically
//! in a class body, an object literal, an interface and a type literal. tsz
//! reports both through the accessor checker's parameter-grammar layer,
//! `check_setter_parameter_grammar` (`checkers/accessor_checker.rs`).
//!
//! Before this suite the rule was reported for **class members only**. Its one
//! call site was `check_setter_parameter`, reached from
//! `check_accessor_declaration_with_request`, which is itself reached only from
//! the class member walk (`member_declaration_checks.rs`). Object literals
//! route accessors through `types/computation/object_literal/accessor_element.rs`
//! and interface / type-literal members through the type-member walk; neither
//! path saw the rule, so all three containers silently accepted both shapes.
//!
//! Two properties of the family are load-bearing and each is pinned below:
//!
//! 1. The two arms are **independent**, not a match. `TS1052` and `TS1053` can
//!    both be live in one member list, and they anchor differently — `TS1052`
//!    on the accessor's *name*, `TS1053` on the `...` token. A shared anchor,
//!    or an early return after the first arm, fails the `obj_both` row and
//!    nothing else.
//! 2. It is the parameter's *shape*, not the container and not the parameter's
//!    name or type. Every legal-setter negative below is the same container and
//!    the same binder shape as its positive, so a check that fired on the
//!    presence of a `set` accessor rather than on the parameter fails all five.
//!
//! Every expectation here was recorded from `typescript@7.0.2` under
//! `--noEmit --strict --lib es2022 --target es2022`, not derived from the rule.
//! Anchors are pinned as 0-based offsets (`tsc`'s 1-based column minus one)
//! because the blame *site* is half of this family. Binder names are distinct
//! in every row so nothing can key on an identifier string.

use tsz_checker::test_utils::check_source_strict;

/// Every diagnostic as `TS<code>@<0-based start>`, sorted — the exact shape the
/// oracle rows were recorded in.
fn sites(source: &str) -> Vec<String> {
    let mut out: Vec<String> = check_source_strict(source)
        .iter()
        .map(|d| format!("TS{}@{}", d.code, d.start))
        .collect();
    out.sort();
    out
}

#[track_caller]
fn assert_sites(source: &str, expected: &[&str]) {
    let actual = sites(source);
    let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(actual, expected, "source: {source}");
}

// ---------------------------------------------------------------------------
// Object-literal container. Previously silent on both arms.
// ---------------------------------------------------------------------------

#[test]
fn object_literal_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "const holder = {\n    set alpha(...spread: string[]) {},\n};\n",
        &["TS1053@31"],
    );
}

#[test]
fn object_literal_setter_parameter_initializer_reports_ts1052() {
    assert_sites(
        "const carrier = {\n    set beta(weight: string = \"x\") {},\n};\n",
        &["TS1052@26"],
    );
}

/// Both arms live in one object literal, on different members. `TS1053` anchors
/// at the `...` token, `TS1052` at the accessor name — the row that proves the
/// arms are independent and do not share an anchor.
#[test]
fn object_literal_reports_both_arms_at_their_own_anchors() {
    assert_sites(
        "const dual = {\n    set gamma(...pile: string[]) {},\n    set delta(mass: number = 2) {},\n};\n",
        &["TS1052@60", "TS1053@29"],
    );
}

/// A nested object literal is a distinct traversal from the outermost one.
#[test]
fn nested_object_literal_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "const outerBag = {\n    innerBag: {\n        set epsilon(...heap: boolean[]) {},\n    },\n};\n",
        &["TS1053@55"],
    );
}

/// An object literal in a class method's return position: the enclosing class
/// walk runs, but the literal is not a class member. Pins that the fix routes
/// through the object-literal path and is not being masked by the class one.
#[test]
fn object_literal_returned_from_class_method_reports_ts1053() {
    assert_sites(
        "class Vessel {\n    build() {\n        return { set zeta(...batch: number[]) {} };\n    }\n}\n",
        &["TS1053@55"],
    );
}

#[test]
fn object_literal_legal_setter_stays_clean() {
    assert_sites("const clean = {\n    set eta(value: string) {},\n};\n", &[]);
}

// ---------------------------------------------------------------------------
// Interface container. Previously silent.
// ---------------------------------------------------------------------------

#[test]
fn interface_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "interface Portal {\n    set theta(...stack: string[]);\n}\n",
        &["TS1053@33"],
    );
}

#[test]
fn interface_legal_setter_stays_clean() {
    assert_sites("interface Gate {\n    set iota(value: string);\n}\n", &[]);
}

// ---------------------------------------------------------------------------
// Type-literal container. Previously silent. Shares the type-member walk with
// interfaces, so both are pinned rather than one standing in for the other.
// ---------------------------------------------------------------------------

#[test]
fn type_literal_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "type Panel = {\n    set kappa(...queue: number[]);\n};\n",
        &["TS1053@29"],
    );
}

#[test]
fn type_literal_legal_setter_stays_clean() {
    assert_sites(
        "type Frame = {\n    set lambdaProp(value: number);\n};\n",
        &[],
    );
}

// ---------------------------------------------------------------------------
// Generic containers: the rule reads the parameter node, so a type-parameter
// element type changes nothing. Both the class and the type-member arm.
// ---------------------------------------------------------------------------

#[test]
fn generic_class_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "class Crate<T> {\n    set rho(...units: T[]) {}\n}\n",
        &["TS1053@29"],
    );
}

#[test]
fn generic_type_literal_setter_rest_parameter_reports_ts1053() {
    assert_sites(
        "type Sack<T> = {\n    set sigma(...units: T[]);\n};\n",
        &["TS1053@31"],
    );
}

// ---------------------------------------------------------------------------
// Class container: already covered before this change. These are the
// regression rows — the extraction must leave the class arm byte-identical.
// ---------------------------------------------------------------------------

#[test]
fn class_setter_rest_parameter_still_reports_ts1053() {
    assert_sites(
        "class Anchor {\n    set mu(...chunks: string[]) {}\n}\n",
        &["TS1053@26"],
    );
}

#[test]
fn class_setter_parameter_initializer_still_reports_ts1052() {
    assert_sites(
        "class Beacon {\n    set nu(level: number = 3) {}\n}\n",
        &["TS1052@23"],
    );
}

#[test]
fn static_class_setter_rest_parameter_still_reports_ts1053() {
    assert_sites(
        "class Compass {\n    static set xi(...marks: boolean[]) {}\n}\n",
        &["TS1053@34"],
    );
}

#[test]
fn ambient_class_setter_rest_parameter_still_reports_ts1053() {
    assert_sites(
        "declare class Derrick {\n    set omicron(...rods: string[]);\n}\n",
        &["TS1053@40"],
    );
}

#[test]
fn class_legal_setter_stays_clean() {
    assert_sites("class Ledger {\n    set pi(value: string) {}\n}\n", &[]);
}

// ---------------------------------------------------------------------------
// The early-return chain, and why the two arms must stand down.
//
// `tsc`'s `checkGrammarAccessor` reports AT MOST ONE diagnostic per accessor —
// it is a chain of early returns, in this order:
//
//     TS1094 (accessor has type parameters)
//       -> TS1049 (value-parameter count is not exactly 1)
//         -> TS1095 (setter has a return type annotation)
//           -> TS1053 (rest parameter)
//             -> TS1051 (optional parameter)
//               -> TS1052 (parameter initializer)
//
// tsz splits that chain across two layers: TS1094/TS1049/TS1095/TS1051 are
// emitted by the PARSER, TS1053/TS1052 by this checker. So the ordering cannot
// be a local early return inside either one — the checker has to re-test the
// earlier links' conditions before emitting, which is what
// `check_setter_parameter_grammar` does.
//
// `check_source_strict` surfaces CHECKER diagnostics only; the parser's codes
// are not in its output at all (see `check_source_with_parse_health`'s doc
// comment). That makes it the exact instrument for this property and no other:
// an empty result here means the checker's two arms correctly stood down and
// left the parser's single diagnostic as the whole answer. A non-empty result
// is tsz about to emit a second, duplicate grammar error that `tsc` never
// reports. The `tsc` column below is the full oracle output for the same
// source, recorded from `typescript@7.0.2`, and every row is a case where the
// suppressed code is one of this file's own two.
// ---------------------------------------------------------------------------

/// `tsc`: `TS1095` alone. Without the row-3 guard tsz adds `TS1053` beside it.
#[test]
fn return_type_annotation_stands_down_the_rest_arm() {
    assert_sites(
        "class Kappa {\n    set aa(...v: string[]): void {}\n}\n",
        &[],
    );
    assert_sites(
        "interface IfaceA {\n    set cc(...v: string[]): void;\n}\n",
        &[],
    );
}

/// `tsc`: `TS1095` alone. Without the row-3 guard tsz adds `TS1052` beside it.
#[test]
fn return_type_annotation_stands_down_the_initializer_arm() {
    assert_sites(
        "class Mu {\n    set gg(v: string = \"z\"): void {}\n}\n",
        &[],
    );
}

/// KNOWN DIVERGENCE, pinned deliberately — the object-literal container cannot
/// reach the row-3 guard, and the cause is upstream of this file.
///
/// `parse_object_set_accessor`
/// (`tsz-parser/src/parser/state_expressions_literals/object_members.rs`) parses
/// the return type and **throws it away** — `let _ = self.parse_return_type();`
/// — then builds the node with `type_annotation: NodeIndex::NONE` hard-coded.
/// So an object-literal setter's return annotation is invisible to every
/// checker-side consumer, this guard included. That is the same hard-coded-empty
/// defect #16276 fixed for interface and type-literal accessors, in the one
/// container it did not cover.
///
/// The oracle reports `TS1095` alone for both rows. tsz reports `TS1095` from
/// the parser plus the code below from the checker. The shape needs a setter
/// that is *already* ill-formed (an explicit return type) to reach, so it is
/// strictly narrower than the family this file fixes — but it is a real
/// divergence and is pinned rather than left silent. Restoring the annotation
/// in the parser makes both rows go to `&[]` with no change to this file's
/// logic; when that lands, this test fails and should be folded into the two
/// above.
#[test]
fn object_literal_return_type_annotation_is_invisible_to_the_guard() {
    assert_sites(
        "const objA = {\n    set bb(...v: string[]): void {},\n};\n",
        &["TS1053@26"],
    );
    assert_sites(
        "const objC = {\n    set hh(v: string = \"z\"): void {},\n};\n",
        &["TS1052@23"],
    );
}

/// `tsc`: `TS1049` alone. Without the row-2 guard tsz adds `TS1053` beside it.
#[test]
fn wrong_value_parameter_count_stands_down_the_rest_arm_in_every_container() {
    assert_sites(
        "class Lambda {\n    set dd(first: string, ...rest: string[]) {}\n}\n",
        &[],
    );
    assert_sites(
        "const objB = {\n    set ee(first: string, ...rest: string[]) {},\n};\n",
        &[],
    );
    assert_sites(
        "interface IfaceB {\n    set ff(first: string, ...rest: string[]);\n}\n",
        &[],
    );
}

/// `tsc`: `TS1094` alone.
#[test]
fn accessor_type_parameters_stand_down_the_rest_arm() {
    assert_sites("class Rho {\n    set nn<T>(...v: string[]) {}\n}\n", &[]);
}

/// The row-2 count excludes a leading `this` parameter, so this is a
/// ONE-value-parameter setter and the rest arm still fires. `tsc`:
/// `TS1053` at the `...`, plus a `TS2784` this-parameter error that this
/// harness does not carry. The counter-test to the three rows above: a guard
/// that counted declared parameters rather than value parameters would return
/// early here and report nothing.
#[test]
fn leading_this_parameter_does_not_count_toward_the_value_parameter_count() {
    assert_sites(
        "class Sigma {\n    set oo(this: Sigma, ...v: string[]) {}\n}\n",
        &["TS1053@38"],
    );
    assert_sites(
        "class Ups {\n    set qq(this: Ups, v: string = \"k\") {}\n}\n",
        &["TS1052@20"],
    );
}
