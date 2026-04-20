#[test]
fn switch_case_scoping() {
    let mut t = FourslashTest::new(
        "
        function test(x: number) {
            switch (x) {
                case 1: {
                    const /*def*/result = 'one';
                    console.log(/*ref*/result);
                    break;
                }
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn object_method_shorthand_hover() {
    let mut t = FourslashTest::new(
        "
        const /*obj*/obj = {
            greet() { return 'hello'; },
            farewell() { return 'bye'; }
        };
    ",
    );
    t.hover("obj")
        .expect_found()
        .expect_display_string_contains("obj");
}

#[test]
fn tuple_type_hover() {
    let mut t = FourslashTest::new(
        "
        const /*t*/pair: [string, number] = ['hello', 42];
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("pair");
}

#[test]
fn template_literal_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/EventName = `on${string}`;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("EventName");
}

#[test]
fn enum_with_values_symbols() {
    let mut t = FourslashTest::new(
        "
        enum HttpStatus {
            OK = 200,
            NotFound = 404,
            InternalServerError = 500
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("HttpStatus");
}

#[test]
fn multiple_export_forms_symbols() {
    let mut t = FourslashTest::new(
        "
        export const A = 1;
        export function B() {}
        export class C {}
        export interface D {}
        export type E = string;
        export enum F { X }
    ",
    );
    let result = t.document_symbols("test.ts");
    result
        .expect_found()
        .expect_symbol("A")
        .expect_symbol("B")
        .expect_symbol("C")
        .expect_symbol("D")
        .expect_symbol("E")
        .expect_symbol("F");
}

#[test]
fn index_signature_hover() {
    let mut t = FourslashTest::new(
        "
        interface /*t*/StringMap {
            [key: string]: number;
        }
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("StringMap");
}

#[test]
fn overloaded_method_hover() {
    let mut t = FourslashTest::new(
        "
        class Calculator {
            /*fn*/add(a: number, b: number): number;
            add(a: string, b: string): string;
            add(a: any, b: any): any { return a + b; }
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

// =============================================================================
// Linked Editing Range Tests (JSX)
// =============================================================================

#[test]
fn linked_editing_non_jsx() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 1;
    ",
    );
    // Non-JSX code should not have linked editing ranges
    t.linked_editing_ranges("x").expect_none();
}

// =============================================================================
// Code Actions Tests
// =============================================================================

#[test]
fn code_actions_whole_file() {
    let t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    // Clean file should have no code actions (or some refactorings)
    let _ = t.code_actions("test.ts");
}

