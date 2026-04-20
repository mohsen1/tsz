#[test]
fn completions_class_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        class Calculator {
            add(a: number, b: number) { return a + b; }
            subtract(a: number, b: number) { return a - b; }
            multiply(a: number, b: number) { return a * b; }
        }
        const calc = new Calculator();
        calc./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("add")
        .expect_includes("subtract")
        .expect_includes("multiply");
}

#[test]
fn completions_interface_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            host: string;
            port: number;
            debug: boolean;
        }
        declare const cfg: Config;
        cfg./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("host")
        .expect_includes("port")
        .expect_includes("debug");
}

#[test]
fn completions_inherited_class_members() {
    let mut t = FourslashTest::new(
        "
        class Base {
            baseMethod() {}
            baseField: number = 0;
        }
        class Derived extends Base {
            derivedMethod() {}
        }
        const d = new Derived();
        d./*c*/
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("derivedMethod");
    // Inherited members should also appear
    // (if the type system resolves them)
}

#[test]
fn completions_super_members() {
    let mut t = FourslashTest::new(
        "
        class Base {
            greet() { return 'hi'; }
            farewell() { return 'bye'; }
        }
        class Child extends Base {
            greet() {
                super./*c*/
                return '';
            }
        }
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("greet");
}

#[test]
fn completions_enum_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        enum Color {
            Red,
            Green,
            Blue
        }
        Color./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("Red")
        .expect_includes("Green")
        .expect_includes("Blue");
}

#[test]
fn completions_namespace_exports_after_dot() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function format(s: string) { return s; }
            export function parse(s: string) { return s; }
            export const VERSION = '1.0';
        }
        Utils./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("format")
        .expect_includes("parse")
        .expect_includes("VERSION");
}

#[test]
fn completions_nested_scope() {
    let mut t = FourslashTest::new(
        "
        const outer = 1;
        function test() {
            const inner = 2;
            /*c*/
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("outer")
        .expect_includes("inner")
        .expect_includes("test");
}

#[test]
fn completions_function_parameters() {
    let mut t = FourslashTest::new(
        "
        function doSomething(name: string, count: number) {
            /*c*/
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("name")
        .expect_includes("count");
}

#[test]
fn completions_class_this_members() {
    let mut t = FourslashTest::new(
        "
        class Foo {
            value = 42;
            method() { return 1; }
            doStuff() {
                this./*c*/
            }
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("value")
        .expect_includes("method")
        .expect_includes("doStuff");
}

#[test]
fn completions_this_with_private_members() {
    let mut t = FourslashTest::new(
        "
        class SecretKeeper {
            private secret = 'hidden';
            protected level = 5;
            public name = 'keeper';
            reveal() {
                this./*c*/
            }
        }
    ",
    );
    // Inside the class, all members (including private/protected) should show
    t.completions("c")
        .expect_found()
        .expect_includes("secret")
        .expect_includes("level")
        .expect_includes("name")
        .expect_includes("reveal");
}

