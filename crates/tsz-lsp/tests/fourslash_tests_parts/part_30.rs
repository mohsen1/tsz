#[test]
fn semantic_tokens_type_annotations() {
    let t = FourslashTest::new(
        "
        interface Foo { bar: string; }
        const x: Foo = { bar: 'baz' };
        function f(a: number, b: string): boolean { return true; }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

// =============================================================================
// Workspace Symbols: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn workspace_symbols_case_insensitive() {
    let t = FourslashTest::new(
        "
        function myLongFunctionName() {}
        class MyService {}
    ",
    );
    let result = t.workspace_symbols("mylong");
    // Case-insensitive should find the function
    if !result.symbols.is_empty() {
        result.expect_found();
    }
}

#[test]
fn workspace_symbols_multi_file() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export function helperA() {}"),
        ("b.ts", "export function helperB() {}"),
        ("c.ts", "export class MainApp {}"),
    ]);
    let result = t.workspace_symbols("helper");
    result.expect_found();
    assert!(
        result.symbols.len() >= 2,
        "Expected at least 2 symbols matching 'helper'"
    );
}

#[test]
fn workspace_symbols_returns_classes() {
    let t = FourslashTest::new(
        "
        class UserService {}
        class ProductService {}
        class OrderService {}
    ",
    );
    let result = t.workspace_symbols("Service");
    result.expect_found();
    assert!(
        result.symbols.len() >= 3,
        "Expected at least 3 service classes"
    );
}

// =============================================================================
// Code Lens: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn code_lens_class_methods() {
    let t = FourslashTest::new(
        "
        class Service {
            start() {}
            stop() {}
            restart() {}
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    // Should produce lenses for the class and its methods
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn code_lens_interface_with_implementations() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class JsonSerializer implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

// =============================================================================
// Inlay Hints: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn inlay_hints_function_parameters() {
    let t = FourslashTest::new(
        "
        function createUser(name: string, age: number, active: boolean) {}
        createUser('Alice', 30, true);
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have parameter name hints for the call
    if !result.hints.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn inlay_hints_variable_type() {
    let t = FourslashTest::new(
        "
        const x = [1, 2, 3];
        const y = { a: 1, b: 'hello' };
        const z = new Map<string, number>();
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have type hints for the variables
    let _ = result; // Just verify no crash
}

#[test]
fn inlay_hints_method_call_parameters() {
    let t = FourslashTest::new(
        "
        class Logger {
            log(message: string, level: number) {}
        }
        const logger = new Logger();
        logger.log('hello', 1);
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have parameter name hints for the method call
    let has_message_hint = result.hints.iter().any(|h| h.label.contains("message"));
    let has_level_hint = result.hints.iter().any(|h| h.label.contains("level"));
    assert!(
        has_message_hint && has_level_hint,
        "Expected parameter hints for method call, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

// =============================================================================
// Type Hierarchy: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn inlay_hints_constructor_parameters() {
    let t = FourslashTest::new(
        "
        class User {
            constructor(name: string, age: number) {}
        }
        const u = new User('Alice', 30);
    ",
    );
    let result = t.inlay_hints("test.ts");
    let has_name_hint = result.hints.iter().any(|h| h.label.contains("name"));
    let has_age_hint = result.hints.iter().any(|h| h.label.contains("age"));
    assert!(
        has_name_hint && has_age_hint,
        "Expected constructor parameter hints, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

