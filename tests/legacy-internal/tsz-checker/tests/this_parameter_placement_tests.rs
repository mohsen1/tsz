//! Regression tests for the `this`-parameter placement family: TS2680, TS2681,
//! TS2784 and the TypeScript-syntax half of TS2730.
//!
//! `tsc` decides all four in `checkParameter` from two structural facts and
//! nothing else — the parameter's index in its own list, and the `SyntaxKind`
//! of the container. Three of the four codes existed in tsz's generated message
//! table with zero call sites anywhere, so a `this` parameter in an illegal
//! position or an illegal container was silently accepted; TS2730 was wired for
//! JS files only, through the JSDoc `@this` tag, and never for a real `this`
//! parameter node in a TypeScript arrow function.
//!
//! The arms are independent rather than a match over the container kind:
//! a `this` parameter that is both misplaced *and* in a constructing container
//! draws TS2680 *and* TS2681, and the `both_arms_*` rows below pin that.
//!
//! Every binder name is distinct per row so nothing can key on an identifier
//! string, and each positive shape is paired with the same shape holding a
//! *legal* leading `this` parameter, so a check that fired on the mere presence
//! of a `this` parameter would fail the negatives.

use crate::test_utils::check_source_strict_messages;

fn codes(source: &str) -> Vec<u32> {
    let mut found: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    found.sort_unstable();
    found
}

#[track_caller]
fn assert_reports(source: &str, expected: &[u32]) {
    let found = codes(source);
    let mut want = expected.to_vec();
    want.sort_unstable();
    let relevant: Vec<u32> = found
        .iter()
        .copied()
        .filter(|c| matches!(c, 2680 | 2681 | 2730 | 2784))
        .collect();
    assert_eq!(
        relevant, want,
        "this-parameter diagnostics for source:\n{source}\nall codes: {found:?}"
    );
}

#[track_caller]
fn assert_clean(source: &str) {
    assert_reports(source, &[]);
}

// ---------------------------------------------------------------------------
// TS2680 — a `this` parameter that is not the first parameter
// ---------------------------------------------------------------------------

#[test]
fn ts2680_function_declaration_this_parameter_after_another() {
    assert_reports("function fa20(xa20: number, this: string) {}", &[2680]);
}

#[test]
fn ts2680_class_method_this_parameter_after_another() {
    assert_reports("class Ca20 { ma20(ya20: number, this: Ca20) {} }", &[2680]);
}

#[test]
fn ts2680_interface_method_signature_this_parameter_after_another() {
    assert_reports(
        "interface Ia20 { mb20(za20: number, this: Ia20): void; }",
        &[2680],
    );
}

#[test]
fn ts2680_function_type_node_this_parameter_after_another() {
    assert_reports("type Ta20 = (wa20: number, this: string) => void;", &[2680]);
}

#[test]
fn ts2680_function_expression_this_parameter_after_another() {
    assert_reports(
        "const ka20 = function (va20: number, this: string) {};",
        &[2680],
    );
}

#[test]
fn ts2680_third_position_still_reports_once() {
    assert_reports(
        "function fb20(ua20: number, ta20: string, this: symbol) {}",
        &[2680],
    );
}

// ---------------------------------------------------------------------------
// TS2681 — constructing containers: Constructor, ConstructSignature,
// ConstructorType. All three construct, so none has a `this` to annotate.
// ---------------------------------------------------------------------------

#[test]
fn ts2681_class_constructor() {
    assert_reports("class Cb20 { constructor(this: Cb20) {} }", &[2681]);
}

#[test]
fn ts2681_interface_construct_signature() {
    assert_reports("interface Ib20 { new (this: Ib20): Ib20; }", &[2681]);
}

#[test]
fn ts2681_constructor_type_node() {
    assert_reports("type Tb20 = new (this: string) => string;", &[2681]);
}

#[test]
fn ts2681_ambient_class_constructor() {
    assert_reports("declare class Cc20 { constructor(this: Cc20); }", &[2681]);
}

#[test]
fn ts2681_type_literal_construct_signature() {
    assert_reports("type Tc20 = { new (this: string): string };", &[2681]);
}

// ---------------------------------------------------------------------------
// TS2784 — get/set accessors
// ---------------------------------------------------------------------------

#[test]
fn ts2784_class_getter() {
    assert_reports("class Cd20 { get pa20(this: Cd20) { return 1; } }", &[2784]);
}

#[test]
fn ts2784_class_setter() {
    assert_reports(
        "class Ce20 { set pb20(this: Ce20, ra20: number) {} }",
        &[2784],
    );
}

#[test]
fn ts2784_static_class_getter() {
    assert_reports(
        "class Cf20 { static get pc20(this: typeof Cf20) { return 1; } }",
        &[2784],
    );
}

#[test]
fn ts2784_ambient_class_getter() {
    assert_reports(
        "declare class Cg20 { get pd20(this: Cg20): number; }",
        &[2784],
    );
}

// ---------------------------------------------------------------------------
// TS2730 — an arrow function's `this` is lexical, so it cannot declare one.
// The pre-existing JS/JSDoc `@this`-tag arm triggers on a tag with no parameter
// node, so these rows exercise a path that arm never reached.
// ---------------------------------------------------------------------------

