//! Conditional-bodied type-alias applications lose their alias name in
//! diagnostics once the conditional reduces to a concrete type, matching `tsc`.
//!
//! `type Tail<T extends any[]> = T extends [any, ...infer R] ? R : []` applied
//! as `Tail<Src>` displays as the resolved `[number, string]`, not `Tail<Src>`,
//! independent of whether the argument is spelled inline or via a named alias.
//! Before the fix tsz kept the unreduced alias-application form whenever the
//! argument was a named alias (`Lazy(DefId)`), because the reduced result
//! retained an application display alias the `tsc` policy
//! (`prefer_application_display_alias = !resolved_has_conditional_body`)
//! withholds.
//!
//! Negative controls confirm the gate is scoped to *reduced* conditional
//! bodies: mapped-type aliases keep their name, and a still-deferred generic
//! conditional keeps its `Alias<T>` form.
//!
//! Binder/type-parameter/alias names are varied across cases so the rendering
//! is proven structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn single(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

/// Core fix: a conditional alias applied to a *named* tuple alias displays the
/// reduced tuple, not the `Tail<Src>` application form.
#[test]
fn conditional_alias_named_tuple_arg_shows_resolved_tuple() {
    let diags = check_strict(
        "type Tail<L extends any[]> = L extends [any, ...infer Rest] ? Rest : [];\n\
         type Triple = [boolean, number, string];\n\
         const bad: [number] = (null as any as Tail<Triple>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type '[number, string]' is not assignable to type '[number]'"),
        "named-alias argument should resolve to the tuple, not the alias form; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("Tail<"),
        "the reduced conditional must drop its alias name; got: {}",
        diag.message_text
    );
}

/// Regression guard: the inline-argument form already resolved and must keep
/// doing so — the named-alias fix must not regress it.
#[test]
fn conditional_alias_inline_tuple_arg_shows_resolved_tuple() {
    let diags = check_strict(
        "type Drop<S extends any[]> = S extends [unknown, ...infer Tail] ? Tail : [];\n\
         const bad: [number] = (null as any as Drop<[boolean, number, string]>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type '[number, string]' is not assignable to type '[number]'"),
        "inline-argument conditional should resolve to the tuple; got: {}",
        diag.message_text
    );
}

/// A non-distributive conditional (wrapped check type) over a named argument
/// also resolves — the gate is keyed on the conditional body, not on
/// distributivity.
#[test]
fn non_distributive_conditional_named_arg_shows_resolved_tuple() {
    let diags = check_strict(
        "type Rest<A extends any[]> = [A] extends [[any, ...infer R]] ? R : [];\n\
         type Items = [boolean, number, string];\n\
         const bad: [number] = (null as any as Rest<Items>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type '[number, string]' is not assignable to type '[number]'"),
        "non-distributive conditional should resolve to the tuple; got: {}",
        diag.message_text
    );
}

/// Conditional alias reducing to an object: the resolved object shape is shown
/// (TS2741), not the `Wrap<Arg>` application form.
#[test]
fn conditional_alias_named_arg_reducing_to_object_shows_resolved_object() {
    let diags = check_strict(
        "type Wrap<V> = V extends string ? { s: V } : { n: V };\n\
         type ArgT = number;\n\
         const bad: { q: 1 } = (null as any as Wrap<ArgT>);\n",
    );
    let diag = single(&diags, 2741);
    assert!(
        diag.message_text.contains("type '{ n: number; }'"),
        "conditional reducing to an object should show the resolved shape; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("Wrap<"),
        "the reduced conditional must drop its alias name; got: {}",
        diag.message_text
    );
}

/// Conditional alias reducing to a union over a named argument resolves to the
/// union, not the application form.
#[test]
fn conditional_alias_named_arg_reducing_to_union_shows_resolved_union() {
    let diags = check_strict(
        "type Either<P> = P extends [infer X, infer Y] ? X | Y : never;\n\
         type PairT = [number, string];\n\
         const bad: boolean = (null as any as Either<PairT>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type 'string | number' is not assignable to type 'boolean'"),
        "conditional reducing to a union should show the resolved union; got: {}",
        diag.message_text
    );
}

