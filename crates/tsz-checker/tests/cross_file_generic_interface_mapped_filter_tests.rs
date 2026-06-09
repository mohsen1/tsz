//! Filtered-key mapped types over cross-file / lazy-application generic
//! interfaces (refs #12951).
//!
//! Structural rule: when a homomorphic key-filtering mapped type
//! `{ [K in keyof W]-?: W[K] extends V<any> ? K : never }[keyof W]` is
//! instantiated (`W -> Conc`), the instantiator must NOT eagerly evaluate the
//! resulting mapped under its resolver-less (`NoopResolver`) context when the
//! per-key conditional's condition (`check_type` / `extends_type`) references a
//! *lazy application* `Application(Lazy(DefId), args)` — e.g. an imported
//! generic interface `V<any>`. Under the `NoopResolver` the per-key subtype
//! check silently fails, every surviving key collapses to the false branch, and
//! the whole indexed access reduces to `never` (tsz then reports a spurious
//! `TS2322`). The instantiator now defers such mapped types to the outer
//! evaluator (which has a real `TypeResolver`), completing the existing
//! bare-`Lazy` guard so it also covers lazy *applications*.
//!
//! The passing matrix varies the binder names, distributivity, and the
//! conditional condition (`extends V<any>` vs `extends object`) so the rule is
//! exercised structurally rather than for one fixture.
//!
//! Known remaining limitation (kept as an `#[ignore]` witness): when the
//! generic interface is referenced through a *namespace* import inside a
//! generic alias body (`import * as L; W[K] extends L.Validator<any>`), the
//! reference lowers to an opaque `Lazy(DefId)` whose body is never registered
//! for that synthesized `DefId` (the def carries no name/kind), so even the
//! outer evaluator cannot resolve it and the key filter still collapses. That
//! is a distinct cross-file interface body-registration gap in the binder /
//! checker `DefId` plumbing, tracked under #12951.

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

fn check(main: &str, lib: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(&[("main.ts", main), ("lib.ts", lib)], "main.ts", opts())
}

/// `RK<Conc>` must resolve to `"array" | "bool"` (not `never`): the positive
/// assignment `const _y: "Y" = (... as Tg)` and the membership `const _k: R =
/// "array"` both type-check, and the negative `const _bad: R = "nope"` is
/// rejected (guarding against `R` widening to `string`/`never`).
fn assert_filtered_keys_preserved(main_body: &str, lib: &str, label: &str) {
    let diags = check(main_body, lib);
    let codes: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    // Exactly one TS2322 is expected — the intentional negative `_bad` line.
    assert_eq!(
        codes.len(),
        1,
        "[{label}] expected exactly the negative-control TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        codes[0].1.contains("\"nope\""),
        "[{label}] the single TS2322 must be the negative control on `\"nope\"`, got: {codes:?}"
    );
}

const PROBE_TAIL: &str = r#"
    type Tg = "array" extends R ? "Y" : "N";
    const _y: "Y" = (null as any as Tg);
    const _k: R = "array";
    const _bad: R = "nope";
"#;

#[test]
fn named_import_distributive_filter() {
    assert_filtered_keys_preserved(
        &format!(
            r#"
            import {{ Validator }} from "./lib";
            type Conc = {{ array: Validator<number>; bool: Validator<boolean> }};
            type RK<W> = {{ [K in keyof W]-?: W[K] extends Validator<any> ? K : never }}[keyof W];
            type R = RK<Conc>;
            {PROBE_TAIL}
            "#
        ),
        "export interface Validator<T> { (props: object): null; tag?: T; }",
        "named-import-distributive",
    );
}

#[test]
fn renamed_binders_named_import() {
    // Vary every binder name so the fix cannot depend on identifier text.
    assert_filtered_keys_preserved(
        &format!(
            r#"
            import {{ Check }} from "./lib";
            type Shape = {{ array: Check<number>; bool: Check<boolean> }};
            type Keys<Source> = {{
                [Prop in keyof Source]-?: Source[Prop] extends Check<any> ? Prop : never
            }}[keyof Source];
            type R = Keys<Shape>;
            {PROBE_TAIL}
            "#
        ),
        "export interface Check<Payload> { (props: object): null; tag?: Payload; }",
        "renamed-binders",
    );
}

