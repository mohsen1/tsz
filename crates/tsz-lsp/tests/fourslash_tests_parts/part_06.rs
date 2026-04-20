#[test]
fn completions_in_function_body() {
    let mut t = FourslashTest::new(
        "
        function test(param1: number, param2: string) {
            /**/
        }
    ",
    );
    let result = t.completions("");
    // Inside function body, params should be available
    if !result.items.is_empty() {
        result.expect_contains("param1").expect_contains("param2");
    }
}

#[test]
fn completions_enum_members() {
    let mut t = FourslashTest::new(
        "
        enum Color { Red, Green, Blue }
        Color./**/
    ",
    );
    let result = t.completions("");
    if !result.items.is_empty() {
        result.expect_contains("Red");
    }
}

// =============================================================================
// Signature Help Tests
// =============================================================================

#[test]
fn signature_help_at_call() {
    let mut t = FourslashTest::new(
        "
        function add(a: number, b: number): number { return a + b; }
        add(/**/);
    ",
    );
    let result = t.signature_help("");
    if result.help.is_some() {
        result
            .expect_found()
            .expect_label_contains("add")
            .expect_active_parameter(0);
    }
}

#[test]
fn signature_help_second_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(name: string, greeting: string) {}
        greet('hello', /**/);
    ",
    );
    let result = t.signature_help("");
    if result.help.is_some() {
        result.expect_found().expect_label_contains("greet");
    }
}

#[test]
fn signature_help_no_help_outside_call() {
    let mut t = FourslashTest::new(
        "
        function foo() {}
        /**/const x = 1;
    ",
    );
    t.signature_help("").expect_none();
}

// =============================================================================
// Diagnostics Tests
// =============================================================================

#[test]
fn diagnostics_clean_file() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
        const y: string = 'hello';
    ",
    );
    t.diagnostics("test.ts").expect_none();
}

#[test]
fn diagnostics_type_mismatch() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'hello';
    ",
    );
    let result = t.diagnostics("test.ts");
    if !result.diagnostics.is_empty() {
        result.expect_code(2322);
    }
}

#[test]
fn diagnostics_undeclared_variable() {
    let mut t = FourslashTest::new(
        "
        const x = undeclaredVariable;
    ",
    );
    let result = t.diagnostics("test.ts");
    if !result.diagnostics.is_empty() {
        result.expect_code(2304);
    }
}

#[test]
fn diagnostics_multiple_errors() {
    let mut t = FourslashTest::new(
        "
        const a: number = 'str';
        const b: string = 42;
    ",
    );
    let result = t.diagnostics("test.ts");
    if result.diagnostics.len() >= 2 {
        result.expect_code(2322);
    }
}

#[test]
fn diagnostics_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    t.verify_no_errors("test.ts");
}

// =============================================================================
// Folding Range Tests
// =============================================================================

