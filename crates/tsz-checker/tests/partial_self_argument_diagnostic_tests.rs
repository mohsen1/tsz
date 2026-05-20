use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

#[test]
fn homomorphic_optional_mapped_self_argument_does_not_emit_ts2345() {
    let source = r#"
type MyPartial<T> = { [K in keyof T]?: T[K] };

declare function patch<T>(target: T, update: MyPartial<T>): void;

const config = { host: "localhost", port: 5432 };
patch(config, config);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "homomorphic optional mapped self-argument should not emit TS2345: {diagnostics:#?}"
    );
}

#[test]
fn renamed_homomorphic_optional_mapped_self_argument_does_not_emit_ts2345() {
    let source = r#"
type OptionalShape<Value> = { [Prop in keyof Value]?: Value[Prop] };

declare function patch<Subject>(target: Subject, update: OptionalShape<Subject>): void;

const config = { host: "localhost", port: 5432 };
patch(config, config);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "renamed homomorphic optional mapped self-argument should not emit TS2345: {diagnostics:#?}"
    );
}

#[test]
fn homomorphic_optional_mapped_alias_operand_instantiates_to_source() {
    let source = r#"
type Boxed<T> = { value: T };
type OptionalBox<T> = { [Prop in keyof Boxed<T>]?: Boxed<T>[Prop] };

declare function patch<Subject>(target: Boxed<Subject>, update: OptionalBox<Subject>): void;

const boxed = { value: 1 };
patch(boxed, boxed);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "homomorphic optional mapped alias operand should not emit TS2345: {diagnostics:#?}"
    );
}

#[test]
fn non_homomorphic_optional_mapped_self_argument_still_emits_ts2345() {
    let source = r#"
type OptionalStrings<T> = { [K in keyof T]?: string };

declare function patch<T>(target: T, update: OptionalStrings<T>): void;

const config = { host: 1 };
patch(config, config);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2345),
        "non-homomorphic optional mapped self-argument should still emit TS2345: {diagnostics:#?}"
    );
}
