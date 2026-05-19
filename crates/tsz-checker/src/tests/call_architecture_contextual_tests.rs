//! Regression tests for contextual call architecture edge cases.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_messages<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

/// Regression test: generic overloads with `ThisType` markers should not produce
/// false TS2339 on `this` property accesses inside object literal methods.
///
/// The issue was that during overload resolution, the first-pass argument
/// collection uses union-contextual types with unresolved type parameters.
/// The `ThisType`<Data & Readonly<Props> & Instance> marker extracted from the
/// callable had uninstantiated Data/Props, causing `this.bar` to fail. The
/// fix defers the hard-error rejection for generic overloads until after the
/// instantiated retry, which re-evaluates with concrete types.
#[test]
fn vue_like_this_type_inference_no_false_ts2339() {
    let diags = check_source_diagnostics(
        r#"
interface Instance {
    _instanceBrand: never
}

type DataDef<Data, Props> = (this: Readonly<Props> & Instance) => Data

type PropsDefinition<T> = {
    [K in keyof T]: T[K]
}

interface Options<
    Data = ((this: Instance) => object),
    PropsDef = {}
    > {
    data?: Data
    props?: PropsDef
    watch?: Record<string, WatchHandler<any>>
}

type WatchHandler<T> = (val: T, oldVal: T) => void;

type ThisTypedOptions<Data, Props> =
    Options<DataDef<Data, Props>, PropsDefinition<Props>> &
    ThisType<Data & Readonly<Props> & Instance>

declare function test<Data, Props>(fn: ThisTypedOptions<Data, Props>): void;
declare function test(fn: Options): void;

test({
    props: {
        foo: ''
    },

    data(): { bar: boolean } {
        return {
            bar: true
        }
    },

    watch: {
        foo(newVal: string, oldVal: string): void {
            this.bar = false
        }
    }
})
"#,
    );

    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for Vue-like ThisType inference, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

/// Suppress spurious TS2339 for property access on a type parameter whose
/// constraint failed to resolve (e.g., `T extends typeof a` where `a` is a
/// destructured parameter not in scope for type parameter constraints).
///
/// The two-pass type parameter resolution creates an initial unconstrained
/// `TypeId` and a refined constrained one. Destructured object binding elements
/// can cache the stale unconstrained `TypeId`, which then triggers a false
/// TS2339 "Property does not exist on type 'T'" even though the constraint
/// error (TS2552) already covers the diagnostic.
#[test]
fn no_false_ts2339_for_destructured_param_with_error_type_param_constraint() {
    let diags = check_source_diagnostics(
        r#"
function f0<T extends typeof a>(a: T) {
    a.b;
}
function f1<T extends typeof a>({a}: {a:T}) {
    a.b;
}
function f2<T extends typeof a>([a]: T[]) {
    a.b;
}
class A {
    m0<T extends typeof a>(a: T) {
        a.b
    }
    m1<T extends typeof a>({a}: {a:T}) {
        a.b
    }
    m2<T extends typeof a>([a]: T[]) {
        a.b
    }
}
"#,
    );

    // tsc emits only TS2552 for each `typeof a` in the type parameter constraint.
    // No TS2339 should be emitted for `a.b` in the body.
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on type param with error constraint, got: {:?}",
        diagnostic_messages(&ts2339)
    );

    // TS2552 should be emitted for each `typeof a` in the constraints.
    let ts2552 = diagnostics_with_code(&diags, 2552);
    assert!(
        ts2552.len() >= 6,
        "Expected at least 6 TS2552 for unresolved typeof in constraints, got {}",
        ts2552.len()
    );
}
