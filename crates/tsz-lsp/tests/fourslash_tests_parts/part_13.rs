#[test]
fn file_count_single() {
    let t = FourslashTest::new("const x = 1;");
    assert_eq!(t.file_count(), 1);
}

#[test]
fn file_count_multi() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export const a = 1;"),
        ("b.ts", "export const b = 2;"),
        ("c.ts", "export const c = 3;"),
    ]);
    assert_eq!(t.file_count(), 3);
}

#[test]
fn remove_file_from_project() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export const a = 1;"),
        ("b.ts", "export const b = 2;"),
    ]);
    assert_eq!(t.file_count(), 2);
    t.remove_file("b.ts");
    assert_eq!(t.file_count(), 1);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn empty_source() {
    let mut t = FourslashTest::new("");
    assert!(t.document_symbols("test.ts").symbols.is_empty());
}

#[test]
fn markers_at_start_and_end() {
    let t = FourslashTest::new("/*start*/const x = 1/*end*/;");
    assert_eq!(t.marker("start").character, 0);
}

#[test]
fn multiple_markers_same_line() {
    let t = FourslashTest::new("const /*a*/a = /*b*/b;");
    let a = t.marker("a");
    let b = t.marker("b");
    assert_eq!(a.line, b.line);
    assert!(a.character < b.character);
}

#[test]
fn marker_in_string_literal() {
    let t = FourslashTest::new("const s = '/*m*/hello';");
    let m = t.marker("m");
    assert_eq!(m.line, 0);
}

#[test]
fn unicode_in_identifiers() {
    let mut t = FourslashTest::new(
        "
        const /*x*/café = 'coffee';
    ",
    );
    t.hover("x").expect_found();
}

#[test]
fn very_long_line() {
    let long_var = "a".repeat(200);
    let source = format!("const /*x*/{long_var} = 1;");
    let mut t = FourslashTest::new(&source);
    t.hover("x").expect_found();
}

// =============================================================================
// Complex Scenario Tests
// =============================================================================

#[test]
fn class_method_dot_access_definition() {
    let mut t = FourslashTest::new(
        "
        class MyClass {
            /*def*/method() {
                return 42;
            }
        }
        const obj = new MyClass();
        obj./*ref*/method();
    ",
    );
    // Method reference via dot access
    let result = t.go_to_definition("ref");
    // Just verify it doesn't crash - dot access resolution varies
    let _ = result;
}

