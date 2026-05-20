use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322_diagnostic(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one TS2322 diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related_messages(diagnostic: &Diagnostic) -> Vec<&str> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.as_str())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related_messages(diagnostic)
        .iter()
        .any(|message| message.contains(expected))
}

#[test]
fn generic_class_instance_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
class Animal {
  name: string = "";
}

class Dog extends Animal {
  bark(): void {}
}

interface Box<T> {
  value: T;
}

declare let animalBox: Box<Animal>;
declare let dogBox: Box<Dog>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<Animal>' is not assignable to type 'Box<Dog>'."),
        "expected generic class instance display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type 'Animal' but required in type 'Dog'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
    assert!(
        !diagnostic.message_text.contains("typeof Animal")
            && !diagnostic.message_text.contains("typeof Dog"),
        "class instance type arguments must not display as constructor types, got {diagnostic:#?}"
    );
    assert!(
        !has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "generic wrapper property should not hide the nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_interface_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Animal { name: string; }
interface Dog extends Animal { bark(): void; }
interface Box<T> { value: T; }

declare let animalBox: Box<Animal>;
declare let dogBox: Box<Dog>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<Animal>' is not assignable to type 'Box<Dog>'."),
        "expected generic interface display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type 'Animal' but required in type 'Dog'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
    assert!(
        !has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "generic wrapper property should not hide the nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_object_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let animalBox: Box<{ name: string }>;
declare let dogBox: Box<{ name: string; bark(): void }>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic.message_text.contains(
            "Type 'Box<{ name: string; }>' is not assignable to type 'Box<{ name: string; bark(): void; }>'."
        ),
        "expected generic object display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type '{ name: string; }' but required in type '{ name: string; bark(): void; }'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_object_property_mismatch_preserves_multiple_missing_properties() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let emptyBox: Box<{}>;
declare let targetBox: Box<{ a: string; b: number }>;

targetBox = emptyBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<{}>' is not assignable to type 'Box<{ a: string; b: number; }>'."),
        "expected generic object display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '{}' is missing the following properties from type '{ a: string; b: number; }': a, b"
        ),
        "expected nested multiple-missing-property message, got {diagnostic:#?}"
    );
}

#[test]
fn generic_primitive_property_mismatch_keeps_property_path_when_nested_source_is_unstable() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let numberBox: Box<number>;
declare let stringBox: Box<string>;

stringBox = numberBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<number>' is not assignable to type 'Box<string>'."),
        "expected generic primitive display, got {diagnostic:#?}"
    );
    assert!(
        has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "expected fallback property-path message, got {diagnostic:#?}"
    );
    assert!(
        !has_related(
            &diagnostic,
            "Type 'Box<number>' is not assignable to type 'string'."
        ),
        "must not render malformed nested primitive reason, got {diagnostic:#?}"
    );
}

#[test]
fn direct_object_property_mismatch_keeps_property_path() {
    let diagnostic = ts2322_diagnostic(
        r#"
declare let source: { p: {} };
declare let target: { p: { a: string } };

target = source;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type '{ p: {}; }' is not assignable to type '{ p: { a: string; }; }'."),
        "expected direct object mismatch display, got {diagnostic:#?}"
    );
    assert!(
        has_related(&diagnostic, "Types of property 'p' are incompatible."),
        "non-generic object property mismatch should keep property path, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'a' is missing in type '{}' but required in type '{ a: string; }'."
        ),
        "expected nested missing property under direct property path, got {diagnostic:#?}"
    );
}
