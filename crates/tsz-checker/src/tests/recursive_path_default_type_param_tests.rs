//! Regression tests for recursive `Path<T, K extends keyof T = keyof T>`
//! evaluating to `never` (issue #6256).
//!
//! Structural rule: when a recursive conditional type alias is parameterized
//! by `<T, K extends keyof T = keyof T>` (or any equivalent name choice) and
//! distributes over `K extends string`, the default type argument must drive
//! distribution over the union of `T`'s keys. Each distributed branch may
//! return a string literal directly or a template literal that recurses into
//! `T[K]`. The result must be the union of all resulting paths, never `never`.
//!
//! Anti-hardcoding: every rule below must work regardless of identifier
//! choices, the predicate used to detect nesting (`extends object`,
//! `extends Record<string, any>`), and the recursion call site.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const PATH_TK: &str = r#"
type Path<T, K extends keyof T = keyof T> =
  K extends string
    ? T[K] extends object
      ? K | `${K}.${Path<T[K]>}`
      : K
    : never;
"#;

const OBJ_FIXTURE: &str = "interface Obj { a: { b: { c: number } }; d: string; }";

/// Direct repro from issue #6256: `Path<T, K extends keyof T = keyof T>` with
/// `T[K] extends object` recursion. All four reachable paths must be valid.
#[test]
fn recursive_path_default_keyof_object_yields_full_path_union() {
    let source = format!(
        r#"
{PATH_TK}
{OBJ_FIXTURE}
type Paths = Path<Obj>;

const p1: Paths = "a";
const p2: Paths = "a.b";
const p3: Paths = "a.b.c";
const p4: Paths = "d";
"#
    );
    let codes = strict_codes_with_libs(&source);
    assert!(
        !codes.contains(&2322),
        "Path<Obj> must include all valid paths, not collapse to never. Got: {codes:?}"
    );
}

/// Anti-hardcoding: rename the iteration type parameter (`K` → `Key`).
/// The structural rule must not depend on identifier names.
#[test]
fn recursive_path_renamed_iter_param_yields_full_path_union() {
    let codes = strict_codes_with_libs(
        r#"
type Path<T, Key extends keyof T = keyof T> =
  Key extends string
    ? T[Key] extends object
      ? Key | `${Key}.${Path<T[Key]>}`
      : Key
    : never;

interface Obj { a: { b: { c: number } }; d: string; }
type Paths = Path<Obj>;

const p1: Paths = "a";
const p2: Paths = "a.b";
const p3: Paths = "a.b.c";
const p4: Paths = "d";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Renamed iteration param Key must not change the result. Got: {codes:?}"
    );
}

/// Anti-hardcoding: rename the value type parameter (`T` → `U`).
#[test]
fn recursive_path_renamed_value_param_yields_full_path_union() {
    let codes = strict_codes_with_libs(
        r#"
type Path<U, K extends keyof U = keyof U> =
  K extends string
    ? U[K] extends object
      ? K | `${K}.${Path<U[K]>}`
      : K
    : never;

interface Nested { x: { y: { z: number } }; m: boolean; }
type Paths = Path<Nested>;

const p1: Paths = "x";
const p2: Paths = "x.y";
const p3: Paths = "x.y.z";
const p4: Paths = "m";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Renamed value param U must not change the result. Got: {codes:?}"
    );
}

/// Anti-hardcoding: rename *both* type parameters simultaneously so neither
/// axis can be relied upon as a fixed spelling.
#[test]
fn recursive_path_renamed_both_params_yields_full_path_union() {
    let codes = strict_codes_with_libs(
        r#"
type Walk<U, P extends keyof U = keyof U> =
  P extends string
    ? U[P] extends object
      ? P | `${P}.${Walk<U[P]>}`
      : P
    : never;

interface Tree { root: { leaf: { value: string } }; flag: boolean; }
type Paths = Walk<Tree>;

const a: Paths = "root";
const b: Paths = "root.leaf";
const c: Paths = "root.leaf.value";
const d: Paths = "flag";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Renaming both T->U and K->P must not change the result. Got: {codes:?}"
    );
}

/// Variant from the issue's comment: `T[Key] extends Record<string, any>`
/// instead of `extends object`. Same structural rule applies.
#[test]
fn recursive_path_record_predicate_yields_full_path_union() {
    let codes = strict_codes_with_libs(
        r#"
type Path<T, Key extends keyof T = keyof T> =
  Key extends string
    ? T[Key] extends Record<string, any>
      ? `${Key}` | `${Key}.${Path<T[Key]>}`
      : `${Key}`
    : never;

interface Deep {
  server: { ssl: { enabled: boolean } };
  log: string;
}

type Paths = Path<Deep>;

const p1: Paths = "server";
const p2: Paths = "server.ssl";
const p3: Paths = "server.ssl.enabled";
const p4: Paths = "log";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Path with Record<string, any> predicate must produce full union. Got: {codes:?}"
    );
}

/// Negative case: an invalid path string must still emit TS2322. Confirms the
/// recursion produces the *correct* union, not a `string` widening that would
/// silently accept anything.
#[test]
fn recursive_path_default_keyof_rejects_invalid_path() {
    let source = format!(
        r#"
{PATH_TK}
{OBJ_FIXTURE}
type Paths = Path<Obj>;

const bogus: Paths = "bogus";
const wrong_leaf: Paths = "a.b.bogus";
"#
    );
    let codes = strict_codes_with_libs(&source);
    let count_2322 = codes.iter().filter(|&&c| c == 2322).count();
    assert!(
        count_2322 >= 2,
        "Invalid path strings must still produce TS2322. Got: {codes:?}"
    );
}

/// Explicit second type argument matches the default. Both forms must agree.
#[test]
fn recursive_path_explicit_arg_matches_default() {
    let source = format!(
        r#"
{PATH_TK}
{OBJ_FIXTURE}
type PathsImplicit = Path<Obj>;
type PathsExplicit = Path<Obj, keyof Obj>;

const a: PathsImplicit = "a.b.c";
const b: PathsExplicit = "a.b.c";
const c: PathsImplicit = "d";
const d: PathsExplicit = "d";
"#
    );
    let codes = strict_codes_with_libs(&source);
    assert!(
        !codes.contains(&2322),
        "Explicit `keyof Obj` second arg must match default. Got: {codes:?}"
    );
}

/// Single-level (no recursion fires): every key is a leaf. The default-keyof
/// distribution must still produce the union of top-level keys.
#[test]
fn recursive_path_single_level_yields_keys_union() {
    let source = format!(
        r#"
{PATH_TK}
interface Flat {{ x: number; y: string; z: boolean; }}
type Paths = Path<Flat>;

const a: Paths = "x";
const b: Paths = "y";
const c: Paths = "z";
"#
    );
    let codes = strict_codes_with_libs(&source);
    assert!(
        !codes.contains(&2322),
        "Single-level Path must yield the union of top-level keys. Got: {codes:?}"
    );
}
