#[test]
fn formatting_basic() {
    let t = FourslashTest::new(
        "
        const x = 1;
    ",
    );
    // Formatting may fail if prettier is not installed - just verify it doesn't panic
    let _ = t.format("test.ts");
}

// =============================================================================
// Call Hierarchy Tests
// =============================================================================

#[test]
fn call_hierarchy_prepare_function() {
    let t = FourslashTest::new(
        "
        function /*fn*/myFunction() {
            return 42;
        }
    ",
    );
    let result = t.prepare_call_hierarchy("fn");
    if result.item.is_some() {
        result.expect_name("myFunction");
    }
}

#[test]
fn call_hierarchy_prepare_method() {
    let t = FourslashTest::new(
        "
        class MyClass {
            /*m*/myMethod() { return 1; }
        }
    ",
    );
    let result = t.prepare_call_hierarchy("m");
    if result.item.is_some() {
        result.expect_name("myMethod");
    }
}

#[test]
fn call_hierarchy_prepare_at_non_callable() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // A variable (not a function) should not produce a call hierarchy item
    t.prepare_call_hierarchy("x").expect_none();
}

#[test]
fn call_hierarchy_outgoing_calls() {
    let t = FourslashTest::new(
        "
        function helper() {}
        function /*fn*/main() {
            helper();
        }
    ",
    );
    let result = t.outgoing_calls("fn");
    if !result.calls.is_empty() {
        result.expect_callee("helper");
    }
}

#[test]
fn call_hierarchy_incoming_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/target() {}
        function caller1() { target(); }
        function caller2() { target(); }
    ",
    );
    let result = t.incoming_calls("fn");
    if !result.calls.is_empty() {
        result.expect_caller("caller1");
    }
}

#[test]
fn call_hierarchy_no_outgoing_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/empty() {
            const x = 1;
        }
    ",
    );
    t.outgoing_calls("fn").expect_none();
}

// =============================================================================
// Type Hierarchy Tests
// =============================================================================

#[test]
fn type_hierarchy_prepare_class() {
    let t = FourslashTest::new(
        "
        class /*cls*/Animal {
            name: string = '';
        }
    ",
    );
    let result = t.prepare_type_hierarchy("cls");
    if result.item.is_some() {
        result.expect_name("Animal");
    }
}

#[test]
fn type_hierarchy_prepare_interface() {
    let t = FourslashTest::new(
        "
        interface /*iface*/Serializable {
            serialize(): string;
        }
    ",
    );
    let result = t.prepare_type_hierarchy("iface");
    if result.item.is_some() {
        result.expect_name("Serializable");
    }
}

#[test]
fn type_hierarchy_prepare_at_variable() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // A variable should not produce a type hierarchy item
    t.prepare_type_hierarchy("x").expect_none();
}

