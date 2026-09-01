//! Regression tests: `noImplicitOverride` (TS4114/TS4115/TS4116/TS4117) must be
//! suppressed in **every** ambient spelling, not just on a literal `declare class`.
//!
//! `tsc` gates the implicit-override requirement on `nodeInAmbientContext`
//! (`node.flags & NodeFlags.Ambient`) in `checkMemberForOverrideModifier`. That flag
//! covers four spellings of the same concept:
//!
//! 1. `declare class D extends B { ... }`
//! 2. a class inside `declare namespace N { ... }`
//! 3. a class inside `declare module "m" { ... }`
//! 4. any class in a `.d.ts` file, including one written `export class D`
//!
//! tsz previously tested only spelling 1, via
//! `has_declare_modifier(&class_data.modifiers)` — a syntactic test for whether the
//! class node literally carries the `declare` keyword. Spellings 2-4 carry no
//! `declare` modifier on the class node itself, so they wrongly kept
//! `noImplicitOverride` enabled and reported a false TS4114.
//!
//! The explicit-`override`-modifier diagnostics (TS4112/TS4113) are *not* ambient
//! gated in `tsc` and must keep firing in all four spellings; those are the negative
//! controls here.
//!
//! Every expectation below was verified against `tsc@7.0.2` with
//! `--noEmit --strict --noImplicitOverride --target es2017`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_with_options};

const TS4113_MEMBER_CANNOT_HAVE_OVERRIDE: u32 = 4113;
const TS4114_MEMBER_MUST_HAVE_OVERRIDE: u32 = 4114;

