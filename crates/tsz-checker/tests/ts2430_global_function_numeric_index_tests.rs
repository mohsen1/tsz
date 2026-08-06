//! Augmenting the *global* `Function` interface with a numeric index signature
//! makes a bare function value no longer assignable to `Function` — and, because
//! `lib.es5.d.ts` declares `CallableFunction`/`NewableFunction extends Function`
//! with `apply`/`call`/`bind` overloads whose `this` parameter is a function
//! type, exposes those overrides as incompatible (`TS2430`).
//!
//! This is #16525's residual. #16473/#16519/#16534 made the overload-coverage
//! pass reach the member comparison, but the underlying relation
//! `check_subtype(fn, Function)` still answered assignable: the boxed-`Function`
//! "second opinion" substitutes the global interface for the source, and when
//! the target *is* the augmented global `Function`, that substitution is
//! identity-true and masks the missing numeric index a concrete function value
//! does not carry.
//!
//! `tsc` (6.0.2, `--target es2015 --lib es2015`, non-strict) reports `TS2322`
//! for the direct `const g: Function = fn` assignment ("Index signature for type
//! 'number' is missing") and `TS2430` for a user interface that extends the
//! augmented `Function` with such an override. The un-augmented interface, a
//! *string*-index augmentation, and the dual-`any`-index waiver all still accept
//! a function, so the rule is scoped to an unwaived numeric index.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

const TS2322: u32 = 2322;
const TS2430: u32 = 2430;

fn diags(source: &str) -> Vec<(u32, String)> {
    // The full (compiled) `lib.es5.d.ts` is required: the numeric-index rule is
    // exposed by `CallableFunction`/`NewableFunction extends Function`, and a
    // user `interface Function` augmentation must merge into the *same* global
    // interface the way the driver merges it. The stripped test lib does not.
    let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !libs.is_empty(),
        "compiled es5 lib fixture should be available"
    );
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

fn has_code(source: &str, code: u32) -> bool {
    diags(source).iter().any(|(c, _)| *c == code)
}

// =========================================================================
// Direct assignment (TS2322): the relation `check_subtype(fn, Function)`.
// =========================================================================

#[test]
fn augmented_global_function_rejects_a_bare_function_assignment() {
    let source = r#"
interface Bar { b: number; }
interface Function { [n: number]: Bar; }
declare const fn: (this: any) => any;
const g: Function = fn;
"#;
    assert!(
        has_code(source, TS2322),
        "once `Function` carries a numeric index a bare function no longer \
         satisfies it, got {:?}",
        diags(source)
    );
}

#[test]
fn augmented_global_function_rejects_via_renamed_augmentation_members() {
    // Nothing keys on the augmentation's member/value spelling — only on the
    // fact that the *global* `Function` gained a numeric index.
    let source = r#"
interface Widget { payload: string; }
interface Function { [slot: number]: Widget; }
declare const handler: () => void;
const stored: Function = handler;
"#;
    assert!(has_code(source, TS2322), "got {:?}", diags(source));
}

// =========================================================================
// Interface heritage (TS2430): a user interface that extends the augmented
// *global* `Function` with an *overloaded* `apply` whose `this` is a function
// type — the user-space witness of `lib.es5.d.ts`'s
// `CallableFunction`/`NewableFunction extends Function`. Two-or-more overloads
// route the member through `check_interface_overload_coverage` →
// `check_callable_subtype` → `are_this_parameters_compatible`, the exact path
// the four conformance fixtures exercise on the lib interfaces.
//
// (The single-override form takes a different member path that this harness
// does not reach for a lib base; the lib interfaces' overloads are what the
// conformance fixtures actually score.)
// =========================================================================

#[test]
fn overloaded_this_override_extending_augmented_global_function_is_ts2430() {
    let source = r#"
interface Bar { b: number; }
interface Function { [n: number]: Bar; }
interface MyCallable extends Function {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R;
}
"#;
    assert!(
        has_code(source, TS2430),
        "an overloaded function-typed `this` override cannot satisfy a base whose \
         type (the augmented global `Function`) carries a numeric index, got {:?}",
        diags(source)
    );
}

