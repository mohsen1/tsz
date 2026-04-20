#[test]
fn definition_const_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/myVar = 42;
        /*ref*/myVar + 1;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_let_variable() {
    let mut t = FourslashTest::new(
        "
        let /*def*/x = 'hello';
        console.log(/*ref*/x);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_function_declaration() {
    let mut t = FourslashTest::new(
        "
        function /*def*/greet(name: string) { return name; }
        /*ref*/greet('world');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_reference() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Foo { value = 1; }
        const f: /*ref*/Foo = new Foo();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_reference() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/IPoint { x: number; y: number; }
        const p: /*ref*/IPoint = { x: 0, y: 0 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*def*/StringOrNumber = string | number;
        const val: /*ref*/StringOrNumber = 42;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_reference() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green, Blue }
        const c: /*ref*/Color = Color.Red;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_parameter_reference() {
    let mut t = FourslashTest::new(
        "
        function foo(/*def*/x: number) {
            return /*ref*/x + 1;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_destructured_variable() {
    let mut t = FourslashTest::new(
        "
        const { /*def*/name } = { name: 'hello' };
        console.log(/*ref*/name);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_at_declaration_itself() {
    let mut t = FourslashTest::new(
        "
        const /*self*/x = 1;
    ",
    );
    t.go_to_definition("self").expect_found();
}

