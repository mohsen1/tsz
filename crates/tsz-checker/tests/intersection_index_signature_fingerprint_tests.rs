use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn intersection_index_signature_repro_displays_structural_index_values() {
    let source = r#"
type A = { a: string };
type B = { b: string };

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type constr<Source, Tgt> = { [K in keyof Source]: string } & Pick<Tgt, Exclude<keyof Tgt, keyof Source>>;
type s = constr<{}, { [key: string]: { a: string } }>;

declare const q: s;
q["asd"].b;

const d: { [key: string]: {a: string, b: string} } = q;
"#;
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diags = check_source(source, "test.ts", opts);
    let messages: Vec<_> = diags
        .iter()
        .map(|diag| (diag.code, diag.message_text.as_str()))
        .collect();

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2339 && *message == "Property 'b' does not exist on type '{ a: string; }'."
        }),
        "expected TS2339 to display the structural index value type, got {messages:#?}"
    );
    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2322
                && message.contains(
                    "Type 's' is not assignable to type '{ [key: string]: { a: string; b: string; }; }'.",
                )
        }),
        "expected TS2322 target index value to skip the A & B display alias, got {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|(_, message)| !message.contains("type 'A'.") && !message.contains("A & B")),
        "diagnostics should not preserve A/A & B aliases for this repro, got {messages:#?}"
    );
}
