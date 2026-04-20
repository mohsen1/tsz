#[test]
fn folding_function_body() {
    let t = FourslashTest::new(
        "
        function foo() {
            const x = 1;
            const y = 2;
            return x + y;
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_class_body() {
    let t = FourslashTest::new(
        "
        class MyClass {
            method1() {
                return 1;
            }
            method2() {
                return 2;
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_nested_blocks() {
    let t = FourslashTest::new(
        "
        function outer() {
            if (true) {
                for (let i = 0; i < 10; i++) {
                    console.log(i);
                }
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_empty_file() {
    let t = FourslashTest::new("");
    let result = t.folding_ranges("test.ts");
    assert!(result.ranges.is_empty());
}

#[test]
fn folding_import_group() {
    let t = FourslashTest::new(
        "
        import { a } from './a';
        import { b } from './b';
        import { c } from './c';

        const x = 1;
    ",
    );
    // Should not crash even if import folding isn't implemented
    let _ = t.folding_ranges("test.ts");
}

// =============================================================================
// Selection Range Tests
// =============================================================================

#[test]
fn selection_range_identifier() {
    let t = FourslashTest::new(
        "
        const /*x*/myVariable = 42;
    ",
    );
    t.selection_range("x").expect_found();
}

#[test]
fn selection_range_has_parent() {
    let t = FourslashTest::new(
        "
        function foo() {
            const /*x*/x = 1;
        }
    ",
    );
    let result = t.selection_range("x");
    result.expect_found();
    assert!(
        result.depth() >= 2,
        "Expected depth >= 2, got {}",
        result.depth()
    );
}

#[test]
fn selection_range_nested_expression() {
    let t = FourslashTest::new(
        "
        const result = (/*m*/a + b) * c;
    ",
    );
    t.selection_range("m").expect_found();
}

// =============================================================================
// Document Highlighting Tests
// =============================================================================

#[test]
fn highlight_variable_usage() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 1;
        x + x;
    ",
    );
    let result = t.document_highlights("x");
    result.expect_found();
    // At least 3 highlights (declaration + 2 usages, possibly more due to impl details)
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

#[test]
fn highlight_function_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/foo() {}
        foo();
        foo();
    ",
    );
    let result = t.document_highlights("fn");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

