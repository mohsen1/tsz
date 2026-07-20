//! Construct-signature parameter variance (issue #14859).
//!
//! `tsc` reserves constructor-parameter **bivariance** for class-derived
//! constructor functions (`typeof Class`, whose construct-signature declaration
//! is a class `Constructor`). A standalone `new (...) => T` construct-signature
//! **type literal** and an **interface** construct signature (declaration kind
//! `ConstructSignature`) compare their parameters **strictly** — contravariantly
//! under `strictFunctionTypes` — exactly like a call-signature literal.
//!
//! Before the fix tsz compared every construct signature's parameters
//! bivariantly, silently accepting assignments `tsc` rejects (a false negative).

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

const TS2322: u32 = 2322;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

/// Compile `source` under `--strict` and return the emitted diagnostic codes.
fn diagnostic_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("variance.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "variance.ts".to_string(),
        strict_options(),
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn count(source: &str, code: u32) -> usize {
    diagnostic_codes(source)
        .into_iter()
        .filter(|c| *c == code)
        .count()
}

// ---------------------------------------------------------------------------
// `new (...) => T` construct-signature TYPE LITERALS: strict (contravariant).
// ---------------------------------------------------------------------------

#[test]
fn literal_construct_signature_rejects_subtype_param() {
    // `new (x: 1) => object` is NOT assignable to `new (x: number) => object`.
    let source = "
        type CtorNum = new (x: number) => object;
        type CtorLit = new (x: 1) => object;
        declare const cl: CtorLit;
        const bad: CtorNum = cl;
    ";
    assert_eq!(
        count(source, TS2322),
        1,
        "expected TS2322 for a narrowed-parameter `new (...) =>` literal under strictFunctionTypes",
    );
}

#[test]
fn literal_construct_signature_accepts_contravariant_param() {
    // The contravariant-OK direction stays assignable:
    // `new (x: number) => object` IS assignable to `new (x: 1) => object`.
    let source = "
        type CtorNum = new (x: number) => object;
        type CtorLit = new (x: 1) => object;
        declare const cn: CtorNum;
        const good: CtorLit = cn;
    ";
    assert_eq!(
        count(source, TS2322),
        0,
        "the contravariant-OK direction must remain accepted",
    );
}

#[test]
fn literal_construct_signature_param_variance_is_name_independent() {
    // Same defect with renamed binders / string-literal params, so a spelling
    // fix would not pass.
    let source = "
        type MakeWide = new (token: string) => { tag: 0 };
        type MakeNarrow = new (token: \"a\") => { tag: 0 };
        declare const narrow: MakeNarrow;
        const wide: MakeWide = narrow;
    ";
    assert_eq!(count(source, TS2322), 1);
}

// ---------------------------------------------------------------------------
// INTERFACE construct signatures: strict (contravariant), same as literals.
// ---------------------------------------------------------------------------

#[test]
fn interface_construct_signature_rejects_subtype_param() {
    let source = "
        interface ICtorNum { new (x: number): object; }
        interface ICtorLit { new (x: 1): object; }
        declare const icl: ICtorLit;
        const bad: ICtorNum = icl;
    ";
    assert_eq!(
        count(source, TS2322),
        1,
        "interface construct signatures must compare parameters strictly",
    );
}

// ---------------------------------------------------------------------------
// CLASS constructors (`typeof Class`): bivariant — must NOT regress.
// ---------------------------------------------------------------------------

#[test]
fn class_constructor_params_stay_bivariant_both_directions() {
    let source = "
        class CNum { constructor(x: number) {} }
        class CLit { constructor(x: 1) {} }
        declare const tn: typeof CNum;
        declare const tl: typeof CLit;
        const a: typeof CNum = tl;
        const b: typeof CLit = tn;
    ";
    assert_eq!(
        count(source, TS2322),
        0,
        "class-derived constructor parameters must keep tsc's bivariance",
    );
}

#[test]
fn class_source_does_not_loosen_strict_construct_target() {
    let source = "
        class Animal { animal = true; }
        class Dog extends Animal { dog = true; }
        class DogCtor { constructor(value: Dog) {} }
        class AnimalCtor { constructor(value: Animal) {} }
        const strictTarget: new (value: Animal) => DogCtor = DogCtor;
        const classTarget: typeof AnimalCtor = DogCtor;
    ";
    assert_eq!(
        count(source, TS2322),
        1,
        "variance is selected from the target declaration kind",
    );
}

#[test]
fn class_extending_construct_typed_value_preserves_strict_provenance() {
    let source = "
        declare const Base: new (value: 1) => object;
        class Derived extends Base {}
        const strictTarget: new (value: number) => object = Derived;
    ";
    assert_eq!(
        count(source, TS2322),
        1,
        "inherited construct-signature types stay strict",
    );
}

#[test]
fn inherited_constructor_target_retains_declaration_provenance() {
    let source = "
        class Animal { animal = true; }
        class Dog extends Animal { dog = true; }
        declare const StrictBase: new (value: Animal) => object;
        class StrictDerived extends StrictBase {}
        class RealBase { constructor(value: Animal) {} }
        class RealDerived extends RealBase {}
        class DogCtor { constructor(value: Dog) {} }
        const strictTarget: typeof StrictDerived = DogCtor;
        const bivariantTarget: typeof RealDerived = DogCtor;
    ";
    assert_eq!(
        count(source, TS2322),
        1,
        "inherited constructor signatures retain their original declaration kind",
    );
}

// ---------------------------------------------------------------------------
// CALL-signature literals already contravariant — guards the shared path.
// ---------------------------------------------------------------------------

#[test]
fn call_signature_literal_rejects_subtype_param() {
    let source = "
        type FnNum = (x: number) => void;
        type FnLit = (x: 1) => void;
        declare const fl: FnLit;
        const bad: FnNum = fl;
    ";
    assert_eq!(count(source, TS2322), 1);
}
