//! Cross-file generic-alias call-parameter inference (refs #12937).
//!
//! Structural rule: when a generic call signature's parameter type is a
//! generic type alias declared in another file (`type W<T> = Opts<T>`,
//! re-exported through `useQuery`), the constraint walker must expand the
//! application `W<P>` (P = inference placeholder) to its body so the
//! signature's type parameters receive candidates from the call arguments.
//!
//! Before the fix, expanding `W<P>` produced `Opts<P>` whose nested `Opts`
//! reference — lowered as part of the cross-file alias body — survived as an
//! `UnresolvedTypeName` with no `SymbolId` in the calling file's context.
//! `expand_type_alias_application` returned `None`, the placeholder never
//! reached `queryFn: () => T`, and `T` collapsed to `unknown`. That made the
//! context-sensitive `select` callback parameter `unknown` and raised a
//! spurious `TS18046` on `data.slice(...)` where `tsc` reports nothing.
//!
//! The matrix varies the alias shape (plain alias, homomorphic mapped wrapper,
//! intersection wrapper, and the combination) and the binder names, so the fix
//! is exercised structurally rather than for one fixture.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

fn opts() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        ..CheckerOptions::default()
    }
}

const MAIN: &str = r#"
    import { useQuery } from "./dep";
    const getEntries = (): number[] => [1];
    export const r = useQuery({
        queryFn: getEntries,
        select: (data) => data.slice(0, 1),
    });
"#;

fn check(dep: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(&[("main.ts", MAIN), ("dep.ts", dep)], "main.ts", opts())
}

fn assert_no_implicit_any(dep: &str, label: &str) {
    let diags = check(dep);
    let ts18046: Vec<_> = diags.iter().filter(|d| d.code == 18046).collect();
    assert!(
        ts18046.is_empty(),
        "[{label}] expected no TS18046 (data inferred from queryFn through the imported \
         generic alias), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn plain_generic_alias_parameter() {
    assert_no_implicit_any(
        r#"
        interface Opts<TFn = unknown> { queryFn?: () => TFn; select?: (d: TFn) => unknown; }
        type W<TFn = unknown> = Opts<TFn>;
        export declare function useQuery<TFn = unknown>(options: W<TFn>): TFn;
        "#,
        "plain-alias",
    );
}

#[test]
fn homomorphic_mapped_alias_parameter() {
    assert_no_implicit_any(
        r#"
        interface Opts<TFn = unknown> { queryFn?: () => TFn; select?: (d: TFn) => unknown; }
        type Mapped<TFn = unknown> = { [K in keyof Opts<TFn>]: Opts<TFn>[K] };
        export declare function useQuery<TFn = unknown>(options: Mapped<TFn>): TFn;
        "#,
        "mapped-alias",
    );
}

#[test]
fn intersection_alias_parameter() {
    assert_no_implicit_any(
        r#"
        interface Opts<TFn = unknown> { queryFn?: () => TFn; select?: (d: TFn) => unknown; }
        type Wrapped<TFn = unknown> = Opts<TFn> & { initialData?: undefined };
        export declare function useQuery<TFn = unknown>(options: Wrapped<TFn>): TFn;
        "#,
        "intersection-alias",
    );
}

#[test]
fn mapped_and_intersection_alias_parameter() {
    // The real-world `@tanstack/vue-query` shape: a homomorphic mapped type
    // intersected with an extra member, all behind one imported alias.
    assert_no_implicit_any(
        r#"
        interface Opts<TFn = unknown> { queryFn?: () => TFn; select?: (d: TFn) => unknown; }
        type Mapped<TFn = unknown> = { [K in keyof Opts<TFn>]: Opts<TFn>[K] };
        type Wrapped<TFn = unknown> = Mapped<TFn> & { initialData?: undefined };
        export declare function useQuery<TFn = unknown>(options: Wrapped<TFn>): TFn;
        "#,
        "mapped+intersection-alias",
    );
}

#[test]
fn renamed_binders_still_infer() {
    // Vary the binder names so the fix cannot depend on any identifier text.
    assert_no_implicit_any(
        r#"
        interface QueryConfig<Payload = unknown> {
            queryFn?: () => Payload;
            select?: (d: Payload) => unknown;
        }
        type ConfigAlias<Payload = unknown> = QueryConfig<Payload> & { initialData?: undefined };
        export declare function useQuery<Payload = unknown>(
            options: ConfigAlias<Payload>,
        ): Payload;
        "#,
        "renamed-binders",
    );
}
