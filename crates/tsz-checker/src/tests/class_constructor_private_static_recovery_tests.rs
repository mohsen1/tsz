//! Current-class static private member lookup during constructor-side
//! re-entry must recover the owning class member without exposing inherited
//! private members.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn class_name_static_private_self_reference_in_generic_method_is_clean() {
    let codes = diagnostic_codes(
        r#"
class CacheBox<Name extends string> {
  private static store: CacheBox<string>[] = [];

  cast(_name: ([Name] extends [string] ? string : string)) {}

  pushThis() {
    CacheBox.store.push(this);
  }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "expected no TS2339 for own private static access, got {codes:?}"
    );
}

#[test]
fn renamed_class_static_private_self_reference_in_generic_method_is_clean() {
    let codes = diagnostic_codes(
        r#"
class Registry<Token extends string> {
  private static entries: Registry<string>[] = [];

  cast(_token: ([Token] extends [string] ? string : string)) {}

  remember() {
    Registry.entries.push(this);
  }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "expected no TS2339 for renamed own private static access, got {codes:?}"
    );
}
