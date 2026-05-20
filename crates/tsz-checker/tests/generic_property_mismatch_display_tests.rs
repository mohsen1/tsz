use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, check_source_diagnostics, diagnostic_code_messages};
use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

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

fn check_es2015(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    ))
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

#[test]
fn generic_class_property_array_display_uses_owner_type_arg() {
    let diagnostics = check_es2015(
        r#"
class MyList<T> {
    public size: number;
    public data: T[];
    constructor(n: number) {
        this.size = n;
        this.data = [] as any;
    }
    public clone() {
        return new MyList<T>(this.size);
    }
}
declare let a: MyList<string>;
var d: MyList<number> = a.clone();
"#,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("MyList<string>")),
        "expected generic class display to recover T from T[] member declaration, got: {messages:#?}",
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("MyList<string[]>")),
        "generic class display should not use the raw T[] property type as the owner argument: {messages:#?}",
    );
}

#[test]
fn construct_only_generic_interface_display_keeps_type_arg() {
    let diagnostics = check_es2015(
        r#"
interface I1<T> { new (arg: T): object };
function f2<T>(args: T) {
    var v1!: { [index: string]: I1<T> };
    var v2 = v1['test'];
    var y = v2(args);
}
"#,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2348)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Value of type 'I1<T>' is not callable")),
        "expected TS2348 to preserve the construct-only interface application, got: {messages:#?}",
    );
}
