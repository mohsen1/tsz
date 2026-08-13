//! An interface that extends `Function` and adds a genuine data member is not
//! the `Function` interface itself, even though it inherits `apply`/`call`/
//! `bind` and stays comfortably under the structural-fallback property-count
//! cap.
//!
//! Structural rule: `tsc` treats a function value's apparent type as its call
//! signatures plus the members of the global `Function` interface — nothing
//! more. `interface IResultCallback extends Function { x: number }` requires
//! `x` in addition to that surface, so a bare function value (which has no
//! `x`) is not assignable to it (`TS2345`/`TS2322`).
//!
//! tsz's `core_dispatch` compatibility bridge decided "target requires
//! nothing beyond `Function`'s surface" via
//! `is_function_interface_structural`, which delegated to
//! `matches_global_function_interface_shape` — a probe that only checked for
//! `apply`/`call`/`bind` plus a property-count cap. Any interface `extends
//! Function` with up to ~17 additional properties still passed that probe,
//! so the bridge answered `True` unconditionally and the `x: number`
//! requirement was silently dropped (a false negative). Fixed in
//! `tsz_solver::type_queries::global_interfaces::object_shape_matches_global_function_interface`
//! by additionally requiring every property the candidate shape declares to
//! belong to the real boxed `Function` interface's own surface.
//!
//! These tests need the real global `Function` interface, so they load the
//! default lib bundle rather than using the no-lib `check_source` helper.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn libs() -> Vec<Arc<LibFile>> {
    load_default_lib_files()
}

fn codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> =
        check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs())
            .iter()
            .map(|(code, _)| *code)
            .collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

fn has_code(source: &str, code: u32) -> bool {
    codes(source).contains(&code)
}

#[test]
fn arrow_function_argument_missing_extra_property_is_ts2345() {
    let source = r#"
interface IResultCallback extends Function {
    x: number;
}
declare function fn(cb: IResultCallback): void;
fn((a: any, b: any) => true);
"#;
    assert!(
        has_code(source, 2345),
        "an arrow function has no `x` property, so it cannot satisfy an \
         interface that extends Function and requires one; got {:?}",
        codes(source)
    );
}

#[test]
fn function_expression_argument_missing_extra_property_is_ts2345() {
    let source = r#"
interface IResultCallback extends Function {
    x: number;
}
declare function fn(cb: IResultCallback): void;
fn(function (a: any, b: any) { return true; });
"#;
    assert!(
        has_code(source, 2345),
        "same verdict for a function expression argument, not just an arrow; got {:?}",
        codes(source)
    );
}

#[test]
fn direct_assignment_missing_extra_property_is_ts2322() {
    let source = r#"
interface IResultCallback extends Function {
    x: number;
}
const cb: IResultCallback = (a: any, b: any) => true;
"#;
    assert!(
        has_code(source, 2322),
        "the same relation applies to a direct assignment, not only a call argument; got {:?}",
        codes(source)
    );
}

#[test]
fn renamed_binders_reach_the_same_ts2345_verdict() {
    // Binder-name variation: nothing about this rule may key on the
    // interface being spelled `IResultCallback` or the extra member `x`.
    let source = r#"
interface Zorb extends Function {
    q: string;
}
declare function j(cb: Zorb): void;
j((a: any, b: any) => true);
"#;
    assert!(has_code(source, 2345), "got {:?}", codes(source));
}

#[test]
fn an_optional_extra_property_does_not_require_presence() {
    // Negative: an optional extra property does not need to be present, so a
    // bare function still satisfies the interface.
    let source = r#"
interface OptionalPropCallback extends Function {
    x?: number;
}
declare function h(cb: OptionalPropCallback): void;
h(() => {});
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "an optional extra member does not block a bare function argument"
    );
}

#[test]
fn an_interface_extending_function_with_no_extra_members_still_accepts_a_function() {
    // Negative that localizes the rule: with no extra member at all, the
    // interface is exactly the Function surface and the bridge must still
    // answer assignable.
    let source = r#"
interface PlainCallback extends Function {}
declare function g(cb: PlainCallback): void;
g(() => {});
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "an interface that adds nothing to Function must still accept a bare function"
    );
}

#[test]
fn redeclaring_a_real_function_member_still_accepts_a_function() {
    // Negative: redeclaring a member that already belongs to Function's own
    // surface (`length`) is not a "genuine extra property" and must not
    // trip the new check.
    let source = r#"
interface LengthCallback extends Function {
    readonly length: number;
}
declare function k(cb: LengthCallback): void;
k(() => {});
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "redeclaring a real Function member is not an extra property"
    );
}
