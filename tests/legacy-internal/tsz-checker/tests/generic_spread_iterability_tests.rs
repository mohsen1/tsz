use crate::test_utils::check_source_diagnostics;

fn has_2488(src: &str) -> bool {
    check_source_diagnostics(src).iter().any(|d| d.code == 2488)
}

fn no_spread_errors(src: &str) -> bool {
    check_source_diagnostics(src)
        .iter()
        .all(|d| !matches!(d.code, 2488 | 2345 | 2322))
}

// ── Parameters<T> (the reported bug) ─────────────────────────────────────────

#[test]
fn parameters_t_call_spread_no_error() {
    assert!(no_spread_errors(
        r#"
function call<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}
"#
    ));
}

#[test]
fn parameters_t_array_spread_no_error() {
    assert!(no_spread_errors(
        r#"
function collect<T extends (...args: any[]) => any>(
    ...args: Parameters<T>
): Parameters<T> {
    const copy = [...args];
    return copy as Parameters<T>;
}
"#
    ));
}

#[test]
fn parameters_renamed_type_param_no_error() {
    // The fix must not be keyed on the type-parameter name — using 'U' must work
    // identically to 'T' (guards against hardcoding per §25).
    assert!(no_spread_errors(
        r#"
function relay<U extends (...args: any[]) => any>(
    fn: U,
    ...args: Parameters<U>
): ReturnType<U> {
    return fn(...args);
}
"#
    ));
}

// ── ConstructorParameters<T> ─────────────────────────────────────────────────

#[test]
fn constructor_parameters_t_spread_no_error() {
    assert!(no_spread_errors(
        r#"
function construct<C extends new (...args: any[]) => any>(
    ctor: C,
    ...args: ConstructorParameters<C>
): InstanceType<C> {
    return new ctor(...args);
}
"#
    ));
}

// ── ReturnType<T> ─────────────────────────────────────────────────────────────

#[test]
fn return_type_t_spread_no_error_at_generic_level() {
    // tsc defers the check to instantiation: at the generic declaration site the
    // return type might resolve to a non-tuple, but that is caught per call-site.
    assert!(no_spread_errors(
        r#"
function spreadReturn<T extends () => any[]>(
    fn: T,
    ...args: ReturnType<T>
): void {
    consume(...args);
}
declare function consume(...args: any[]): void;
"#
    ));
}

// ── Generic IndexAccess type ──────────────────────────────────────────────────

#[test]
fn generic_index_access_spread_no_error() {
    assert!(no_spread_errors(
        r#"
function forward<T extends Record<string, any[]>>(
    key: keyof T,
    data: T,
    ...args: T[typeof key]
): void {
    sink(...args);
}
declare function sink(...args: any[]): void;
"#
    ));
}

// ── Concrete non-iterable types still error ───────────────────────────────────

#[test]
fn concrete_non_iterable_still_errors() {
    assert!(has_2488(
        r#"
const n: number = 5;
const arr = [...n];
"#
    ));
}

#[test]
fn concrete_object_without_iterator_still_errors() {
    assert!(has_2488(
        r#"
const obj: { a: number } = { a: 1 };
const arr = [...obj];
"#
    ));
}
