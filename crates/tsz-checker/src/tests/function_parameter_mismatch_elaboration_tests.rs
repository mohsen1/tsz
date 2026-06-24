//! Regression tests for function/signature relation failure elaboration.
//!
//! Structural rule: when a function type is not assignable to another function
//! type because a parameter is incompatible, tsc reports the signature line
//! followed by a `Types of parameters 'a' and 'b' are incompatible.` frame and
//! the contravariant leaf relation. tsz must produce the same chain instead of
//! stopping at the bare signature line.
//!
//! Two pieces cooperate:
//!
//! 1. The solver compares parameters before the return type (matching tsc's
//!    `compareSignaturesRelated`), so when both a parameter *and* the return
//!    type mismatch, the parameter mismatch is the reported reason.
//! 2. The checker renderer descends into the parameter reason to emit the
//!    `Types of parameters` frame plus the contravariant leaf.
//!
//! The rule is structural (independent of identifier spelling and of whether
//! the function appears directly, as an object property, or via an interface
//! method), so the matrix below varies all three.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_diagnostics, check_with_options, strict_checker_options};

/// Full elaboration text (primary message plus every related-information line)
/// of the single error with `code` in `source`, checked under strict options.
fn elaboration(source: &str, code: u32) -> String {
    elaboration_with(source, code, strict_checker_options())
}

