use tsz_checker::test_utils::check_source_code_messages;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn ts2416_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2416).then_some(message))
        .collect()
}

#[test]
fn construct_signature_expression_base_property_override_emits_ts2416() {
    let messages = ts2416_messages(
        r#"
type Construct<T> = new () => T;
declare function makeBase<T>(): Construct<T>;
type Shape = { alpha: number };

class Derived extends makeBase<Shape>() {
    alpha!: string;
}
"#,
    );

    assert!(
        messages.iter().any(|message| {
            message.contains("Property 'alpha' in type 'Derived'")
                && message.contains("base type 'Shape'")
        }),
        "expected TS2416 for construct-signature expression base, got {messages:?}"
    );
}

#[test]
fn same_named_constructor_function_and_type_alias_base_emits_ts2416() {
    let messages = ts2416_messages(
        r#"
type Constructor<T> = new () => T;
declare function Constructor<T>(): Constructor<T>;
type Shape = { alpha: number };

class Derived extends Constructor<Shape>() {
    alpha: string;
}
"#,
    );

    assert!(
        messages.iter().any(|message| {
            message.contains("Property 'alpha' in type 'Derived'")
                && message.contains("base type 'Shape'")
        }),
        "expected TS2416 for same-named constructor function/type alias base, got {messages:?}"
    );
}

#[test]
fn construct_signature_expression_base_declared_field_emits_ts2416() {
    let messages = ts2416_messages(
        r#"
type Construct<T> = new () => T;
declare function makeBase<T>(): Construct<T>;
type Shape = { alpha: number };

class Derived extends makeBase<Shape>() {
    alpha: string;
}
"#,
    );

    assert!(
        messages.iter().any(|message| {
            message.contains("Property 'alpha' in type 'Derived'")
                && message.contains("base type 'Shape'")
        }),
        "expected TS2416 for declared field without initializer, got {messages:?}"
    );
}

#[test]
fn renamed_construct_signature_expression_base_property_override_emits_ts2416() {
    let messages = ts2416_messages(
        r#"
type Maker<Result> = new () => Result;
declare function build<Result>(): Maker<Result>;
type RecordLike = { payload: boolean };

class Widget extends build<RecordLike>() {
    payload!: { tag: "nope" };
}
"#,
    );

    assert!(
        messages.iter().any(|message| {
            message.contains("Property 'payload' in type 'Widget'")
                && message.contains("base type 'RecordLike'")
        }),
        "expected TS2416 under renamed binders, got {messages:?}"
    );
}

#[test]
fn intersection_construct_signature_expression_base_property_override_emits_ts2416() {
    let messages = ts2416_messages(
        r#"
type Construct<T> = new () => T;
declare function makeBase<T>(): Construct<T>;
type Left = { keep: number };
type Right = { clash: string };

class Derived extends makeBase<Left & Right>() {
    clash!: number;
}
"#,
    );

    assert!(
        messages.iter().any(|message| {
            message.contains("Property 'clash' in type 'Derived'")
                && message.contains("base type 'Left & Right'")
        }),
        "expected TS2416 for intersection instance base, got {messages:?}"
    );
}

#[test]
fn compatible_construct_signature_expression_base_property_override_has_no_ts2416() {
    let messages = ts2416_messages(
        r#"
type Construct<T> = new () => T;
declare function makeBase<T>(): Construct<T>;
type Shape = { alpha: number };

class Derived extends makeBase<Shape>() {
    alpha!: number;
}
"#,
    );

    assert!(
        messages.is_empty(),
        "did not expect TS2416 for compatible construct-signature expression base, got {messages:?}"
    );
}
