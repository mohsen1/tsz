#[test]
fn definition_enum_string_member_access() {
    let mut t = FourslashTest::new(
        "
        enum Color {
            /*def*/Red = 'red',
            Green = 'green',
            Blue = 'blue'
        }
        const c = Color./*ref*/Red;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_namespace_member_access() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function /*def*/format(s: string) { return s; }
        }
        Utils./*ref*/format('hello');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_static_member() {
    let mut t = FourslashTest::new(
        "
        class MathUtils {
            static /*def*/PI = 3.14;
        }
        const pi = MathUtils./*ref*/PI;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_static_method() {
    let mut t = FourslashTest::new(
        "
        class Factory {
            static /*def*/create() { return new Factory(); }
        }
        Factory./*ref*/create();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_this_property_access() {
    let mut t = FourslashTest::new(
        "
        class Foo {
            /*def*/value = 42;
            getIt() {
                return this./*ref*/value;
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_optional_property() {
    let mut t = FourslashTest::new(
        "
        interface Options {
            /*def*/verbose?: boolean;
            output?: string;
        }
        function run(opts: Options) {
            if (opts./*ref*/verbose) {
                console.log('verbose mode');
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_inherited_class_member() {
    let mut t = FourslashTest::new(
        "
        class Base {
            /*def*/baseMethod() { return 1; }
        }
        class Derived extends Base {
            derivedMethod() { return 2; }
        }
        const d = new Derived();
        d./*ref*/baseMethod();
    ",
    );
    // Should resolve to Base.baseMethod
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Go-to-Definition: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn definition_function_expression_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/add = function(a: number, b: number) {
            return a + b;
        };
        /*ref*/add(1, 2);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_default_export_class() {
    let mut t = FourslashTest::new(
        "
        export default class /*def*/MyClass {}
        const x = new /*ref*/MyClass();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_const_enum() {
    let mut t = FourslashTest::new(
        "
        const enum /*def*/Status { Active, Inactive }
        const s: /*ref*/Status = Status.Active;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

