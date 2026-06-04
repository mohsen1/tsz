#[test]
fn mapped_type_generic_indexed_access_no_ts2349() {
    // Repro from TypeScript#49338: element access with a generic key on a mapped
    // type should produce a callable result via solver template substitution,
    // not TS2349 "This expression is not callable".
    let source = r#"
type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

declare const typeHandlers: TypeHandlers;
const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2349),
        "generic indexed access into mapped type should be callable, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2344),
        "generic indexed access into mapped type should preserve the `keyof TypesMap` constraint, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "mapped type object literal handlers should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_application_generic_indexed_call_preserves_key_correlation() {
    // Structural rule: indexing a homomorphic mapped alias application with a
    // generic key preserves the key in the callable template. The return type is
    // Model[Key], not the union Model[keyof Model].
    let source = r#"
type Readers<T> = { [K in keyof T]: (value: T[K]) => T[K] };

type Model = {
    alpha: { tag: "alpha"; value: number };
    beta: { tag: "beta"; value: string };
};

declare const model: Model;
declare const readers: Readers<Model>;

function read<Key extends keyof Model>(key: Key): Model[Key] {
    return readers[key](model[key]);
}
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "homomorphic mapped alias application indexed with a generic key should keep return correlation, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2345),
        "homomorphic mapped alias application indexed with a generic key should keep argument correlation, got: {diagnostics:?}"
    );
}

#[test]
fn renamed_mapped_application_generic_indexed_call_preserves_key_correlation() {
    // Same rule with different type parameter and mapped variable names to guard
    // against spelling-based fixes.
    let source = r#"
type Accessors<Input> = { [Slot in keyof Input]: (item: Input[Slot]) => Input[Slot] };

type Store = {
    left: { side: "left"; count: number };
    right: { side: "right"; label: string };
};

declare const store: Store;
declare const accessors: Accessors<Store>;

function get<X extends keyof Store>(slot: X): Store[X] {
    return accessors[slot](store[slot]);
}
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "renamed homomorphic mapped alias application should keep return correlation, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2345),
        "renamed homomorphic mapped alias application should keep argument correlation, got: {diagnostics:?}"
    );
}

#[test]
fn concrete_union_callable_still_rejects_uncorrelated_union_argument() {
    let source = r#"
declare const fnUnion:
    ((value: { tag: "alpha"; value: number }) => { tag: "alpha"; value: number })
    | ((value: { tag: "beta"; value: string }) => { tag: "beta"; value: string });
declare const value:
    { tag: "alpha"; value: number }
    | { tag: "beta"; value: string };

fnUnion(value);
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diagnostics, 2345),
        "uncorrelated concrete union calls should still be rejected, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_as_clause_key_collision_keeps_first_source_modifiers() {
    // Structural rule: when an `as` clause remaps several source keys onto the
    // same output name, tsc unions the value contributions but keeps the
    // optional/readonly modifiers of the FIRST source key in declaration order
    // (see `resolveMappedTypeMembers`). The first key `a` here is
    // `readonly a?`, so the merged `x` is readonly+optional and assigning to it
    // is rejected with TS2540.
    let source = r#"
type Source = { readonly a?: number; b: string; c: boolean };
type Merged = { [K in keyof Source as "x"]: Source[K] };
declare const merged: Merged;
merged.x = 1;
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diagnostics, 2540),
        "merged collision key inherits readonly from the first source key, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_as_clause_key_collision_merges_value_union() {
    // The merged collision key accepts any contribution of the unioned value
    // type. `{ x: 1 }` satisfies `Merged` because `x` is
    // `number | string | boolean | undefined` (optional, from first key `a`).
    let source = r#"
type Source = { readonly a?: number; b: string; c: boolean };
type Merged = { [K in keyof Source as "x"]: Source[K] };
const ok: Merged = { x: 1 };
const ok2: Merged = { x: "s" };
const ok3: Merged = {};
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "merged collision key value union should accept each contribution, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2741),
        "merged collision key is optional, so empty object is allowed, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_as_clause_key_collision_first_key_not_readonly_allows_write() {
    // When the first source key is neither readonly nor optional, the merged key
    // is writable: renaming `Renamed` to guard against spelling-based fixes.
    let source = r#"
type Bag = { first: number; readonly second?: string };
type Renamed = { [Slot in keyof Bag as "merged"]: Bag[Slot] };
declare const bag: Renamed;
bag.merged = 1;
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2540),
        "merged key inherits writability from the first source key, got: {diagnostics:?}"
    );
}
