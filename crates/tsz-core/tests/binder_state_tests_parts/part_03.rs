#[test]
fn test_namespace_member_resolution_basic() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const x = 1;
    export function foo() { return 2; }
    export class Bar { value: number = 3; }
    export enum Color { Red, Green }
}

const a = NS.x;
const b = NS.foo();
const c = new NS.Bar();
const d = NS.Color.Red;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Namespace should be bound
    assert!(
        binder.file_locals.has("NS"),
        "NS namespace should be in file_locals"
    );

    let ns_sym_id = binder.file_locals.get("NS").expect("NS should exist");
    let ns_symbol = binder
        .get_symbol(ns_sym_id)
        .expect("NS symbol should exist");

    // Namespace should have exports
    assert!(ns_symbol.exports.is_some(), "NS should have exports");
    let exports = ns_symbol.exports.as_ref().unwrap();

    // All exported members should be in exports
    assert!(exports.get("x").is_some(), "x should be in NS exports");
    assert!(exports.get("foo").is_some(), "foo should be in NS exports");
    assert!(exports.get("Bar").is_some(), "Bar should be in NS exports");
    assert!(
        exports.get("Color").is_some(),
        "Color should be in NS exports"
    );
}

#[test]
fn test_namespace_member_resolution_nested() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export const value = 42;
        export function getDouble() { return value * 2; }
    }
}

const a = Outer.Inner.value;
const b = Outer.Inner.getDouble();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Outer namespace should be bound
    assert!(
        binder.file_locals.has("Outer"),
        "Outer namespace should be in file_locals"
    );

    let outer_sym_id = binder.file_locals.get("Outer").expect("Outer should exist");
    let outer_symbol = binder
        .get_symbol(outer_sym_id)
        .expect("Outer symbol should exist");

    // Outer should have Inner in its exports
    assert!(outer_symbol.exports.is_some(), "Outer should have exports");
    let outer_exports = outer_symbol.exports.as_ref().unwrap();
    assert!(
        outer_exports.get("Inner").is_some(),
        "Inner should be in Outer exports"
    );
}

#[test]
fn test_namespace_member_resolution_non_exported() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const exported = 1;
    const notExported = 2;
}

const a = NS.exported;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let ns_sym_id = binder.file_locals.get("NS").expect("NS should exist");
    let ns_symbol = binder
        .get_symbol(ns_sym_id)
        .expect("NS symbol should exist");

    // Only exported members should be in exports
    let exports = ns_symbol.exports.as_ref().expect("NS should have exports");
    assert!(
        exports.get("exported").is_some(),
        "exported should be in NS exports"
    );
    assert!(
        exports.get("notExported").is_none(),
        "notExported should NOT be in NS exports"
    );
}

#[test]
fn test_namespace_deep_chain_resolution() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace A {
    export namespace B {
        export namespace C {
            export const deepValue = "deep";
            export function deepFunc() { return "func"; }
        }
    }
}

const a = A.B.C.deepValue;
const b = A.B.C.deepFunc();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // A namespace should be bound
    assert!(
        binder.file_locals.has("A"),
        "A namespace should be in file_locals"
    );

    let a_sym_id = binder.file_locals.get("A").expect("A should exist");
    let a_symbol = binder.get_symbol(a_sym_id).expect("A symbol should exist");

    // A should have B in exports
    let a_exports = a_symbol.exports.as_ref().expect("A should have exports");
    assert!(a_exports.get("B").is_some(), "B should be in A exports");
}

#[test]
fn test_enum_member_access() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Color {
    Red,
    Green,
    Blue
}

const a = Color.Red;
const b = Color.Green;
const c = Color.Blue;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Color enum should be bound
    assert!(
        binder.file_locals.has("Color"),
        "Color enum should be in file_locals"
    );

    let color_sym_id = binder.file_locals.get("Color").expect("Color should exist");
    let color_symbol = binder
        .get_symbol(color_sym_id)
        .expect("Color symbol should exist");

    // Enum should have members in exports
    assert!(color_symbol.exports.is_some(), "Color should have exports");
    let exports = color_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Red").is_some(),
        "Red should be in Color exports"
    );
    assert!(
        exports.get("Green").is_some(),
        "Green should be in Color exports"
    );
    assert!(
        exports.get("Blue").is_some(),
        "Blue should be in Color exports"
    );
}

#[test]
fn test_enum_namespace_merging_access() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Direction {
    Up = 1,
    Down = 2
}
namespace Direction {
    export function getName(d: Direction): string {
        return d === Direction.Up ? "Up" : "Down";
    }
    export const helperValue = 99;
}

