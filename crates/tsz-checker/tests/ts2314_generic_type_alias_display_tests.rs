//! TS2314 ("Generic type '{0}' requires {1} type argument(s).") renders the
//! generic type's name from `declarationNameToString`/`typeToString`: a generic
//! type **alias** shows its bare name (`callback`), while a generic
//! **interface**/**class** shows its declared type parameters (`Array<T>`,
//! `I<T>`). Pinned against the tsc 7.0.2 oracle
//! (`compiler/typeAliasDeclarationEmit.ts` reports `callback`, and the many
//! interface cases such as `genericInterfacesWithoutTypeArguments.ts` report
//! `I<T>`).

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2314_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2314)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn generic_type_alias_renders_bare_name_in_ts2314() {
    // `callback` is a generic type alias used without type arguments in a
    // constraint. tsc names it bare: `Generic type 'callback' requires 1 type
    // argument(s).` — not `callback<T>`.
    let source = r#"
type callback<T> = () => T;
type CallbackArray<T extends callback> = () => T;
"#;
    let messages = ts2314_messages(source);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Generic type 'callback' requires")),
        "expected the bare alias name `callback`, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("callback<")),
        "a generic type alias must not carry its type parameters in TS2314, got: {messages:?}"
    );
}

#[test]
fn generic_type_alias_bare_name_is_independent_of_binder_names() {
    // Same shape, renamed binders — the rule is structural, so the bare name
    // follows whatever the alias is called.
    let source = r#"
type Boxed<Elem> = { value: Elem };
type UsesBoxed<X extends Boxed> = X;
"#;
    let messages = ts2314_messages(source);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Generic type 'Boxed' requires")),
        "expected the bare alias name `Boxed`, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Boxed<")),
        "renaming the alias must not reintroduce its type parameters, got: {messages:?}"
    );
}

#[test]
fn generic_interface_keeps_type_parameters_in_ts2314() {
    // Control: a generic *interface* used without arguments keeps its type
    // parameters (`I<T>`), matching tsc — so the alias rule above must not
    // strip parameters from interfaces.
    let source = r#"
interface I<T> { value: T; }
type UsesI<X extends I> = X;
"#;
    let messages = ts2314_messages(source);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Generic type 'I<T>' requires")),
        "a generic interface must keep its type parameters in TS2314, got: {messages:?}"
    );
}
