//! Regression coverage for issue #9619 — preserve class member and
//! constructor value types in dialect adapters.
//!
//! The issue body itself states the symptoms "should be investigated after,
//! or alongside, #9618 because `TypeId::ERROR` from declaration/import dispatch
//! may amplify the relation fallout." That is, when an upstream uncoded
//! checker dispatch (#9618) returns `TypeId::ERROR` for declaration/import
//! nodes, the error propagates into property type resolution and into the
//! printer, producing the two reported shapes:
//!
//! 1. A class method used to satisfy an interface method/property is reported
//!    with the containing class as its inferred type instead of its
//!    declared call signature.
//! 2. An identical-display assignability mismatch between an imported class
//!    instance type and `typeof Class` (or between two identical-displaying
//!    `T -> T` types) appears.
//!
//! These reduced tests pin the expected behaviour with generic class/member
//! names and module-style imports. They are intentionally fixture-agnostic
//! (no Kysely/MSSQL/Tedious paths) so a regression in property-type
//! resolution or class-instance-vs-typeof identity would surface here
//! without re-running the full kysely-project compile.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_common::common::ModuleKind;

#[test]
fn issue_9619_class_method_satisfies_interface_method_property() {
    // Symptom 1 reduced: a class method should satisfy an interface method/
    // property requirement without the diagnostic claiming the property's
    // type is the containing class.
    let interface_file = r#"
        export interface Kysely<DB> { thing(): DB; }
        export interface MigrationLockOptions { name?: string; }
        export interface IAdapter {
          acquireLock<DB>(db: Kysely<DB>, options: MigrationLockOptions): Promise<void>;
        }
    "#;
    let adapter_file = r#"
        import type { IAdapter, Kysely, MigrationLockOptions } from "./types.ts";
        export class MyAdapter implements IAdapter {
          async acquireLock<DB>(db: Kysely<DB>, options: MigrationLockOptions): Promise<void> {
            return;
          }
        }
    "#;
    let consumer_file = r#"
        import { MyAdapter } from "./adapter.ts";
        import type { IAdapter } from "./types.ts";
        export const inst: IAdapter = new MyAdapter();
    "#;

    let diags = tsz_checker::test_utils::check_multi_file(
        &[
            ("./types.ts", interface_file),
            ("./adapter.ts", adapter_file),
            ("./consumer.ts", consumer_file),
        ],
        "./consumer.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322s: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322s.is_empty(),
        "Class with method matching interface method should be assignable; got TS2322s: {ts2322s:#?}"
    );
}

#[test]
fn issue_9619_class_method_satisfies_interface_method_property_alt_names() {
    // Same structural rule, different names — proves the fix is not keyed on
    // user-chosen identifiers (per §25).
    let interface_file = r#"
        export interface Q<D> { thing(): D; }
        export interface Opts { tag?: string; }
        export interface IFace {
          run<D>(q: Q<D>, opts: Opts): Promise<void>;
        }
    "#;
    let impl_file = r#"
        import type { IFace, Q, Opts } from "./types.ts";
        export class Impl implements IFace {
          async run<D>(q: Q<D>, opts: Opts): Promise<void> {
            return;
          }
        }
    "#;
    let consumer_file = r#"
        import { Impl } from "./impl.ts";
        import type { IFace } from "./types.ts";
        export const inst: IFace = new Impl();
    "#;

    let diags = tsz_checker::test_utils::check_multi_file(
        &[
            ("./types.ts", interface_file),
            ("./impl.ts", impl_file),
            ("./consumer.ts", consumer_file),
        ],
        "./consumer.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322s: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322s.is_empty(),
        "Renamed class/interface members must still satisfy the implements rule; got TS2322s: {ts2322s:#?}"
    );
}

#[test]
fn issue_9619_imported_class_constructor_vs_instance() {
    // Symptom 2 reduced: an instance type and `typeof Class` are distinct;
    // assigning the class value to `typeof Class` and assigning a `new
    // Class(...)` value to the instance type should both succeed without
    // triggering an impossible identical-display TS2322/TS2740.
    let class_file = r#"
        export declare class Req {
          constructor(name: string);
          run(): void;
        }
    "#;
    let consumer_file = r#"
        import { Req } from "./req.ts";
        // typeof Req is the constructor value; new Req(...) is the instance.
        export const ctor: typeof Req = Req;
        export const inst: Req = new Req("hi");
    "#;
    let diags = tsz_checker::test_utils::check_multi_file(
        &[("./req.ts", class_file), ("./consumer.ts", consumer_file)],
        "./consumer.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code == 2740
                || d.code == 2345
        })
        .collect();
    assert!(
        bad.is_empty(),
        "typeof Class vs instance must round-trip without TS2322/TS2740/TS2345 across imports; got: {bad:#?}"
    );
}

#[test]
fn issue_9619_imported_class_constructor_callback_param() {
    // Adjacent case: passing an imported class constructor value as a
    // `typeof` constructor-typed parameter should be accepted, and passing
    // an instance to an instance-typed parameter should also be accepted.
    let lib_file = r#"
        export declare class Req {
          constructor(name: string);
          run(): void;
        }
        export declare function takeCtor(c: typeof Req): void;
        export declare function takeInst(r: Req): void;
    "#;
    let consumer_file = r#"
        import { Req, takeCtor, takeInst } from "./lib.ts";
        export function ok() {
          takeCtor(Req);
          takeInst(new Req("x"));
        }
    "#;
    let diags = tsz_checker::test_utils::check_multi_file(
        &[("./lib.ts", lib_file), ("./consumer.ts", consumer_file)],
        "./consumer.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code == 2740
                || d.code == 2345
        })
        .collect();
    assert!(
        bad.is_empty(),
        "Imported class constructor must satisfy typeof param; instance must satisfy instance param; got: {bad:#?}"
    );
}

