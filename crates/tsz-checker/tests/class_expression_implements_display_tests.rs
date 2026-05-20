use tsz_checker::test_utils::check_source_code_messages;

fn ts2420_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2420).then_some(message))
        .collect()
}

#[test]
fn anonymous_class_expression_uses_outer_binding_name_in_ts2420() {
    let messages = ts2420_messages(
        r#"
interface Greetable { greet(): string; }
const BadGreeter = class implements Greetable {
  constructor(private name: string) {}
};
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2420 diagnostic, got {messages:#?}"
    );
    assert!(
        messages[0].contains("Class 'BadGreeter' incorrectly implements interface 'Greetable'."),
        "expected outer binding in class display, got {messages:#?}"
    );
    assert!(
        messages[0].contains("type 'BadGreeter'"),
        "expected outer binding in missing-member elaboration, got {messages:#?}"
    );
    assert!(
        !messages[0].contains("<anonymous>"),
        "bound anonymous class expression should not display as anonymous, got {messages:#?}"
    );
}

#[test]
fn anonymous_class_expression_display_uses_renamed_outer_binding() {
    let messages = ts2420_messages(
        r#"
interface Runnable { run(): void; }
const BrokenWorker = class implements Runnable {};
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2420 diagnostic, got {messages:#?}"
    );
    assert!(
        messages[0].contains("Class 'BrokenWorker' incorrectly implements interface 'Runnable'."),
        "expected renamed outer binding in class display, got {messages:#?}"
    );
    assert!(
        messages[0].contains("type 'BrokenWorker'"),
        "expected renamed outer binding in missing-member elaboration, got {messages:#?}"
    );
}

#[test]
fn generic_anonymous_class_expression_keeps_type_parameters_in_ts2420() {
    let messages = ts2420_messages(
        r#"
interface BoxLike<T> { value: T; }
const BrokenBox = class<T> implements BoxLike<T> {};
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2420 diagnostic, got {messages:#?}"
    );
    assert!(
        messages[0].contains("Class 'BrokenBox<T>' incorrectly implements interface 'BoxLike<T>'."),
        "expected generic outer binding in class display, got {messages:#?}"
    );
    assert!(
        messages[0].contains("type 'BrokenBox<T>'"),
        "expected generic outer binding in missing-member elaboration, got {messages:#?}"
    );
}

#[test]
fn named_class_expression_prefers_inner_class_name_in_ts2420() {
    let messages = ts2420_messages(
        r#"
interface Greetable { greet(): string; }
const Outer = class Inner implements Greetable {};
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "expected one TS2420 diagnostic, got {messages:#?}"
    );
    assert!(
        messages[0].contains("Class 'Inner' incorrectly implements interface 'Greetable'."),
        "expected inner class name to take precedence, got {messages:#?}"
    );
    assert!(
        messages[0].contains("type 'Inner'"),
        "expected inner class name in missing-member elaboration, got {messages:#?}"
    );
    assert!(
        !messages[0].contains("Outer"),
        "named class expression should not use outer binding, got {messages:#?}"
    );
}
