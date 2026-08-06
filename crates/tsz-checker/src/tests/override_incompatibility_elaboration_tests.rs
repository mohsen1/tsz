//! Regression tests for TS2416/TS2417 property-override incompatibility
//! *elaboration*.
//!
//! Structural rule: when a derived member's type is not assignable to the base
//! member's type, tsc renders the override lead
//! (`Property 'p' in type 'D' is not assignable to the same property in base
//! type 'B'.`) followed by the *same* structural elaboration chain it produces
//! for the equivalent TS2322/TS2345 assignment — the contravariant parameter
//! frame, the missing/optional-property frame, the nested property path, and so
//! on. Previously the override paths emitted only the single
//! `Type 'S' is not assignable to type 'T'.` frame (or, for a missing-property
//! failure, the *wrong* wrapper frame) and dropped everything deeper.
//!
//! The fix routes every override/`implements`/JSDoc-`@implements` mismatch
//! through the shared `relation -> reason -> diagnostic` assignability gateway,
//! so the chain matches tsc regardless of the heritage form.
//!
//! The rule is structural (independent of identifier spelling), so the cases
//! below vary binder names where a name appears in the rendered output.
//!
//! `elaboration` folds in every related-information line, so the trailing
//! `'x' is declared here.` (`TS2728`) in the expectations below is part of the
//! rendered output tsc produces, not an extra chain frame. tsc emits that
//! pointer on a *nested* missing-property frame exactly as it does on a
//! top-level one; tsz dropped it at `depth > 0` until #16443's nested-
//! elaboration fix. Oracled on `typescript@7.0.2` with `--noEmit --strict
//! --pretty --target es2022 --lib es2022`: the parameter-chain case anchors on
//! `bark` at `2:28`, the renamed-binder case on `howl` at `2:40`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_codes, check_with_options, strict_checker_options};

/// Full elaboration text (primary message plus every related-information line)
/// of the single diagnostic with `code` in `source`, checked under strict
/// options.
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

/// A property-typed (non-method) function override that fails on parameter
/// contravariance must descend into the `Types of parameters` frame and the
/// contravariant leaf, instead of stopping at the bare signature line.
#[test]
fn extends_property_function_override_elaborates_parameter_chain() {
    let text = elaboration(
        r#"
class Animal { kind: "animal" = "animal"; }
class Dog extends Animal { bark(): void {} }
class Base { handler: (value: Animal) => void = () => {}; }
class Derived extends Base { handler: (value: Dog) => void = () => {}; }
"#,
        2416,
    );
    assert_eq!(
        text,
        "Property 'handler' in type 'Derived' is not assignable to the same property in base type 'Base'.\n\
         Type '(value: Dog) => void' is not assignable to type '(value: Animal) => void'.\n\
         Types of parameters 'value' and 'value' are incompatible.\n\
         Property 'bark' is missing in type 'Animal' but required in type 'Dog'.\n\
         'bark' is declared here.",
    );
}

/// A missing-property object override surfaces the `Property ... is missing`
/// frame *directly* under the lead (no spurious `Type 'S' is not assignable to
/// type 'T'.` wrapper), matching tsc.
#[test]
fn extends_object_override_missing_property_skips_wrapper_frame() {
    let text = elaboration(
        r#"
class Base { shape: { width: number; height: number } = { width: 0, height: 0 }; }
class Derived extends Base { shape: { width: number } = { width: 0 }; }
"#,
        2416,
    );
    assert_eq!(
        text,
        "Property 'shape' in type 'Derived' is not assignable to the same property in base type 'Base'.\n\
         Property 'height' is missing in type '{ width: number; }' but required in type '{ width: number; height: number; }'.",
    );
}

/// A nested object property mismatch surfaces the wrapper frame, the
/// `The types of 'a.b' are incompatible` path frame, and the leaf relation.
#[test]
fn extends_object_override_nested_property_path_elaborates() {
    let text = elaboration(
        r#"
class Base { box: { inner: { value: number } } = { inner: { value: 0 } }; }
class Derived extends Base { box: { inner: { value: string } } = { inner: { value: "" } }; }
"#,
        2416,
    );
    assert_eq!(
        text,
        "Property 'box' in type 'Derived' is not assignable to the same property in base type 'Base'.\n\
         Type '{ inner: { value: string; }; }' is not assignable to type '{ inner: { value: number; }; }'.\n\
         The types of 'inner.value' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// The `implements` heritage path produces the identical parameter-chain
/// elaboration; a method member's parameter incompatibility descends past the
/// signature line.
#[test]
fn implements_method_parameter_mismatch_elaborates_parameter_chain() {
    let text = elaboration(
        r#"
interface Sink { absorb(quantity: number): void; }
class Bucket implements Sink { absorb(quantity: string): void {} }
"#,
        2416,
    );
    assert_eq!(
        text,
        "Property 'absorb' in type 'Bucket' is not assignable to the same property in base type 'Sink'.\n\
         Type '(quantity: string) => void' is not assignable to type '(quantity: number) => void'.\n\
         Types of parameters 'quantity' and 'quantity' are incompatible.\n\
         Type 'number' is not assignable to type 'string'.",
    );
}

/// The elaboration is keyed on structure, not identifiers: renaming every
/// binder yields the same chain shape with the new names.
#[test]
fn override_elaboration_is_structural_not_identifier_keyed() {
    let text = elaboration(
        r#"
class CreatureKind { kind: "creature" = "creature"; }
class HoundKind extends CreatureKind { howl(): void {} }
class Origin { onPick: (subject: CreatureKind) => void = () => {}; }
class Refined extends Origin { onPick: (subject: HoundKind) => void = () => {}; }
"#,
        2416,
    );
    assert_eq!(
        text,
        "Property 'onPick' in type 'Refined' is not assignable to the same property in base type 'Origin'.\n\
         Type '(subject: HoundKind) => void' is not assignable to type '(subject: CreatureKind) => void'.\n\
         Types of parameters 'subject' and 'subject' are incompatible.\n\
         Property 'howl' is missing in type 'CreatureKind' but required in type 'HoundKind'.\n\
         'howl' is declared here.",
    );
}

// ---------------------------------------------------------------------------
// Negative guards: routing instance-method overrides through the no-erase
// relation must not introduce TS2416 false positives. Method parameters stay
// bivariant, and a faithful generic override is still accepted.
// ---------------------------------------------------------------------------

/// Method parameter bivariance is preserved: an override may *widen* a method
/// parameter (tsc accepts this for method-declared members).
#[test]
fn method_parameter_widening_override_is_accepted() {
    let codes = check_source_codes(
        r#"
class Animal { kind: "animal" = "animal"; }
class Dog extends Animal { bark(): void {} }
class Base { greet(value: Dog): void {} }
class Derived extends Base { greet(value: Animal): void {} }
"#,
    );
    assert!(
        !codes.contains(&2416),
        "unexpected TS2416 (method-parameter bivariance must be preserved). Got: {codes:?}"
    );
}

/// A faithful generic method override that keeps the type parameter is accepted.
#[test]
fn faithful_generic_method_override_is_accepted() {
    let codes = check_source_codes(
        r#"
class Base { wrap<T extends string>(value: T): T { return value; } }
class Derived extends Base { wrap<T extends string>(value: T): T { return value; } }
"#,
    );
    assert!(
        !codes.contains(&2416),
        "unexpected TS2416 (faithful generic override must be accepted). Got: {codes:?}"
    );
}
