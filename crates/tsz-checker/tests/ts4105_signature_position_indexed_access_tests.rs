//! TS4105 ("Private or protected member '{0}' cannot be accessed on a type
//! parameter.") for indexed-access types in *signature* positions.
//!
//! tsc reports TS4105 for `this["<nonpublic>"]` / `T["<nonpublic>"]` wherever
//! the indexed-access type appears, including class method return and parameter
//! type annotations, standalone function declarations, function-type literals,
//! constructor parameters, accessors, overload signatures, and abstract method
//! signatures. tsz previously only ran indexed-access type validation for
//! type-alias bodies and interface members, so the same annotation on a class
//! method or function declaration silently passed. These tests pin the
//! signature-position behavior across declaration kinds, with renamed binders
//! (no name-specific shortcut) and negative cases (public members must stay
//! clean).

use tsz_checker::context::CheckerOptions;

const TS4105: u32 = 4105;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_with_options(source, CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// A class method return-type annotation of `this["<private>"]` reports TS4105.
#[test]
fn class_method_return_this_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Vault {
    private secret!: number;
    reveal(): this["secret"] { return this.secret; }
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a class method return `this[\"secret\"]`; got {diags:?}"
    );
}

/// A class method parameter annotation of `this["<protected>"]` reports TS4105.
#[test]
fn class_method_param_this_indexed_protected_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Channel {
    protected token!: string;
    accept(value: this["token"]): void {}
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a class method parameter `this[\"token\"]`; got {diags:?}"
    );
}

/// A standalone function declaration return type `T["<private>"]` reports TS4105.
#[test]
fn function_declaration_return_type_param_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Store { private data!: number; }
function read<T extends Store>(): T["data"] { return 0 as any; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a function declaration return `T[\"data\"]`; got {diags:?}"
    );
}

/// A function-type literal with its own type parameters reports TS4105 in its
/// return annotation, and resolving the type parameter must NOT leak a spurious
/// TS2304 (Cannot find name).
#[test]
fn function_type_literal_return_indexed_private_reports_ts4105_only() {
    let diags = diagnostics(
        r#"
class Bag { private payload!: string; }
const get: <T extends Bag>() => T["payload"] = null as any;
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a function-type return `T[\"payload\"]`; got {diags:?}"
    );
    assert_eq!(
        count(&diags, 2304),
        0,
        "the function type's own type parameter must resolve (no TS2304); got {diags:?}"
    );
}

/// Renamed binders behave identically (no name-specific shortcut).
#[test]
fn class_method_signature_renamed_binders_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Crate {
    protected hidden!: boolean;
    inspect(probe: this["hidden"]): void {}
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 with renamed binders; got {diags:?}"
    );
}

/// An overload signature return type of `this["<private>"]` reports TS4105.
#[test]
fn overload_signature_return_this_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Dispatcher {
    private handle!: number;
    on(name: string): this["handle"];
    on(name: number): number;
    on(name: any): any { return name; }
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for an overload return `this[\"handle\"]`; got {diags:?}"
    );
}

/// An abstract method signature return type of `this["<protected>"]` reports
/// TS4105 even though it has no body.
#[test]
fn abstract_method_signature_this_indexed_protected_reports_ts4105() {
    let diags = diagnostics(
        r#"
abstract class Shape {
    protected area!: number;
    abstract measure(): this["area"];
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for an abstract method return `this[\"area\"]`; got {diags:?}"
    );
}

/// Public members indexed through `this`/`Base` in signature positions must NOT
/// report TS4105 (return, parameter, and property positions all stay clean).
#[test]
fn public_member_signature_positions_no_ts4105() {
    let diags = diagnostics(
        r#"
class Base { open!: string; }
class Clean {
    open!: string;
    ret(): this["open"] { return this.open; }
    take(value: this["open"]): void {}
    field: Base["open"] = "x";
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "public members in signature positions must not trigger TS4105; got {diags:?}"
    );
}

/// Generic keyed access over a type parameter in method/function signatures must
/// stay clean — this is the common `obj[key]` shape and must not regress into a
/// false TS4105/TS2536.
#[test]
fn generic_keyed_access_signatures_stay_clean() {
    let diags = diagnostics(
        r#"
class Emitter<M> {
    on<E extends keyof M>(e: E, cb: (p: M[E]) => void): void {}
    get<E extends keyof M>(e: E): M[E] { return null as any; }
}
function pick<T, K extends keyof T>(obj: T, key: K): T[K] { return obj[key]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "generic keyed access must not trigger TS4105; got {diags:?}"
    );
    assert_eq!(
        count(&diags, 2536),
        0,
        "generic keyed access must not trigger TS2536; got {diags:?}"
    );
}

/// Utility/generic type applications indexed by a public literal property must
/// stay clean. The signature-position TS4105 probe is only for bare
/// type-parameter/`this` candidates such as `T["private"]`, not for resolving
/// generic helper applications like `Parameters<F>["length"]`.
#[test]
fn generic_type_application_literal_index_signatures_stay_clean() {
    let diags = diagnostics(
        r#"
type DataFirst = (value: string, count: number) => void;
type Box<T> = { value: T };

function arity(value: Parameters<DataFirst>["length"]): Parameters<DataFirst>["length"] {
    return value;
}

function unwrap<T>(value: Box<T>["value"]): T {
    return value;
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "generic type applications with public literal indexes must not trigger TS4105; got {diags:?}"
    );
}

/// `infer` variables inside conditional type branches are not in the signature
/// walk's type-parameter scope. The TS4105 probe must therefore recurse through
/// `R["length"]` without resolving `R` and emitting a spurious TS2304.
#[test]
fn conditional_infer_indexed_access_does_not_report_ts2304() {
    let diags = diagnostics(
        r#"
type TailLength<T extends unknown[]> = T extends [...unknown[], ...infer R]
    ? R["length"]
    : never;

type Value = TailLength<[1, 2, 3]>;
"#,
    );
    assert_eq!(
        count(&diags, 2304),
        0,
        "infer variables in conditional branches must not be resolved by the TS4105 probe; got {diags:?}"
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "infer variables are not TS4105 candidates; got {diags:?}"
    );
}
