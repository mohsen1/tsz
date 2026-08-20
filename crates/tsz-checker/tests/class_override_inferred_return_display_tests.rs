//! Regression coverage for the TS2416 / TS2417 override-incompatibility
//! elaboration head when the overriding member is a method with an *inferred*
//! return type.
//!
//! When such a member's failure reason is a contravariant parameter mismatch,
//! the depth-0 source display was rendered through the `AssignmentSource`
//! display role. That role re-resolves the diagnostic's anchor as a value
//! expression; for a method-name anchor it walks up to the method declaration
//! and types it as the method's inferred return type — collapsing
//! `(x: number) => void` to just `void` (or `undefined`, `number`, …). The
//! override entry now supplies the structural source display directly so the
//! head prints the member's real function type regardless of whether the
//! return type was annotated.
//!
//! Binder names are varied across cases so the assertions cannot be satisfied
//! by any name-scoped shortcut.

use tsz_checker::test_utils::check_source_diagnostics;

/// The full TS2416/TS2417 message text: the lead plus every elaboration frame
/// (`related_information`) joined by newlines, for the first diagnostic with
/// `code`.
fn override_elaboration(source: &str, code: u32) -> String {
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected a TS{code} diagnostic, got {diagnostics:#?}"));
    std::iter::once(diag.message_text.clone())
        .chain(
            diag.related_information
                .iter()
                .map(|r| r.message_text.clone()),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn implements_inferred_void_method_shows_full_signature_not_return_type() {
    let elaboration = override_elaboration(
        r#"
interface Sink { consume(value: string): void; }
class Bucket implements Sink { consume(value: number) {} }
"#,
        2416,
    );

    assert!(
        elaboration.contains(
            "Type '(value: number) => void' is not assignable to type '(value: string) => void'."
        ),
        "override head must print the full method signature, got:\n{elaboration}"
    );
    assert!(
        !elaboration.contains("Type 'void' is not assignable"),
        "inferred void return must not collapse the source to its return type, got:\n{elaboration}"
    );
}

#[test]
fn extends_inferred_void_method_shows_full_signature_not_return_type() {
    let elaboration = override_elaboration(
        r#"
class Base { handle(token: string): void {} }
class Derived extends Base { handle(token: number) {} }
"#,
        2416,
    );

    assert!(
        elaboration.contains(
            "Type '(token: number) => void' is not assignable to type '(token: string) => void'."
        ),
        "extends override head must print the full method signature, got:\n{elaboration}"
    );
    assert!(
        !elaboration.contains("Type 'void' is not assignable"),
        "inferred void return must not collapse the source to its return type, got:\n{elaboration}"
    );
}

#[test]
fn implements_inferred_nonvoid_return_shows_full_signature() {
    // The return type is inferred as `number`; the source head must still be
    // the whole function type, not the bare return type.
    let elaboration = override_elaboration(
        r#"
interface Reader { read(key: string): number; }
class Store implements Reader { read(key: number) { return 1; } }
"#,
        2416,
    );

    assert!(
        elaboration.contains(
            "Type '(key: number) => number' is not assignable to type '(key: string) => number'."
        ),
        "inferred non-void return must keep the full signature, got:\n{elaboration}"
    );
    assert!(
        !elaboration.contains("Type 'number' is not assignable to type '(key: string) => number'"),
        "inferred number return must not collapse the source to its return type, got:\n{elaboration}"
    );
}

#[test]
fn implements_object_param_mismatch_inferred_void_shows_full_signature() {
    let elaboration = override_elaboration(
        r#"
interface Draw { paint(shape: { width: string }): void; }
class Canvas implements Draw { paint(shape: { width: number }) {} }
"#,
        2416,
    );

    assert!(
        elaboration.contains(
            "Type '(shape: { width: number; }) => void' is not assignable to type '(shape: { width: string; }) => void'."
        ),
        "object-typed param mismatch must keep the full signature, got:\n{elaboration}"
    );
    assert!(
        !elaboration.contains("Type 'void' is not assignable"),
        "inferred void return must not collapse the source to its return type, got:\n{elaboration}"
    );
}

#[test]
fn implements_explicit_return_annotation_still_shows_full_signature() {
    // The annotated-return path was already correct; pin it so the fix does not
    // regress the case that flowed through the value-display role.
    let elaboration = override_elaboration(
        r#"
interface Port { send(payload: string): void; }
class Wire implements Port { send(payload: number): void {} }
"#,
        2416,
    );

    assert!(
        elaboration
            .contains("Type '(payload: number) => void' is not assignable to type '(payload: string) => void'."),
        "explicit-return override head must print the full signature, got:\n{elaboration}"
    );
}
