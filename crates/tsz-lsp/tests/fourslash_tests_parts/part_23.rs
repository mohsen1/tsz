#[test]
fn references_multiple_declarations() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        const y = /*r1*/x + 2;
        const z = /*r2*/x + 3;
        const w = /*r3*/x + /*r4*/x;
    ",
    );
    t.references("def").expect_found().expect_count(5);
}

#[test]
fn references_from_middle_usage() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        const y = /*ref*/x + 2;
        const z = x + 3;
    ",
    );
    // Finding references from a usage should find all refs including declaration
    t.references("ref").expect_found();
}

#[test]
fn references_class_with_heritage() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Base { value = 1; }
        class Child extends /*r1*/Base {}
        class GrandChild extends Child {}
        const b: /*r2*/Base = new Base();
    ",
    );
    // 4 references: definition + heritage clause + type annotation + constructor call
    t.references("def").expect_found().expect_count(4);
}

// =============================================================================
// References: Additional Patterns
// =============================================================================

#[test]
fn references_import_alias() {
    let mut t = FourslashTest::new(
        "
        type /*def*/Str = string;
        const x: /*r1*/Str = 'hello';
        const y: /*r2*/Str = 'world';
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_enum_member() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green, Blue }
        const c: Color = Color.Red;
        function paint(color: /*r1*/Color) {}
    ",
    );
    t.references("def").expect_found().expect_count(4);
}

// =============================================================================
// Definition: Edge Cases
// =============================================================================

#[test]
fn definition_default_parameter() {
    let mut t = FourslashTest::new(
        "
        const /*def*/DEFAULT = 42;
        function foo(x = /*ref*/DEFAULT) { return x; }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_computed_property() {
    let mut t = FourslashTest::new(
        "
        const /*def*/KEY = 'hello';
        const obj = { [/*ref*/KEY]: 1 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_template_literal_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'world';
        const greeting = `hello ${/*ref*/name}`;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Hover: Edge Cases
// =============================================================================

#[test]
fn hover_const_assertion() {
    let mut t = FourslashTest::new(
        "
        const /*h*/colors = ['red', 'green', 'blue'] as const;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_destructured_parameter() {
    let mut t = FourslashTest::new(
        "
        function foo({ /*h*/x, y }: { x: number; y: string }) {
            return x;
        }
    ",
    );
    t.hover("h").expect_found();
}

// =============================================================================
// Definition: Extends/Implements Resolution
// =============================================================================

