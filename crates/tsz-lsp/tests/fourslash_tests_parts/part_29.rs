#[test]
fn folding_object_literal() {
    let t = FourslashTest::new(
        "
        const config = {
            host: 'localhost',
            port: 80,
            debug: true,
        };
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

// =============================================================================
// Selection Range: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn selection_range_in_function_body() {
    let t = FourslashTest::new(
        "
        function test() {
            const x = /*m*/42;
        }
    ",
    );
    let result = t.selection_range("m");
    result.expect_found().expect_has_parent();
}

#[test]
fn selection_range_in_class_method() {
    let t = FourslashTest::new(
        "
        class Foo {
            bar() {
                return /*m*/this;
            }
        }
    ",
    );
    t.selection_range("m").expect_found().expect_has_parent();
}

#[test]
fn selection_range_in_object_literal() {
    let t = FourslashTest::new(
        "
        const obj = {
            key: /*m*/'value',
        };
    ",
    );
    t.selection_range("m").expect_found();
}

// =============================================================================
// Document Highlights: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn highlight_class_name_usage() {
    let t = FourslashTest::new(
        "
        class /*h*/Foo {}
        const a: Foo = new Foo();
        const b = new Foo();
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights for class usage"
    );
}

#[test]
fn highlight_enum_name() {
    let t = FourslashTest::new(
        "
        enum /*h*/Color { Red, Green, Blue }
        const c: Color = Color.Red;
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights for enum"
    );
}

#[test]
fn highlight_interface_name() {
    let t = FourslashTest::new(
        "
        interface /*h*/Serializable {
            serialize(): string;
        }
        class Item implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights for interface"
    );
}

// =============================================================================
// Semantic Tokens: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn semantic_tokens_enum_declaration() {
    let t = FourslashTest::new(
        "
        enum Direction {
            Up = 'UP',
            Down = 'DOWN',
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_generic_function() {
    let t = FourslashTest::new(
        "
        function map<T, U>(arr: T[], fn: (item: T) => U): U[] {
            return arr.map(fn);
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_decorators() {
    let t = FourslashTest::new(
        "
        function log(target: any) { return target; }

        @log
        class MyService {
            @log
            method() {}
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

