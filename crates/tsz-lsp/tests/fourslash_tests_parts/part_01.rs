#[test]
fn definition_no_result_at_keyword() {
    let mut t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    t.go_to_definition("kw").expect_none();
}

#[test]
fn definition_nested_function() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            function /*def*/inner() { return 1; }
            return /*ref*/inner();
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_arrow_function_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/add = (a: number, b: number) => a + b;
        /*ref*/add(1, 2);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_generic_type() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Container<T> { value: T; }
        const c: /*ref*/Container<number> = { value: 42 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_for_loop_variable() {
    let mut t = FourslashTest::new(
        "
        for (let /*def*/i = 0; /*ref*/i < 10; i++) {}
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_try_catch_variable() {
    let mut t = FourslashTest::new(
        "
        try {
        } catch (/*def*/err) {
            console.log(/*ref*/err);
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_export_default_function() {
    let mut t = FourslashTest::new(
        "
        export default function /*def*/main() {}
        /*ref*/main();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_overloaded_function() {
    let mut t = FourslashTest::new(
        "
        function /*def*/overloaded(x: number): number;
        function overloaded(x: string): string;
        function overloaded(x: any): any { return x; }
        /*ref*/overloaded(1);
    ",
    );
    t.go_to_definition("ref").expect_found();
}

#[test]
fn definition_shorthand_property() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'world';
        const obj = { /*ref*/name };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_deeply_nested() {
    let mut t = FourslashTest::new(
        "
        function a() {
            function b() {
                function c() {
                    function d() {
                        const /*x*/x = 1;
                        return /*ref*/x;
                    }
                }
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("x");
}

