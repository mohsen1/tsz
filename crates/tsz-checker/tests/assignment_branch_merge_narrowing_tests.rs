use tsz_checker::test_utils::check_source_strict_codes;

const TS2322: u32 = 2322;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

#[test]
fn branch_assignment_preserves_defined_local_after_intervening_read() {
    let diagnostics = codes(
        r#"
declare const arr: (number | undefined)[];
declare function maybeNumber(): number | undefined;

function f() {
    let value = maybeNumber();
    if (!value) {
        value = 5;
        arr.push(value);
    }
    const z: number = value;
}
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "branch assignment should narrow value to number after the merge, got {diagnostics:?}"
    );
}

#[test]
fn branch_without_assignment_still_keeps_possibly_undefined() {
    let diagnostics = codes(
        r#"
declare function maybeNumber(): number | undefined;

function f() {
    let value = maybeNumber();
    if (!value) {
        value;
    }
    const z: number = value;
}
"#,
    );

    assert!(
        diagnostics.contains(&TS2322),
        "without a branch assignment the value can still be undefined, got {diagnostics:?}"
    );
}

#[test]
fn branch_array_literal_assignment_narrows_returned_let_binding() {
    // deepkit-type repro for issue #14219: an array-literal RHS reassignment in a
    // guard must kill the `undefined` member of the `let` binding so the inferred
    // return type is `string[]`, not `string[] | undefined`.
    let diagnostics = codes(
        r#"
declare function maybeLabels(): string[] | undefined;

function getLabels() {
    let value = maybeLabels();
    if (!value) {
        value = ["a"];
    }
    return value;
}

const n: number = getLabels().length;
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "array-literal branch assignment should narrow value to string[], got {diagnostics:?}"
    );
}

#[test]
fn branch_object_literal_assignment_narrows_returned_let_binding() {
    // Object-literal sibling of #14219: the object-literal RHS must also narrow
    // away `undefined` through the flow fallback resolver.
    let diagnostics = codes(
        r#"
declare function maybeConfig(): { a: number } | undefined;

function getConfig() {
    let value = maybeConfig();
    if (!value) {
        value = { a: 1 };
    }
    return value;
}

const n: number = getConfig().a;
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "object-literal branch assignment should narrow value to {{ a: number }}, got {diagnostics:?}"
    );
}

#[test]
fn branch_parameter_identifier_assignment_narrows_inferred_return() {
    // #14728 (kysely camel-case `memoize`): a guard reassignment whose RHS is a
    // plain parameter identifier must narrow away `undefined` for the INFERRED
    // return type. The inferred-return walk evaluates only return expressions and
    // `if` conditions, so the sibling `mapped = fallback` is never checked; the
    // flow fallback resolver must still recover the parameter's declared type
    // (`string`) instead of leaking an opaque, unresolved `Lazy`.
    let diagnostics = codes(
        r#"
declare function maybeLabel(): string | undefined;

function pickLabel(fallback: string) {
    let mapped = maybeLabel();
    if (!mapped) {
        mapped = fallback;
    }
    return mapped;
}

const label: string = pickLabel("x");
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "parameter-identifier branch assignment should narrow the inferred return to string, got {diagnostics:?}"
    );
}

#[test]
fn branch_local_const_assignment_narrows_inferred_return() {
    // Same family with a block-scoped `const` local as the reassignment RHS, and
    // deliberately renamed binders so the fix cannot key on identifier text.
    let diagnostics = codes(
        r#"
declare function maybeTitle(): string | undefined;

function resolveTitle(raw: string) {
    const trimmed: string = raw;
    let chosen = maybeTitle();
    if (!chosen) {
        chosen = trimmed;
    }
    return chosen;
}

const title: string = resolveTitle("x");
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "local-const branch assignment should narrow the inferred return to string, got {diagnostics:?}"
    );
}

#[test]
fn branch_property_access_assignment_narrows_inferred_return() {
    // The reassignment RHS is a property access on an annotated object parameter.
    // The flow fallback must resolve the object type-literal annotation so the
    // property read yields `string`, not `any` over an opaque `Lazy`.
    let diagnostics = codes(
        r#"
declare function maybeName(): string | undefined;

function resolveName(source: { key: string }) {
    let picked = maybeName();
    if (!picked) {
        picked = source.key;
    }
    return picked;
}

const name: string = resolveName({ key: "x" });
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "property-access branch assignment should narrow the inferred return to string, got {diagnostics:?}"
    );
}

#[test]
fn branch_generic_identifier_assignment_infers_type_parameter() {
    // Generic body: the inferred return must be `T`, not `T | undefined`, when the
    // guard reassigns the binding from another `T`-typed parameter.
    let diagnostics = codes(
        r#"
function coalesce<T>(input: T | undefined, fallback: T) {
    let resolved = input;
    if (!resolved) {
        resolved = fallback;
    }
    return resolved;
}

const out: number = coalesce<number>(undefined, 1);
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "generic identifier branch assignment should infer the return as T, got {diagnostics:?}"
    );
}

#[test]
fn branch_genuine_undefined_path_keeps_possibly_undefined_inferred_return() {
    // Negative/fallback: when a path genuinely leaves the binding `undefined`
    // (the guard does not cover every entry), the inferred return must REMAIN
    // `string | undefined` so the assignment to a `string` still reports TS2322.
    let diagnostics = codes(
        r#"
declare function maybeLabel(): string | undefined;

function pickLabel(fallback: string, flag: boolean) {
    let mapped = maybeLabel();
    if (flag) {
        mapped = fallback;
    }
    return mapped;
}

const label: string = pickLabel("x", true);
"#,
    );

    assert!(
        diagnostics.contains(&TS2322),
        "a genuine undefined path must keep the inferred return possibly-undefined, got {diagnostics:?}"
    );
}
