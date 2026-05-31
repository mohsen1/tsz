use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_code_messages, check_source_with_libs_code_messages, load_lib_files,
};

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
        .collect()
}

fn diagnostic_messages_with_es5(source: &str) -> Vec<String> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn object_literal_property_initializer_keeps_property_target_application_display() {
    let messages = ts2322_messages(
        r#"
interface Sink<Value> {
    take(value: Value): number;
}
interface Outer<Value> {
    item: Sink<Value>;
}
class Source<Value> implements Sink<Value> {
    take(value: Value): number {
        return 1;
    }
}

let bad: Outer<string> = { item: new Source<number>() };
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 diagnostic, got {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Type 'Source<number>' is not assignable to type 'Sink<string>'."),
        "object-literal property mismatch should display the property target, got: {message}"
    );
    assert!(
        !message.contains("type 'Outer<string>'"),
        "enclosing variable annotation must not repaint the property target, got: {message}"
    );
}

#[test]
fn uppercase_object_special_diagnostic_uses_global_object_identity() {
    let messages = diagnostic_messages_with_es5(
        r#"
declare let value: Object;
let needsProp: { required: string } = value;
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("The 'Object' type is assignable to very few other types")),
        "uppercase global Object should use the special tsc diagnostic, got: {messages:#?}"
    );
}

#[test]
fn lowercase_object_keyword_does_not_use_uppercase_object_special_diagnostic() {
    let messages = diagnostic_messages_with_es5(
        r#"
declare let value: object;
let needsProp: { required: string } = value;
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains("required")),
        "lowercase object keyword should still report the missing property, got: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message
                .contains("The 'Object' type is assignable to very few other types")),
        "lowercase object keyword must not be classified as the global Object interface, got: {messages:#?}"
    );
}

#[test]
fn object_literal_property_target_display_is_not_tied_to_specific_names() {
    let messages = ts2322_messages(
        r#"
interface Receiver<TItem> {
    receive(item: TItem): number;
}
interface Container<TItem> {
    value: Receiver<TItem>;
}
class Producer<TItem> implements Receiver<TItem> {
    receive(item: TItem): number {
        return 1;
    }
}

let bad: Container<string> = { value: new Producer<number>() };
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 diagnostic for renamed shape, got {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Type 'Producer<number>' is not assignable to type 'Receiver<string>'."),
        "renamed property mismatch should still display the structural property target, got: {message}"
    );
    assert!(
        !message.contains("type 'Container<string>'"),
        "renamed enclosing annotation must not repaint the property target, got: {message}"
    );
}

#[test]
fn object_literal_property_initializer_through_alias_keeps_property_target_display() {
    let messages = ts2322_messages(
        r#"
interface Sink<Value> {
    take(value: Value): number;
}
interface Outer<Value> {
    item: Sink<Value>;
}
type AliasOuter<Value> = Outer<Value>;
class Source<Value> implements Sink<Value> {
    take(value: Value): number {
        return 1;
    }
}

let bad: AliasOuter<string> = { item: new Source<number>() };
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 diagnostic for aliased container, got {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Type 'Source<number>' is not assignable to type 'Sink<string>'."),
        "aliased object-literal property mismatch should display the property target, got: {message}"
    );
    assert!(
        !message.contains("type 'AliasOuter<string>'") && !message.contains("type 'Outer<string>'"),
        "aliased enclosing annotation must not repaint the property target, got: {message}"
    );
}

#[test]
fn non_object_literal_assignment_keeps_enclosing_target_display() {
    let messages = ts2322_messages(
        r#"
interface Sink<Value> {
    take(value: Value): number;
}
interface Outer<Value> {
    item: Sink<Value>;
}
class Source<Value> implements Sink<Value> {
    take(value: Value): number {
        return 1;
    }
}

let source: Outer<number> = { item: new Source<number>() };
let bad: Outer<string> = source;
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 diagnostic for non-object-literal assignment, got {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Type 'Outer<number>' is not assignable to type 'Outer<string>'."),
        "non-object-literal assignment should keep the enclosing target display, got: {message}"
    );
}
