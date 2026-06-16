use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// Regression: `typeof x === "function"` narrowing over a union member that is
/// a generic type-alias instantiation (`TypeData::Application`) must resolve the
/// alias to its underlying shape before classifying it. A generic alias whose
/// body is a function type (`type Fn<A> = (a: A) => R`) was previously dropped
/// from the function branch because the structural function predicate cannot see
/// through a deferred `Application` and `evaluate_type` leaves it deferred
/// without a resolver — producing a spurious TS2349 ("not callable") on the
/// narrowed call and a TS2322 on the conditional result.
///
/// Mirrors `resolveStaleTime`/`resolveQueryBoolean` in the `TanStack` Query
/// `query-core` package. Binder names are intentionally varied from the corpus
/// witness so the fix cannot key off any identifier.
#[test]
fn typeof_function_narrows_generic_function_alias() {
    let source = r#"
type Callback<A> = (input: A) => number
function run<A>(cb: undefined | Callback<A>, arg: A): number | undefined {
    return typeof cb === 'function' ? cb(arg) : cb
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2349 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no TS2349/TS2322 narrowing a generic function alias, got: {diags:#?}"
    );
}

/// The corpus witness shape: the alias body is itself a UNION that contains a
/// function arm (`Primitive | ((q) => Primitive)`). The true branch must keep
/// only the callable arm; the false branch must keep only the non-callable
/// arms. The `if`/`else` form exercises both `narrow_to_function` and
/// `narrow_excluding_function`.
#[test]
fn typeof_function_narrows_alias_to_union_with_function_arm() {
    let source = r#"
type Stale = number | 'static'
type StaleOption<A> = Stale | ((input: A) => Stale)
function resolve<A>(opt: undefined | StaleOption<A>, arg: A): Stale | undefined {
    if (typeof opt === 'function') {
        return opt(arg)
    }
    return opt
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2349 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no TS2349/TS2322 for alias-to-union narrowing, got: {diags:#?}"
    );
}

/// Negative guard: a generic alias whose body is a plain object (no call
/// signature) must NOT be treated as callable by `typeof === "function"`. The
/// fix only promotes members that genuinely resolve to a function shape, so the
/// false branch keeps the object and the true branch is `never`.
#[test]
fn typeof_function_does_not_promote_generic_object_alias() {
    let source = r#"
type Box<A> = { value: A }
function pick<A>(maybe: undefined | Box<A>): A | undefined {
    if (typeof maybe === 'function') {
        return undefined
    }
    return maybe?.value
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2349 | 2322 | 2339))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no spurious diagnostics for a non-function generic alias, got: {diags:#?}"
    );
}
