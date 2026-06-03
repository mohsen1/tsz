//! Regression tests: no false TS2315 on imported generic type aliases.
//!
//! Structural rule: when a generic type alias is exported from one file and
//! imported in another, applying type arguments to the import must not emit
//! TS2315 ("Type 'X' is not generic").  The checker resolves the import alias
//! to the original declaration's symbol before inspecting type parameters;
//! `symbol_declaration_has_type_parameters` must find parameters in the
//! cross-file arena for that resolved symbol.
//!
//! Reproduces the ts-toolbelt false-positive family where `HasPath<…>` and
//! `ComputeRaw<…>` were incorrectly flagged as non-generic.

use tsz_checker::test_utils::check_multi_file;
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

/// A simple mapped-type alias exported from one file and applied with type
/// args in another must not produce TS2315.
#[test]
fn no_ts2315_on_imported_mapped_generic_alias() {
    let diags = check_multi_file(
        &[
            (
                "types.ts",
                r#"
export type Compute<O extends object> = { [K in keyof O]: O[K] };
"#,
            ),
            (
                "main.ts",
                r#"
import type { Compute } from './types';
type A = Compute<{ x: number; y: string }>;
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    let ts2315: Vec<_> = diags.iter().filter(|d| d.code == 2315).collect();
    assert!(
        ts2315.is_empty(),
        "False TS2315 on imported mapped generic; got: {ts2315:?}"
    );
}

/// Conditional type alias with two type params.
#[test]
fn no_ts2315_on_imported_conditional_generic_alias() {
    let diags = check_multi_file(
        &[
            (
                "types.ts",
                r#"
export type HasPath<O extends object, P extends string> =
    P extends keyof O ? true : false;
"#,
            ),
            (
                "main.ts",
                r#"
import type { HasPath } from './types';
type B = HasPath<{ x: number }, 'x'>;
type C = HasPath<{ x: number }, 'missing'>;
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    let ts2315: Vec<_> = diags.iter().filter(|d| d.code == 2315).collect();
    assert!(
        ts2315.is_empty(),
        "False TS2315 on imported conditional generic with two params; got: {ts2315:?}"
    );
}

/// Re-export through an intermediate barrel.  The declaration lives in
/// `impl.ts`, is re-exported from `index.ts`, and imported in `main.ts`.
#[test]
fn no_ts2315_on_re_exported_generic_alias() {
    let diags = check_multi_file(
        &[
            (
                "impl.ts",
                r#"
export type ComputeRaw<A extends object> = A extends Function
    ? A
    : { [K in keyof A]: A[K] } & unknown;
"#,
            ),
            (
                "index.ts",
                r#"
export type { ComputeRaw } from './impl';
"#,
            ),
            (
                "main.ts",
                r#"
import type { ComputeRaw } from './index';
type X = ComputeRaw<{ a: number }>;
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    let ts2315: Vec<_> = diags.iter().filter(|d| d.code == 2315).collect();
    assert!(
        ts2315.is_empty(),
        "False TS2315 on re-exported generic alias through barrel; got: {ts2315:?}"
    );
}

/// Multiple generic aliases imported at once — ensures the fix scales to
/// several imports in the same file.
#[test]
fn no_ts2315_on_multiple_imported_generic_aliases() {
    let diags = check_multi_file(
        &[
            (
                "types.ts",
                r#"
export type Id<T> = T;
export type Pair<A, B> = [A, B];
export type Triple<A, B, C> = [A, B, C];
"#,
            ),
            (
                "main.ts",
                r#"
import type { Id, Pair, Triple } from './types';
type X = Id<number>;
type Y = Pair<string, boolean>;
type Z = Triple<number, string, boolean>;
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    let ts2315: Vec<_> = diags.iter().filter(|d| d.code == 2315).collect();
    assert!(
        ts2315.is_empty(),
        "False TS2315 on one of several imported generics; got: {ts2315:?}"
    );
}

/// Negative: TS2315 MUST still fire when a non-generic alias is applied
/// with type args across a module boundary.
#[test]
fn ts2315_fires_on_imported_non_generic_alias_with_type_args() {
    let diags = check_multi_file(
        &[
            ("types.ts", "export type Plain = string;\n"),
            (
                "main.ts",
                r#"
import type { Plain } from './types';
type X = Plain<number>;
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    let ts2315: Vec<_> = diags.iter().filter(|d| d.code == 2315).collect();
    assert!(
        !ts2315.is_empty(),
        "TS2315 must fire on imported non-generic alias used with type args; got no TS2315 in: {diags:?}"
    );
}
