//! Regression tests for #17585: a generic class method whose return type
//! re-instantiates the enclosing class through a self-referential type
//! parameter constraint (`method<Incoming extends AnyC>(...): C<...>`, where
//! `AnyC` is an alias back to `C` itself) must not spuriously fail a
//! constraint check just because the enclosing class's real instance type is
//! not yet published.
//!
//! Root cause: `class_instance_phase0_prescan_this`
//! (`crates/tsz-checker/src/types/class_type/instance.rs`) — a provisional
//! prescan used only so `this.method()` resolves during other members' body
//! inference — resolves each method's return type before the class's own
//! authoritative instance type is published. For a self-referential method
//! like `merge` below, that premature resolution sees the enclosing class as
//! an unresolved placeholder and wrongly reports its constraint as
//! unsatisfied (TS2536/TS2344). `push_diagnostic`'s first-wins dedup then
//! keeps this wrong early diagnostic even though the later, authoritative
//! re-check (once the real instance type is published) resolves cleanly —
//! matching the mechanism #17589 already fixed for property initializers.
//!
//! The fix only discards TS2536/TS2344 diagnostics produced during this
//! prescan (and only then clears the node-type cache to force the later
//! authoritative re-check) — every other diagnostic from the same prescan
//! call is left untouched, since this prescan is sometimes the only pass
//! that visits a given node and a blanket rollback would silently drop
//! genuine diagnostics (e.g. TS2304 for an unresolved name in the return
//! type — see `missing_name_in_return_type_still_reported` below, a
//! regression caught by `compare-to-parent.sh` against
//! `errorsInGenericTypeReference.ts`).
//!
//! Binder names are varied across cases (anti-hardcoding): the fix keys off
//! structure (a method-return-type re-instantiation of the enclosing,
//! not-yet-published class), never a specific identifier. Shapes avoid
//! default-lib utility types (`Record`, `Omit`) since this harness
//! (`check_source_strict_codes`) does not load default lib files.

use crate::test_utils::check_source_strict_codes;

fn codes(src: &str) -> Vec<u32> {
    check_source_strict_codes(src)
}

fn assert_clean(src: &str) {
    let got = codes(src);
    assert!(
        got.is_empty(),
        "expected no diagnostics, got: {got:?}\n{src}"
    );
}

// ---------------------------------------------------------------------------
// The reported witness — a reduction of zod's real `ZodObject.merge` shape
// (three class type parameters, a self-referential `AnyZodObject`-style
// alias constraint, an indexed access threaded through a second generic
// alias before re-instantiating the class).
// ---------------------------------------------------------------------------

#[test]
fn generic_method_return_reinstantiates_class_through_self_referential_alias_is_clean() {
    assert_clean(
        "
        type RawShape = { [key: string]: unknown };
        class Base<Output = any, Def = any, Input = Output> {}
        type ExtendShape<A extends RawShape, B extends RawShape> = A & B;
        class Obj<T extends RawShape, UnknownKeys = any, Catchall = any> extends Base<any, any, any> {
            readonly _shape!: T;
            merge<Incoming extends AnyObj>(
                merging: Incoming
            ): Obj<ExtendShape<T, Incoming['_shape']>, UnknownKeys, Catchall> {
                return {} as any;
            }
        }
        type AnyObj = Obj<any, any, any>;
        ",
    );
}

#[test]
fn renamed_binders_still_clean() {
    // Same shape, every identifier renamed — the fix must key off structure,
    // not names.
    assert_clean(
        "
        type Shape2 = { [key: string]: unknown };
        class Root<X = any, Y = any, Z = X> {}
        type Merge2<P extends Shape2, Q extends Shape2> = P & Q;
        class Wrapper<S extends Shape2, K = any, C = any> extends Root<any, any, any> {
            readonly payload!: S;
            combine<Other extends AnyWrapper>(
                other: Other
            ): Wrapper<Merge2<S, Other['payload']>, K, C> {
                return {} as any;
            }
        }
        type AnyWrapper = Wrapper<any, any, any>;
        ",
    );
}

// ---------------------------------------------------------------------------
// Simpler single-type-parameter reduction (no second generic wrapper) — the
// smallest shape that still needs both a constrained enclosing type
// parameter and a return type that re-applies the enclosing class using the
// method type parameter's indexed access.
// ---------------------------------------------------------------------------

#[test]
fn single_type_param_self_referential_return_is_clean() {
    assert_clean(
        "
        type RawShape = { [key: string]: unknown };
        class Box<T extends RawShape> {
            readonly shape!: T;
            merge<Incoming extends AnyBox>(x: Incoming): Box<Incoming['shape']> {
                return {} as any;
            }
        }
        type AnyBox = Box<RawShape>;
        ",
    );
}

// ---------------------------------------------------------------------------
// Must stay clean — false-positive guards for adjacent shapes that must not
// regress from the deferral.
// ---------------------------------------------------------------------------

#[test]
fn unconstrained_class_type_param_was_already_clean() {
    // An unconstrained enclosing type parameter never triggered the FP; must
    // remain clean.
    assert_clean(
        "
        class Plain<T> {
            readonly value!: T;
            identity<U>(x: U): Plain<U> {
                return {} as any;
            }
        }
        ",
    );
}

#[test]
fn non_self_referential_generic_method_stays_clean() {
    // The method's return type does not re-instantiate the enclosing class —
    // never part of this family, must remain clean.
    assert_clean(
        "
        class Container<T> {
            readonly value!: T;
            map<U>(f: (x: T) => U): U {
                return f(this.value);
            }
        }
        ",
    );
}

// ---------------------------------------------------------------------------
// Negative control — a genuine constraint violation on the same self-
// referential shape must still be reported once the deferred/authoritative
// re-check runs. The deferral must not turn into a false negative.
// ---------------------------------------------------------------------------

#[test]
fn genuine_constraint_violation_on_same_shape_still_reported() {
    let got = codes(
        "
        type RawShape = { [key: string]: unknown };
        class Box<T extends RawShape> {
            readonly shape!: T;
            merge<Incoming extends AnyBox>(x: Incoming): Box<Incoming['shape']> {
                return {} as any;
            }
        }
        type AnyBox = Box<RawShape>;
        declare const b: Box<{ a: string }>;
        // `number` is not assignable to `AnyBox` — a genuine TS2345 at the call site.
        b.merge(1 as unknown as number);
        ",
    );
    assert!(
        got.contains(&2345),
        "expected TS2345 for the genuine violation, got: {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative control — an unrelated diagnostic produced by the SAME prescan
// call (a method return type that is not self-referential) must survive the
// speculative window untouched. Reduced from the conformance regression this
// fix's first attempt introduced (`errorsInGenericTypeReference.ts`,
// `testMethod1(): Foo<{ x: V }>`): only TS2536/TS2344 may be discarded here,
// never a genuine TS2304 for an unresolved name in the same return type.
// ---------------------------------------------------------------------------

#[test]
fn missing_name_in_return_type_still_reported() {
    let got = codes(
        "
        class Foo<T> {}
        class Bar {
            method1(): Foo<{ x: V }> { return null as any; }
        }
        ",
    );
    assert_eq!(got, vec![2304], "expected exactly one TS2304, got: {got:?}");
}
