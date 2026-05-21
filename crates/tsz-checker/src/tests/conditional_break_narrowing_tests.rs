use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn diagnostic_summaries(diagnostics: &[Diagnostic]) -> Vec<(u32, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
        .collect()
}

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

#[test]
fn while_true_conditional_break_preserves_break_site_narrowing() {
    let diagnostics = check_source_diagnostics(
        r#"
function f(x: string | number) {
    while (true) {
        if (typeof x === "string") {
            break;
        }
        return;
    }
    const t: string = x;
}
"#,
    );

    assert_eq!(
        diagnostics.len(),
        0,
        "expected conditional break narrowing to make `x` string after the loop, got {:?}",
        diagnostic_summaries(&diagnostics)
    );
}

#[test]
fn while_true_conditional_break_survives_body_assignment() {
    let diagnostics = check_source_diagnostics(
        r#"
function f(value: string | number) {
    while (true) {
        if (typeof value === "string") {
            break;
        }
        value = "done";
    }
    const t: string = value;
}
"#,
    );

    assert_eq!(
        diagnostics.len(),
        0,
        "expected only the break edge to reach the post-loop flow, got {:?}",
        diagnostic_summaries(&diagnostics)
    );
}

#[test]
fn while_true_conditional_break_survives_throwing_body() {
    let diagnostics = check_source_diagnostics(
        r#"
function f(item: { kind: "a"; a: number } | { kind: "b"; b: string }) {
    while (true) {
        if (item.kind === "a") {
            break;
        }
        throw "unreachable";
    }
    const t: { kind: "a"; a: number } = item;
}
"#,
    );

    assert_eq!(
        diagnostics.len(),
        0,
        "expected discriminant narrowing from the break edge, got {:?}",
        diagnostic_summaries(&diagnostics)
    );
}

#[test]
fn while_true_unconditional_break_does_not_invent_narrowing() {
    let diagnostics = check_source_diagnostics(
        r#"
function f(x: string | number) {
    while (true) {
        break;
    }
    const t: string = x;
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected unconditional break to keep the original union, got {:?}",
        diagnostic_summaries(&diagnostics)
    );
}

#[test]
fn while_true_two_conditional_breaks_preserve_union() {
    let diagnostics = check_source_diagnostics(
        r#"
function f(input: string | number) {
    while (true) {
        if (typeof input === "string") {
            break;
        }
        if (typeof input === "number") {
            break;
        }
    }
    const t: string = input;
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected both break edges to preserve the original union, got {:?}",
        diagnostic_summaries(&diagnostics)
    );
}