fn elaboration_with(source: &str, code: u32, options: CheckerOptions) -> String {
    let diags = check_with_options(source, options);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// A direct function-to-function assignment whose only difference is a parameter
/// type surfaces the `Types of parameters` frame and the contravariant leaf.
#[test]
fn direct_function_parameter_mismatch_elaborates_parameter_chain() {
    let text = elaboration(
        r#"
let f: (x: string) => string;
let g: (x: number) => number;
f = g;
"#,
        2322,
    );
    assert!(
        text.contains("Types of parameters 'x' and 'x' are incompatible."),
        "Expected the parameter-incompatibility frame. Got: {text:?}"
    );
    // Parameters are contravariant: the leaf compares the target parameter
    // against the source parameter, so `string` is the one not assignable.
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// When *both* a parameter and the return type mismatch, tsc reports the
/// parameter mismatch (parameters are compared first). tsz must not report the
/// return type instead.
#[test]
fn parameter_is_preferred_over_return_when_both_mismatch() {
    let text = elaboration(
        r#"
type A = { m: (x: string) => string };
type B = { m: (x: number) => number };
declare const b: B;
const a: A = b;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'm' are incompatible."),
        "Expected the property frame. Got: {text:?}"
    );
    assert!(
        text.contains("Types of parameters 'x' and 'x' are incompatible."),
        "Expected the parameter frame to be preferred over a return-type frame. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
    assert!(
        !text.contains("returned by"),
        "Return-type elaboration must not be reported when a parameter mismatches. Got: {text:?}"
    );
}

/// Same rule for a function-typed object property; renamed identifiers prove the
/// fix is structural rather than keyed on the spelling `x`.
#[test]
fn property_function_parameter_mismatch_renamed_identifiers() {
    let text = elaboration(
        r#"
type Target = { handler: (payload: string) => void };
type Source = { handler: (payload: number) => void };
declare const s: Source;
const t: Target = s;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'handler' are incompatible."),
        "Expected the property frame. Got: {text:?}"
    );
    assert!(
        text.contains("Types of parameters 'payload' and 'payload' are incompatible."),
        "Expected the renamed parameter frame. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// The offending parameter index is reported, not always the first parameter.
#[test]
fn second_parameter_mismatch_names_the_correct_parameter() {
    let text = elaboration(
        r#"
let f: (a: string, b: string) => void;
let g: (a: string, b: number) => void;
f = g;
"#,
        2322,
    );
    assert!(
        text.contains("Types of parameters 'b' and 'b' are incompatible."),
        "Expected the second parameter to be named. Got: {text:?}"
    );
}

/// Method-shorthand signatures (an interface method, not a function-typed
/// property) elaborate the same parameter chain when assigned.
#[test]
fn interface_method_shorthand_parameter_mismatch_elaborates_chain() {
    let text = elaboration(
        r#"
interface Target { transform(value: string): void; }
interface Source { transform(value: number): void; }
declare const s: Source;
const t: Target = s;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'transform' are incompatible."),
        "Expected the property frame. Got: {text:?}"
    );
    assert!(
        text.contains("Types of parameters 'value' and 'value' are incompatible."),
        "Expected the parameter frame for a method-shorthand signature. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// A return-only mismatch (parameters identical) still reports the return type,
/// proving the parameter-first ordering does not suppress return diagnostics.
#[test]
fn return_only_mismatch_still_reports_return() {
    let text = elaboration_with(
        r#"
let f: () => string;
let g: () => number;
f = g;
"#,
        2322,
        strict_checker_options(),
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the return leaf for a return-only mismatch. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of parameters"),
        "A return-only mismatch must not emit a parameter frame. Got: {text:?}"
    );
}

/// A function passed as a *call argument* (TS2345 surface) whose parameter is
/// contravariantly incompatible must surface the same `Types of parameters`
/// frame and contravariant leaf that the direct-assignment (TS2322) surface
/// already renders. Previously the call-argument path dropped the entire chain
/// and stopped at the bare `Argument of type … is not assignable …` headline.
#[test]
fn call_argument_function_parameter_mismatch_elaborates_parameter_chain() {
    let text = elaboration(
        r#"
declare function take(cb: (value: number) => void): void;
take((value: string) => {});
"#,
        2345,
    );
    assert!(
        text.contains("Types of parameters 'value' and 'value' are incompatible."),
        "Expected the parameter-incompatibility frame under TS2345. Got: {text:?}"
    );
    // Parameters are contravariant: the leaf compares the target parameter
    // (`number`) against the source parameter (`string`).
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the contravariant parameter leaf under TS2345. Got: {text:?}"
    );
}

/// The offending parameter index is reported on the call-argument surface too,
/// and identifiers are taken from the signatures (not hard-coded), proving the
/// rule is structural.
#[test]
fn call_argument_second_parameter_mismatch_names_the_correct_parameter() {
    let text = elaboration(
        r#"
declare function reg(handler: (a: string, b: number) => void): void;
reg((a: string, b: string) => {});
"#,
        2345,
    );
    assert!(
        text.contains("Types of parameters 'b' and 'b' are incompatible."),
        "Expected the second parameter to be named under TS2345. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the contravariant leaf for the second parameter. Got: {text:?}"
    );
}

/// A callback argument to an interface *method* elaborates the same chain,
/// proving the rule does not depend on the callee being a free function.
#[test]
fn call_argument_interface_method_callback_parameter_mismatch_elaborates_chain() {
    let text = elaboration(
        r#"
interface Emitter { listen(cb: (payload: number) => void): void; }
declare const em: Emitter;
em.listen((payload: string) => {});
"#,
        2345,
    );
    assert!(
        text.contains("Types of parameters 'payload' and 'payload' are incompatible."),
        "Expected the parameter frame for a method callback argument. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// The architectural invariant: for the *same* function-parameter mismatch the
/// call-argument (TS2345) and direct-assignment (TS2322) surfaces must render
/// the same elaboration chain (the renderer routes both through the shared
/// `render_failure_reason` source of truth). Only the headline code differs.
#[test]
fn call_argument_and_assignment_render_identical_parameter_chain() {
    let argument_chain = elaboration(
        r#"
declare function take(cb: (value: number) => void): void;
take((value: string) => {});
"#,
        2345,
    );
    let assignment_chain = elaboration(
        r#"
let target: (value: number) => void;
let source: (value: string) => void;
target = source;
"#,
        2322,
    );
    // Drop each headline (first line); the related-information chain beneath it
    // must be identical across the two surfaces.
    let related = |text: &str| text.split('\n').skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(
        related(&argument_chain),
        related(&assignment_chain),
        "TS2345 and TS2322 must elaborate the same parameter chain. \
         argument={argument_chain:?} assignment={assignment_chain:?}"
    );
}

/// A shorthand **method member** in an object literal (`{ m(x: string) {} }`)
/// must elaborate the same parameter chain that the property-arrow form
/// (`{ m: (x: string) => {} }`) already produces. Previously the method-member
/// path emitted a bare signature line with no `Types of parameters` frame.
#[test]
fn object_literal_method_member_parameter_mismatch_elaborates_chain() {
    let text = elaboration(
        r#"
interface I { m(x: number): void; }
const i: I = { m(x: string) {} };
"#,
        2322,
    );
    assert!(
        text.contains("Types of parameters 'x' and 'x' are incompatible."),
        "Expected the parameter frame for an object-literal method member. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// Routing the method-member mismatch through the canonical relation -> reason
/// -> diagnostic boundary must not resolve the method name as a value reference.
/// A regression here surfaces a spurious TS2304 "Cannot find name 'm'".
#[test]
fn object_literal_method_member_mismatch_emits_no_cannot_find_name() {
    let diags = check_source_diagnostics(
        r#"
interface I { m(x: number): void; }
const i: I = { m(x: string) {} };
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != 2304),
        "Method-member elaboration must not resolve the method name as a value (no TS2304). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// The shorthand method member and the property-arrow member describe the same
/// structural mismatch, so they must render the identical elaboration chain
/// (headline plus related information).
#[test]
fn object_literal_method_member_matches_property_arrow_chain() {
    let method_form = elaboration(
        r#"
interface I { m: (x: number) => void; }
const i: I = { m(x: string) {} };
"#,
        2322,
    );
    let arrow_form = elaboration(
        r#"
interface I { m: (x: number) => void; }
const i: I = { m: (x: string) => {} };
"#,
        2322,
    );
    assert_eq!(
        method_form, arrow_form,
        "Method-member and property-arrow members must render the same chain. \
         method={method_form:?} arrow={arrow_form:?}"
    );
}

/// The same chain must surface on the call-argument (TS2345-adjacent) surface
/// when an object literal with a mismatched method member is passed as an
/// argument; the per-property elaboration still anchors a TS2322 at the member.
#[test]
fn call_argument_object_literal_method_member_parameter_mismatch_elaborates_chain() {
    let text = elaboration(
        r#"
interface I { m(x: number): void; }
declare function take(i: I): void;
take({ m(x: string) {} });
"#,
        2322,
    );
    assert!(
        text.contains("Types of parameters 'x' and 'x' are incompatible."),
        "Expected the parameter frame for a method member passed as an argument. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the contravariant parameter leaf. Got: {text:?}"
    );
}

/// When the offending parameter is itself a callback whose own parameter is
/// contravariantly incompatible, tsc keeps descending — one `Types of
/// parameters` frame per callback nesting level — rather than stopping at the
/// outer signature line. The single-callback case is the common callback-heavy
/// shape (rxjs/kysely operators, event listeners).
#[test]
fn nested_callback_parameter_mismatch_elaborates_full_chain() {
    let text = elaboration(
        r#"
let target: (cb: (x: string) => void) => void;
let source: (cb: (x: number) => void) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(cb: (x: number) => void) => void' is not assignable to type '(cb: (x: string) => void) => void'.\n\
         Types of parameters 'cb' and 'cb' are incompatible.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the full nested-callback parameter chain. Got: {text:?}"
    );
}

/// Three callback nesting levels: tsc re-prints the current callback signature
/// relation line before every second `Types of parameters` frame and flips the
/// contravariant leaf orientation with the nesting parity. The renderer must
/// reproduce that layout exactly.
#[test]
fn deeply_nested_callback_reprints_signature_and_flips_leaf_orientation() {
    let text = elaboration(
        r#"
let target: (a: (b: (c: string) => void) => void) => void;
let source: (a: (b: (c: number) => void) => void) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(a: (b: (c: number) => void) => void) => void' is not assignable to type '(a: (b: (c: string) => void) => void) => void'.\n\
         Types of parameters 'a' and 'a' are incompatible.\n\
         Types of parameters 'b' and 'b' are incompatible.\n\
         Type '(c: number) => void' is not assignable to type '(c: string) => void'.\n\
         Types of parameters 'c' and 'c' are incompatible.\n\
         Type 'string' is not assignable to type 'number'.",
        "Expected the signature reprint and flipped leaf orientation. Got: {text:?}"
    );
}

/// The nesting rule is structural: renamed binders and a call-argument (TS2345)
/// surface produce the same descending chain, proving it is not keyed on the
/// `x`/`cb` spelling or the assignment surface.
#[test]
fn nested_callback_chain_is_structural_renamed_and_call_argument() {
    let text = elaboration(
        r#"
declare function register(listener: (evt: (payload: string) => void) => void): void;
declare const handler: (evt: (payload: number) => void) => void;
register(handler);
"#,
        2345,
    );
    assert!(
        text.contains("Types of parameters 'evt' and 'evt' are incompatible.")
            && text.contains("Types of parameters 'payload' and 'payload' are incompatible.")
            && text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the renamed nested-callback chain under TS2345. Got: {text:?}"
    );
}

/// A callback whose innermost parameter is an object leads the leaf with the
/// object relation header `Type '{ … }' is not assignable to type '{ … }'.`
/// before drilling into `Types of property 'p' …`, matching tsc.
#[test]
fn nested_callback_object_parameter_leaf_emits_object_header() {
    let text = elaboration(
        r#"
let target: (cb: (o: { a: string }) => void) => void;
let source: (cb: (o: { a: number }) => void) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(cb: (o: { a: number; }) => void) => void' is not assignable to type '(cb: (o: { a: string; }) => void) => void'.\n\
         Types of parameters 'cb' and 'cb' are incompatible.\n\
         Types of parameters 'o' and 'o' are incompatible.\n\
         Type '{ a: number; }' is not assignable to type '{ a: string; }'.\n\
         Types of property 'a' are incompatible.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the object header before the property drill. Got: {text:?}"
    );
}

/// A callback whose innermost parameter fails because a required property is
/// missing self-heads with the `Property 'p' is missing …` summary directly
/// under the parameter frame — no object header — matching tsc.
#[test]
fn nested_callback_missing_property_leaf_self_heads() {
    let text = elaboration(
        r#"
let target: (cb: (o: { a: string; b: number }) => void) => void;
let source: (cb: (o: { a: string }) => void) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(cb: (o: { a: string; }) => void) => void' is not assignable to type '(cb: (o: { a: string; b: number; }) => void) => void'.\n\
         Types of parameters 'cb' and 'cb' are incompatible.\n\
         Types of parameters 'o' and 'o' are incompatible.\n\
         Property 'b' is missing in type '{ a: string; }' but required in type '{ a: string; b: number; }'.",
        "Expected the missing-property summary to self-head. Got: {text:?}"
    );
}

/// A nested callback that differs on its *return* type (not a parameter) is not
/// part of the parameter-chain layout reproduced here; the renderer must leave
/// the prior signature-only rendering intact rather than emit a partial chain
/// (no dangling `Types of parameters` frame).
#[test]
fn nested_callback_inner_return_mismatch_does_not_emit_partial_chain() {
    let text = elaboration(
        r#"
let target: (a: (b: () => string) => void) => void;
let source: (a: (b: () => number) => void) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(a: (b: () => number) => void) => void' is not assignable to type '(a: (b: () => string) => void) => void'.",
        "A return-terminated callback chain must not leave a dangling parameter frame. Got: {text:?}"
    );
}

/// A contravariant parameter whose leaf relation fails through a **union
/// source** (the target parameter is a wider union than the source parameter)
/// must drill the failing union member beneath the `Types of parameters` frame,
/// not stop at the bare signature line. This is the witnessed regression: the
/// contravariant-leaf renderer previously accepted only scalar/missing-property
/// /object-property leaves and dropped the whole chain for a union leaf.
#[test]
fn union_source_parameter_leaf_elaborates_member_chain() {
    let text = elaboration(
        r#"
let target: (x: string | number) => void;
let source: (x: string) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: string) => void' is not assignable to type '(x: string | number) => void'.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type 'string | number' is not assignable to type 'string'.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the union-source member chain under the parameter frame. Got: {text:?}"
    );
}

/// The mirror case: the contravariant leaf relation fails because the source
/// parameter is a **union target** that the (narrower) target parameter cannot
/// satisfy. tsc self-heads the union line with no further member drill, exactly
/// as it does for the same relation at the top level.
#[test]
fn union_target_parameter_leaf_elaborates_union_line() {
    let text = elaboration(
        r#"
let target: (x: string) => void;
let source: (x: "a" | "b") => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: \"a\" | \"b\") => void' is not assignable to type '(x: string) => void'.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type 'string' is not assignable to type '\"a\" | \"b\"'.",
        "Expected the union-target parameter line. Got: {text:?}"
    );
}

/// A contravariant parameter whose leaf is an **array element** mismatch
/// self-heads with the `se[] … te[]` line and drills the element relation.
#[test]
fn array_element_parameter_leaf_elaborates_element_chain() {
    let text = elaboration(
        r#"
let target: (x: number[]) => void;
let source: (x: string[]) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: string[]) => void' is not assignable to type '(x: number[]) => void'.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type 'number[]' is not assignable to type 'string[]'.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the array-element chain under the parameter frame. Got: {text:?}"
    );
}

/// A contravariant parameter whose leaf is a **tuple element** mismatch is
/// header-led: the parameter-pair header precedes the `Type at position N …`
/// positional line and the element relation.
#[test]
fn tuple_element_parameter_leaf_elaborates_positional_chain() {
    let text = elaboration(
        r#"
let target: (x: [number, number]) => void;
let source: (x: [string, string]) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: [string, string]) => void' is not assignable to type '(x: [number, number]) => void'.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type '[number, number]' is not assignable to type '[string, string]'.\n\
         Type at position 0 in source is not compatible with type at position 0 in target.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the tuple positional chain under the parameter frame. Got: {text:?}"
    );
}

/// A contravariant parameter whose leaf is an **index-signature** mismatch is
/// header-led: the parameter-pair header precedes the `'string' index
/// signatures are incompatible.` line and the value relation.
#[test]
fn index_signature_parameter_leaf_elaborates_index_chain() {
    let text = elaboration(
        r#"
let target: (x: { [k: string]: number }) => void;
let source: (x: { [k: string]: string }) => void;
target = source;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: { [k: string]: string; }) => void' is not assignable to type '(x: { [k: string]: number; }) => void'.\n\
         Types of parameters 'x' and 'x' are incompatible.\n\
         Type '{ [k: string]: number; }' is not assignable to type '{ [k: string]: string; }'.\n\
         'string' index signatures are incompatible.\n\
         Type 'number' is not assignable to type 'string'.",
        "Expected the index-signature chain under the parameter frame. Got: {text:?}"
    );
}

/// The union-leaf rule is structural (renamed binders) and surface-independent:
/// the same descending chain appears under the call-argument (TS2345) surface,
/// and renamed parameters prove it is not keyed on the `x` spelling.
#[test]
fn union_source_parameter_leaf_is_structural_renamed_and_call_argument() {
    let renamed = elaboration(
        r#"
let dst: (payload: string | number) => void;
let src: (payload: string) => void;
dst = src;
"#,
        2322,
    );
    assert!(
        renamed.contains("Types of parameters 'payload' and 'payload' are incompatible.")
            && renamed.contains("Type 'string | number' is not assignable to type 'string'.")
            && renamed.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the renamed union-source chain. Got: {renamed:?}"
    );

    let call_argument = elaboration(
        r#"
declare function take(cb: (value: string | number) => void): void;
take((value: string) => {});
"#,
        2345,
    );
    assert!(
        call_argument.contains("Types of parameters 'value' and 'value' are incompatible.")
            && call_argument.contains("Type 'string | number' is not assignable to type 'string'.")
            && call_argument.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the union-source chain under TS2345. Got: {call_argument:?}"
    );
}

/// The architectural invariant for the union leaf too: the call-argument
/// (TS2345) and direct-assignment (TS2322) surfaces render the identical
/// related-information chain (both route through the shared
/// `render_failure_reason`); only the headline code differs.
#[test]
fn union_source_call_argument_and_assignment_render_identical_chain() {
    let argument_chain = elaboration(
        r#"
declare function take(cb: (value: string | number) => void): void;
take((value: string) => {});
"#,
        2345,
    );
    let assignment_chain = elaboration(
        r#"
let target: (value: string | number) => void;
let source: (value: string) => void;
target = source;
"#,
        2322,
    );
    let related = |text: &str| text.split('\n').skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(
        related(&argument_chain),
        related(&assignment_chain),
        "TS2345 and TS2322 must elaborate the same union-source parameter chain. \
         argument={argument_chain:?} assignment={assignment_chain:?}"
    );
}

#[test]
fn class_method_with_fewer_params_implements_interface_with_more_params() {
    // TypeScript allows a class method to implement an interface method with
    // fewer required parameters; extra parameters are simply ignored by the
    // implementation. This mirrors the Kysely pattern where dialect adapters
    // implement `acquireMigrationLock(db, options)` with only `(db)`.
    let diags = check_source_diagnostics(
        r#"
interface Options { timeout?: number }
declare class DB<T> {}

interface Adapter {
    lock(db: DB<any>, options: Options): Promise<void>
}

class MssqlAdapter implements Adapter {
    async lock(_db: DB<any>): Promise<void> {}
}

// Direct function-type assignment: fewer params is valid
type F2 = (a: string, b: number) => void;
const f1: (a: string) => void = (_a) => {};
const assignCheck: F2 = f1;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: fewer-param function must be assignable to more-param type; got: {ts2322:?}"
    );
}
