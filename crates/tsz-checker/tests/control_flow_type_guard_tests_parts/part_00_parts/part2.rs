#[test]
fn type_predicate_narrowing_survives_when_false_branch_terminates() {
    let diagnostics = strict_diagnostics(
        r#"
function isNumber(value: unknown): value is number {
    return typeof value === "number";
}

function test(x: unknown) {
    if (!isNumber(x)) {
        return;
    }
    let n: number = x;
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "predicate narrowing should survive after terminating false branch, got: {diagnostics:?}"
    );
}

#[test]
fn exhaustive_typeof_chain_on_unknown_leaves_empty_object_residual() {
    let diagnostics = strict_diagnostics(
        r#"
function narrowUnknown(x: unknown) {
    if (typeof x === "string") return;
    if (typeof x === "number") return;
    if (typeof x === "boolean") return;
    if (typeof x === "undefined") return;
    if (typeof x === "object") return;
    if (typeof x === "function") return;
    if (typeof x === "symbol") return;
    if (typeof x === "bigint") return;

    const remaining: never = x;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '{}' is not assignable to type 'never'")
        }),
        "expected exhaustive typeof exclusions from unknown to leave {{}}, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message
                .contains("Type 'unknown' is not assignable to type 'never'")),
        "exhaustive typeof exclusions should not leave unknown, got: {diagnostics:?}"
    );
}

#[test]
fn exhaustive_typeof_chain_with_renamed_value_and_negated_conditions_leaves_empty_object() {
    let diagnostics = strict_diagnostics(
        r#"
function narrowCandidate(candidate: unknown) {
    if (!(typeof candidate !== "string")) return;
    if (!(typeof candidate !== "number")) return;
    if (!(typeof candidate !== "boolean")) return;
    if (!(typeof candidate !== "undefined")) return;
    if (!(typeof candidate !== "object")) return;
    if (!(typeof candidate !== "function")) return;
    if (!(typeof candidate !== "symbol")) return;
    if (!(typeof candidate !== "bigint")) return;

    const remaining: never = candidate;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '{}' is not assignable to type 'never'")
        }),
        "renamed negated typeof exclusions should leave {{}}, got: {diagnostics:?}"
    );
}

#[test]
fn partial_typeof_chain_on_unknown_stays_unknown() {
    let diagnostics = strict_diagnostics(
        r#"
function partial(x: unknown) {
    if (typeof x === "string") return;
    if (typeof x === "number") return;

    const remaining: never = x;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'unknown' is not assignable to type 'never'")
        }),
        "partial typeof exclusions should keep unknown, got: {diagnostics:?}"
    );
}

/// Regression test: type predicate narrowing with discriminated union members.
///
/// When interfaces have string literal discriminant properties (e.g., `kind: "a"`),
/// the reverse subtype check in `narrow_to_type` could produce false positives from
/// the global subtype cache, causing non-matching union members to be kept instead
/// of filtered out.
#[test]
fn test_type_predicate_narrowing_discriminated_union() {
    let source = r#"
interface A { kind: "a"; x: number }
interface B { kind: "b"; y: string }

function isA(v: A | B): v is A { return v.kind === "a"; }

declare const v: A | B;
if (isA(v)) {
    let check: A = v;  // Should work - v narrowed to A
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Should NOT have TS2322 — v is narrowed to A
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Type predicate narrowing failed for discriminated union: {ts2322:?}"
    );
}
