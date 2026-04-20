#[test]
fn test_namespace_binding_debug() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace foo {
    export class Provide {}
    class NotExported {}
    export function bar() {}
    function baz() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Check if 'foo' was bound
    let foo_sym_id = binder
        .file_locals
        .get("foo")
        .expect("'foo' should be in file_locals");
    let foo_symbol = binder
        .get_symbol(foo_sym_id)
        .expect("foo symbol should exist");

    // Check if exports were captured
    assert!(foo_symbol.exports.is_some(), "foo should have exports");

    // Check that ONLY exported members are in exports
    let exports = foo_symbol.exports.as_ref().unwrap();

    // Exported members should be present
    assert!(
        exports.get("Provide").is_some(),
        "Provide should be in foo's exports"
    );
    assert!(
        exports.get("bar").is_some(),
        "bar should be in foo's exports"
    );

    // Non-exported members should NOT be in exports
    assert!(
        exports.get("NotExported").is_none(),
        "NotExported should NOT be in foo's exports"
    );
    assert!(
        exports.get("baz").is_none(),
        "baz should NOT be in foo's exports"
    );

    // Should have exactly 2 exports
    assert_eq!(exports.len(), 2, "foo should have exactly 2 exports");
}

#[test]
fn test_import_alias_binding() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export class C {}
}
import Alias = NS.C;
var x: Alias;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Check that 'Alias' was bound as an ALIAS symbol
    let alias_sym_id = binder
        .file_locals
        .get("Alias")
        .expect("'Alias' should be in file_locals");
    let alias_symbol = binder
        .get_symbol(alias_sym_id)
        .expect("Alias symbol should exist");

    // Verify it has the ALIAS flag
    assert_eq!(
        alias_symbol.flags & symbol_flags::ALIAS,
        symbol_flags::ALIAS,
        "Alias should have ALIAS flag"
    );

    // Verify it has a declaration
    assert!(
        !alias_symbol.declarations.is_empty(),
        "Alias should have declarations"
    );
}

// =============================================================================
// Symbol Merging Tests (Namespace/Class/Function/Enum)
// =============================================================================

#[test]
fn test_namespace_exports_merge_across_decls() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const a = 1;
    const hidden = 2;
}
namespace Merge {
    export function foo() {}
    export const b = 3;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(exports.get("a").is_some(), "a should be in Merge exports");
    assert!(exports.get("b").is_some(), "b should be in Merge exports");
    assert!(
        exports.get("foo").is_some(),
        "foo should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 3, "Merge should have exactly 3 exports");
}

#[test]
fn test_class_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
class Merge {
    constructor() {}
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_namespace_class_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
class Merge {
    constructor() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_function_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
function Merge() {}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_namespace_function_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
function Merge() {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_enum_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    // Enum members should be in exports when merging with namespace (TypeScript behavior)
    assert!(exports.get("A").is_some(), "A should be in Merge exports");
    assert_eq!(
        exports.len(),
        2,
        "Merge should have exactly 2 exports (enum member A + namespace export extra)"
    );
}

#[test]
fn test_namespace_enum_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
enum Merge {
    A,
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    // Enum members should be in exports when merging with namespace (TypeScript behavior)
    assert!(exports.get("A").is_some(), "A should be in Merge exports");
    assert_eq!(
        exports.len(),
        2,
        "Merge should have exactly 2 exports (namespace export extra + enum member A)"
    );
}

// =============================================================================
// Performance and Edge Case Tests
// =============================================================================

/// Tests that deeply nested binary expressions don't cause stack overflow.
/// Uses 50,000 chained additions to stress test the binding walk.
#[test]
fn test_binder_deep_binary_expression() {
    const COUNT: usize = 50000;
    let mut source = String::with_capacity(COUNT * 4);
    for i in 0..COUNT {
        if i > 0 {
            source.push_str(" + ");
        }
        source.push('0');
    }
    source.push(';');

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.is_empty());
}

// =============================================================================
// Namespace Member Resolution Tests
// =============================================================================

