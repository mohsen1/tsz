use crate::diagnostics::Diagnostic;

fn check_source_with_default_libs(source: &str) -> Vec<Diagnostic> {
    crate::test_utils::check_source_diagnostics(source)
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Filter out TS2318 ("Cannot find global type") which fires when lib files aren't loaded.
fn semantic_errors(diags: &[Diagnostic]) -> Vec<u32> {
    diags
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| d.code)
        .collect()
}

/// Minimal Promise/PromiseLike type definitions for tests.
const PROMISE_LIB: &str = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    new <T>(executor: (resolve: (value: T | PromiseLike<T>) => void, reject: (reason?: any) => void) => void): Promise<T>;
}
declare var Promise: PromiseConstructor;
"#;

#[test]
fn contextual_type_through_new_promise_variable_decl() {
    // `const p: Promise<string> = new Promise(resolve => resolve("hello"))` should
    // infer T = string from the contextual type, producing no errors.
    let source = format!(
        r#"{PROMISE_LIB}
const p: Promise<string> = new Promise(resolve => resolve("hello"));"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for contextually typed new Promise, got: {errors:?}"
    );
}

#[test]
fn contextual_type_through_await_new_promise() {
    // `const s: string = await new Promise(resolve => resolve("ok"))` should
    // infer T = string via the await contextual type union.
    let source = format!(
        r#"{PROMISE_LIB}
async function f() {{ const s: string = await new Promise(resolve => resolve("ok")); }}"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for await new Promise with contextual type, got: {errors:?}"
    );
}

#[test]
fn contextual_type_async_return_new_promise() {
    // Note: the full async return + new Promise fix requires real lib files because
    // resolve_global_interface_type("Promise") doesn't find local declarations.
    // This test verifies the code doesn't crash; the full fix is validated by
    // the contextuallyTypeAsyncFunctionReturnType conformance test.
    let source = format!(
        r#"{PROMISE_LIB}
interface Obj {{ key: "value"; }}
async function f(): Promise<Obj> {{
    return new Promise(resolve => {{
        resolve({{ key: "value" }});
    }});
}}"#
    );
    let diags = check_source_with_default_libs(&source);
    // Without real lib files, global Promise resolution fails and inference
    // falls back to unknown, producing TS2322/TS2345. This is expected.
    // The important thing is no crash and the code path executes.
    let _ = semantic_errors(&diags);
}

#[test]
fn tuple_expression_negative_index_emits_t2514() {
    // `as const` makes the literal a readonly tuple. Without it, `["a", 1]`
    // is inferred as `(string | number)[]` and TS2514 is not expected.
    let diags = check_source_with_default_libs(
        r#"
const tuple = ["a", 1] as const;
const bad = tuple[-1];
"#,
    );

    assert!(
        has_code(&diags, 2514),
        "Expected TS2514 for tuple expression negative index, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn private_name_access_unknown_reports_18046() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: unknown) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 18046).count(),
        5,
        "Expected 5 TS18046 diagnostics for private access on unknown, got: {errors:?}"
    );
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        1,
        "Expected one TS2339 diagnostic for undeclared private name, got: {errors:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("#bar")),
        "Expected the TS2339 diagnostic to mention '#bar': {diags:?}"
    );
}

#[test]
fn private_name_access_never_reports_2339() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: never) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        5,
        "Expected 5 TS2339 diagnostics for private access on never, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|code| *code == 2339),
        "Expected only TS2339 diagnostics, got: {errors:?}"
    );
}

/// Structural rule: when `strictNullChecks` is off, property access on
/// `unknown` resolves against `unknown`'s apparent type — the
/// `Object.prototype` surface (`toString`, `valueOf`, `hasOwnProperty`, ...)
/// — instead of either always failing (dot access) or masking every key to
/// `any` (element access). A genuinely missing member still reports TS2339
/// for dot access; `strictNullChecks` on is unaffected (TS18046/TS2571 still
/// fire unconditionally, matching tsc's `unknown` restriction).
mod unknown_non_strict_apparent_member_access {
    use super::*;
    use crate::test_utils::{check_source_non_strict, check_source_non_strict_codes};

