//! Regression matrices for the construction-side signature boundary
//! (issue #13022).
//!
//! The converted pathways construct signature-bearing types through
//! `query_boundaries::construct_signatures`; these matrices pin the observable
//! behavior of each converted construction site:
//!
//! - overload compatibility (TS2394) for function and constructor overloads
//!   (`overload_compatibility.rs`: constructor function + constructor-only
//!   callable construction);
//! - type-argument application to constructor/callable types
//!   (`type_resolution/constructors.rs` + `callable_type_arguments.rs`:
//!   instantiated callable rebuilds, TS2345/TS2322 downstream);
//! - class-implements overload-set combination
//!   (`class_implements_checker/core.rs`: call-only callable, TS2416).
//!
//! Binder names are varied across cases per the anti-hardcoding gate.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source(source, "case.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

// ── Overloaded call matrix (TS2394 pathway) ─────────────────────────────────

#[test]
fn function_overload_set_compatible_with_widened_implementation() {
    let source = r#"
function renderUnit(value: string): string;
function renderUnit(value: number): number;
function renderUnit(value: string | number): string | number {
    return value;
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2394),
        "compatible overload set must not report TS2394, got: {diags:?}"
    );
}

#[test]
fn function_overload_set_incompatible_implementation_reports_ts2394() {
    let source = r#"
function parseChunk(raw: string): string;
function parseChunk(raw: number): number {
    return raw;
}
"#;
    let diags = codes(source);
    assert!(
        diags.contains(&2394),
        "implementation incompatible with its overload must report TS2394, got: {diags:?}"
    );
}

#[test]
fn constructor_overload_set_compatible_with_widened_implementation() {
    let source = r#"
class Envelope {
    constructor(seed: string);
    constructor(seed: number);
    constructor(seed: string | number) {}
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2394),
        "compatible constructor overloads must not report TS2394, got: {diags:?}"
    );
}

#[test]
fn constructor_overload_set_incompatible_implementation_reports_ts2394() {
    let source = r#"
class Ledger {
    constructor(entry: string);
    constructor(entry: number) {}
}
"#;
    let diags = codes(source);
    assert!(
        diags.contains(&2394),
        "constructor implementation incompatible with its overload must report TS2394, got: {diags:?}"
    );
}

// ── Class/assignability matrix (constructor instantiation + implements) ────

#[test]
fn generic_base_class_type_argument_application_accepts_matching_super_arg() {
    let source = r#"
class Carton<T> {
    payload: T;
    constructor(payload: T) {
        this.payload = payload;
    }
}
class LabelCarton extends Carton<string> {
    constructor() {
        super("label");
    }
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2345),
        "matching super argument against instantiated base constructor must not report TS2345, got: {diags:?}"
    );
}

#[test]
fn generic_base_class_type_argument_application_rejects_mismatched_super_arg() {
    let source = r#"
class Basket<T> {
    payload: T;
    constructor(payload: T) {
        this.payload = payload;
    }
}
class CountBasket extends Basket<string> {
    constructor() {
        super(41);
    }
}
"#;
    let diags = codes(source);
    assert!(
        diags.contains(&2345),
        "mismatched super argument against instantiated base constructor must report TS2345, got: {diags:?}"
    );
}

#[test]
fn instantiation_expression_call_signature_application_matrix() {
    // Positive: instantiated alias keeps the substituted signature.
    let ok = r#"
function lift<T>(value: T): T {
    return value;
}
const liftNum = lift<number>;
const widened: number = liftNum(7);
"#;
    let ok_diags = codes(ok);
    assert!(
        !ok_diags.contains(&2322),
        "instantiation expression with matching use must not report TS2322, got: {ok_diags:?}"
    );

    // Negative: the substituted return type must flow to assignments.
    let bad = r#"
function box<T>(value: T): T {
    return value;
}
const boxNum = box<number>;
const label: string = boxNum(7);
"#;
    let bad_diags = codes(bad);
    assert!(
        bad_diags.contains(&2322),
        "instantiation expression result misused must report TS2322, got: {bad_diags:?}"
    );
}

#[test]
fn implements_overloaded_interface_method_matrix() {
    // Positive: implementation handles the full overload set.
    let ok = r#"
interface Codec {
    convert(input: string): number;
    convert(input: number): string;
}
class WideCodec implements Codec {
    convert(input: string | number): any {
        return input;
    }
}
"#;
    let ok_diags = codes(ok);
    assert!(
        !ok_diags.contains(&2416),
        "implementation covering the full overload set must not report TS2416, got: {ok_diags:?}"
    );

    // Negative: implementation covering only one overload fails the combined
    // overload-set check.
    let bad = r#"
interface Mapper {
    remap(input: string): number;
    remap(input: number): string;
}
class NarrowMapper implements Mapper {
    remap(input: string): number {
        return 0;
    }
}
"#;
    let bad_diags = codes(bad);
    assert!(
        bad_diags.contains(&2416),
        "implementation covering one overload of the set must report TS2416, got: {bad_diags:?}"
    );
}
