#[test]
fn generic_function_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/identity<T>(arg: T): T { return arg; }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("identity");
}

#[test]
fn intersection_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/NamedPoint = { name: string } & { x: number; y: number };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("NamedPoint");
}

#[test]
fn conditional_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/IsString<T> = T extends string ? true : false;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("IsString");
}

#[test]
fn mapped_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/Readonly<T> = { readonly [K in keyof T]: T[K] };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("Readonly");
}

#[test]
fn decorator_class_symbols() {
    let mut t = FourslashTest::new(
        "
        function Component(target: any) {}
        @Component
        class MyComponent {
            render() {}
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("MyComponent");
}

#[test]
fn async_function_hover() {
    let mut t = FourslashTest::new(
        "
        async function /*fn*/fetchData(): Promise<string> {
            return 'data';
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("fetchData");
}

#[test]
fn generator_function_hover() {
    let mut t = FourslashTest::new(
        "
        function* /*fn*/counter() {
            yield 1;
            yield 2;
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("counter");
}

#[test]
fn rest_parameter_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/sum(.../*args*/numbers: number[]) {
            return numbers.reduce((a, b) => a + b, 0);
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("sum");
    t.hover("args")
        .expect_found()
        .expect_display_string_contains("numbers");
}

#[test]
fn optional_parameter_definition() {
    let mut t = FourslashTest::new(
        "
        function greet(/*def*/name?: string) {
            return `Hello, ${/*ref*/name || 'world'}`;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn default_parameter_definition() {
    let mut t = FourslashTest::new(
        "
        function greet(/*def*/greeting: string = 'Hello') {
            return /*ref*/greeting + '!';
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

