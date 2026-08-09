//! Regression tests for issue #17091: `export = N` (or `export { N }`), where
//! `N` is an *imported* alias resolving to an uninstantiated namespace (every
//! member is a type, or the namespace is empty), must not report TS2708
//! ("Cannot use namespace 'N' as a value.").
//!
//! Structural rule: an export assignment/declaration exports whatever meaning
//! a name carries — value OR type — so naming an alias whose only meaning is
//! a namespace's TYPE is legal, exactly as it already is for a *local*
//! uninstantiated namespace and for a plain type-only alias. tsz's
//! `alias_resolves_to_uninstantiated_namespace` guard
//! (`types/computation/identifier/resolved.rs`) unconditionally reported
//! TS2708 for any value-position use, missing the export-target exception the
//! two sibling guards (local-namespace, type-only-alias) already had.
//!
//! Both conditions are load-bearing: an *instantiated* imported namespace
//! must still resolve as a value (no regression), and a genuine property
//! access through a namespace alias (`import * as D; export = D.N`) is a
//! different code path and must keep reporting TS2339 for a missing member.
//!
//! Oracle-verified against pinned `typescript@7.0.2` (`--module commonjs`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            no_lib: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn export_equals_imported_alias_to_interface_only_namespace_is_clean() {
    let dep = r#"export namespace N { export interface Q {} }"#;
    let main = r#"
import { N } from "./dep";
export = N;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <imported alias to an interface-only namespace> must not report TS2708; got {codes:?}"
    );
}

#[test]
fn export_equals_imported_alias_to_nested_type_only_namespace_is_clean() {
    let dep = r#"export namespace Outer { export namespace Inner { export interface Q {} } }"#;
    let main = r#"
import { Outer } from "./dep";
export = Outer;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <imported alias to a nested all-type namespace> must not report TS2708; got {codes:?}"
    );
}

#[test]
fn export_equals_imported_alias_to_empty_namespace_is_clean() {
    let dep = r#"export namespace Empty { }"#;
    let main = r#"
import { Empty } from "./dep";
export = Empty;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <imported alias to an empty namespace> must not report TS2708; got {codes:?}"
    );
}

#[test]
fn export_brace_imported_alias_to_type_only_namespace_is_clean() {
    let dep = r#"export namespace Widget { export interface Shape {} }"#;
    let main = r#"
import { Widget } from "./dep";
export { Widget };
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export {{ <imported alias to a type-only namespace> }} must not report TS2708; got {codes:?}"
    );
}

#[test]
fn export_equals_imported_alias_to_instantiated_namespace_stays_clean_regression_guard() {
    // Regression control: an *instantiated* imported namespace (has a real
    // value member) was already correctly clean before this fix and must
    // stay clean — this exercises the same alias-resolution path with the
    // opposite `is_instantiated` verdict.
    let dep = r#"export namespace Counter { export const total = 1; }"#;
    let main = r#"
import { Counter } from "./dep";
export = Counter;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <imported alias to an instantiated namespace> must stay clean; got {codes:?}"
    );
}

#[test]
fn export_equals_local_uninstantiated_namespace_stays_clean_regression_guard() {
    // Regression control: a *local* (non-imported) uninstantiated namespace
    // was already correctly clean via the sibling `is_namespace` guard and
    // must not regress from this alias-specific fix.
    let main = r#"
namespace Local { export interface Q {} }
export = Local;
"#;
    let codes = codes(&[("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <local uninstantiated namespace> must stay clean; got {codes:?}"
    );
}

#[test]
fn export_equals_namespace_alias_property_access_does_not_gain_ts2708() {
    // Regression control: `import * as D; export = D.N` is a property
    // access on the namespace-import object, not a bare alias identifier —
    // a different code path entirely, untouched by the alias-identifier fix
    // above. tsc reports TS2339 for the missing member here (oracle-verified,
    // pinned `typescript@7.0.2`), but the `no_lib` unit harness cannot
    // reproduce that specific code exactly (it emits lib-dependent TS2318s
    // instead of TS2339 without real lib types loaded — a harness gap, not a
    // behavior this fix touches). What this fix must not do is turn a
    // property-access miss into a namespace-as-value TS2708; assert that.
    let dep = r#"export namespace N { export interface Q {} }"#;
    let main = r#"
import * as D from "./dep";
export = D.N;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <property access through a namespace import> must not report TS2708 (that would be this fix regressing into the wrong diagnostic); got {codes:?}"
    );
}

#[test]
fn export_equals_renamed_imported_alias_to_type_only_namespace_is_clean() {
    // Structural, not name-driven: rename both the namespace and the local
    // binding to confirm the guard is keyed on the parent-node shape, not on
    // any specific identifier text.
    let dep = r#"export namespace Zephyr { export interface Signal {} }"#;
    let main = r#"
import { Zephyr as Renamed } from "./dep";
export = Renamed;
"#;
    let codes = codes(&[("/proj/dep.ts", dep), ("/proj/m.ts", main)], "/proj/m.ts");
    assert!(
        !codes.contains(&2708),
        "export = <renamed imported alias to a type-only namespace> must not report TS2708; got {codes:?}"
    );
}
