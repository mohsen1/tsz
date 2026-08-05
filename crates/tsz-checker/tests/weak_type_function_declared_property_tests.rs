//! Regression for #16485: a weak (all-optional) object target named `call`
//! wrongly accepted a bare function source that `tsc` rejects with TS2559;
//! `apply` and every other name already rejected correctly.
//!
//! Root cause, two independent bugs stacked in the same "lawyer" (Compat)
//! weak-type gate (`CompatChecker::violates_weak_type`,
//! `crates/tsz-solver/src/relations/compat_weak.rs`):
//!
//! 1. **Structural rule.** `tsc`'s weak-type rule scans the source's own
//!    *declared* properties (`getPropertiesOfType`) — empty for a bare
//!    function or a callable value with no declared members — never its
//!    apparent type. tsz instead synthesized `call`/`apply` (and `prototype`
//!    for a constructor) as stand-in "declared" properties
//!    (`function_like_weak_type_properties`, `crates/tsz-solver/src/relations/compat.rs`),
//!    so a weak target that happened to name one of those two members always
//!    read as "shares a name" and skipped the rejection tsc always reports.
//! 2. Once (1) is fixed by using real declared properties, the synthesized
//!    set is no longer available to trigger the rule when a source has a
//!    call/construct signature but zero declared properties (a bare
//!    function's declared-property list is empty, which would otherwise read
//!    as "nothing to compare" and vacuously pass). `weak_type_source_properties`
//!    now threads that trigger through as an explicit
//!    `has_call_or_construct_signature` flag instead of folding it into the
//!    property list, mirroring tsc's
//!    `getPropertiesOfType(source).length > 0 || typeHasCallOrConstructSignatures(source)`.
//!
//! Separately, `crate::utils::has_common_property_name`'s merge-scan requires
//! both property slices sorted by `Atom`; the old synthesized-name path
//! appended `call` then `apply` in call order (not `Atom` order), which is
//! *why* only one of the two names manifested as broken — an unsorted merge
//! scan can walk past a real match depending on interning order. The fix
//! removes the unsorted synthetic list entirely rather than patching its
//! sort order.
//!
//! Rows pinned against `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, has_diagnostic_code, load_default_lib_files,
};

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

fn has_ts2559(source: &str) -> bool {
    has_diagnostic_code(&get_diagnostics(source), 2559)
}

fn is_clean(source: &str) -> bool {
    get_diagnostics(source).is_empty()
}

#[test]
fn bare_function_source_weak_target_named_call_reports_ts2559() {
    // The exact #16485 repro: only `call` was wrongly accepted.
    assert!(has_ts2559("var w4: { call?: any } = () => {};"));
}

#[test]
fn bare_function_source_weak_target_named_apply_reports_ts2559() {
    // Control: already correct before the fix, must stay correct after.
    assert!(has_ts2559("var w6: { apply?: any } = () => {};"));
}

#[test]
fn bare_function_source_weak_target_named_zzz_reports_ts2559() {
    // Control: unrelated name, already correct.
    assert!(has_ts2559("var w7: { zzz?: any } = () => {};"));
}

#[test]
fn bare_function_source_weak_target_named_bind_reports_ts2559() {
    assert!(has_ts2559("var w: { bind?: any } = () => {};"));
}

#[test]
fn named_function_declaration_weak_target_named_call_reports_ts2559() {
    // Renamed binder, declared-function form instead of an arrow literal.
    assert!(has_ts2559(
        r#"
function producer() {}
var w: { call?: any } = producer;
"#
    ));
}

#[test]
fn class_constructor_weak_target_named_call_reports_ts2560() {
    // A class constructor is call/construct-signature-like too; its own
    // declared static members do not include `call`. tsc reports the
    // constructible-value variant of this rule, TS2560 ("Did you mean to
    // call it?"), not TS2559.
    assert!(has_diagnostic_code(
        &get_diagnostics(
            r#"
class Widget {}
var w: { call?: any } = Widget;
"#
        ),
        2560
    ));
}

#[test]
fn callable_interface_with_shared_declared_property_is_not_ts2559() {
    // Positive control: a callable source that genuinely DECLARES a property
    // sharing the target's name must not be rejected by the weak rule — the
    // real per-property type check takes over instead (TS2322 here, since the
    // types disagree).
    let diags = get_diagnostics(
        r#"
interface HasA { (): void; a: string }
declare const src: HasA;
var w: { a?: number } = src;
"#,
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == 2559),
        "shared declared property must not trigger TS2559. Got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(c, _)| *c == 2322),
        "mismatched property type must still report TS2322. Got: {diags:#?}"
    );
}

#[test]
fn callable_interface_with_shared_declared_property_matching_type_is_clean() {
    let src = r#"
interface HasA { (): void; a: string }
declare const src: HasA;
var w: { a?: string } = src;
"#;
    assert!(is_clean(src), "Got: {:#?}", get_diagnostics(src));
}

#[test]
fn callable_interface_without_shared_declared_property_reports_ts2559() {
    // Negative: the callable source's declared property ("b") does not match
    // the weak target's ("call"), so tsz must not fall back to treating
    // `call` as an implicit member of every callable value.
    assert!(has_ts2559(
        r#"
interface HasB { (): void; b: string }
declare const src: HasB;
var w: { call?: any } = src;
"#
    ));
}

#[test]
fn required_call_only_target_still_accepts_a_function_control() {
    // Regression guard: a *required* single-property target (`{ call(x: any): any }`)
    // takes a different dispatcher bridge, unaffected by the weak-type fix.
    assert!(is_clean(
        r#"
declare var target: { call(x: any): any };
declare var source: (x: any) => void;
target = source;
"#
    ));
}

#[test]
fn intersection_member_weak_suppression_still_accepts_a_function_control() {
    // Regression guard: `in_intersection_member_check` suppression (a
    // different code path, `crates/tsz-solver/src/relations/subtype/rules/objects.rs`)
    // must keep accepting a function as a member of an intersection whose
    // other member is a weak object.
    let src = r#"
declare var value: (() => void) & { brand?: number };
var i: (() => void) & { brand?: number } = value;
"#;
    assert!(is_clean(src), "Got: {:#?}", get_diagnostics(src));
}

#[test]
fn genuine_weak_object_source_still_reports_ts2559_control() {
    // Regression guard: a non-function weak-vs-weak mismatch is untouched.
    assert!(has_ts2559(
        r#"
interface Weak { a?: 1; b?: 2 }
declare const noCommon: { z: 3 };
const wk: Weak = noCommon;
"#
    ));
}