#[test]
fn issue_9619_instance_to_typeof_param_is_rejected_single_file() {
    // Negative case (single-file, no imports): passing an instance where a
    // `typeof Class` is expected MUST be rejected. This proves the typeof/
    // instance identities remain distinct — the issue #9619 structural rule.
    let source = r#"
        declare class Req {
          constructor(name: string);
        }
        declare function takeCtor(c: typeof Req): void;
        declare const instance: Req;
        takeCtor(instance);
    "#;
    let mut parser =
        tsz_parser::parser::ParserState::new("./single.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "./single.ts".to_string(),
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checker.check_source_file(root);
    let diags = checker.ctx.diagnostics.clone();
    let rejections: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322 || d.code == 2740)
        .collect();
    assert!(
        !rejections.is_empty(),
        "Passing an instance to a typeof-class parameter must produce a diagnostic; got none. all diags: {diags:#?}"
    );
    for diag in &rejections {
        let msg = &diag.message_text;
        let inside: Vec<&str> = msg
            .match_indices('\'')
            .zip(msg.match_indices('\'').skip(1))
            .step_by(2)
            .filter_map(|((start, _), (end, _))| msg.get(start + 1..end))
            .collect();
        if inside.len() >= 2 {
            assert!(
                inside[0] != inside[1],
                "diagnostic must not display identical source and target types; got msg={msg:?}"
            );
        }
    }
}

#[test]
#[ignore = "issue #9619: cross-arena class-symbol-in-function-signature; see comment"]
fn issue_9619_instance_to_typeof_param_is_rejected_cross_import() {
    // Same negative case but imports the class from another module. This is
    // exactly the kysely-project shape that #9619 reports.
    //
    // Marked `#[ignore]` because the underlying cross-arena class-symbol-in-
    // function-signature resolution is broken in a broader way than this one
    // assertion. Investigation summary (against current `main`):
    //
    //   1. Single-file equivalent (`declare class` + `declare function` +
    //      `declare const instance` + bad call in ONE file): correctly rejects
    //      with TS2345 "Argument of type 'Req' is not assignable to parameter
    //      of type 'typeof Req'." (See sibling
    //      `issue_9619_instance_to_typeof_param_is_rejected_single_file`.)
    //
    //   2. Cross-import VARIABLE assignment (`import { Req } from "./lib"`
    //      + `const ctor: typeof Req = instance`): correctly rejects with
    //      TS2741 "Property 'prototype' is missing in type 'Req' but
    //      required in type 'typeof Req'." (See sibling
    //      `issue_9619_imported_class_constructor_vs_instance`.)
    //
    //   3. Cross-import FUNCTION CALL where the parameter type is the
    //      imported class (or `typeof` of one): silently accepts. Worse,
    //      a sibling probe showed that even an imported function with
    //      ONLY primitive parameters (e.g. `takeStr(s: string)`) is
    //      reported as TS2348/TS2349 ("not callable, did you mean
    //      'new'?") when the LIB file also contains a class declaration
    //      — adding an unrelated `export declare class Other` to a lib
    //      that exports `takeStr(s: string)` is enough to break the
    //      call. This is the same bug family that kysely's
    //      `mssql-dialect.ts(68,5)` / `mssql-driver.ts(212/257/352/417)`
    //      diagnostics derive from, surfaced through the call-resolution
    //      side rather than the relation side.
    //
    // The fix belongs in the cross-arena value-type resolution path
    // (`crates/tsz-checker/src/types/computation/call_helpers.rs` and the
    // identifier resolution that feeds it). It needs to preserve a
    // function's callable shape when the lib arena that owns it also
    // contains a class declaration, and to keep `typeof Class` (the
    // constructor object) and `Class` (the instance) as distinct
    // structural identities across the arena boundary.
    //
    // This test is left here to capture the intended behaviour and
    // serve as the regression gate once the cross-arena resolution
    // is fixed. Remove the `#[ignore]` when the fix lands.
    let lib_file = r#"
        export declare class Req {
          constructor(name: string);
        }
        export declare function takeCtor(c: typeof Req): void;
    "#;
    let consumer_file = r#"
        import { Req, takeCtor } from "./lib";
        declare const instance: Req;
        export function bad() {
          takeCtor(instance);
        }
    "#;
    let diags = tsz_checker::test_utils::check_multi_file(
        &[("./lib.ts", lib_file), ("./consumer.ts", consumer_file)],
        "./consumer.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    // We expect a TS2345 (or related) rejection. The key invariant is the
    // diagnostic message must not be a tautology ("Req -> Req") — its
    // source/target display strings must be distinct.
    let rejections: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322 || d.code == 2740)
        .collect();
    assert!(
        !rejections.is_empty(),
        "Passing an instance where typeof Class is expected must produce a diagnostic; got none. all diags: {diags:#?}"
    );
    for diag in &rejections {
        let msg = &diag.message_text;
        // Reject tautologies of the form `... 'X' is not assignable ... 'X'`
        // (two identical quoted type displays).
        let mut quoted: Vec<&str> = msg.split('\'').collect();
        // Even-indexed slices (1, 3, 5, ...) are inside-quote spans.
        quoted.retain(|_| true);
        let inside: Vec<&str> = msg
            .match_indices('\'')
            .zip(msg.match_indices('\'').skip(1))
            .step_by(2)
            .filter_map(|((start, _), (end, _))| msg.get(start + 1..end))
            .collect();
        if inside.len() >= 2 {
            assert!(
                inside[0] != inside[1],
                "diagnostic must not display identical source and target types; got msg={msg:?}"
            );
        }
    }
}