const a = Direction.Up;
const b = Direction.getName(Direction.Down);
const c = Direction.helperValue;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Direction should be bound (merged enum and namespace)
    assert!(
        binder.file_locals.has("Direction"),
        "Direction should be in file_locals"
    );

    let dir_sym_id = binder
        .file_locals
        .get("Direction")
        .expect("Direction should exist");
    let dir_symbol = binder
        .get_symbol(dir_sym_id)
        .expect("Direction symbol should exist");

    // Direction should have both enum members and namespace exports
    assert!(
        dir_symbol.exports.is_some(),
        "Direction should have exports"
    );
    let exports = dir_symbol.exports.as_ref().unwrap();

    // Enum members
    assert!(
        exports.get("Up").is_some(),
        "Up should be in Direction exports"
    );
    assert!(
        exports.get("Down").is_some(),
        "Down should be in Direction exports"
    );

    // Namespace exports
    assert!(
        exports.get("getName").is_some(),
        "getName should be in Direction exports"
    );
    assert!(
        exports.get("helperValue").is_some(),
        "helperValue should be in Direction exports"
    );
}

#[test]
fn test_enum_with_initialized_members() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Status {
    Pending = 0,
    Active = 1,
    Done = 2
}

const a = Status.Pending;
const b = Status.Active;
const c = Status.Done;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Status enum should be bound
    assert!(
        binder.file_locals.has("Status"),
        "Status enum should be in file_locals"
    );

    let status_sym_id = binder
        .file_locals
        .get("Status")
        .expect("Status should exist");
    let status_symbol = binder
        .get_symbol(status_sym_id)
        .expect("Status symbol should exist");

    // Enum should have all members in exports
    assert!(
        status_symbol.exports.is_some(),
        "Status should have exports"
    );
    let exports = status_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Pending").is_some(),
        "Pending should be in Status exports"
    );
    assert!(
        exports.get("Active").is_some(),
        "Active should be in Status exports"
    );
    assert!(
        exports.get("Done").is_some(),
        "Done should be in Status exports"
    );
}

#[test]
fn test_const_enum_declaration() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
const enum Priority {
    Low = 1,
    Medium = 2,
    High = 3
}

const a = Priority.Low;
const b = Priority.Medium;
const c = Priority.High;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Priority const enum should be bound
    assert!(
        binder.file_locals.has("Priority"),
        "Priority const enum should be in file_locals"
    );

    let priority_sym_id = binder
        .file_locals
        .get("Priority")
        .expect("Priority should exist");
    let priority_symbol = binder
        .get_symbol(priority_sym_id)
        .expect("Priority symbol should exist");

    // Should have CONST_ENUM flag
    assert_eq!(
        priority_symbol.flags & symbol_flags::CONST_ENUM,
        symbol_flags::CONST_ENUM,
        "Priority should have CONST_ENUM flag"
    );

    // Should have exports
    assert!(
        priority_symbol.exports.is_some(),
        "Priority should have exports"
    );
    let exports = priority_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Low").is_some(),
        "Low should be in Priority exports"
    );
    assert!(
        exports.get("Medium").is_some(),
        "Medium should be in Priority exports"
    );
    assert!(
        exports.get("High").is_some(),
        "High should be in Priority exports"
    );
}

#[test]
fn test_namespace_reopening_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Reopened {
    export const first = 1;
}
namespace Reopened {
    export const second = 2;
    export function combined() { return first + second; }
}

const a = Reopened.first;
const b = Reopened.second;
const c = Reopened.combined();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let reopened_sym_id = binder
        .file_locals
        .get("Reopened")
        .expect("Reopened should exist");
    let reopened_symbol = binder
        .get_symbol(reopened_sym_id)
        .expect("Reopened symbol should exist");

    // Should have all exports from both declarations
    let exports = reopened_symbol
        .exports
        .as_ref()
        .expect("Reopened should have exports");

    assert!(
        exports.get("first").is_some(),
        "first should be in Reopened exports"
    );
    assert!(
        exports.get("second").is_some(),
        "second should be in Reopened exports"
    );
    assert!(
        exports.get("combined").is_some(),
        "combined should be in Reopened exports"
    );
    assert_eq!(exports.len(), 3, "Reopened should have exactly 3 exports");
}

#[test]
fn test_enum_namespace_merging_with_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum ErrorCode {
    NotFound = 404,
    ServerError = 500
}
namespace ErrorCode {
    export function getMessage(code: ErrorCode): string {
        if (code === ErrorCode.NotFound) return "Not Found";
        if (code === ErrorCode.ServerError) return "Server Error";
        return "Unknown";
    }
}

const err1 = ErrorCode.NotFound;
const msg1 = ErrorCode.getMessage(ErrorCode.NotFound);
const err2 = ErrorCode.ServerError;
const msg2 = ErrorCode.getMessage(ErrorCode.ServerError);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let error_code_sym_id = binder
        .file_locals
        .get("ErrorCode")
        .expect("ErrorCode should exist");
    let error_code_symbol = binder
        .get_symbol(error_code_sym_id)
        .expect("ErrorCode symbol should exist");

    // Should have both enum members and namespace function
    let exports = error_code_symbol
        .exports
        .as_ref()
        .expect("ErrorCode should have exports");

    assert!(
        exports.get("NotFound").is_some(),
        "NotFound should be in ErrorCode exports"
    );
    assert!(
        exports.get("ServerError").is_some(),
        "ServerError should be in ErrorCode exports"
    );
    assert!(
        exports.get("getMessage").is_some(),
        "getMessage should be in ErrorCode exports"
    );
}

// =============================================================================
// Scope Chain Traversal Tests
// =============================================================================

