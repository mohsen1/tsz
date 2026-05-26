//! Regression tests for `keyof` preserving precise user `unique symbol` keys
//! on class instance members.
//!
//! Structural rule: when a class instance member is declared with a computed
//! property name whose expression resolves to a concrete `unique symbol`
//! binding, `keyof` of the instance type must include that exact
//! `typeof <symbol>` singleton key, not a string-shaped synthetic key or the
//! generic `symbol` type.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_code_messages as compile_and_get_diagnostics,
    diagnostic_code_messages,
};

const PRELUDE: &str = r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;
"#;

fn no_ts2344(source: &str) {
    let full = format!("{PRELUDE}{source}");
    let diagnostics = compile_and_get_diagnostics(&full);
    let ts2344: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "expected class `keyof` to preserve the precise unique-symbol key; got: {diagnostics:#?}"
    );
}

fn diagnostics_multi(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &[],
    ))
}

#[test]
fn keyof_class_field_user_unique_symbol_key_is_precise() {
    no_ts2344(
        r#"
declare const marker: unique symbol;
class C { [marker] = 1; a = 2; }
type K = keyof C;
type _ = Expect<Equal<K, typeof marker | "a">>;
"#,
    );
}

#[test]
fn keyof_class_method_user_unique_symbol_key_is_precise_with_renamed_binding() {
    no_ts2344(
        r#"
declare const p: unique symbol;
class C { [p](): number { return 1; } label = ""; }
type K = keyof C;
type _ = Expect<Equal<K, typeof p | "label">>;
"#,
    );
}

#[test]
fn keyof_class_accessor_user_unique_symbol_key_is_precise() {
    no_ts2344(
        r#"
declare const token: unique symbol;
class C { get [token](): number { return 1; } name = ""; }
type K = keyof C;
type _ = Expect<Equal<K, typeof token | "name">>;
"#,
    );
}

#[test]
fn keyof_class_field_unique_symbol_survives_local_alias_wrapper() {
    no_ts2344(
        r#"
declare const wrapped: unique symbol;
class C { [wrapped] = 1; a = 2; }
type Box<T> = T;
type K = keyof Box<C>;
type _ = Expect<Equal<K, typeof wrapped | "a">>;
"#,
    );
}

#[test]
fn keyof_class_field_imported_unique_symbol_accepts_expected_keys() {
    let diagnostics = diagnostics_multi(
        &[
            (
                "keys.ts",
                r#"
export declare const importedKey: unique symbol;
"#,
            ),
            (
                "consumer.ts",
                r#"
import { importedKey } from "./keys";
class C { [importedKey] = 1; a = 2; }
type Box<T> = T;
type K = keyof Box<C>;
const symbolKey: K = importedKey;
const stringKey: K = "a";
const wrongKey: K = "missing";
"#,
            ),
        ],
        "consumer.ts",
    );
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "expected only the missing string key to fail assignment to imported-symbol `keyof`; got: {diagnostics:#?}"
    );
}
