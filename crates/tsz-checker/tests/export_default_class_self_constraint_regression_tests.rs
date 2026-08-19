//! Regression fences for the `export default class` + self-referential
//! type-parameter constraint TS2344 family (#17570, and its cross-file
//! import-cycle twin #17567).
//!
//! Both issues bottomed out in the class-instance-type resolution path:
//! `resolve_lazy_lookup_only` (`context/resolver.rs`) could hand back the
//! class's constructor/static side while `symbol_instance_types` was still
//! empty, and the class-constructor / instance-shape build could publish or
//! consume a bare `Lazy(DefId)` self-reference as the finished instance type.
//! The former was fixed by #17589 (static-property-initializer deferral);
//! the latter by #17619 (`is_incomplete_class_type` deferred-like-tsc arm in
//! `validate_type_args_against_params`, wired to the
//! `class_instance_resolution_set`).
//!
//! Both fixes were merged with regression suites in the code path they
//! touched, but neither pinned the *single-file* row-5/-6 variants (#17570
//! comment 2026-08-18T08:26 explicitly notes "this now has no fence") nor the
//! *isolated minimal* cross-file cycle repro of #17567 (the sprawling
//! `tsz-cli` unit test covers the superset, not the minimum). Left unpinned,
//! a future change to any of the three cooperating layers (constructor-shape
//! deferral, instance-shape deferral, `is_incomplete_class_type`) could
//! silently regress one row without a signal.
//!
//! Rule of thumb: every row here is `tsc`-clean (verified against the
//! `typescript@7.0.2` oracle at the time each row was minimized), so a
//! diagnostic below is a regression. Binder names vary across rows so the
//! decision cannot depend on any identifier text.

use tsz_checker::test_utils::{
    check_multi_file_with_libs_stamped, check_source_diagnostics, load_lib_files,
    strict_checker_options,
};
use tsz_common::diagnostics::Diagnostic;

fn multi_file_clean(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
    check_multi_file_with_libs_stamped(files, entry, strict_checker_options(), &libs)
}

// ---------------------------------------------------------------------------
// #17570 — single-file default-export class, self-referential generic
// constraint on a member. The tightest control pair is row 5 (instance
// arrow property, historically TS2344) vs row 7 (annotation, always clean).
// ---------------------------------------------------------------------------