/// Negative control: a *mapped* type alias keeps its name (matching `tsc`).
/// The conditional-body gate must not strip aliases for non-conditional bodies.
#[test]
fn mapped_alias_named_arg_keeps_alias_name() {
    let diags = check_strict(
        "type MyReadonly<T> = { readonly [K in keyof T]: T[K] };\n\
         interface Shape { a: number; }\n\
         const bad: { z: number } = (null as any as MyReadonly<Shape>);\n",
    );
    let diag = single(&diags, 2741);
    assert!(
        diag.message_text.contains("type 'MyReadonly<Shape>'"),
        "a mapped alias must keep its name; got: {}",
        diag.message_text
    );
}

/// Negative control: a still-deferred generic conditional (free type parameter)
/// keeps its `Alias<T>` form because it has not reduced to a concrete type.
#[test]
fn deferred_generic_conditional_keeps_alias_name() {
    let diags = check_strict(
        "type Tail<T extends any[]> = T extends [any, ...infer R] ? R : [];\n\
         function f<U extends any[]>(u: U): [number] {\n\
         return null as any as Tail<U>;\n\
         }\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type 'Tail<U>' is not assignable to type '[number]'"),
        "a deferred generic conditional must keep its alias form; got: {}",
        diag.message_text
    );
}

/// Core #10914 repro: a *recursive* conditional-bodied alias must expand to a
/// fully resolved structural type — every nested helper application, down to
/// the scalar leaves, drops its name. tsc 6.0.2 renders
/// `{ readonly a: { readonly b: number; }; readonly c: string; }`, never
/// leaking `DeepReadonly<number>` / `DeepReadonly<string>` internals.
#[test]
fn recursive_conditional_alias_expands_fully_without_leaking_helper_names() {
    let diags = check_strict(
        "type DeepRO<V> = V extends object ? { readonly [K in keyof V]: DeepRO<V[K]> } : V;\n\
         type Settings = { a: { b: number }; c: string };\n\
         const bad: number = (null as any as DeepRO<Settings>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text.contains(
            "Type '{ readonly a: { readonly b: number; }; readonly c: string; }' \
             is not assignable to type 'number'"
        ),
        "recursive conditional alias must expand fully to the resolved object; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("DeepRO<"),
        "no internal helper application may leak into the diagnostic; got: {}",
        diag.message_text
    );
}

/// A conditional-bodied alias application that reduces to a *scalar* drops its
/// alias name like the object case — tsc renders `number`, never
/// `DeepRO<number>`. Binder names differ from the recursive case so the
/// rendering is proven structural.
#[test]
fn conditional_alias_reducing_to_scalar_shows_resolved_scalar() {
    let diags = check_strict(
        "type DeepRO<W> = W extends object ? { readonly [P in keyof W]: DeepRO<W[P]> } : W;\n\
         const bad: 1 = (null as any as DeepRO<number>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type 'number' is not assignable to type '1'"),
        "a scalar-reducing conditional alias must show the resolved scalar; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("DeepRO<"),
        "the reduced conditional must drop its alias name; got: {}",
        diag.message_text
    );
}

/// A conditional-bodied alias application that reduces (non-distributively) to a
/// *union* shows the resolved union, not the application form. tsc 6.0.2
/// renders `"a" | "b"`.
#[test]
fn conditional_alias_reducing_to_union_branch_shows_resolved_union() {
    let diags = check_strict(
        "type FirstTwo<S> = S extends string ? \"a\" | \"b\" : never;\n\
         const bad: 1 = (null as any as FirstTwo<\"x\">);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text
            .contains("Type '\"a\" | \"b\"' is not assignable to type '1'"),
        "a union-reducing conditional must show the resolved union; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("FirstTwo<"),
        "the reduced conditional must drop its alias name; got: {}",
        diag.message_text
    );
}
