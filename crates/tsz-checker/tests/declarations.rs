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

        // DeclarationChecker no longer owns TS2564; canonical CheckerState path emits it.
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors from DeclarationChecker delegation path, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_with_definite_assignment_assertion() {
    // Test that TS2564 is NOT reported for properties with definite assignment assertion (!)
    let source = r"
class Foo {
x!: number;  // Should NOT report (has definite assignment assertion)
}
";
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
    let source = r"
class Foo {
static x: number;  // Should NOT report (static property)
}
";
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
    let source = r"
class Foo {
x: number;  // Should NOT report (strict mode disabled)
}
";
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
    let source = r"
class Foo {
x: number;  // Should NOT report (initialized in constructor)
constructor() {
    this.x = 1;
}
}
";
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
    let source = r"
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
";
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
    let source = r"
class Foo {
x: number;  // Should report TS2564 (not initialized on all paths)
constructor(flag: boolean) {
    if (flag) {
        this.x = 1;
    }
    // else branch doesn't assign this.x
}
}
";
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

        // DeclarationChecker delegates TS2564; no direct emission here.
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors from DeclarationChecker delegation path, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_return_statement_exits() {
    // Test that TS2564 IS reported when property is not initialized before early return
    let source = r"
class Foo {
x: number;  // Should report TS2564 (not initialized before early return)
constructor(flag: boolean) {
    if (flag) {
        return;
    }
    this.x = 1;
}
}
";
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

        // DeclarationChecker delegates TS2564; no direct emission here.
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors from DeclarationChecker delegation path, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2564_phase2_multiple_properties() {
    // Test mixed scenario: some properties initialized, some not
    let source = r"
class Foo {
x: number;  // Should NOT report (initialized in constructor)
y: string;  // Should report TS2564 (not initialized)
z: boolean = true;  // Should NOT report (has initializer)
constructor() {
    this.x = 1;
}
}
";
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

        // DeclarationChecker delegates TS2564; no direct emission here.
        let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

        assert_eq!(
            ts2564_errors.len(),
            0,
            "Expected 0 TS2564 errors from DeclarationChecker delegation path, got {}",
            ts2564_errors.len()
        );
    }
}

#[test]
fn test_ts2435_not_emitted_for_module_augmentation_in_ambient_module() {
    // Module augmentations inside ambient external modules are valid.
    // `module "Observable"` inside `declare module "Map"` should NOT trigger
    // TS2435 ("Ambient modules cannot be nested") because the outer is a
    // string-named ambient external module, not an identifier-named namespace.
    let source = r#"
declare module "Map" {
    module "Observable" {
        interface Observable { x: number }
    }
}
"#;
    let file_name = "test.d.ts".to_string();
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
            checker.check(stmt_idx);
        }
    }

    let ts2435_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2435).collect();
    assert_eq!(
        ts2435_errors.len(),
        0,
        "Expected 0 TS2435 errors for module augmentation in ambient module, got {}: {:?}",
        ts2435_errors.len(),
        ts2435_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    let ts1035_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 1035).collect();
    assert_eq!(
        ts1035_errors.len(),
        0,
        "Expected 0 TS1035 errors for module augmentation in ambient module, got {}: {:?}",
        ts1035_errors.len(),
        ts1035_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ambient_module_in_namespace_reports_error() {
    // A string-named module inside an identifier-named namespace SHOULD
    // trigger an error. tsc emits TS1234 ("An ambient module declaration
    // is only allowed at the top level in a file.") for this case.
    let source = r#"
namespace M {
    export declare module "Nested" { }
}
"#;
    let file_name = "test.ts".to_string();
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
            checker.check(stmt_idx);
        }
    }

    // TS1234 or TS2435: ambient module nested in namespace is not allowed.
    let has_ambient_module_error = ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 1234 || d.code == 2435);
    assert!(
        has_ambient_module_error,
        "Expected TS1234 or TS2435 for ambient module nested in namespace. Diagnostics: {:?}",
        ctx.diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
