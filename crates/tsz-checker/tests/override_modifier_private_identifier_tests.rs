//! Regression tests: `override` on a private-identifier (`#foo`) class member
//! must still be validated (TS4112/TS4113) even though private names never
//! structurally participate in inheritance.
//!
//! `tsc`'s model: a `#foo` name is lexically scoped to the class body that
//! declares it, so a derived class's `#foo` is never the same binding as a
//! same-spelled `#foo` in a base class — there is no legal spelling under
//! which `override` on a private name can be correct. tsz previously skipped
//! override checking entirely for any member whose name starts with `#`
//! (`crates/tsz-checker/src/classes/class_checker.rs`, and the mirror path in
//! `crates/tsz-checker/src/classes/class_member_info.rs` for bases resolved
//! only at the type level, e.g. cross-file generic bases) — silently dropping
//! TS4113 (base exists) and, for the type-level path, TS4112 (no base) too.
//!
//! Every expectation below was verified against the pinned `typescript@7.0.2`
//! oracle with `--noEmit --strict --lib es2022 --target es2022`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file;
use tsz_checker::test_utils::check_with_options;
use tsz_common::common::ModuleKind;

const TS4112: u32 = 4112;
const TS4113: u32 = 4113;
const TS4117: u32 = 4117;
const TS2416: u32 = 2416;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    check_with_options(source, opts())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

// ---------------------------------------------------------------------------
// TS4113: base class exists but does not (and structurally cannot) declare
// the private name.
// ---------------------------------------------------------------------------

#[test]
fn override_private_property_with_base_reports_ts4113() {
    let source = r#"
class B {}
class C extends B { override #x = 1; }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

#[test]
fn override_private_method_with_base_reports_ts4113() {
    let source = r#"
class B {}
class C extends B { override #m(): void {} }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

#[test]
fn override_private_getter_with_base_reports_ts4113() {
    let source = r#"
class B {}
class C extends B { override get #x(): number { return 1; } }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

#[test]
fn override_static_private_property_with_base_reports_ts4113() {
    let source = r#"
class B {}
class C extends B { static override #x = 1; }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

/// A private getter+setter pair independently reports TS4113 for each
/// accessor — unlike the TS2416 compat check, the accessor-pair
/// canonicalization does not apply to override-validity, matching `tsc`.
#[test]
fn override_private_accessor_pair_reports_ts4113_twice() {
    let source = r#"
class B {}
class C extends B {
  override get #x(): number { return 1; }
  override set #x(v: number) {}
}
"#;
    assert_eq!(codes(source), vec![TS4113, TS4113]);
}

/// Renamed-binder control: the class and member names carry no significance,
/// only the private-identifier shape does.
#[test]
fn override_private_property_with_base_reports_ts4113_renamed() {
    let source = r#"
class Animal {}
class Dog extends Animal { override #secretName = "Rex"; }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

/// Even when the base class declares an identically-spelled private member,
/// `tsc` still reports TS4113 — the two `#x` names are unrelated bindings,
/// lexically scoped to their own class bodies. This is the control that
/// falsifies "match by spelling"-style fixes.
#[test]
fn override_private_property_matching_base_private_name_still_reports_ts4113() {
    let source = r#"
class B { #x = 1; }
class C extends B { override #x = 2; }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

/// A generic base resolved via the AST (same file) hits the same code path.
#[test]
fn override_private_method_generic_base_reports_ts4113() {
    let source = r#"
class Base<T> { m(x: T): T { return x; } }
class Derived<T> extends Base<T> { override #priv(x: T): T { return x; } }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

/// `tsc`'s "did you mean" suggestion (TS4117) is a plain edit-distance match
/// against the base's own member names and is not privacy-aware: `#xxxx` is
/// one edit from base's public `xxxx`, so `tsc` (and tsz) suggest it despite
/// the member being private. This is not privacy-specific behavior to
/// preserve on purpose — it's a side effect of reusing the same suggestion
/// pool tsc uses — but it is the pinned oracle's actual output.
#[test]
fn override_private_property_suggests_close_base_name() {
    let diags = check_with_options(
        r#"
class B { xxxx = 1; }
class C extends B { override #xxxx = 1; }
"#,
        opts(),
    );
    assert_eq!(
        diags.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![TS4117]
    );
}

// ---------------------------------------------------------------------------
// TS4112: no base class at all. This path (`report_overrides_without_base`)
// was already correct before this fix — held here as a regression control.
// ---------------------------------------------------------------------------

#[test]
fn override_private_property_without_base_reports_ts4112() {
    let source = "class C { override #x = 1; }";
    assert_eq!(codes(source), vec![TS4112]);
}

#[test]
fn override_static_private_method_without_base_reports_ts4112() {
    let source = "class C { static override #m(): void {} }";
    assert_eq!(codes(source), vec![TS4112]);
}

// ---------------------------------------------------------------------------
// Negative controls: no `override` keyword and no `noImplicitOverride` — a
// private member is never required to declare `override`, even when a base
// class declares a same-spelled private member, since there is nothing to
// structurally implicitly override.
// ---------------------------------------------------------------------------

#[test]
fn private_property_shadowing_base_private_no_override_is_clean() {
    let source = r#"
class B { #x = 1; }
class C extends B { #x = 2; }
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn private_property_shadowing_base_private_no_implicit_override_required() {
    let diags = check_with_options(
        r#"
class B { #x = 1; }
class C extends B { #x = 2; }
"#,
        CheckerOptions {
            strict: true,
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        diags.iter().map(|d| d.code).collect::<Vec<_>>(),
        Vec::<u32>::new()
    );
}

/// A private member is never type-compatibility checked against the base
/// (TS2416), even when the derived and base private members carry
/// incompatible types — they are unrelated bindings.
#[test]
fn private_property_type_mismatch_vs_base_private_no_ts2416() {
    let diags = check_with_options(
        r#"
class B { #x: string = "a"; }
class C extends B { #x: number = 1; }
"#,
        opts(),
    );
    assert!(
        !has_code(&diags, TS2416),
        "private members must never be TS2416-compared against a base's same-spelled private member, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Positive control: an ordinary (non-private) member keeps reporting exactly
// as before — this fix must not touch the public/protected override path.
// ---------------------------------------------------------------------------

#[test]
fn override_public_property_without_matching_base_member_still_reports_ts4113() {
    let source = r#"
class B {}
class C extends B { override x = 1; }
"#;
    assert_eq!(codes(source), vec![TS4113]);
}

// ---------------------------------------------------------------------------
// Cross-file / type-level base resolution path
// (`check_override_members_against_type`, `class_member_info.rs`): a base
// imported from another module whose full instance type is resolved
// structurally rather than via a local AST class node hits a separate code
// path from the single-file cases above and needs its own coverage.
// ---------------------------------------------------------------------------

#[test]
fn override_private_method_cross_file_generic_base_reports_ts4113() {
    let diags = check_multi_file(
        &[
            (
                "./base.ts",
                r#"
export class Base<T> {
  m(x: T): T { return x; }
}
"#,
            ),
            (
                "./derived.ts",
                r#"
import { Base } from "./base";
export class Derived<T> extends Base<T> {
  override #priv(x: T): T { return x; }
}
"#,
            ),
        ],
        "./derived.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        diags.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![TS4113],
        "cross-file generic base must still validate a private override, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
