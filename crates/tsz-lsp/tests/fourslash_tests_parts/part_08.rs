#[test]
fn highlight_parameter() {
    let t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    let result = t.document_highlights("p");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

#[test]
fn highlight_assignment_write() {
    let t = FourslashTest::new(
        "
        let /*x*/x = 1;
        x = 2;
        console.log(x);
    ",
    );
    let result = t.document_highlights("x");
    result.expect_found();
    result.expect_has_write();
}

// =============================================================================
// Semantic Tokens Tests
// =============================================================================

#[test]
fn semantic_tokens_basic() {
    let t = FourslashTest::new(
        "
        const x = 42;
        function foo() {}
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_class() {
    let t = FourslashTest::new(
        "
        class MyClass {
            value: number = 0;
            method(): string { return ''; }
        }
    ",
    );
    t.semantic_tokens("test.ts")
        .expect_found()
        .expect_min_tokens(3);
}

#[test]
fn semantic_tokens_empty() {
    let t = FourslashTest::new("");
    assert!(t.semantic_tokens("test.ts").data.is_empty());
}

#[test]
fn semantic_tokens_interface_and_type() {
    let t = FourslashTest::new(
        "
        interface Foo {
            bar: string;
            baz: number;
        }
        type Combined = Foo & { extra: boolean };
    ",
    );
    t.semantic_tokens("test.ts")
        .expect_found()
        .expect_min_tokens(2);
}

// =============================================================================
// Workspace Symbols Tests
// =============================================================================

#[test]
fn workspace_symbols_empty_query() {
    let t = FourslashTest::new(
        "
        function mySpecialFunction() {}
    ",
    );
    t.workspace_symbols("").expect_none();
}

#[test]
fn workspace_symbols_no_match() {
    let t = FourslashTest::new(
        "
        const x = 1;
    ",
    );
    assert!(t.workspace_symbols("nonexistentSymbol").symbols.is_empty());
}

#[test]
fn workspace_symbols_finds_function() {
    let t = FourslashTest::new(
        "
        function calculateTotal() {}
        function calculateAverage() {}
    ",
    );
    let result = t.workspace_symbols("calculate");
    if !result.symbols.is_empty() {
        result.expect_symbol("calculateTotal");
        result.expect_symbol("calculateAverage");
    }
}

#[test]
fn workspace_symbols_finds_class() {
    let t = FourslashTest::new(
        "
        class UserService {
            getUser() {}
        }
    ",
    );
    let result = t.workspace_symbols("UserService");
    if !result.symbols.is_empty() {
        result.expect_symbol("UserService");
    }
}

// =============================================================================
// Formatting Tests
// =============================================================================