    #[test]
    fn dot_access_to_object_prototype_member_is_clean() {
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
call().toString();
call().valueOf();
call().hasOwnProperty("x");
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Expected no diagnostics for Object.prototype members on non-strict unknown, got: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn dot_access_to_missing_member_still_reports_ts2339() {
        let codes = check_source_non_strict_codes(
            r#"
declare var call: { <T>(): T };
call().nonexistent();
"#,
        );
        assert_eq!(
            codes,
            vec![2339],
            "A member absent from Object.prototype must still be TS2339 under non-strict unknown"
        );
    }

    #[test]
    fn dot_access_renamed_binder_still_resolves() {
        // Same shape with different identifiers, to rule out a name-string check.
        let diags = check_source_non_strict(
            r#"
declare var produce: { <Widget>(): Widget };
produce().toString();
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Renamed binder must not change resolution: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn bracket_access_to_object_prototype_member_resolves_real_type() {
        // If tsz masked the member to `any` (its pre-fix behavior), this
        // assignment would be silently accepted instead of reporting TS2322.
        let codes = check_source_non_strict_codes(
            r#"
declare var call: { <T>(): T };
var mismatch: number = call()["toString"]();
"#,
        );
        assert_eq!(
            codes,
            vec![2322],
            "Bracket access must resolve the real Object.prototype member type, got: {codes:?}"
        );
    }

    #[test]
    fn bracket_access_to_missing_member_stays_implicit_any() {
        // Unchanged pre-fix behavior: a non-Object member via bracket access
        // on non-strict `unknown` still falls back to implicit `any`, not TS2339.
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
call()["nonexistent"]();
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Bracket access to a missing member should stay implicit any under non-strict unknown: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn general_index_access_on_unknown_is_unaffected() {
        // A non-literal (dynamic) index has no fixed name to check against
        // Object.prototype, so this path is untouched by the fix.
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
declare var key: string;
call()[key];
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "General index access on non-strict unknown must stay unaffected: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn strict_null_checks_still_rejects_object_prototype_member() {
        // Regression guard: the strict-mode gate must still fire
        // unconditionally, even for a genuine Object.prototype member. A
        // plain identifier receiver gets the named TS18046 form; the call
        // expression in the sibling test below gets the unnamed TS2571 form
        // (no printable base name) — both are the strict-mode block, neither
        // falls through to the non-strict apparent-member resolution.
        let diags = check_source_with_default_libs(
            r#"
declare var u: unknown;
u.toString();
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors,
            vec![18046],
            "strictNullChecks must still block unknown member access unconditionally: {errors:?}"
        );
    }

    #[test]
    fn strict_null_checks_still_rejects_object_prototype_member_no_printable_name() {
        let diags = check_source_with_default_libs(
            r#"
declare var call: { <T>(): T };
call().toString();
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors,
            vec![2571],
            "strictNullChecks must still block unknown member access without a printable base name: {errors:?}"
        );
    }
}

#[test]
fn inherited_static_member_element_access_emits_ts2576() {
    let diags = check_source_with_default_libs(
        r#"
class Base {
    static count = 1;
    static get size() {
        return 2;
    }
}
class Derived extends Base {}
const value = new Derived();
value["count"];
value["size"];
"#,
    );

    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2576).count(),
        2,
        "Expected TS2576 for inherited static field and accessor element access, got: {errors:?}"
    );
}

/// Element-access reads of a well-known symbol reached through a `const`
/// alias must resolve identically to the literal `Symbol.<name>` spelling
/// (#16961). These need the real `Array<T>`/`SymbolConstructor` lib shape
/// (not a hand-stub), so they route through [`check_source_with_libs`] with
/// [`load_default_lib_files`] and skip gracefully when the vendored
/// TypeScript lib assets aren't present in this checkout, matching the
/// established pattern for lib-dependent unit tests in this crate.
mod well_known_symbol_element_access_through_alias_tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source_with_libs, load_default_lib_files};

    fn check_with_default_libs(source: &str) -> Option<Vec<crate::diagnostics::Diagnostic>> {
        let libs = load_default_lib_files();
        if libs.is_empty() {
            return None;
        }
        Some(check_source_with_libs(
            source,
            "test.ts",
            CheckerOptions::default(),
            &libs,
        ))
    }

