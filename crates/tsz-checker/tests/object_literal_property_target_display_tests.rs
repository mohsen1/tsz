use tsz_checker::test_utils::check_source_code_messages;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
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
