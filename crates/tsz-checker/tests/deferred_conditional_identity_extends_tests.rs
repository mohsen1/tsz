//! Regression tests for #9673: identity-style conditional comparisons over
//! unresolved conditional aliases must preserve uncertainty.
//!
//! Structural rule: when the compared function-return types contain deferred
//! conditional aliases, `tsc` cannot prove identity or non-identity for a free
//! type parameter. The identity conditional therefore remains `boolean`, and
//! constraining it to `false` reports TS2344.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_strict, check_source_with_libs, diagnostic_count,
    load_compiled_lib_files, load_default_lib_files,
};

fn assert_one_ts2344(source: &str, label: &str) {
    let diagnostics = check_source_strict(source);
    let count = diagnostic_count(&diagnostics, 2344);
    assert_eq!(
        count, 1,
        "[{label}] expected one TS2344 from boolean not satisfying false, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn identity_extends_of_deferred_conditionals_reports_boolean_constraint_error() {
    assert_one_ts2344(
        r#"
type Eq<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type Left<T> = T extends string ? 1 : 2;
type Right<T> = T extends string ? 1 : 3;
type AssertFalse<X extends false> = X;

type Bad<T> = AssertFalse<Eq<Left<T>, Right<T>>>;
"#,
        "issue repro",
    );
}

#[test]
fn identity_extends_of_deferred_conditionals_is_name_invariant() {
    assert_one_ts2344(
        r#"
type Same<X, Y> =
  (<P>() => P extends X ? "yes" : "no") extends
  (<P>() => P extends Y ? "yes" : "no") ? true : false;

type One<Q> = Q extends number ? "yes" : "no";
type Two<Q> = Q extends number ? "yes" : "maybe";
type ExpectFalse<V extends false> = V;

type Bad<Q> = ExpectFalse<Same<One<Q>, Two<Q>>>;
"#,
        "renamed params",
    );
}

#[test]
fn mapped_conditional_key_can_index_source_type() {
    let libs = load_default_lib_files();
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "index.ts",
                r#"
import { test2 } from "./other";

export function wrappedTest2<T, K extends string>(obj: T, k: K) {
  return test2(obj, k);
}

export type Obj = {
  a: number;
  readonly foo: string;
};

export const processedInternally2 = wrappedTest2({} as Obj, "a");
"#,
            ),
            (
                "other.ts",
                r#"
type OmitUnveiled<T, K extends string | number | symbol> = {
  [P in Exclude<keyof T, K>]: T[P];
};

export function test2<T, K extends string>(obj: T, k: K): OmitUnveiled<T, K> {
  return {} as any;
}
"#,
            ),
        ],
        "index.ts",
        CheckerOptions::default(),
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2536);
    assert_eq!(
        count, 0,
        "[mapped key filter] expected no TS2536, got {count}: {diagnostics:#?}"
    );
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "index.ts",
                r#"
import { test2 } from "./other";
export const processedInternally2 = test2({ a: 1, foo: "" }, "a");
"#,
            ),
            (
                "other.ts",
                r#"
type OmitReal<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type OmitUnveiled<T, K extends string | number | symbol> = {
  [P in Exclude<keyof T, K>]: T[P];
};

export function test1<T, K extends string>(obj: T, k: K): OmitReal<T, K> {
  return {} as any;
}

export function test2<T, K extends string>(obj: T, k: K): OmitUnveiled<T, K> {
  return {} as any;
}
"#,
            ),
        ],
        "other.ts",
        CheckerOptions {
            emit_declarations: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2536);
    assert_eq!(
        count, 0,
        "[mapped key filter other entry] expected no TS2536, got {count}: {diagnostics:#?}"
    );

    let diagnostics = check_source_with_libs(
        r#"
type OmitReal<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type OmitUnveiled<T, K extends string | number | symbol> = {
  [P in Exclude<keyof T, K>]: T[P];
};

export function test1<T, K extends string>(obj: T, k: K): OmitReal<T, K> {
  return {} as any;
}

export function test2<T, K extends string>(obj: T, k: K): OmitUnveiled<T, K> {
  return {} as any;
}
"#,
        "other.ts",
        CheckerOptions {
            emit_declarations: true,
            ..CheckerOptions::default()
        },
        &load_compiled_lib_files(&[
            "lib.es5.d.ts",
            "lib.es2015.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2015.symbol.d.ts",
            "lib.es2015.symbol.wellknown.d.ts",
        ]),
    );
    let count = diagnostic_count(&diagnostics, 2536);
    assert_eq!(
        count, 0,
        "[mapped key filter direct] expected no TS2536, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn mapped_identity_intersection_satisfies_index_signature_constraint() {
    let libs = load_default_lib_files();
    let diagnostics = check_source_with_libs(
        r#"
type Foo<IdentifierT extends Record<PropertyKey, PropertyKey>> = IdentifierT;
type Merge<T> = { [k in keyof T]: T[k] };
type Bar<IdentifierT extends Record<PropertyKey, PropertyKey>, T> = {
  [k in keyof T]: Foo<Merge<IdentifierT & { k: k }>>;
};
"#,
        "test.ts",
        tsz_checker::test_utils::strict_checker_options(),
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2344);
    assert_eq!(
        count, 0,
        "[mapped intersection] expected no TS2344, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn mapped_union_with_foreign_keys_still_reports_ts2536() {
    let libs = load_default_lib_files();
    let diagnostics = check_source_with_libs(
        r#"
type Bad<T, U> = {
  [P in keyof T | keyof U]: T[P];
};
"#,
        "test.ts",
        tsz_checker::test_utils::strict_checker_options(),
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2536);
    assert_eq!(
        count, 1,
        "[mapped union] expected one TS2536 for foreign keys, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn homomorphic_mapped_read_keeps_source_indexed_value_type() {
    let libs = load_default_lib_files();
    let diagnostics = check_source_with_libs(
        r#"
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };

function unbox<T>(x: Box<T>): T {
  return x.value;
}

function unboxify<T extends object>(obj: Boxified<T>): T {
  let result = {} as T;
  for (let k in obj) {
    result[k] = unbox(obj[k]);
  }
  return result;
}
"#,
        "test.ts",
        tsz_checker::test_utils::strict_checker_options(),
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2322);
    assert_eq!(
        count, 0,
        "[homomorphic mapped read] expected no TS2322, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn intersection_keyof_still_reports_when_indexing_bare_type_param() {
    let libs = load_default_lib_files();
    let diagnostics = check_source_with_libs(
        r#"
function ff3<T>(t: T, k: keyof (T & {})) {
  t[k];
}
"#,
        "test.ts",
        tsz_checker::test_utils::strict_checker_options(),
        &libs,
    );
    let count = diagnostic_count(&diagnostics, 2536);
    assert_eq!(
        count, 1,
        "[intersection keyof] expected one TS2536, got {count}: {diagnostics:#?}"
    );
}
