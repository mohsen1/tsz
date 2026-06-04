#[test]
fn combined_diagnostics_and_code_actions() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'not a number';
    ",
    );

    // Should have a diagnostic
    t.diagnostics("test.ts").expect_found();

    // Code actions should be available
    let actions = t.code_actions("test.ts");
    // Just verify no crash
    let _ = actions;
}

#[test]
fn combined_folding_selection_highlights() {
    let t = FourslashTest::new(
        "
        function /*f*/processData(data: string[]) {
            const results: string[] = [];
            for (const item of data) {
                results.push(item.trim());
            }
            return results;
        }
    ",
    );

    // Folding ranges
    t.folding_ranges("test.ts").expect_found();

    // Selection range inside
    t.selection_range("f").expect_found();

    // Highlights
    let hl = t.document_highlights("f");
    hl.expect_found();
}

#[test]
fn edit_file_preserves_definition() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );

    // Initially should work
    t.go_to_definition("ref").expect_at_marker("def");

    // Edit the file - add a new line
    t.edit_file("test.ts", "const x = 1;\nconst y = 2;\nx;");

    // Should still have symbols
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("x")
        .expect_symbol("y");
}

#[test]
fn edit_file_new_function_appears() {
    let mut t = FourslashTest::new("const x = 1;");

    // Add a function
    t.edit_file("test.ts", "const x = 1;\nfunction newFunc() { return x; }");

    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("x")
        .expect_symbol("newFunc");
}

#[test]
fn edit_file_fixes_diagnostic() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'bad';
    ",
    );

    // Should have error
    t.diagnostics("test.ts").expect_found();

    // Fix the type error
    t.edit_file("test.ts", "const x: number = 42;");

    // Should be clean now
    t.verify_no_errors("test.ts");
}