#[test]
fn export_default_class_static_arrow_property_with_self_constraint_is_clean() {
    // Row 4 in the #17570 matrix: `static make = <R extends Schema>(...) => ...`.
    // Historically TS2344; fixed by #17589.
    let diagnostics = check_source_diagnostics(
        r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema {
  readonly mark!: number;
  static make = <R extends Schema>(b: (x: number) => MarkOf<R>): R => null as any;
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "static arrow property with `<R extends EnclosingClass>` must not raise TS2344 \
         (the enclosing class's instance type has the constraint's required member): \
         {diagnostics:#?}"
    );
}

#[test]
fn export_default_generic_class_instance_arrow_property_with_self_constraint_is_clean() {
    // Row 5 of the #17570 matrix, with the class carrying its own type
    // parameter (`<Init = any>`). This is the shape #17567's real fixture
    // uses (`Schema<T = any>`), and it goes clean through the #17619
    // `is_incomplete_class_type` deferral. Kept as a positive control so a
    // future change cannot regress the generic-class instance-arrow row.
    let diagnostics = check_source_diagnostics(
        r#"
type PayloadOf<Ref extends { readonly payload: unknown }> = Ref["payload"];
export default class Envelope<Init = any> {
  readonly payload!: Init;
  wrap = <Ref extends Envelope>(build: (n: number) => PayloadOf<Ref>): Ref => null as any;
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "instance arrow property with `<Ref extends EnclosingClass>` on a generic \
         `export default class` must not raise TS2344: {diagnostics:#?}"
    );
}

#[test]
fn export_default_generic_class_instance_function_expression_property_with_self_constraint_is_clean()
 {
    // Row 6 of the #17570 matrix (function-expression form), likewise
    // guarded on a generic class. The `<T = any>` axis is kept distinct from
    // the non-generic rows below (fixed via #17743) because the two shapes
    // resolve through different deferral paths (#17619 vs #17629/#17743).
    let diagnostics = check_source_diagnostics(
        r#"
type TagOf<Sub extends { readonly tag: unknown }> = Sub["tag"];
export default class Frame<Init = any> {
  readonly tag!: Init;
  emit = function <Sub extends Frame>(build: (s: string) => TagOf<Sub>): Sub {
    return null as any;
  };
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "instance function-expression property with `<Sub extends EnclosingClass>` on a \
         generic `export default class` must not raise TS2344: {diagnostics:#?}"
    );
}

// ---------------------------------------------------------------------------
// #17743 — the two rows below were harness-only divergences: the CLI was
// clean (fixed by #17629), but this direct-`CheckerState` harness still
// reported the pre-#17629 TS2344. Root cause: without a
// `global_symbol_file_index`, `compute_class_symbol_type`'s local-declaration
// predicate could not accept an `export default class` (the class NODE's
// symbol is the default-export binding, never the `CLASS`-flagged symbol
// under computation), so the class computed to a degraded type and the
// deferred self-reference resolved through it. Fixed by routing the
// node-symbol alternative through the shared `class_self_reference_symbol`
// rule (#17629 rule 1); these rows now fence the family end to end through
// this entry path too.
// ---------------------------------------------------------------------------

#[test]
fn export_default_nongeneric_class_instance_arrow_property_with_self_constraint_is_clean() {
    let diagnostics = check_source_diagnostics(
        r#"
type PayloadOf<Ref extends { readonly payload: unknown }> = Ref["payload"];
export default class Envelope {
  readonly payload!: number;
  wrap = <Ref extends Envelope>(build: (n: number) => PayloadOf<Ref>): Ref => null as any;
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "instance arrow property on a NON-generic `export default class` must not raise \
         TS2344 through the direct-CheckerState harness path (#17743): {diagnostics:#?}"
    );
}

#[test]
fn export_default_nongeneric_class_genuine_violation_keeps_ts2344_in_harness() {
    // Negative control for the #17743 fix: a constraint the class genuinely
    // violates (no `payload` member) must keep its real TS2344 through this
    // same direct-`CheckerState` entry path — the fix only lets the local
    // class declaration be recognized, it does not disable the check.
    let diagnostics = check_source_diagnostics(
        r#"
type PayloadOf<Ref extends { readonly payload: unknown }> = Ref["payload"];
export default class Envelope {
  readonly other!: number;
  wrap = <Ref extends Envelope>(build: (n: number) => PayloadOf<Ref>): Ref => null as any;
}
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2344),
        "a genuinely violated self-referential constraint must still report TS2344 through \
         the harness path: {diagnostics:#?}"
    );
}

#[test]
fn export_default_nongeneric_class_instance_function_expression_property_with_self_constraint_is_clean()
 {
    let diagnostics = check_source_diagnostics(
        r#"
type TagOf<Sub extends { readonly tag: unknown }> = Sub["tag"];
export default class Frame {
  readonly tag!: string;
  emit = function <Sub extends Frame>(build: (s: string) => TagOf<Sub>): Sub {
    return null as any;
  };
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "instance function-expression property on a NON-generic `export default class` must \
         not raise TS2344 through the direct-CheckerState harness path (#17743): {diagnostics:#?}"
    );
}

#[test]
fn plain_class_static_arrow_property_with_self_constraint_stays_clean() {
    // Row 1 in the #17570 matrix, kept as a positive control against a
    // future over-narrow fix that only special-cases the `export default`
    // declaration form. `class` + `export default X` (row 3) has always
    // been clean; a fix that regressed a plain `class` would break both.
    let diagnostics = check_source_diagnostics(
        r#"
type Item<Container extends { readonly item: unknown }> = Container["item"];
class Box {
  readonly item!: number;
  static build = <Container extends Box>(b: (n: number) => Item<Container>): Container =>
    null as any;
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "plain (non-default-exported) class with the same shape must stay clean: {diagnostics:#?}"
    );
}

#[test]
fn export_default_class_static_method_with_self_constraint_stays_clean() {
    // Row 8 in the #17570 matrix — method rather than initializer. Always
    // clean (methods defer their body typing through the existing
    // `instance.rs`/`constructor.rs` prescan path); kept as a positive
    // control to make sure a fix cannot flip it.
    let diagnostics = check_source_diagnostics(
        r#"
type Rev<Layer extends { readonly stamp: unknown }> = Layer["stamp"];
export default class Sheet {
  readonly stamp!: number;
  static print<Layer extends Sheet>(build: (n: number) => Rev<Layer>): Layer {
    return null as any;
  }
}
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "static method with the same self-constraint must stay clean: {diagnostics:#?}"
    );
}

// ---------------------------------------------------------------------------
// #17567 — cross-file import-cycle variant. Same bug family (the class's
// own instance type collapses to `Lazy(self)` while the constraint is
// being validated), but the reentrancy trigger is a `Schema.Meta<T>`
// qualified static type in a constructor parameter list.
//
// The full-featured version of this fixture is
// `tsz-cli::driver_tests::cross_file_class_namespace_merge_value_keeps_call_signature_in_import_cycle`.
// This one is the *isolated minimum* the reopened #17567 body prescribes:
// a bare side-effect `import "./helper"` in the schema file, plus a
// type-position use of `Schema` in a helper field. That combination is
// what makes the schema-side reentrancy fire without any cross-call.
// ---------------------------------------------------------------------------

#[test]
fn cross_file_import_cycle_class_namespace_merge_self_constraint_isolated_minimum_is_clean() {
    let diagnostics = multi_file_clean(
        &[
            (
                "/schema.ts",
                r#"
import "./helper";
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
class Schema<T = any> {
  readonly mark!: T;
  static make = <R extends Schema>(
    build: (x: number) => MarkOf<R>,
    meta: Schema.Meta<R>,
  ): R => new Schema(build as any, meta) as unknown as R;
  private constructor(_b: (x: number) => T, _m: Schema.Meta<Schema<T>>) {}
}
namespace Schema {
  export type Meta<R> = { tag: string; inner: R };
}
export default Schema;
"#,
            ),
            (
                "/helper.ts",
                r#"
import Schema from "./schema";
export default class Helper {
  x: Schema | undefined;
}
"#,
            ),
        ],
        "/schema.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "the isolated minimum of #17567 (bare side-effect import + type-position use of the \
         class+namespace-merge across an import cycle) must not raise TS2344 through the \
         class's own self-referential constraint validation: {diagnostics:#?}"
    );
}

#[test]
fn cross_file_import_cycle_class_namespace_merge_self_constraint_renamed_binders_is_clean() {
    // Same shape as the previous row but every user-facing name renamed
    // (per the anti-hardcoding gate): a future fix keyed on `"Schema"` /
    // `"Meta"` / `"Helper"` text would pass the first row and fail this one.
    let diagnostics = multi_file_clean(
        &[
            (
                "/model.ts",
                r#"
import "./consumer";
type SlotOf<Ref extends { readonly slot: unknown }> = Ref["slot"];
class Blueprint<Init = any> {
  readonly slot!: Init;
  static forge = <Ref extends Blueprint>(
    build: (n: number) => SlotOf<Ref>,
    trim: Blueprint.Trim<Ref>,
  ): Ref => new Blueprint(build as any, trim) as unknown as Ref;
  private constructor(_b: (x: number) => Init, _t: Blueprint.Trim<Blueprint<Init>>) {}
}
namespace Blueprint {
  export type Trim<Ref> = { tag: string; body: Ref };
}
export default Blueprint;
"#,
            ),
            (
                "/consumer.ts",
                r#"
import Blueprint from "./model";
export default class Consumer {
  view: Blueprint | undefined;
}
"#,
            ),
        ],
        "/model.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "renamed-binder twin of the isolated #17567 minimum must also stay clean \
         (guards against a name-keyed fix): {diagnostics:#?}"
    );
}