#[test]
fn renamed_binders_overloaded_override_extending_augmented_global_function_is_ts2430() {
    // Nothing keys on the derived interface name or the augmentation's spelling.
    let source = r#"
interface Element { count: number; }
interface Function { [slot: number]: Element; }
interface Fluent extends Function {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R;
}
"#;
    assert!(has_code(source, TS2430), "got {:?}", diags(source));
}

#[test]
fn overloaded_override_without_numeric_index_stays_silent() {
    // The negative that localizes the rule to the numeric index: drop only the
    // index augmentation and the overloaded function-typed `this` override is
    // accepted (matching tsc, which flags these overrides only once `Function`
    // becomes numeric-indexed).
    let source = r#"
interface MyCallable extends Function {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R;
}
"#;
    assert!(
        !has_code(source, TS2430),
        "without a numeric index on `Function` the override is valid, got {:?}",
        diags(source)
    );
}

// =========================================================================
// String-index augmentation (concrete value): the same rule holds for a
// non-`any` string index — the shape `objectTypeWithCallSignature-` /
// `objectTypeWithConstructSignatureHidingMembersOfExtendedFunction.ts` give
// `Function` (`data: number; [x: string]: Object`). tsc emits the same two
// `this`-type `TS2430`s there. Only an `any`-valued string index is waived.
// =========================================================================

#[test]
fn concrete_valued_string_index_augmentation_rejects_a_function() {
    let source = r#"
interface Function { data: number; [x: string]: Object; }
declare const fn: (this: any) => any;
const g: Function = fn;
"#;
    assert!(
        has_code(source, TS2322),
        "a concrete-valued string index is unsatisfiable by a bare function, got {:?}",
        diags(source)
    );
}

#[test]
fn overloaded_override_extending_concrete_string_indexed_function_is_ts2430() {
    let source = r#"
interface Function { data: number; [x: string]: Object; }
interface MyCallable extends Function {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R;
}
"#;
    assert!(has_code(source, TS2430), "got {:?}", diags(source));
}

// =========================================================================
// Negatives: the rule must be scoped to an *unwaived* index on the
// *global* `Function`.
// =========================================================================

#[test]
fn unaugmented_global_function_still_accepts_a_function() {
    let source = r#"
declare const fn: (value: number) => string;
const g: Function = fn;
"#;
    assert!(
        !has_code(source, TS2322),
        "with no augmentation a function is still assignable to `Function`, got {:?}",
        diags(source)
    );
}

#[test]
fn any_valued_string_index_augmentation_still_accepts_a_function() {
    // A permissive `any`-valued string index waives every index obligation
    // (`indexSignaturesRelatedTo` short-circuit), so a function is still
    // assignable. Only this `any` form is waived — see the concrete-valued
    // cases below.
    let source = r#"
interface Function { [k: string]: any; }
declare const fn: (this: any) => any;
const g: Function = fn;
"#;
    assert!(
        !has_code(source, TS2322),
        "an `any`-valued string-index augmentation does not reject a function, got {:?}",
        diags(source)
    );
}

#[test]
fn dual_any_index_augmentation_still_accepts_a_function() {
    // A co-present `any`-valued string index waives the missing numeric index,
    // matching tsc's `indexSignaturesRelatedTo` short-circuit.
    let source = r#"
interface Function { [k: string]: any; [n: number]: any; }
declare const fn: (this: any) => any;
const g: Function = fn;
"#;
    assert!(
        !has_code(source, TS2322),
        "a dual-any-index augmentation waives the numeric requirement, got {:?}",
        diags(source)
    );
}

#[test]
fn compatible_override_under_augmented_global_function_stays_silent() {
    // A `this: Function` override (not a function-typed `this`) still satisfies
    // the augmented base, so the augmentation alone must not manufacture a
    // TS2430 for every extender.
    let source = r#"
interface Bar { b: number; }
interface Function { [n: number]: Bar; }
interface Compatible extends Function {
    apply(this: Function, thisArg: any): any;
}
"#;
    assert!(
        !has_code(source, TS2430),
        "an override whose `this` is `Function` itself is still compatible, got {:?}",
        diags(source)
    );
}
