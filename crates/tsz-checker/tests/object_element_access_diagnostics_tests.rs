use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn has_diagnostic_at(diagnostics: &[Diagnostic], code: u32, start: usize) -> bool {
    diagnostics
        .iter()
        .any(|diag| diag.code == code && diag.start == start as u32)
}

#[test]
fn annotated_element_access_initializer_preserves_inner_call_and_index_errors() {
    let source = r#"
class PetFood {
    constructor(name: string, whereToBuy: number) {}
}
class IceCream {
    constructor(flavor: string) {}
}
class Cookie {
    constructor(flavor: string, isGlutenFree: boolean) {}
}

var foods = new PetFood[new IceCream('Mint') , Cookie('Chocolate', false) , new Cookie('Peanut', true)];
var foods2: PetFood = new PetFood[new IceCream('Mint') , Cookie('Chocolate', false) , new Cookie('Peanut', true)];
"#;

    let diagnostics = check_source_diagnostics(source);
    let first_index_start = source
        .find("new IceCream('Mint')")
        .expect("first index expression");
    let first_cookie_start = source
        .find("Cookie('Chocolate', false)")
        .expect("first constructor call without new");
    let second_line_start = source.find("var foods2").expect("second declaration");
    let second_index_start = second_line_start
        + source[second_line_start..]
            .find("new IceCream('Mint')")
            .expect("second index expression");
    let second_cookie_start = second_line_start
        + source[second_line_start..]
            .find("Cookie('Chocolate', false)")
            .expect("second constructor call without new");

    assert!(
        has_diagnostic_at(&diagnostics, 2538, first_index_start),
        "expected TS2538 at first element-access index, got {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_at(&diagnostics, 2348, first_cookie_start),
        "expected TS2348 at first constructor call, got {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_at(&diagnostics, 2538, second_index_start),
        "expected TS2538 to survive contextual retry for annotated initializer, got {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_at(&diagnostics, 2348, second_cookie_start),
        "expected TS2348 to survive contextual retry for annotated initializer, got {diagnostics:#?}"
    );
}