#[test]
fn extends_object_condition_named_import() {
    // The condition is `extends object` and the *check* side is the cross-file
    // interface application. Both keys survive because a resolved interface IS an
    // object; this guards the `check_type` side of the deferral rule (a key
    // filter must not drop keys merely because the checked type is a cross-file
    // generic-interface application).
    assert_filtered_keys_preserved(
        &format!(
            r#"
            import {{ Validator }} from "./lib";
            type Conc = {{ array: Validator<number>; bool: Validator<boolean> }};
            type RK<W> = {{ [K in keyof W]-?: W[K] extends object ? K : never }}[keyof W];
            type R = RK<Conc>;
            {PROBE_TAIL}
            "#
        ),
        "export interface Validator<T> { (props: object): null; tag?: T; }",
        "extends-object",
    );
}

#[test]
fn non_distributive_named_import() {
    assert_filtered_keys_preserved(
        &format!(
            r#"
            import {{ Validator }} from "./lib";
            type Conc = {{ array: Validator<number>; bool: Validator<boolean> }};
            type RK<W> = {{ [K in keyof W]-?: [W[K]] extends [Validator<any>] ? K : never }}[keyof W];
            type R = RK<Conc>;
            {PROBE_TAIL}
            "#
        ),
        "export interface Validator<T> { (props: object): null; tag?: T; }",
        "non-distributive",
    );
}

#[test]
fn same_file_interface_filter_unaffected() {
    // Regression guard: the local-interface form (which the instantiator could
    // already evaluate eagerly) must stay correct after the deferral change.
    assert_filtered_keys_preserved(
        &format!(
            r#"
            interface Validator<T> {{ (props: object): null; tag?: T; }}
            type Conc = {{ array: Validator<number>; bool: Validator<boolean> }};
            type RK<W> = {{ [K in keyof W]-?: W[K] extends Validator<any> ? K : never }}[keyof W];
            type R = RK<Conc>;
            {PROBE_TAIL}
            "#
        ),
        "export {};",
        "same-file",
    );
}

#[test]
#[ignore = "refs #12951: a namespace-qualified cross-file generic interface \
            (`import * as L; L.Validator<any>`) used inside a generic alias body \
            lowers to an orphan `Lazy(DefId)` with no registered body/name; even \
            the outer evaluator cannot resolve it, so the key filter still \
            collapses to `never`. Needs cross-file interface body registration in \
            the binder/checker `DefId` plumbing (separate from the instantiator \
            deferral fixed here)."]
fn namespace_import_distributive_filter_known_limitation() {
    // Witness for the remaining root cause. Unignore once the namespace-member
    // interface `DefId` resolves to its body on the evaluator path.
    assert_filtered_keys_preserved(
        &format!(
            r#"
            import * as L from "./lib";
            type Conc = {{ array: L.Validator<number>; bool: L.Validator<boolean> }};
            type RK<W> = {{ [K in keyof W]-?: W[K] extends L.Validator<any> ? K : never }}[keyof W];
            type R = RK<Conc>;
            {PROBE_TAIL}
            "#
        ),
        "export interface Validator<T> { (props: object): null; tag?: T; }",
        "namespace-import",
    );
}

// Note: the *infer-bearing* value-map regression (an `extends V<infer X>`
// per-key conditional that must keep evaluating eagerly rather than deferring)
// is guarded by the conformance fixture
// `TypeScript/tests/cases/conformance/jsx/tsxLibraryManagedAttributes.tsx`,
// which exercises the full intersection/assignability render path where the
// drift surfaces. A minimal indexed-access unit case reduces correctly even
// when deferred (the outer evaluator handles a direct `R["k"]`), so it would
// not catch the regression — the conformance baseline is the authoritative
// guard here.
