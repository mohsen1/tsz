#[test]
fn definition_abstract_class() {
    let mut t = FourslashTest::new(
        "
        abstract class /*def*/Shape {
            abstract area(): number;
        }
        class Circle extends /*ref*/Shape {
            area() { return 3.14; }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_in_union() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/A { x: number; }
        interface B { y: string; }
        type C = /*ref*/A | B;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_in_intersection() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/X { a: number; }
        interface Y { b: string; }
        type Z = /*ref*/X & Y;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_generic_constraint() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/HasLength { length: number; }
        function longest<T extends /*ref*/HasLength>(a: T, b: T): T {
            return a.length >= b.length ? a : b;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_return_type_reference() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Result { success: boolean; }
        function getResult(): /*ref*/Result {
            return { success: true };
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_array_element_type() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Item { id: number; }
        const items: /*ref*/Item[] = [];
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_promise_type_argument() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Data { value: string; }
        async function fetch(): Promise</*ref*/Data> {
            return { value: '' };
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Hover: Property Access & Members (NEW)
// =============================================================================

#[test]
fn hover_interface_property() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            host: string;
            port: number;
        }
        const cfg: Config = { host: 'localhost', port: 80 };
        cfg./*h*/host;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_class_method_call() {
    let mut t = FourslashTest::new(
        "
        class Calc {
            add(a: number, b: number) { return a + b; }
        }
        const c = new Calc();
        c./*h*/add(1, 2);
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_enum_member_dot_access() {
    let mut t = FourslashTest::new(
        "
        enum Status {
            Active = 1,
            Inactive = 0
        }
        Status./*h*/Active;
    ",
    );
    t.hover("h").expect_found();
}

