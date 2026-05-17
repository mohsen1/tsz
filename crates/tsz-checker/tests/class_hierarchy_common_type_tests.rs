use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn inferred_return_and_conditional_keep_input_class_supertype() {
    let source = r#"
class Base {
    base: string = "";
}
class Derived1 extends Base {
    first: number = 1;
}
class Derived2 extends Base {
    second: number = 2;
}

function pickOne(index: number) {
    if (index === 1) return new Derived1();
    if (index === 2) return new Derived2();
    return new Base();
}

declare const flag: boolean;
const chosen = flag ? new Derived1() : new Base();

const badFn: () => Derived1 = pickOne;
const badChosen: Derived1 = chosen;
"#;

    let diagnostics = check_source_diagnostics(source);
    let fn_mismatch = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for assigning wider return type to narrower function");
    assert!(
        fn_mismatch.message_text.contains("=> Base"),
        "inferred function return should reduce to the input supertype `Base`: {fn_mismatch:?}"
    );
    assert!(
        !fn_mismatch.message_text.contains("Derived2"),
        "inferred function return should not keep the sibling class union: {fn_mismatch:?}"
    );

    let chosen_mismatch = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .expect("expected TS2741 for assigning `Base` conditional result to `Derived1`");
    assert!(
        chosen_mismatch.message_text.contains("type 'Base'"),
        "conditional result should reduce to the input supertype `Base`: {chosen_mismatch:?}"
    );
}
