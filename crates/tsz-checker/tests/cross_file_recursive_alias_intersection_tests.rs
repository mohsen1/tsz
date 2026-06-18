//! Regression tests: a cross-file generic type alias whose body is an
//! intersection that references another declaration in the same file
//! (`type Out<T> = Iface<T> & { … }`) must not send the checker into unbounded
//! recursion when the alias is imported and instantiated.
//!
//! Structural rule: raw `SymbolId`s are file-local and collide across per-file
//! binders, so the same numeric id can name a declared type in the alias's own
//! file and an import alias in the importing file. That collision can make the
//! alias and its inner reference resolve to each other (`Out` ↔ `Iface`),
//! producing a self-referential instantiated body. Resolving such a body would
//! recurse — through `resolve_property_access_with_env`'s intersection-member
//! walk and through `type_reference_symbol_type_with_params`'s alias-forwarding
//! recursion — until the stack overflows and the whole compile aborts. Bounded
//! depth guards on those two recursion paths turn the crash into bounded,
//! deterministic resolution. This shape is exactly valibot's
//! `OutputDataset<O, I> = Dataset<O, I> & { … }` (#13212).
//!
//! The guards are keyed on recursion depth, not on any identifier, so the tests
//! vary binder names to confirm the fix is structural.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(files, entry, strict_opts())
}

/// Reading a member of a single-parameter alias-intersection imported from
/// another file resolves cleanly (the inner interface member is substituted),
/// matching `tsc`. Previously this stack-overflowed.
#[test]
fn member_read_through_imported_alias_intersection_is_clean() {
    let diags = check(
        &[
            (
                "base.ts",
                "export interface Dataset<O> { value: O; }\n\
                 export type OutputDataset<O> = Dataset<O> & { meta: 1 };\n",
            ),
            (
                "use.ts",
                "import type { OutputDataset } from './base';\n\
                 export function read(x: OutputDataset<boolean>) { return x.value; }\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics reading a member through an imported alias \
         intersection; got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Same shape with different binder names and an explicit return annotation —
/// confirms the fix is not keyed on any identifier.
#[test]
fn renamed_binders_member_read_is_clean() {
    let diags = check(
        &[
            (
                "schema.ts",
                "export interface Payload<T> { body: T; }\n\
                 export type Wrapped<T> = Payload<T> & { tag: 'x' };\n",
            ),
            (
                "consumer.ts",
                "import type { Wrapped } from './schema';\n\
                 export function take(w: Wrapped<string>): string { return w.body; }\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics for renamed alias-intersection member read; got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The alias-intersection re-exported through a type-only barrel and then
/// instantiated also resolves cleanly without recursing.
#[test]
fn member_read_through_reexported_alias_intersection_is_clean() {
    let diags = check(
        &[
            (
                "core.ts",
                "export interface Cell<O> { value: O; }\n\
                 export type Boxed<O> = Cell<O> & { frozen: true };\n",
            ),
            ("barrel.ts", "export type { Boxed } from './core';\n"),
            (
                "app.ts",
                "import type { Boxed } from './barrel';\n\
                 export function get(b: Boxed<number>): number { return b.value; }\n",
            ),
        ],
        "app.ts",
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics through a re-exported alias intersection; got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Const assignment of a well-typed object literal to a single-parameter
/// alias-intersection annotation is accepted (the inner member is substituted),
/// matching `tsc`.
#[test]
fn const_assignment_to_imported_alias_intersection_is_clean() {
    let diags = check(
        &[
            (
                "base.ts",
                "export interface Dataset<O> { value: O; }\n\
                 export type OutputDataset<O> = Dataset<O> & { meta: 1 };\n",
            ),
            (
                "use.ts",
                "import type { OutputDataset } from './base';\n\
                 export const ok: OutputDataset<boolean> = { value: true, meta: 1 };\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics for a well-typed alias-intersection assignment; got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Robustness: the two-parameter / tuple-bearing shape (valibot's
/// `OutputDataset<O, I> = Dataset<O, I> & { issues?: [I] }`) and the
/// import-both-names shape used to crash the compiler with a stack overflow.
/// They must now terminate. The exact residual diagnostics belong to the
/// separate canonical cross-arena instantiation work (#13212 F1); this test
/// only asserts the compile completes instead of aborting the process.
#[test]
fn pathological_shapes_terminate_without_crashing() {
    // Two-parameter tuple-bearing alias intersection (valibot's shape).
    let _ = check(
        &[
            (
                "schema.ts",
                "export interface Dataset<O, I> { readonly value: O; readonly typed: boolean; }\n\
                 export type OutputDataset<O, I> = Dataset<O, I> & { readonly issues?: [I]; };\n",
            ),
            (
                "boolean.ts",
                "import type { OutputDataset } from './schema';\n\
                 export function f(x: OutputDataset<boolean, string>) { return x.value; }\n",
            ),
        ],
        "boolean.ts",
    );

    // Importing both the alias and its inner interface (raw-SymbolId collision).
    let _ = check(
        &[
            (
                "base.ts",
                "export interface Dataset<O> { value: O; }\n\
                 export type OutputDataset<O> = Dataset<O> & { meta: 1 };\n",
            ),
            (
                "use.ts",
                "import type { OutputDataset, Dataset } from './base';\n\
                 export const x: OutputDataset<boolean> = { value: true, meta: 1 };\n",
            ),
        ],
        "use.ts",
    );
}
