use tsz_checker::test_utils::check_source_strict_messages_without_missing_libs as check_strict;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_strict(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
        .collect()
}

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    check_strict(source)
}

#[test]
fn optional_homomorphic_mapped_index_access_displays_source_index() {
    let messages = ts2322_messages(
        r#"
type Maybe<T> = { [Q in keyof T]?: T[Q] };
function f<T, U extends T>(x: T, y: Maybe<U>, k: keyof T) {
    y[k] = x[k];
}
"#,
    );

    assert!(
        messages.iter().any(|message| message
            .contains("Type 'T[keyof T]' is not assignable to type 'U[keyof T] | undefined'.")),
        "expected homomorphic optional mapped index display, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Maybe<U>[keyof T]")),
        "diagnostic should not preserve the alias-index spelling, got: {messages:#?}"
    );
}

#[test]
fn readonly_homomorphic_mapped_index_access_displays_source_index() {
    let messages = ts2322_messages(
        r#"
type Frozen<X> = { readonly [R in keyof X]: X[R] };
function f<T, U extends T>(x: T, y: Frozen<U>, k: keyof T) {
    y[k] = x[k];
}
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'T[keyof T]' is not assignable to type 'U[keyof T")),
        "expected readonly homomorphic mapped index display, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Frozen<U>[keyof T]")),
        "diagnostic should not preserve the alias-index spelling, got: {messages:#?}"
    );
}

#[test]
fn inline_homomorphic_mapped_assignment_reports_generic_value_mismatch() {
    let messages = ts2322_messages(
        r#"
function f<T, U extends T>(x: { [P in keyof T]: T[P] }, y: { [P in keyof T]: U[P] }) {
    y = x;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [P in keyof T]: T[P]; }' is not assignable to type '{ [P in keyof T]: U[P]; }'."
        )),
        "expected inline homomorphic mapped assignment mismatch, got: {messages:#?}"
    );
}

#[test]
fn constrained_key_homomorphic_mapped_assignment_reports_generic_value_mismatch() {
    let messages = ts2322_messages(
        r#"
function f<T, U extends T, K extends keyof T>(x: { [P in K]: T[P] }, y: { [P in K]: U[P] }) {
    y = x;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [P in K]: T[P]; }' is not assignable to type '{ [P in K]: U[P]; }'."
        )),
        "expected constrained-key homomorphic mapped assignment mismatch, got: {messages:#?}"
    );
}

#[test]
fn broad_string_mapped_assignment_source_displays_as_index_signature() {
    let messages = ts2322_messages(
        r#"
function f<Key extends string>(
    left: { [Name in Key]: number },
    right: { [Slot in string]: number },
) {
    left = right;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [x: string]: number; }' is not assignable to type '{ [Name in Key]: number; }'."
        )),
        "expected broad mapped source to display as a structural string index signature, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Record<string")),
        "inline broad mapped source must not be repainted as Record, got: {messages:#?}"
    );
}

#[test]
fn broad_number_mapped_assignment_source_displays_as_index_signature() {
    let messages = ts2322_messages(
        r#"
function f<Idx extends number>(
    left: { [Cell in Idx]: boolean },
    right: { [Offset in number]: boolean },
) {
    left = right;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [x: number]: boolean; }' is not assignable to type '{ [Cell in Idx]: boolean; }'."
        )),
        "expected broad mapped source to display as a structural number index signature, got: {messages:#?}"
    );
}

#[test]
fn broad_string_mapped_assignment_source_preserves_generic_value_type() {
    let messages = ts2322_messages(
        r#"
function f<Key extends string, Value>(
    left: { [Name in Key]: number },
    right: { [Slot in string]: Value },
) {
    left = right;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [x: string]: Value; }' is not assignable to type '{ [Name in Key]: number; }'."
        )),
        "expected broad mapped source to keep unrelated generic value type, got: {messages:#?}"
    );
}

#[test]
fn explicit_generic_mapped_alias_source_surface_is_preserved() {
    let messages = ts2322_messages(
        r#"
type Bag<Value> = { [Key in string]: Value };
function f(right: Bag<string>) {
    let left: number;
    left = right;
}
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'Bag<string>' is not assignable to type 'number'.")),
        "explicit generic mapped alias should keep its declared source surface, got: {messages:#?}"
    );
}

#[test]
fn generic_application_arg_preserves_homomorphic_index_alias_surface() {
    let messages = ts2322_messages(
        r#"
type NonNullable<T> = T & {};
function f<T>(x: Partial<T>[keyof T], y: NonNullable<Partial<T>[keyof T]>) {
    y = x;
}
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message.contains("NonNullable<Partial<T>[keyof T]>")),
        "expected generic application argument to preserve alias-index spelling, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("NonNullable<T[keyof T] | undefined>")),
        "diagnostic should not simplify the indexed access inside NonNullable, got: {messages:#?}"
    );
}

#[test]
fn mapped_identity_accepts_keyof_type_alias_constraint() {
    let messages = diagnostic_messages(
        r#"
function f<T>() {
    type K = keyof T;
    var x: { [P in keyof T]: T[P] };
    var x: { [Q in keyof T]: T[Q] };
    var x: { [R in K]: T[R] };
}
"#,
    );

    assert!(
        messages.is_empty(),
        "expected mapped identity through keyof alias to be accepted, got: {messages:#?}"
    );
}
