#[test]
fn hover_private_method() {
    let mut t = FourslashTest::new(
        "
        class Service {
            private /*h*/fetchData() { return null; }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("fetchData");
}

#[test]
fn hover_static_property() {
    let mut t = FourslashTest::new(
        "
        class Counter {
            static /*h*/count = 0;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("count");
}

#[test]
fn hover_computed_property() {
    let mut t = FourslashTest::new(
        "
        const key = 'name';
        const obj = { [key]: 'value' };
        const /*h*/x = obj;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("x");
}

// =============================================================================
// Hover: JSDoc & Documentation (NEW)
// =============================================================================

#[test]
fn hover_jsdoc_param_tags() {
    let mut t = FourslashTest::new(
        "
        /**
         * Calculates the sum of two numbers.
         * @param a - First number
         * @param b - Second number
         * @returns The sum
         */
        function /*h*/add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("add")
        .expect_documentation_contains("sum");
}

#[test]
fn hover_jsdoc_deprecated() {
    let mut t = FourslashTest::new(
        "
        /**
         * @deprecated Use newMethod instead
         */
        function /*h*/oldMethod() {}
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("deprecated");
}

#[test]
fn hover_jsdoc_example() {
    let mut t = FourslashTest::new(
        "
        /**
         * Formats a name.
         * @example
         * formatName('john') // => 'John'
         */
        function /*h*/formatName(name: string) { return name; }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("example");
}

#[test]
fn hover_jsdoc_multiline() {
    let mut t = FourslashTest::new(
        "
        /**
         * A complex utility function that does several things:
         *
         * 1. Validates input
         * 2. Transforms data
         * 3. Returns result
         */
        function /*h*/process(data: string) { return data; }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("Validates");
}

// =============================================================================
// References: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn references_type_annotation_usage() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Config {
            host: string;
        }
        const a: /*r1*/Config = { host: '' };
        function f(c: /*r2*/Config) {}
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_enum_usage() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green }
        const c: /*r1*/Color = Color.Red;
        function paint(color: /*r2*/Color) {}
    ",
    );
    // 4 references: definition + 2 type annotations + value reference in Color.Red
    t.references("def").expect_found().expect_count(4);
}

#[test]
fn references_type_alias_usage() {
    let mut t = FourslashTest::new(
        "
        type /*def*/ID = string | number;
        const id: /*r1*/ID = '123';
        function getById(id: /*r2*/ID) {}
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

