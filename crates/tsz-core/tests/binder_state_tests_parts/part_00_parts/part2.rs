#[test]
fn test_enum_with_initialized_members() {
    use crate::binder::BinderState;

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