fn no_implicit_override_options() -> CheckerOptions {
    CheckerOptions {
        no_implicit_override: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    check_with_options(source, no_implicit_override_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn codes_in_file(source: &str, file_name: &str) -> Vec<u32> {
    check_source(source, file_name, no_implicit_override_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn count_of(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

// ---------------------------------------------------------------------------
// Positive controls: a non-ambient class still requires `override`.
// ---------------------------------------------------------------------------

#[test]
fn plain_class_method_override_still_reports_ts4114() {
    let found = codes(
        r"
class Base { m(): void {} }
class Derived extends Base { m(): void {} }
",
    );
    assert_eq!(
        count_of(&found, TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        1,
        "non-ambient method override must still report exactly one TS4114, got: {found:?}"
    );
}

#[test]
fn plain_class_property_override_still_reports_ts4114() {
    let found = codes(
        r"
class Base { p: number = 1; }
class Derived extends Base { p: number = 2; }
",
    );
    assert_eq!(
        count_of(&found, TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        1,
        "non-ambient property override must still report exactly one TS4114, got: {found:?}"
    );
}

#[test]
fn plain_class_with_override_keyword_is_clean() {
    let found = codes(
        r"
class Base { m(): void {} }
class Derived extends Base { override m(): void {} }
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "an explicit `override` satisfies noImplicitOverride, got: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Spelling 1: `declare class` — already correct before the fix.
// ---------------------------------------------------------------------------

#[test]
fn declare_class_method_override_is_clean() {
    let found = codes(
        r"
declare class Base { m(): void; }
declare class Derived extends Base { m(): void; }
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "`declare class` is ambient; noImplicitOverride does not apply, got: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Spelling 2: `declare namespace` — the regressing case.
// ---------------------------------------------------------------------------

#[test]
fn class_in_declare_namespace_method_override_is_clean() {
    let found = codes(
        r"
declare namespace N {
  class Base { m(): void; }
  class Derived extends Base { m(): void; }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "a class inside `declare namespace` is ambient, got: {found:?}"
    );
}

#[test]
fn class_in_declare_namespace_property_override_is_clean() {
    let found = codes(
        r"
declare namespace N {
  class Base { p: number; }
  class Derived extends Base { p: number; }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "an ambient property re-declaration needs no `override`, got: {found:?}"
    );
}

#[test]
fn class_in_declare_namespace_accessor_override_is_clean() {
    let found = codes(
        r"
declare namespace N {
  class Base { get v(): number; }
  class Derived extends Base { get v(): number; }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "an ambient accessor override needs no `override`, got: {found:?}"
    );
}

#[test]
fn class_in_declare_namespace_parameter_property_override_is_clean() {
    let found = codes(
        r"
declare namespace N {
  class Base { p: number; }
  class Derived extends Base { constructor(p: number); }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "ambient constructor parameters need no `override`, got: {found:?}"
    );
}

/// Binder names are irrelevant to the rule; only the ambient context is.
#[test]
fn class_in_declare_namespace_renamed_binders_is_clean() {
    let found = codes(
        r"
declare namespace Zqx {
  class Wombat { flurble(): void; }
  class Grommet extends Wombat { flurble(): void; }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "the rule is structural, not name-driven, got: {found:?}"
    );
}

/// The `declare` sits on the outer namespace only; the inner namespace and the
/// classes inside it inherit ambient-ness through the parent chain.
#[test]
fn class_in_nested_declare_namespace_is_clean() {
    let found = codes(
        r"
declare namespace Outer {
  namespace Inner {
    class Base { z(): void; }
    class Derived extends Base { z(): void; }
  }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "ambient-ness is inherited through nested namespaces, got: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Spelling 3: `declare module "m"`.
// ---------------------------------------------------------------------------

#[test]
fn class_in_declare_module_is_clean() {
    let found = codes(
        r#"
declare module "mod" {
  class Base { w(): void; }
  class Derived extends Base { w(): void; }
}
"#,
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "a class inside `declare module` is ambient, got: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Spelling 4: `.d.ts` file, where the class carries no `declare` modifier.
// ---------------------------------------------------------------------------

#[test]
fn export_class_in_declaration_file_is_clean() {
    let found = codes_in_file(
        r"
export declare class Base { t(): void; }
export class Derived extends Base { t(): void; }
",
        "test.d.ts",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "every declaration in a .d.ts is ambient, got: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: TS4112/TS4113 are NOT ambient gated in tsc and must keep
// firing. This is the boundary the fix must not cross.
// ---------------------------------------------------------------------------

#[test]
fn explicit_override_not_in_base_still_reports_ts4113_in_declare_namespace() {
    let found = codes(
        r"
declare namespace N {
  class Base { m(): void; }
  class Derived extends Base { override notInBase(): void; }
}
",
    );
    assert!(
        found.contains(&TS4113_MEMBER_CANNOT_HAVE_OVERRIDE),
        "TS4113 is not ambient gated in tsc and must still fire, got: {found:?}"
    );
}

#[test]
fn explicit_override_not_in_base_still_reports_ts4113_in_declare_class() {
    let found = codes(
        r"
declare class Base { m(): void; }
declare class Derived extends Base { override notInBase(): void; }
",
    );
    assert!(
        found.contains(&TS4113_MEMBER_CANNOT_HAVE_OVERRIDE),
        "TS4113 must still fire on an explicit `declare class`, got: {found:?}"
    );
}

#[test]
fn explicit_override_not_in_base_still_reports_ts4113_in_declaration_file() {
    let found = codes_in_file(
        r"
export declare class Base { m(): void; }
export class Derived extends Base { override notInBase(): void; }
",
        "test.d.ts",
    );
    assert!(
        found.contains(&TS4113_MEMBER_CANNOT_HAVE_OVERRIDE),
        "TS4113 must still fire inside a .d.ts, got: {found:?}"
    );
}

/// A class with no `extends` clause takes the `report_overrides_without_base`
/// path, which threaded the raw `noImplicitOverride` option with no ambient
/// correction at all. `override` without a base is TS4112 in every context.
#[test]
fn override_without_base_class_still_reports_in_declare_namespace() {
    let found = codes(
        r"
declare namespace N {
  class Solo { override m(): void; }
}
",
    );
    assert!(
        found.iter().any(|c| *c == 4112 || *c == 4113),
        "`override` with no base class must still be reported when ambient, got: {found:?}"
    );
}

/// The no-heritage path must not invent a TS4114 for an ambient class either.
#[test]
fn ambient_class_without_base_and_without_override_is_clean() {
    let found = codes(
        r"
declare namespace N {
  class Solo { m(): void; }
}
",
    );
    assert!(
        !found.contains(&TS4114_MEMBER_MUST_HAVE_OVERRIDE),
        "a base-less ambient class has nothing to override, got: {found:?}"
    );
}
