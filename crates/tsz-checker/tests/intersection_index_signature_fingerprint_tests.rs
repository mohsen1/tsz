use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let libs = LIBS.get_or_init(load_default_lib_files);
    check_source_with_libs(source, "test.ts", opts, libs)
        .into_iter()
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

#[test]
fn mapped_index_signature_value_display_does_not_borrow_unrelated_alias() {
    let source = r#"
type A = { a: string };
type B = { b: string };
type constr<Source, Tgt> = { [K in keyof Source]: string } & Pick<Tgt, Exclude<keyof Tgt, keyof Source>>;
type s = constr<{}, { [key: string]: { a: string } }>;

declare const q: s;
q["asd"].a.substr(1);
q["asd"].b;

const d: { [key: string]: { a: string; b: string } } = q;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'b' does not exist on type '{ a: string; }'."
        }),
        "expected TS2339 to display the computed structural index value type, got {messages:#?}"
    );
    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2322
                && message.contains(
                    "Type 's' is not assignable to type '{ [key: string]: { a: string; b: string; }; }'.",
                )
        }),
        "expected TS2322 to keep the source alias surface for the declared variable, got {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|(_, message)| !message.contains("type 'A'.") && !message.contains("A & B")),
        "computed index value should not be repainted as unrelated aliases, got {messages:#?}"
    );
}

#[test]
fn anonymous_index_signature_value_display_stays_structural_despite_alias_collision() {
    let source = r#"
type Alias = { a: string };
declare const q: { [key: string]: { a: string } };
q["x"].b;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'b' does not exist on type '{ a: string; }'."
        }),
        "anonymous index value should display structurally, got {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|(_, message)| !message.contains("type 'Alias'")),
        "anonymous index value should not borrow Alias, got {messages:#?}"
    );
}

#[test]
fn explicit_alias_index_signature_value_display_preserves_alias() {
    let source = r#"
type Alias = { a: string };
declare const q: { [key: string]: Alias };
q["x"].b;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'b' does not exist on type 'Alias'."
        }),
        "explicit alias index value should preserve Alias, got {messages:#?}"
    );
}

#[test]
fn assignment_to_index_signature_preserves_declared_intersection_and_alias_surfaces() {
    let source = r#"
type NamedLeft = { left: string };
type NamedRight = { right: string };

declare let sourceValue: { keep: NamedLeft } & { drift: NamedRight };
declare let targetValue: { [slot: string]: NamedLeft };

targetValue = sourceValue;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2322
                && message
                    == "Type '{ keep: NamedLeft; } & { drift: NamedRight; }' is not assignable to type '{ [slot: string]: NamedLeft; }'."
        }),
        "index-signature assignment should preserve explicit aliases and intersection source display, got {messages:#?}"
    );
}

#[test]
fn assignment_to_primitive_index_signature_preserves_anonymous_intersection_surface() {
    let source = r#"
declare let sourceValue: { alpha: string } & { beta: number };
declare let targetValue: { [slot: string]: string };

targetValue = sourceValue;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2322
                && message
                    == "Type '{ alpha: string; } & { beta: number; }' is not assignable to type '{ [slot: string]: string; }'."
        }),
        "index-signature assignment should keep the declared anonymous intersection source display, got {messages:#?}"
    );
}

#[test]
fn direct_alias_property_receiver_display_still_preserves_alias() {
    let source = r#"
type Alias = { a: string };
declare const q: Alias;
q.b;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'b' does not exist on type 'Alias'."
        }),
        "direct alias receiver should preserve Alias, got {messages:#?}"
    );
}

#[test]
fn renamed_mapped_index_signature_value_display_stays_structural() {
    let source = r#"
type NamedShape = { z: number };
type DropKeys<Source, Target> = { [Key in keyof Source]: boolean } & Pick<Target, Exclude<keyof Target, keyof Source>>;
type Result = DropKeys<{}, { [name: string]: { z: number } }>;

declare const item: Result;
item["qq"].missing;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'missing' does not exist on type '{ z: number; }'."
        }),
        "renamed mapped index value should display structurally, got {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|(_, message)| !message.contains("NamedShape")),
        "renamed repro should not depend on the original alias/property names, got {messages:#?}"
    );
}

#[test]
fn numeric_index_signature_display_uses_argument_type_not_only_literal_syntax() {
    let source = r#"
type NumberValue = { numeric: string };
type StringValue = NumberValue | { named: string };
declare const q: { [index: number]: NumberValue; [key: string]: StringValue };
declare const i: number;
q[i].missing;
"#;
    let messages = diagnostic_messages(source);

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'missing' does not exist on type '{ numeric: string; }'."
        }),
        "number-typed element access should use the number index value display, got {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|(_, message)| !message.contains("StringValue")),
        "number-typed element access must not fall back to the string index display, got {messages:#?}"
    );
}
