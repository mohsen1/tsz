//! Regression tests for literal preservation during overload resolution.
//!
//! Structural rule: when a generic type parameter is inferred from an argument
//! whose type comes from a type annotation (a typed identifier, an `as` /
//! `satisfies` assertion, or an `as const` literal), the literal candidate is
//! non-fresh and must NOT be widened to its primitive — exactly as the
//! single-signature (non-overloaded) call path already behaves. Previously the
//! overload-resolution path failed to thread the type-annotation source markers
//! into inference, so an overloaded generic call (e.g. `Object.fromEntries`)
//! widened `1` to `number` purely because the call was overloaded.
//!
//! See issue #9666.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    check_source_strict, check_source_with_libs_code_messages, load_compiled_lib_files,
    strict_checker_options,
};

fn ts2322_count(source: &str) -> usize {
    check_source_strict(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// A self-contained overloaded generic function that mirrors the shape of
/// `Object.fromEntries`: a generic first overload that captures a tuple value
/// type, plus a non-generic catch-all overload that forces multi-overload
/// resolution. Uses only builtin tuple / index-signature syntax so no lib is
/// required. `value_param` is the type parameter name so we can vary it and
/// prove the fix is structural, not keyed on the spelling `T`.
fn overloaded_fe(v: &str) -> String {
    format!(
        "interface Box<T> {{ readonly v: T; }}\n\
         declare function fe<{v}>(entries: Box<readonly [string, {v}]>): {{ [k: string]: {v} }};\n\
         declare function fe(entries: Box<readonly unknown[]>): any;\n"
    )
}

#[test]
fn typed_identifier_literal_preserved_through_overload() {
    let src = format!(
        "{prelude}\
         declare const e: Box<readonly [string, 1]>;\n\
         const o = fe(e);\n\
         const good: 1 = o.foo;\n",
        prelude = overloaded_fe("V")
    );
    assert_eq!(
        ts2322_count(&src),
        0,
        "literal value `1` from a typed identifier must be preserved (not widened to `number`) \
         through overload resolution"
    );
}

#[test]
fn typed_identifier_literal_preserved_with_renamed_type_param() {
    // Same rule, different type-parameter spelling. If the fix were keyed on the
    // name `T` (or `V`) this would still widen for one of the spellings.
    for param in ["T", "K", "Elem"] {
        let src = format!(
            "{prelude}\
             declare const e: Box<readonly [string, 1]>;\n\
             const o = fe(e);\n\
             const good: 1 = o.foo;\n",
            prelude = overloaded_fe(param)
        );
        assert_eq!(
            ts2322_count(&src),
            0,
            "literal preservation must not depend on the type-parameter name `{param}`"
        );
    }
}

#[test]
fn as_const_literal_preserved_through_overload() {
    // `as const` makes the value a non-fresh literal; it must survive inference.
    let src = format!(
        "{prelude}\
         const o = fe({{ v: ['a', 1] }} as const);\n\
         const good: 1 = o.foo;\n",
        prelude = overloaded_fe("V")
    );
    assert_eq!(
        ts2322_count(&src),
        0,
        "literal value from an `as const` argument must be preserved through overload resolution"
    );
}

#[test]
fn union_literal_preserved_through_overload() {
    let src = format!(
        "{prelude}\
         declare const e: Box<readonly [string, 1 | 2]>;\n\
         const o = fe(e);\n\
         const good: 1 | 2 = o.foo;\n",
        prelude = overloaded_fe("V")
    );
    assert_eq!(
        ts2322_count(&src),
        0,
        "union literal `1 | 2` must be preserved through overload resolution"
    );
}

#[test]
fn non_literal_source_is_not_narrowed_through_overload() {
    // Negative control: a `number`-typed source stays `number`. Assigning it to
    // `1` must still error — the fix preserves existing literals, it does not
    // invent them.
    let src = format!(
        "{prelude}\
         declare const e: Box<readonly [string, number]>;\n\
         const o = fe(e);\n\
         const ok: number = o.foo;\n\
         const bad: 1 = o.foo;\n",
        prelude = overloaded_fe("V")
    );
    assert_eq!(
        ts2322_count(&src),
        1,
        "a `number` value must remain `number`; only the `bad: 1` assignment should fail"
    );
}

#[test]
fn single_overload_baseline_preserves_literal() {
    // The single-signature path already preserved literals; this guards that the
    // overload fix did not regress the non-overloaded baseline.
    let src = "interface Box<T> { readonly v: T; }\n\
               declare function fe<V>(entries: Box<readonly [string, V]>): { [k: string]: V };\n\
               declare const e: Box<readonly [string, 1]>;\n\
               const o = fe(e);\n\
               const good: 1 = o.foo;\n";
    assert_eq!(ts2322_count(src), 0);
}

fn es2019_object_libs() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2019.object.d.ts",
    ])
}

#[test]
fn real_object_from_entries_preserves_typed_identifier_literal() {
    // The exact reported repro from issue #9666, against the real lib overloads.
    let src = "declare const e: Array<[string, 1]>;\n\
               const o = Object.fromEntries(e);\n\
               const x: 1 = o.foo;\n";
    let libs = es2019_object_libs();
    let diags =
        check_source_with_libs_code_messages(src, "test.ts", strict_checker_options(), &libs);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Object.fromEntries over Array<[string, 1]> must yield {{ [k: string]: 1 }}; got {ts2322:?}"
    );
}
