use super::*;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_declaration_checker_variable() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Get the variable statement
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);
        // Test passes if no panic
    }
}

#[test]
fn test_module_augmentation_duplicate_value_exports() {
    let source = r#"
export {};

declare module "./a" {
export const x = 0;
}

declare module "../dir/a" {
export const x = 0;
}
"#;
    let file_name = "/dir/b.ts".to_string();
    let mut parser = ParserState::new(file_name.clone(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name,
        crate::context::CheckerOptions::default(),
    );
    ctx.set_current_file_idx(0);

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        for &stmt_idx in &sf_data.statements.nodes {
            if let Some(stmt_node) = parser.get_arena().get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
            {
                checker.check_module_declaration(stmt_idx);
            }
        }
    }

    let ts2451_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2451).collect();
    assert_eq!(
        ts2451_errors.len(),
        1,
        "Expected 1 TS2451 error, got {}",
        ts2451_errors.len()
    );
}

#[test]
fn test_ts2564_property_without_initializer() {
    // Test that TS2564 is reported for properties without initializers
    let source = r#"
class Foo {
x: number;  // Should report TS2564
y: string = "hello";  // Should NOT report (has initializer)
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    // Get the class declaration
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have one TS2564 error for property 'x'
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            1,
            "Expected 1 TS2564 error, got {}",
            ts2564_errors.len()
        );

        // Verify the error message contains 'x'
        if let Some(err) = ts2564_errors.first() {
            assert!(
                err.message_text.contains("x"),
                "Error message should contain 'x', got: {}",
                err.message_text
            );
        }
    }
}

#[test]
fn test_ts2564_with_definite_assignment_assertion() {
    // Test that TS2564 is NOT reported for properties with definite assignment assertion (!)
    let source = r#"
class Foo {
x!: number;  // Should NOT report (has definite assignment assertion)
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    // Get the class declaration
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have NO TS2564 errors
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_skips_static_properties() {
    // Test that TS2564 is NOT reported for static properties
    let source = r#"
class Foo {
static x: number;  // Should NOT report (static property)
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    // Get the class declaration
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have NO TS2564 errors
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_disabled_when_strict_false() {
    // Test that TS2564 is NOT reported when strict mode is disabled
    let source = r#"
class Foo {
x: number;  // Should NOT report (strict mode disabled)
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Get the class declaration
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have NO TS2564 errors (strict mode disabled)
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors when strict mode disabled, got {}",
            ts2564_errors.len()
        );
    }
}

// ========== Phase 2 Tests: Control Flow Analysis ==========

#[test]
fn test_ts2564_phase2_simple_constructor_initialization() {
    // Test that TS2564 is NOT reported for properties initialized in simple constructor
    let source = r#"
class Foo {
x: number;  // Should NOT report (initialized in constructor)
constructor() {
    this.x = 1;
}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have NO TS2564 errors (property initialized in constructor)
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors for constructor-initialized property, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_conditional_all_paths_assigned() {
    // Test that TS2564 is NOT reported when property is initialized on all code paths
    let source = r#"
class Foo {
x: number;  // Should NOT report (initialized on all paths)
constructor(flag: boolean) {
    if (flag) {
        this.x = 1;
    } else {
        this.x = 2;
    }
}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have NO TS2564 errors (property initialized on all paths)
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors for property initialized on all paths, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_conditional_not_all_paths_assigned() {
    // Test that TS2564 IS reported when property is not initialized on all code paths
    let source = r#"
class Foo {
x: number;  // Should report TS2564 (not initialized on all paths)
constructor(flag: boolean) {
    if (flag) {
        this.x = 1;
    }
    // else branch doesn't assign this.x
}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have 1 TS2564 error (property not initialized on all paths)
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            1,
            "Expected 1 TS2564 error for property not initialized on all paths, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_return_statement_exits() {
    // Test that TS2564 IS reported when property is not initialized before early return
    let source = r#"
class Foo {
x: number;  // Should report TS2564 (not initialized before early return)
constructor(flag: boolean) {
    if (flag) {
        return;
    }
    this.x = 1;
}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have 1 TS2564 error (property not initialized on all exit paths)
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            1,
            "Expected 1 TS2564 error for property not initialized before early return, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_multiple_properties() {
    // Test mixed scenario: some properties initialized, some not
    let source = r#"
class Foo {
x: number;  // Should NOT report (initialized in constructor)
y: string;  // Should report TS2564 (not initialized)
z: boolean = true;  // Should NOT report (has initializer)
constructor() {
    this.x = 1;
}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );

    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
    {
        let mut checker = DeclarationChecker::new(&mut ctx);
        checker.check(stmt_idx);

        // Should have 1 TS2564 error for 'y'
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            1,
            "Expected 1 TS2564 error for property 'y', got {}",
            ts2564_errors.len()
        );

        if let Some(err) = ts2564_errors.first() {
            assert!(
                err.message_text.contains("y"),
                "Error message should contain 'y', got: {}",
                err.message_text
            );
        }
    }
}
