#[test]
fn complex_generic_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/DeepPartial<T> = T extends object
            ? { [K in keyof T]?: DeepPartial<T[K]> }
            : T;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("DeepPartial");
}

#[test]
fn multiple_generics_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/merge<A extends object, B extends object>(a: A, b: B): A & B {
            return { ...a, ...b };
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("merge");
}

#[test]
fn discriminated_union_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/Shape =
            | { kind: 'circle'; radius: number }
            | { kind: 'square'; size: number }
            | { kind: 'rectangle'; width: number; height: number };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("Shape");
}

#[test]
fn nested_class_definition() {
    let mut t = FourslashTest::new(
        "
        class Outer {
            inner = class /*def*/Inner {
                value = 42;
            };
        }
    ",
    );
    t.hover("def")
        .expect_found()
        .expect_display_string_contains("Inner");
}

// =============================================================================
// Go-to-Definition: Property Access & Member Resolution (NEW)
// =============================================================================

#[test]
fn definition_interface_property_access() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            /*def*/host: string;
            port: number;
        }
        const cfg: Config = { host: 'localhost', port: 80 };
        cfg./*ref*/host;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_method_access() {
    let mut t = FourslashTest::new(
        "
        interface Greeter {
            /*def*/greet(name: string): string;
        }
        const g: Greeter = { greet: (n) => `Hello ${n}` };
        g./*ref*/greet('World');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_method_via_type_annotation() {
    let mut t = FourslashTest::new(
        "
        class Animal {
            /*def*/speak() { return 'sound'; }
        }
        const a: Animal = new Animal();
        a./*ref*/speak();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_property_via_new_expression() {
    let mut t = FourslashTest::new(
        "
        class Point {
            /*def*/x: number;
            y: number;
            constructor(x: number, y: number) {
                this.x = x;
                this.y = y;
            }
        }
        const p = new Point(1, 2);
        p./*ref*/x;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_parameter_type_property_access() {
    let mut t = FourslashTest::new(
        "
        interface User {
            /*def*/name: string;
            age: number;
        }
        function greetUser(user: User) {
            return user./*ref*/name;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_member_access() {
    let mut t = FourslashTest::new(
        "
        enum Direction {
            /*def*/Up,
            Down,
            Left,
            Right
        }
        const d = Direction./*ref*/Up;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

