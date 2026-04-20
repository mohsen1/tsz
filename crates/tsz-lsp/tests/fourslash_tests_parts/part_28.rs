#[test]
fn symbols_exported_declarations() {
    let mut t = FourslashTest::new(
        "
        export const VERSION = '1.0';
        export function initialize() {}
        export class App {}
        export interface AppConfig { debug: boolean; }
        export type AppId = string;
        export enum AppStatus { Running, Stopped }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("VERSION")
        .expect_symbol("initialize")
        .expect_symbol("App")
        .expect_symbol("AppConfig")
        .expect_symbol("AppId")
        .expect_symbol("AppStatus");
}

#[test]
fn symbols_module_declarations() {
    let mut t = FourslashTest::new(
        "
        declare module 'my-module' {
            export function doSomething(): void;
        }
    ",
    );
    t.document_symbols("test.ts").expect_found();
}

// =============================================================================
// Diagnostics: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn diagnostics_unused_variable() {
    let mut t = FourslashTest::new(
        "
        const x = 42;
    ",
    );
    // A clean file with just a const should have no errors
    t.verify_no_errors("test.ts");
}

#[test]
fn diagnostics_duplicate_identifier() {
    let mut t = FourslashTest::new(
        "
        let x = 1;
        let x = 2;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

#[test]
fn diagnostics_missing_return_type() {
    let mut t = FourslashTest::new(
        "
        function add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.verify_no_errors("test.ts");
}

#[test]
fn diagnostics_const_reassignment() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        x = 2;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

#[test]
fn diagnostics_property_does_not_exist() {
    let mut t = FourslashTest::new(
        "
        interface Point { x: number; y: number; }
        const p: Point = { x: 1, y: 2 };
        p.z;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

// =============================================================================
// Folding Ranges: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn folding_multiline_comment() {
    let t = FourslashTest::new(
        "
        /**
         * This is a multiline
         * JSDoc comment
         */
        function test() {
            return 1;
        }
    ",
    );
    t.folding_ranges("test.ts")
        .expect_found()
        .expect_min_count(2); // comment + function body
}

#[test]
fn folding_switch_statement() {
    let t = FourslashTest::new(
        "
        function handler(action: string) {
            switch (action) {
                case 'a': {
                    break;
                }
                case 'b': {
                    break;
                }
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_array_literal() {
    let t = FourslashTest::new(
        "
        const items = [
            1,
            2,
            3,
            4,
            5,
        ];
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