    fn semantic_errors(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
        diags.iter().map(|d| d.code).collect()
    }

    /// A well-known symbol read directly off the literal `Symbol` identifier
    /// is the baseline this whole family compares against.
    #[test]
    fn direct_symbol_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const a: unknown[];\na[Symbol.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "direct `Symbol.iterator` element access must stay clean: {diags:?}"
        );
    }

    /// `const S = Symbol; a[S.iterator]` — one `const` alias indirection.
    /// tsc resolves this identically to the direct form; tsz previously
    /// false-positived TS7015 because the read fell through to a raw
    /// `SymbolId` reinterpreted through the wrong binder (#16961).
    #[test]
    fn single_const_alias_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const a: unknown[];\nconst S = Symbol;\na[S.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "`const S = Symbol; a[S.iterator]` must stay clean like the direct form: {diags:?}"
        );
    }

    /// Binder-name independence: the alias must work under an arbitrary
    /// identifier, not just names that look related to `Symbol`.
    #[test]
    fn arbitrary_binder_name_alias_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const a: unknown[];\nconst zzTop = Symbol;\na[zzTop.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "an arbitrarily-named alias must resolve the well-known symbol just like `S`: {diags:?}"
        );
    }

    /// `const S = globalThis.Symbol; a[S.iterator]` — alias through a
    /// `globalThis.Symbol` property access initializer, not a bare identifier.
    #[test]
    fn globalthis_alias_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const a: unknown[];\nconst S = globalThis.Symbol;\na[S.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "`const S = globalThis.Symbol; a[S.iterator]` must stay clean: {diags:?}"
        );
    }

    /// `const Symbol = globalThis.Symbol; a[Symbol.iterator]` — the local
    /// binding shadows the global name `Symbol` but is itself an alias chain
    /// back to the real global, so the read must still resolve.
    #[test]
    fn shadowed_symbol_name_aliased_to_global_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const a: unknown[];\nconst Symbol = globalThis.Symbol;\na[Symbol.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "a `Symbol`-shadowing alias of the real global must still resolve `.iterator`: {diags:?}"
        );
    }

    /// `String.prototype` declares its own `[Symbol.iterator]` member (added
    /// by `lib.es2015.iterable.d.ts`), so an aliased well-known-symbol read
    /// against a `string` receiver resolves cleanly too — `tsc` reports no
    /// diagnostic here (per the oracle in #16961); this same alias fix
    /// recovers this receiver-independent case, which predates #16958 and
    /// was out of that issue's scope but shares the exact mechanism.
    #[test]
    fn string_receiver_aliased_iterator_access_is_clean() {
        let Some(diags) = check_with_default_libs(
            "export {};\ndeclare const s: string;\nconst S = globalThis.Symbol;\ns[S.iterator];\n",
        ) else {
            return;
        };
        assert_eq!(
            semantic_errors(&diags),
            Vec::<u32>::new(),
            "an aliased well-known symbol against `string` must resolve `String.prototype[Symbol.iterator]` \
             cleanly, matching tsc: {diags:?}"
        );
    }

    /// Declaration-time computed member NAMES must stay purely syntactic
    /// (tsc's `isWellKnownSymbolSyntactically`, #16307): an alias-reached
    /// `[S.iterator]` in a declaration position must NOT bind under the
    /// canonical `[Symbol.iterator]` name, unlike the read-side fix above.
    #[test]
    fn aliased_computed_name_declaration_stays_syntactic_not_well_known() {
        let Some(diags) = check_with_default_libs(
            "export {};\nconst S = Symbol;\nclass C {\n  [S.iterator]() { return 1; }\n}\ndeclare const c: C;\nconst x: IterableIterator<number> = c[Symbol.iterator]();\n",
        ) else {
            return;
        };
        let errors = semantic_errors(&diags);
        assert!(
            errors.contains(&2322),
            "an alias-reached computed member NAME must stay off the well-known `[Symbol.iterator]` \
             slot (tsc's #16307 syntactic rule), so `c[Symbol.iterator]()` must not see the aliased \
             declaration's `number` return type as an `IterableIterator<number>`: {errors:?}"
        );
    }
}
