//! Locks in that TS2538 (`Type X cannot be used as an index type`) and TS2348
//! (`Value of type X is not callable. Did you mean to include 'new'?`) survive
//! the variable-declaration "pre-contextual diagnostic reset" that runs when
//! the variable has a type annotation and the initializer is being re-checked
//! with that contextual type.
//!
//! Both diagnostics are structural — they describe the shape of the index
//! expression and the call site, not anything contextual-typing dependent —
//! so they must persist across the reset.
//!
//! Regression: `objectCreationOfElementAccessExpression.ts` — the second
//! `var foods2: MonsterFood[] = new PetFood[...]` lost its TS2538/TS2348 pair
//! while the unannotated `var foods = new PetFood[...]` kept them.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2538_survives_pre_contextual_reset_with_annotation() {
    let source = r#"
class A {}
class B {}
const i: B = new B();
const arr: A[] = new A[i];
"#;

    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2538),
        "expected TS2538 with `: A[]` annotation; got {codes:?}",
    );
}

#[test]
fn ts2348_survives_pre_contextual_reset_with_annotation() {
    let source = r#"
class C {
    constructor(public x: number) {}
}
const arr: number[] = C(1) as any;
"#;
    // Force the call-without-new path through an expression that survives in
    // the variable's contextual-typing range.
    let _ = check_source_codes(source);
    // The above example is intentionally not relied on — the canonical
    // reproducer is the element-access form below, which exercises both
    // codes simultaneously.
    let canonical = r#"
class Food {}
class Cookie extends Food {
    constructor(public flavor: string) { super(); }
}
class PetFood extends Food {
    constructor(public flavor: string) { super(); }
}
const annotated: Food[] = new PetFood[Cookie('chip')];
"#;
    let codes = check_source_codes(canonical);
    assert!(
        codes.contains(&2348),
        "expected TS2348 (call-without-new) with `: Food[]` annotation; got {codes:?}",
    );
}

#[test]
fn structural_codes_present_in_both_annotated_and_unannotated_forms() {
    // Mirror of `objectCreationOfElementAccessExpression.ts`: tsc emits
    // TS2538+TS2348 once per `var` line, regardless of annotation. Before
    // the fix, only the unannotated `var foods` line emitted them.
    let source = r#"
class A {}
class B {
    constructor(public flavor: string) {}
}
declare const ix: B;
var foods = new A[ix , B('chip') , new B('q')];
var foods2: A[] = new A[ix , B('chip') , new B('q')];
"#;
    let codes = check_source_codes(source);
    let count_2538 = codes.iter().filter(|&&c| c == 2538).count();
    let count_2348 = codes.iter().filter(|&&c| c == 2348).count();
    assert_eq!(
        count_2538, 2,
        "expected TS2538 on both var lines; got {codes:?}",
    );
    assert_eq!(
        count_2348, 2,
        "expected TS2348 on both var lines; got {codes:?}",
    );
}
