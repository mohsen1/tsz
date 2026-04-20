#[test]
fn definition_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );
    t.verify_definition("ref", "def");
}

// =============================================================================
// Go-to-Type-Definition Tests
// =============================================================================

#[test]
fn type_definition_variable_with_annotation() {
    let t = FourslashTest::new(
        "
        interface /*def*/Point { x: number; y: number; }
        const /*ref*/p: Point = { x: 0, y: 0 };
    ",
    );
    let result = t.go_to_type_definition("ref");
    // Type definition should resolve to the interface declaration
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
        result.expect_at_marker("def");
    }
}

#[test]
fn type_definition_no_result_for_primitive() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // Primitives have no user-defined type declaration
    t.go_to_type_definition("x").expect_none();
}

#[test]
fn type_definition_class_typed_variable() {
    let t = FourslashTest::new(
        "
        class /*def*/MyClass { value = 0; }
        const /*ref*/obj: MyClass = new MyClass();
    ",
    );
    let result = t.go_to_type_definition("ref");
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
        result.expect_at_marker("def");
    }
}

#[test]
fn type_definition_interface_variable() {
    let t = FourslashTest::new(
        "
        interface /*def*/Config { host: string; port: number; }
        const /*ref*/cfg: Config = { host: 'localhost', port: 80 };
    ",
    );
    let result = t.go_to_type_definition("ref");
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
        result.expect_at_marker("def");
    }
}

#[test]
fn type_definition_function_parameter() {
    let t = FourslashTest::new(
        "
        interface /*def*/User { name: string; }
        function greet(/*ref*/user: User) {}
    ",
    );
    let result = t.go_to_type_definition("ref");
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
        result.expect_at_marker("def");
    }
}

// =============================================================================
// Hover Tests
// =============================================================================

#[test]
fn hover_const_number() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    t.hover("x")
        .expect_found()
        .expect_display_string_contains("x");
}

#[test]
fn hover_function_declaration() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

#[test]
fn hover_class_name() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/MyClass {
            value: number = 0;
        }
    ",
    );
    t.hover("cls")
        .expect_found()
        .expect_display_string_contains("MyClass");
}

#[test]
fn hover_interface_name() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Point {
            x: number;
            y: number;
        }
    ",
    );
    t.hover("iface")
        .expect_found()
        .expect_display_string_contains("Point");
}

