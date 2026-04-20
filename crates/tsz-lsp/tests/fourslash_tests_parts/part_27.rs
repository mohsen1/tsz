#[test]
fn signature_help_rest_parameter() {
    let mut t = FourslashTest::new(
        "
        function sum(...nums: number[]) { return 0; }
        sum(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

#[test]
fn signature_help_optional_params() {
    let mut t = FourslashTest::new(
        "
        function greet(name: string, greeting?: string) {}
        greet(/*c*/);
    ",
    );
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(2);
}

#[test]
fn signature_help_method_call() {
    let mut t = FourslashTest::new(
        "
        class Calc {
            add(a: number, b: number): number { return a + b; }
        }
        const c = new Calc();
        c.add(/*h*/);
    ",
    );
    t.signature_help("h").expect_found();
}

#[test]
fn signature_help_nested_call() {
    let mut t = FourslashTest::new(
        "
        function outer(x: number) { return x; }
        function inner(a: string, b: string) { return a; }
        outer(inner(/*c*/));
    ",
    );
    // Should show inner's signature, not outer's
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(2);
}

#[test]
fn signature_help_arrow_function() {
    let mut t = FourslashTest::new(
        "
        const transform = (input: string, repeat: number): string => input;
        transform(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

#[test]
fn signature_help_constructor() {
    let mut t = FourslashTest::new(
        "
        class Point {
            constructor(x: number, y: number) {}
        }
        new Point(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

// =============================================================================
// Document Symbols: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn symbols_nested_functions() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            function inner() {
                function deepest() {}
            }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("outer");
}

#[test]
fn symbols_class_with_static_members() {
    let mut t = FourslashTest::new(
        "
        class Config {
            static defaultHost = 'localhost';
            static defaultPort = 80;
            host: string;
            port: number;
            constructor(host: string, port: number) {
                this.host = host;
                this.port = port;
            }
            static create() { return new Config('', 0); }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Config");
}

#[test]
fn symbols_abstract_class() {
    let mut t = FourslashTest::new(
        "
        abstract class Shape {
            abstract area(): number;
            abstract perimeter(): number;
            toString() { return `Area: ${this.area()}`; }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Shape");
}

#[test]
fn symbols_type_declarations() {
    let mut t = FourslashTest::new(
        "
        type Point = { x: number; y: number };
        type StringOrNumber = string | number;
        type Callback<T> = (value: T) => void;
    ",
    );
    let result = t.document_symbols("test.ts");
    result.expect_found().expect_count(3);
}

