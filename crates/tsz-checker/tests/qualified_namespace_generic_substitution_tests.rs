//! Regression tests: a namespace-qualified generic **interface** reference
//! (`ns.Interface<Arg>`) must substitute its type arguments, exactly like the
//! bare-name form (`Interface<Arg>`).
//!
//! Structural rule: when a type reference is a qualified name whose resolved
//! member is a generic interface, `tsc` instantiates the interface with the
//! supplied type arguments. tsz built the qualified application through the
//! generic `TypeLowering` path, which keyed the application base to a different
//! `DefId` than the one primed with the interface's declared type parameters —
//! so the application reached relation time with an empty `type_params` list and
//! left every type parameter **unsubstituted** (a free `T`), producing a false
//! `TS2322` (`Type 'T' is not assignable to type 'number'`) on assignment. The
//! qualified path now primes the resolved member's `DefId`
//! (`ensure_def_ready_for_lowering`) and builds `Application(Lazy(def), args)`
//! off it — the same shape the bare-name path produces — when the primed def is
//! a generic interface.
//!
//! Scoped to interface members: classes carry a constructor/instance split and
//! type aliases a distinct alias-body lowering path, both already handled by the
//! existing qualified path. Refs #13212 (cross-arena type-parameter
//! substitution family).

use tsz_checker::test_utils::{check_multi_file_with_global_index, check_source_code_messages};
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn codes_multi(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file_with_global_index(files, entry, strict_opts())
        .iter()
        .map(|d| d.code)
        .collect()
}

const BOX: &str = "export interface Box<T> { value: T; }\n";

/// Witness: `import * as ns; const x: ns.Box<number>` substitutes `T = number`,
/// so a matching object literal is assignable and the member reads as `number`.
#[test]
fn qualified_namespace_generic_interface_substitutes_type_arg() {
    let codes = codes_multi(
        &[
            ("base.ts", BOX),
            (
                "use.ts",
                "import * as B from './base';\n\
                 const x: B.Box<number> = { value: 1 };\n\
                 const y: number = x.value;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        !codes.contains(&2322),
        "qualified ns.Box<number> must substitute T=number (no false TS2322); got: {codes:?}"
    );
}

/// Multiple type parameters are substituted positionally.
#[test]
fn qualified_namespace_generic_interface_multiple_type_params() {
    let codes = codes_multi(
        &[
            ("base.ts", "export interface Pair<A, B> { a: A; b: B; }\n"),
            (
                "use.ts",
                "import * as M from './base';\n\
                 const p: M.Pair<number, string> = { a: 1, b: 'x' };\n\
                 const a: number = p.a;\n\
                 const b: string = p.b;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        !codes.contains(&2322),
        "qualified ns.Pair<number, string> must substitute both params; got: {codes:?}"
    );
}

/// Anti-hardcoding: a renamed namespace alias and renamed binders behave
/// identically — the result is not keyed on any particular identifier text.
#[test]
fn qualified_namespace_generic_interface_renamed_binders() {
    let codes = codes_multi(
        &[
            (
                "schema.ts",
                "export interface Wrapper<Payload> { contents: Payload; }\n",
            ),
            (
                "consumer.ts",
                "import * as Schemas from './schema';\n\
                 const entry: Schemas.Wrapper<string> = { contents: 'ok' };\n\
                 const read: string = entry.contents;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        !codes.contains(&2322),
        "renamed qualified ns.Wrapper<string> must substitute; got: {codes:?}"
    );
}

/// Same-file namespace member: `namespace N { interface Box<T> }` then
/// `N.Box<number>` substitutes too (single-arena path).
#[test]
fn same_file_namespace_generic_interface_substitutes() {
    let codes = check_source_code_messages(
        "namespace N { export interface Box<T> { value: T; } }\n\
         const x: N.Box<number> = { value: 1 };\n\
         const y: number = x.value;\n",
    )
    .into_iter()
    .map(|(code, _)| code)
    .collect::<Vec<_>>();
    assert!(
        !codes.contains(&2322),
        "same-file N.Box<number> must substitute T=number; got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls — substitution must not paper over real errors.
// ---------------------------------------------------------------------------

/// A genuine member mismatch must still surface `TS2322` (the substitution is
/// real, not a blanket suppression).
#[test]
fn qualified_namespace_generic_interface_keeps_real_mismatch() {
    let codes = codes_multi(
        &[
            ("base.ts", BOX),
            (
                "use.ts",
                "import * as B from './base';\n\
                 const x: B.Box<number> = { value: 'oops' };\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        codes.contains(&2322),
        "string value assigned to ns.Box<number> must still emit TS2322; got: {codes:?}"
    );
}

/// A non-generic qualified interface used with type arguments must still emit
/// `TS2315` — the fast path is gated on the member actually being generic.
#[test]
fn qualified_namespace_non_generic_interface_still_ts2315() {
    let codes = codes_multi(
        &[
            ("base.ts", "export interface Plain { x: number; }\n"),
            (
                "use.ts",
                "import * as B from './base';\n\
                 type D = B.Plain<number>;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        codes.contains(&2315),
        "non-generic ns.Plain<number> must still emit TS2315; got: {codes:?}"
    );
}