#[test]
fn ts2730_arrow_function_this_parameter() {
    assert_reports("const aa20 = (this: string, qa20: number) => {};", &[2730]);
}

#[test]
fn ts2730_class_property_arrow_this_parameter() {
    assert_reports("class Ch20 { pf20 = (this: Ch20) => {}; }", &[2730]);
}

// ---------------------------------------------------------------------------
// Independent arms — a misplaced `this` in an illegal container draws both.
// A `match` over the container kind, or an early return after the position
// arm, fails exactly these rows.
// ---------------------------------------------------------------------------

#[test]
fn both_arms_constructor_with_misplaced_this() {
    assert_reports(
        "class Ci20 { constructor(pa21: number, this: Ci20) {} }",
        &[2680, 2681],
    );
}

#[test]
fn both_arms_setter_with_misplaced_this() {
    assert_reports(
        "class Cj20 { set pg20(oa20: number, this: Cj20) {} }",
        &[2680, 2784],
    );
}

#[test]
fn both_arms_arrow_with_misplaced_this() {
    assert_reports(
        "const ba20 = (na20: number, this: string) => {};",
        &[2680, 2730],
    );
}

#[test]
fn both_arms_constructor_type_with_misplaced_this() {
    assert_reports(
        "type Td20 = new (ma20: number, this: string) => string;",
        &[2680, 2681],
    );
}

// ---------------------------------------------------------------------------
// Negatives — a legal leading `this` parameter in every container that allows
// one. These are the same shapes as the positives above; a check that fired on
// the presence of a `this` parameter rather than on its position and container
// fails all of them.
// ---------------------------------------------------------------------------

#[test]
fn legal_leading_this_in_function_declaration() {
    assert_clean("function fc20(this: string, la20: number) {}");
}

#[test]
fn legal_leading_this_in_class_method() {
    assert_clean("class Ck20 { mc20(this: Ck20, ka21: number) {} }");
}

#[test]
fn legal_leading_this_in_static_class_method() {
    assert_clean("class Cl20 { static md20(this: typeof Cl20) {} }");
}

#[test]
fn legal_leading_this_in_ambient_function() {
    assert_clean("declare function fd20(this: void): void;");
}

#[test]
fn legal_leading_this_in_function_type_node() {
    assert_clean("type Te20 = (this: string, ja20: number) => void;");
}

#[test]
fn legal_leading_this_in_interface_method_signature() {
    assert_clean("interface Ic20 { me20(this: Ic20): void; }");
}

#[test]
fn legal_leading_this_in_interface_call_signature() {
    assert_clean("interface Id20 { (this: Id20, ia20: number): void; }");
}

#[test]
fn legal_leading_this_in_function_expression() {
    assert_clean("const ca20 = function (this: string) {};");
}

#[test]
fn legal_leading_this_in_namespace_function() {
    assert_clean("namespace Na20 { export function fe20(this: string) {} }");
}

#[test]
fn legal_leading_this_in_object_literal_method() {
    assert_clean("const da20 = { mf20(this: { mf20(): void }) {} };");
}

// ---------------------------------------------------------------------------
// Negatives — no `this` parameter at all, in the same containers. Guards
// against the walk mistaking an ordinary parameter for a `this` parameter.
// ---------------------------------------------------------------------------

#[test]
fn ordinary_parameters_in_constructor_are_clean() {
    assert_clean("class Cm20 { constructor(ha20: number, ga20: string) {} }");
}

#[test]
fn ordinary_parameters_in_accessor_are_clean() {
    assert_clean("class Cn20 { set ph20(fa20: number) {} }");
}

#[test]
fn ordinary_parameters_in_arrow_are_clean() {
    assert_clean("const ea20 = (ea21: number, da21: string) => {};");
}

#[test]
fn a_parameter_named_like_a_property_is_not_a_this_parameter() {
    assert_clean("function ff20(ca21: number, ba21: string) {}");
}

// ---------------------------------------------------------------------------
// A decorated/modified `this` parameter draws TS1433 (checked elsewhere, in
// the parser) instead of these placement/container codes — never both.
// Oracle-verified (`typescript@7.0.2`, `--experimentalDecorators`): a
// decorator on `this` suppresses TS2680/2681/2784 for that same parameter,
// even when the parameter is also misplaced or in an illegal container.
// ---------------------------------------------------------------------------

#[test]
fn decorated_misplaced_this_does_not_also_report_ts2680() {
    assert_clean(
        "declare function dec21(a: unknown, b: unknown, c: number): void; \
         class Co20 { mg20(@dec21 pa22: number, @dec21 this: Co20) {} }",
    );
}

#[test]
fn decorated_constructor_this_does_not_also_report_ts2681() {
    assert_clean(
        "declare function dec22(a: unknown, b: unknown, c: number): void; \
         class Cp20 { constructor(@dec22 this: Cp20) {} }",
    );
}

#[test]
fn decorated_getter_this_does_not_also_report_ts2784() {
    assert_clean(
        "declare function dec23(a: unknown, b: unknown, c: number): void; \
         class Cq20 { get pi20(@dec23 this: Cq20) { return 1; } }",
    );
}
