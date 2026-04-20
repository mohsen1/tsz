#[test]
fn hover_no_info_on_empty_line() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        /*ws*/
        const y = 2;
    ",
    );
    t.hover("ws").expect_none();
}

#[test]
fn hover_jsdoc_preserved() {
    let mut t = FourslashTest::new(
        "
        /** This is a documented function */
        function /*fn*/documented() {}
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("documented");
}

#[test]
fn hover_arrow_function() {
    let mut t = FourslashTest::new(
        "
        const /*fn*/add = (a: number, b: number) => a + b;
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

#[test]
fn hover_array_literal() {
    let mut t = FourslashTest::new(
        "
        const /*arr*/arr = [1, 2, 3];
    ",
    );
    t.hover("arr")
        .expect_found()
        .expect_display_string_contains("arr");
}

#[test]
fn hover_object_literal() {
    let mut t = FourslashTest::new(
        "
        const /*obj*/obj = { x: 1, y: 'hello' };
    ",
    );
    t.hover("obj")
        .expect_found()
        .expect_display_string_contains("obj");
}

#[test]
fn hover_enum_member() {
    let mut t = FourslashTest::new(
        "
        enum Direction {
            /*up*/Up,
            Down,
            Left,
            Right
        }
    ",
    );
    t.hover("up")
        .expect_found()
        .expect_display_string_contains("Up");
}

#[test]
fn hover_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*t*/StringOrNumber = string | number;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("StringOrNumber");
}

#[test]
fn hover_type_assertion() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42 as const;
    ",
    );
    t.hover("x")
        .expect_found()
        .expect_display_string_contains("x");
}

#[test]
fn hover_template_literal() {
    let mut t = FourslashTest::new(
        "
        const /*name*/name = 'world';
        const greeting = `hello ${name}`;
    ",
    );
    t.hover("name")
        .expect_found()
        .expect_display_string_contains("name");
}

#[test]
fn hover_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/myFunction() {}
    ",
    );
    t.verify_hover_contains("fn", "myFunction");
}

// =============================================================================
// Find References Tests
// =============================================================================

