#[test]
fn inlay_hints_skip_obvious_args() {
    let t = FourslashTest::new(
        "
        function setConfig(options: { a: number }, callback: () => void) {}
        setConfig({ a: 1 }, () => {});
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Object literal and arrow function args should NOT have parameter hints
    let has_options = result.hints.iter().any(|h| h.label.contains("options"));
    let has_callback = result.hints.iter().any(|h| h.label.contains("callback"));
    assert!(
        !has_options && !has_callback,
        "Should skip hints for object literal and callback args, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

#[test]
fn type_hierarchy_deep_inheritance() {
    let t = FourslashTest::new(
        "
        class /*base*/Animal {}
        class /*mammal*/Mammal extends Animal {}
        class /*dog*/Dog extends Mammal {}
    ",
    );
    // Dog's supertypes should include Mammal
    let result = t.supertypes("dog");
    result.expect_found();
}

#[test]
fn type_hierarchy_interface_extends() {
    let t = FourslashTest::new(
        "
        interface /*base*/Printable {
            print(): void;
        }
        interface /*child*/FormattedPrintable extends Printable {
            format(): string;
        }
    ",
    );
    let result = t.supertypes("child");
    result.expect_found();
    assert!(
        result.items.iter().any(|item| item.name == "Printable"),
        "Expected Printable as a supertype"
    );
}

#[test]
fn type_hierarchy_class_implements_interface() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class /*cls*/JsonItem implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.supertypes("cls");
    result.expect_found();
    assert!(
        result.items.iter().any(|item| item.name == "Serializable"),
        "Expected Serializable as a supertype"
    );
}

#[test]
fn type_hierarchy_subtypes_finds_implementations() {
    let t = FourslashTest::new(
        "
        interface /*iface*/Logger {
            log(msg: string): void;
        }
        class ConsoleLogger implements Logger {
            log(msg: string) { console.log(msg); }
        }
        class FileLogger implements Logger {
            log(msg: string) {}
        }
    ",
    );
    let result = t.subtypes("iface");
    result.expect_found();
    assert!(
        result.items.len() >= 2,
        "Expected at least 2 implementations"
    );
}

#[test]
fn type_hierarchy_multiple_interfaces() {
    let t = FourslashTest::new(
        "
        interface Readable { read(): string; }
        interface Writable { write(s: string): void; }
        class /*cls*/Stream implements Readable, Writable {
            read() { return ''; }
            write(s: string) {}
        }
    ",
    );
    let result = t.supertypes("cls");
    result.expect_found();
    assert!(result.items.len() >= 2, "Expected at least 2 supertypes");
}

// =============================================================================
// Call Hierarchy: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn call_hierarchy_prepare_constructor() {
    let t = FourslashTest::new(
        "
        class Foo {
            /*c*/constructor() {}
        }
    ",
    );
    let result = t.prepare_call_hierarchy("c");
    result.expect_found();
}

#[test]
fn call_hierarchy_prepare_arrow_function() {
    let t = FourslashTest::new(
        "
        const /*f*/greet = (name: string) => `Hello ${name}`;
    ",
    );
    let result = t.prepare_call_hierarchy("f");
    result.expect_found();
}

#[test]
fn call_hierarchy_outgoing_from_function() {
    let t = FourslashTest::new(
        "
        function helper() { return 1; }
        function /*f*/main() {
            helper();
            helper();
        }
    ",
    );
    let result = t.outgoing_calls("f");
    result.expect_found();
}

#[test]
fn call_hierarchy_incoming_to_function() {
    let t = FourslashTest::new(
        "
        function /*f*/target() { return 42; }
        function caller1() { target(); }
        function caller2() { target(); }
    ",
    );
    let result = t.incoming_calls("f");
    result.expect_found();
    assert!(result.calls.len() >= 2, "Expected at least 2 callers");
}

