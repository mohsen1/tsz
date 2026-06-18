//! Regression tests: no false TS2315 on generic declarations reached through an
//! import or a named (type-only) re-export when used in heritage position.
//!
//! Structural rule: a heritage reference (`extends X<...>` / `implements X<...>`)
//! that is a plain identifier must resolve its type parameters through the same
//! name-aware import/re-export alias chain a type-reference position uses. The
//! raw-`SymbolId` genericity helpers stop at the unresolved import/re-export
//! alias symbol and report zero type parameters, so a re-exported generic
//! interface/class (`export type { Base } from './base'`) was falsely flagged
//! "Type 'Base' is not generic" (TS2315). The arity/genericity check now goes
//! through `get_reference_type_params_for_symbol` /
//! `count_required_reference_type_params`, which follow the chain.
//!
//! Surfaced by the valibot false-positive family (#13212): generic interface
//! heritage re-exported through barrels.

use tsz_checker::test_utils::check_multi_file;
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn ts2315(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 2315)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Witness: generic interface re-exported through a `export type { … }` barrel,
/// then used in interface `extends` position. tsc accepts it.
#[test]
fn no_ts2315_on_reexported_generic_interface_in_interface_extends() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Base<T> { value: T; }\n"),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "use.ts",
                r#"
import type { Base } from './barrel';
interface Derived extends Base<number> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on re-exported generic interface in extends; got: {found:?}"
    );
}

/// Same re-exported generic interface used in `implements` position on a class.
#[test]
fn no_ts2315_on_reexported_generic_interface_in_class_implements() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Shape<U> { id: U; }\n"),
            ("barrel.ts", "export type { Shape } from './base';\n"),
            (
                "use.ts",
                r#"
import type { Shape } from './barrel';
class Widget implements Shape<string> {
    id: string = '';
}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on re-exported generic interface in implements; got: {found:?}"
    );
}

/// Re-exported generic class used in class `extends` position.
#[test]
fn no_ts2315_on_reexported_generic_class_in_class_extends() {
    let diags = check_multi_file(
        &[
            (
                "base.ts",
                "export class Box<T> { constructor(public item: T) {} }\n",
            ),
            ("barrel.ts", "export { Box } from './base';\n"),
            (
                "use.ts",
                r#"
import { Box } from './barrel';
class NumberBox extends Box<number> {
    constructor() { super(0); }
}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on re-exported generic class in extends; got: {found:?}"
    );
}

/// Two-hop re-export chain (`use` -> `barrel2` -> `barrel1` -> `base`).
#[test]
fn no_ts2315_on_multi_hop_reexported_generic_interface() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Pair<A, B> { a: A; b: B; }\n"),
            ("barrel1.ts", "export type { Pair } from './base';\n"),
            ("barrel2.ts", "export type { Pair } from './barrel1';\n"),
            (
                "use.ts",
                r#"
import type { Pair } from './barrel2';
interface Coords extends Pair<number, number> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on multi-hop re-exported generic interface; got: {found:?}"
    );
}

/// Direct import (no barrel) of a generic interface used in `extends` — sanity
/// that the name-aware path also covers the single-hop import case.
#[test]
fn no_ts2315_on_directly_imported_generic_interface_in_extends() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Container<E> { item: E; }\n"),
            (
                "use.ts",
                r#"
import type { Container } from './base';
interface StringContainer extends Container<string> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on directly imported generic interface in extends; got: {found:?}"
    );
}

/// Renamed re-export (`export type { Base as Renamed }`) used in heritage.
#[test]
fn no_ts2315_on_renamed_reexported_generic_interface() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Original<T> { value: T; }\n"),
            (
                "barrel.ts",
                "export type { Original as Renamed } from './base';\n",
            ),
            (
                "use.ts",
                r#"
import type { Renamed } from './barrel';
interface Derived extends Renamed<boolean> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        found.is_empty(),
        "False TS2315 on renamed re-exported generic interface; got: {found:?}"
    );
}

/// Negative: TS2315 MUST still fire when a re-exported NON-generic interface is
/// used with type arguments in heritage position.
#[test]
fn ts2315_fires_on_reexported_non_generic_interface_in_extends() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Plain { x: number; }\n"),
            ("barrel.ts", "export type { Plain } from './base';\n"),
            (
                "use.ts",
                r#"
import type { Plain } from './barrel';
interface Derived extends Plain<number> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        !found.is_empty(),
        "TS2315 must fire on re-exported non-generic interface used with type args; got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative: TS2315 MUST still fire for a directly imported non-generic type
/// alias used with type arguments in heritage position.
#[test]
fn ts2315_fires_on_imported_non_generic_alias_in_extends() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export type Plain = { x: number };\n"),
            (
                "use.ts",
                r#"
import type { Plain } from './base';
interface Derived extends Plain<number> {}
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    let found = ts2315(&diags);
    assert!(
        !found.is_empty(),
        "TS2315 must fire on imported non-generic alias used with type args; got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
