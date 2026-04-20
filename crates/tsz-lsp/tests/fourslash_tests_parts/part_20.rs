#[test]
fn hover_namespace_function() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function format(s: string) { return s.trim(); }
        }
        Utils./*h*/format(' hello ');
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_class_static_property() {
    let mut t = FourslashTest::new(
        "
        class Config {
            static readonly MAX_SIZE = 100;
        }
        Config./*h*/MAX_SIZE;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_enum_member_without_initializer() {
    let mut t = FourslashTest::new(
        "
        enum Direction { Up, Down, Left, Right }
        Direction./*h*/Left;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn definition_inherited_method_deep_chain() {
    let mut t = FourslashTest::new(
        "
        class A {
            /*def*/greet() { return 'hi'; }
        }
        class B extends A {}
        class C extends B {}
        const c = new C();
        c./*ref*/greet();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_inherited_member() {
    let mut t = FourslashTest::new(
        "
        interface Readable {
            /*def*/read(): string;
        }
        interface BufferedReadable extends Readable {
            buffer(): void;
        }
        const r: BufferedReadable = { read() { return ''; }, buffer() {} };
        r./*ref*/read();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_deep_inheritance() {
    let mut t = FourslashTest::new(
        "
        interface A {
            /*def*/method(): void;
        }
        interface B extends A {
            other(): void;
        }
        interface C extends B {
            third(): void;
        }
        const c: C = { method() {}, other() {}, third() {} };
        c./*ref*/method();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_inherited_via_type_annotation() {
    let mut t = FourslashTest::new(
        "
        class Animal {
            /*def*/speak() { return 'sound'; }
        }
        class Dog extends Animal {
            fetch() { return 'ball'; }
        }
        const pet: Dog = new Dog();
        pet./*ref*/speak();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn hover_this_keyword() {
    let mut t = FourslashTest::new(
        "
        class MyClass {
            value: number = 42;
            getValue() {
                return /*h*/this.value;
            }
        }
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_super_keyword() {
    let mut t = FourslashTest::new(
        "
        class Base {
            greet() { return 'hi'; }
        }
        class Child extends Base {
            greet() {
                return /*h*/super.greet();
            }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("Base");
}

#[test]
fn definition_super_method_call() {
    let mut t = FourslashTest::new(
        "
        class Base {
            /*def*/greet() { return 'hi'; }
        }
        class Child extends Base {
            greet() {
                return super./*ref*/greet();
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

