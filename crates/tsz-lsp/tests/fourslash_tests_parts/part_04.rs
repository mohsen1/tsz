#[test]
fn references_simple_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*r1*/x + /*r2*/x;
    ",
    );
    // Definition + 2 usages = 3 references
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_function() {
    let mut t = FourslashTest::new(
        "
        function /*def*/foo() {}
        /*r1*/foo();
        /*r2*/foo();
    ",
    );
    // Definition + 2 calls = 3 references
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_class() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Point {
            x = 0;
        }
        const p = new /*r1*/Point();
        const q: /*r2*/Point = p;
    ",
    );
    t.references("def").expect_found();
}

#[test]
fn references_no_refs_for_keyword() {
    let mut t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    t.references("kw").expect_none();
}

#[test]
fn references_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    // Parameter declaration + usage in body = 2 refs
    t.references("p").expect_found().expect_count(2);
}

#[test]
fn references_from_usage_site() {
    let mut t = FourslashTest::new(
        "
        const /*def*/y = 1;
        /*ref*/y + 1;
    ",
    );
    // Querying from the usage should also find all refs
    t.references("ref").expect_found().expect_count(2);
}

// =============================================================================
// Rename Tests
// =============================================================================

#[test]
fn rename_simple_variable() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 1;
        x + x;
    ",
    );
    t.rename("x", "newName")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(3); // declaration + 2 usages
}

#[test]
fn rename_function() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/foo() {}
        foo();
    ",
    );
    t.rename("fn", "bar")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(2); // declaration + 1 call
}

#[test]
fn rename_class() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/Foo {
            value = 1;
        }
        const f = new Foo();
        const x: Foo = f;
    ",
    );
    t.rename("cls", "Bar")
        .expect_success()
        .expect_edits_in_file("test.ts");
}

#[test]
fn rename_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    t.rename("p", "person")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(2); // parameter + body usage
}

// =============================================================================
// Document Symbols Tests
// =============================================================================

