#[test]
fn edge_case_empty_interface() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Marker {}
    ",
    );
    t.hover("iface")
        .expect_found()
        .expect_display_string_contains("Marker");
}

#[test]
fn edge_case_single_line_function() {
    let mut t = FourslashTest::new(
        "
        const /*f*/double = (x: number) => x * 2;
    ",
    );
    t.hover("f")
        .expect_found()
        .expect_display_string_contains("double");
}

#[test]
fn edge_case_nested_destructuring() {
    let mut t = FourslashTest::new(
        "
        const { a: { /*def*/b } } = { a: { b: 42 } };
        /*ref*/b;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_spread_operator() {
    let mut t = FourslashTest::new(
        "
        const /*def*/base = { x: 1, y: 2 };
        const extended = { .../*ref*/base, z: 3 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_template_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'World';
        const greeting = `Hello ${/*ref*/name}!`;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_ternary_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/flag = true;
        const result = /*ref*/flag ? 'yes' : 'no';
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_array_destructuring() {
    let mut t = FourslashTest::new(
        "
        const [/*def*/first, second] = [1, 2];
        /*ref*/first;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_for_of_variable() {
    let mut t = FourslashTest::new(
        "
        const items = [1, 2, 3];
        for (const /*def*/item of items) {
            console.log(/*ref*/item);
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_labeled_statement() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_very_long_identifier() {
    let mut t = FourslashTest::new(
        "
        const /*def*/thisIsAVeryLongVariableNameThatShouldStillWorkCorrectly = 42;
        /*ref*/thisIsAVeryLongVariableNameThatShouldStillWorkCorrectly;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

