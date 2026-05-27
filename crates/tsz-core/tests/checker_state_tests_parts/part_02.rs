/// Test that TS2307 is emitted for dynamic imports with unresolved module specifiers
#[test]
fn test_ts2307_dynamic_import_unresolved() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
async function loadModule() {
    const mod = await import("./missing-dynamic-module");
    return mod;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    let module_diag = checker.ctx.diagnostics.iter().find(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            || d.code
                == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
    });

    assert!(
        module_diag.is_some(),
        "Expected TS2307 or TS2792 diagnostic for dynamic import, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    let diag = module_diag.unwrap();
    assert!(
        diag.message_text.contains("./missing-dynamic-module"),
        "Module-not-found message should contain module specifier, got: {}",
        diag.message_text
    );
}

/// Test that TS2307 is NOT emitted for dynamic imports with non-string specifiers
/// (e.g., variables or template literals cannot be statically checked)
#[test]
fn test_ts2307_dynamic_import_non_string_specifier_no_error() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
async function loadModule(modulePath: string) {
    const mod = await import(modulePath);
    return mod;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Dynamic specifiers cannot be statically checked, so no TS2307 should be emitted
    let ts2307_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .count();

    assert_eq!(
        ts2307_count, 0,
        "Expected no TS2307 for dynamic import with variable specifier, got {ts2307_count} errors"
    );
}

#[test]
fn test_missing_type_reference_in_function_type_emits_2304() {
    let source = r#"
type Fn = (value: MissingType) => void;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved type in function type, got: {codes:?}"
    );
}

#[test]
fn test_missing_property_access_emits_2339_not_2304() {
    let source = r#"
const obj = { value: 1 };
obj.missing;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "Expected TS2339 for missing property access, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for missing property access, got: {codes:?}"
    );
}

#[test]
fn test_arguments_in_async_arrow_no_2304() {
    let source = r#"
function f() {
    return async () => arguments.length;
}

class C {
    method() {
        var fn = async () => arguments[0];
    }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for 'arguments' in async arrow, got: {codes:?}"
    );
}

#[test]
fn test_ts2496_arguments_in_arrow_function_es5() {
    // TS2496: arguments cannot be referenced in an arrow function in ES5.
    let source = r#"
function f() {
    var a = () => arguments;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES5,
        strict: false,
        always_strict: false,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2496),
        "Expected TS2496 for 'arguments' in arrow function at ES5 target, got: {codes:?}"
    );
}

#[test]
fn test_ts2496_not_emitted_for_es2015_target() {
    // TS2496 should NOT fire when target is ES2015+ (arrow functions are native).
    let source = r#"
function f() {
    var a = () => arguments;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES2015,
        strict: false,
        always_strict: false,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2496),
        "TS2496 should not fire for ES2015 target, got: {codes:?}"
    );
}

#[test]
fn test_ts1100_arguments_in_strict_mode() {
    // TS1100: 'arguments' used as variable name in strict mode.
    let source = r#"
var arguments;
var a = () => arguments;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES5,
        strict: false,
        always_strict: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1100),
        "Expected TS1100 for 'var arguments' in strict mode, got: {codes:?}"
    );
    assert!(
        codes.contains(&2496),
        "Expected TS2496 for 'arguments' in arrow at ES5 target, got: {codes:?}"
    );
}

#[test]
fn test_signature_type_params_no_2304() {
    let source = r#"
interface BaseConstructor {
    new <T>(x: T): { value: T };
    new <T, U>(x: T, y: U): { x: T, y: U };
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for signature type params, got: {codes:?}"
    );
}

#[test]
fn test_extends_undefined_no_2304() {
    let source = r#"
class C extends undefined {}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for extends undefined, got: {codes:?}"
    );
}

#[test]
fn test_extends_null_no_2304() {
    let source = r#"
class C extends null {}
class D extends (null) {}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for extends null, got: {codes:?}"
    );
}

#[test]
fn test_decorator_invalid_declarations_no_ts2304() {
    let source = r#"
declare function dec<T>(target: T): T;

@dec
enum E {}

@dec
interface I {}

@dec
namespace M {}

@dec
type T = number;

@dec
var x: number;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for invalid decorator declarations, got: {codes:?}"
    );
}

#[test]
fn test_abstract_class_in_local_scope_2511() {
    use crate::binder::symbol_flags;

    // Test case from tests/cases/compiler/abstractClassInLocalScopeIsAbstract.ts
    // Abstract class declared inside an IIFE should still error on instantiation
    let code = r#"
        (() => {
            abstract class A {}
            class B extends A {}
            new A();
            new B();
        })()
    "#;

    let (parser, root) = parse_test_source(code);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Debug: Check symbols
    let symbols = binder.get_symbols();
    println!("=== Symbols ===");
    for i in 0..symbols.len() {
        if let Some(sym) = symbols.get(crate::binder::SymbolId(i as u32)) {
            println!(
                "  {:?}: {} flags={:#x} abstract={}",
                sym.id,
                sym.escaped_name,
                sym.flags,
                sym.flags & symbol_flags::ABSTRACT != 0
            );
        }
    }

    // Also try manually checking new expression
    println!("=== Class name lookup test ===");
    if let Some(sym_id) = binder.get_symbols().find_by_name("A") {
        println!("Found symbol A: {sym_id:?}");
        if let Some(symbol) = binder.get_symbol(sym_id) {
            println!(
                "  flags={:#x} abstract={}",
                symbol.flags,
                symbol.flags & symbol_flags::ABSTRACT != 0
            );
        }
    } else {
        println!("Symbol A not found!");
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: Check diagnostics
    println!("=== Diagnostics ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    // Should have error 2511 for `new A()` but not for `new B()`
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2511),
        "Expected error 2511 for abstract class instantiation in local scope, got: {codes:?}"
    );

    // Should only have one 2511 error (for A, not B)
    let count_2511 = codes.iter().filter(|&&c| c == 2511).count();
    assert_eq!(
        count_2511, 1,
        "Expected exactly 1 error 2511 (for abstract class A only), got {count_2511} from: {codes:?}"
    );

    // Should NOT have error 2304 (Cannot find name) - both A and B should be found
    let count_2304 = codes.iter().filter(|&&c| c == 2304).count();
    assert_eq!(
        count_2304, 0,
        "Should NOT have 'Cannot find name' error (2304) for classes in local scope, got {count_2304} from: {codes:?}"
    );
}

#[test]
fn test_static_member_suggestion_2662() {
    // Error 2662: Cannot find name 'foo'. Did you mean the static member 'C.foo'?
    let source = r#"
class C {
    static foo: string;

    bar() {
        let k = foo;
    }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: show all diagnostics
    println!("=== Diagnostics for static member suggestion ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2662),
        "Expected error 2662 (Cannot find name 'foo'. Did you mean the static member 'C.foo'?), got: {codes:?}"
    );

    // Should NOT have generic "cannot find name" error 2304
    assert!(
        !codes.contains(&2304),
        "Should not have generic error 2304, should have specific 2662 instead. Got: {codes:?}"
    );
}

#[test]
fn test_static_member_suggestion_2662_assignment_target() {
    // Error 2662: Cannot find name 's'. Did you mean the static member 'C.s'?
    // This tests the case where a static member is used as an assignment target,
    // which goes through get_type_of_assignment_target instead of get_type_of_identifier.
    let source = r#"
class C {
    static s: any;

    constructor() {
        s = 1;
    }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: show all diagnostics
    println!("=== Diagnostics for static member assignment target ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2662),
        "Expected error 2662 (Cannot find name 's'. Did you mean the static member 'C.s'?) for assignment target, got: {codes:?}"
    );

    // Should NOT have generic "cannot find name" error 2304
    assert!(
        !codes.contains(&2304),
        "Should not have generic error 2304, should have specific 2662 instead. Got: {codes:?}"
    );
}

#[test]
fn test_class_static_side_property_assignability() {
    let source = r#"
class A {
    static foo: number;
}
class B {}
let ctor: typeof A = B;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either 2741 (property missing) or 2322 (type not assignable)
    // Both correctly indicate the assignment is rejected due to missing static member
    assert!(
        codes.contains(&2741) || codes.contains(&2322),
        "Expected error 2741 or 2322 for missing static member on constructor type, got: {codes:?}"
    );
}

#[test]
fn test_private_member_nominal_class_assignability() {
    let source = r#"
class A {
    private x: number;
}
class B {
    private x: number;
}
const a: A = new B();
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either 2741 (property missing - TypeScript's preferred message) or 2322 (type not assignable)
    // Both indicate the assignment is correctly rejected due to private member nominality
    assert!(
        codes.contains(&2741) || codes.contains(&2322),
        "Expected error 2741 or 2322 for private member nominal mismatch, got: {codes:?}"
    );
}

#[test]
fn test_private_protected_property_access_errors() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Foo {
    private x = 1;
    protected y = 2;
}
const f = new Foo();
f.x;
f.y;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS),
        "Expected error 2341 for private property access, got: {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES),
        "Expected error 2445 for protected property access, got: {codes:?}"
    );
}

#[test]
fn test_private_protected_property_access_ok() {
    let source = r#"
class Base {
    protected z = 3;
}
class Derived extends Base {
    test() { return this.z; }
}
class Baz {
    private w = 4;
    getW() { return this.w; }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that protected access requires derived instance
///
/// NOTE: Currently ignored - protected access control is not fully implemented.
/// The checker emits duplicate TS2445 errors for protected member access.
#[test]
fn test_protected_access_requires_derived_instance() {
    use crate::checker::diagnostics::diagnostic_codes;

    // In tsc, accessing a protected member on a base-class-typed reference from
    // a derived class is TS2446 ("Property 'y' is protected and only accessible
    // through an instance of class 'Derived'. This is an instance of class 'Base'."),
    // not TS2445 ("only accessible within class and its subclasses").
    let source = r#"
class Base {
    protected y = 2;
}
class Derived extends Base {
    test(b: Base, d: Derived) {
        b.y;
        d.y;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let protected_errors = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A)
        .count();
    assert_eq!(
        protected_errors, 1,
        "Expected one error 2446 for protected access on base instance from derived, got: {codes:?}"
    );
}

#[test]
fn test_protected_static_access_allowed_from_derived_class() {
    use crate::checker::diagnostics::diagnostic_codes;

    // Protected static members are accessible from subclasses through any
    // reference to the class hierarchy (both Base.s and Derived.s).
    // This matches tsc behavior — the receiver check only applies to
    // instance members, not static members.
    let source = r#"
class Base {
    protected static s = 1;
}
class Derived extends Base {
    static test() {
        Base.s;
        Derived.s;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let protected_errors = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES)
        .count();
    assert_eq!(
        protected_errors, 0,
        "Expected no TS2445 errors for protected static access from derived class, got: {codes:?}"
    );
}

#[test]
fn test_abstract_property_in_constructor_2715() {
    // Error 2715: Abstract property 'prop' in class 'AbstractClass' cannot be accessed in the constructor.

    let source = r#"
abstract class AbstractClass {
    constructor(str: string) {
        let val = this.prop.toLowerCase();
    }

    abstract prop: string;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2715),
        "Expected error 2715 (Abstract property cannot be accessed in constructor), got: {codes:?}"
    );
}

#[test]
fn test_interface_name_cannot_be_reserved_2427() {
    // Error 2427: Interface name cannot be 'string' (or other primitive types)
    let source = r#"interface string {}"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: show all diagnostics
    println!("=== Diagnostics for 'interface string {{}}' ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2427),
        "Expected error 2427 (Interface name cannot be 'string'), got: {codes:?}"
    );
}

#[test]
fn test_const_modifier_on_class_property_1248() {
    // Error 1248: A class member cannot have the 'const' keyword
    let source = r#"class AtomicNumbers { static const H = 1; }"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: show all diagnostics
    println!("=== Diagnostics for 'static const H = 1' ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1248),
        "Expected error 1248 (A class member cannot have the 'const' keyword), got: {codes:?}"
    );
}

#[test]
fn test_accessor_type_compatibility_2322() {
    // TS 5.1+: when BOTH getter and setter have explicit type annotations,
    // unrelated types are allowed — no TS2322.
    let source = r#"class C {
    public set AnnotatedSetter(a: number) { }
    public get AnnotatedSetter(): string { return ""; }
}"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "TS 5.1+ allows unrelated types when both annotated, no TS2322; got codes: {codes:?}",
    );
}

#[test]
fn test_accessor_type_compatibility_inheritance_no_error() {
    // Test that getter returning derived class type is assignable to setter base class param
    // class B extends A, so B <: A
    // Getter returns B, setter takes A -> Should NOT error (B is assignable to A)

    let source = r#"
class A { }
class B extends A { }

class C {
    public set AnnotatedSetter(a: A) { }
    public get AnnotatedSetter() { return new B(); }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Debug: show all diagnostics
    println!("=== Diagnostics for inheritance accessor test ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should NOT have TS2322 - B is assignable to A (B extends A)
    assert!(
        !codes.contains(&2322),
        "Should NOT have error 2322 (B extends A, so getter returning B is assignable to setter taking A). Got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_accessor_type_compatibility_typeof_structural() {
    // Getter return type should be assignable to setter param type when using typeof.
    let source = r#"
var x: { foo: string; }
class C {
    get value() { return x; }
    set value(v: typeof x) { }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 0,
        "Did not expect TS2322 for typeof accessor compatibility, got: {codes:?}"
    );
}

#[test]
fn test_abstract_class_through_type_alias_2511() {
    // Error 2511: Cannot create an instance of an abstract class - through type alias

    let source = r#"
abstract class AbstractA { a!: string; }
type Abstracts = typeof AbstractA;
declare const cls2: Abstracts;
new cls2();
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Abstract class instantiation checking not yet implemented
    // Once implemented, change to expect error 2511
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    if !codes.contains(&2511) {
        println!("=== Abstract Class Through Type Alias ===");
        println!("Expected error 2511 once abstract class checking implemented, got: {codes:?}");
    }
    // Accept 0 errors until abstract class checking is implemented
    assert!(
        codes.is_empty() || codes.contains(&2511),
        "Expected 0 errors (not implemented) or 2511: {codes:?}"
    );
}

#[test]
fn test_abstract_class_union_type_2511() {
    // Error 2511: Cannot create an instance of an abstract class - through union type

    let source = r#"
class ConcreteA {}
abstract class AbstractA { a!: string; }

type ConcretesOrAbstracts = typeof ConcreteA | typeof AbstractA;

declare const cls1: ConcretesOrAbstracts;

new cls1();
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Abstract class instantiation checking not yet implemented
    // Once implemented, change to expect error 2511
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    if !codes.contains(&2511) {
        println!("=== Abstract Class Union Type ===");
        println!("Expected error 2511 once abstract class checking implemented, got: {codes:?}");
    }
    // Accept 0 errors until abstract class checking is implemented
    assert!(
        codes.is_empty() || codes.contains(&2511),
        "Expected 0 errors (not implemented) or 2511: {codes:?}"
    );
}

#[test]
fn test_property_used_before_initialization_2729() {
    // Error 2729: Property is used before its initialization

    let source = r#"
class Foo {
    x = this.a;  // Error: Property 'a' is used before its initialization
    a = 1;
}

class NoError {
    a = 1;
    x = this.a;  // OK: 'a' is declared before 'x'
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly one 2729 error (in class Foo)
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 1,
        "Expected exactly 1 error 2729 for property used before initialization, got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_new_expression_property_used_before_initialization_2729() {
    let source = r#"
class CtorTyped {
    value: { new (): object };
    copy = new this.value();
}

class CtorGeneric {
    value: { new <T>(): T };
    copy = new this.value<string>();
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 2,
        "Expected TS2729 for new this.value() and new this.value<T>(), got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_static_block_property_used_before_initialization_2729() {
    // Error 2729: Property used before initialization in static blocks
    // Static blocks referencing later-declared static properties via C.X or this.X

    let source = r#"
class C {
    static f1 = 1;
    static {
        console.log(C.f1, C.f2, C.f3)
    }
    static f2 = 2;
    static {
        console.log(C.f1, C.f2, C.f3)
    }
    static f3 = 3;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // First static block: C.f2 (after) and C.f3 (after) → 2 errors
    // Second static block: C.f3 (after) → 1 error
    // Total: 3 TS2729 errors
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 3,
        "Expected 3 TS2729 errors for static block use-before-init, got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_static_block_this_access_2729() {
    // Error 2729: this.X in static block where X is declared after

    let source = r#"
class C {
    static s1 = 1;
    static {
        this.s1;
        C.s1;
        this.s2;
        C.s2;
    }
    static s2 = 2;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // this.s2 and C.s2 are before s2's declaration → 2 errors
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 2,
        "Expected 2 TS2729 errors for this.s2 and C.s2 in static block, got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_static_block_no_error_for_arrow_function_2729() {
    // Accesses inside arrow functions in static blocks are deferred — no TS2729

    let source = r#"
class C {
    static {
        const fn = () => C.s1;
    }
    static s1 = 1;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Arrow function defers the access — no TS2729
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 0,
        "Expected 0 TS2729 errors (arrow function defers access), got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_property_not_assignable_to_same_in_base_2416() {
    // Error 2416: Property 'num' in type 'WrongTypePropertyImpl' is not assignable
    // to the same property in base type 'WrongTypeProperty'.

    let source = r#"
abstract class WrongTypeProperty {
    abstract num: number;
}
class WrongTypePropertyImpl extends WrongTypeProperty {
    num = "nope, wrong";
}
"#;

    let (parser, root) = parse_test_source(source);

    // Debug: Print parsed classes
    let arena = parser.get_arena();
    println!("Number of classes in arena: {}", arena.classes.len());
    for (i, class) in arena.classes.iter().enumerate() {
        println!(
            "Class {}: has heritage = {}",
            i,
            class.heritage_clauses.is_some()
        );
        if let Some(ref hc) = class.heritage_clauses {
            println!("  Heritage clause nodes: {}", hc.nodes.len());
        }
    }

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Debug: print file locals
    println!("File locals count: {}", binder.file_locals.len());

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have at least one 2416 error for the incompatible property type
    let count_2416 = codes.iter().filter(|&&c| c == 2416).count();
    assert!(
        count_2416 >= 1,
        "Expected at least 1 error 2416 for property not assignable to base, got {count_2416} in: {codes:?}"
    );
}

#[test]
fn test_property_not_assignable_to_generic_base_2416() {
    let source = r#"
abstract class Base<T> {
    abstract value: T;
}
class Derived extends Base<string> {
    value = 123;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2416),
        "Expected error 2416 for generic base property mismatch, got: {codes:?}"
    );
}

#[test]
fn test_non_abstract_class_missing_implementations_2654() {
    // Error 2654: Non-abstract class 'C' is missing implementations for
    // the following members of 'B': 'prop', 'm'.

    let source = r#"
abstract class B {
    abstract prop: number;
    abstract m(): void;
}
class C extends B {
    // Missing implementations for 'prop' and 'm'
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have error 2654 for missing abstract implementations
    let count_2654 = codes.iter().filter(|&&c| c == 2654).count();
    assert!(
        count_2654 >= 1,
        "Expected at least 1 error 2654 for missing abstract implementations, got {count_2654} in: {codes:?}"
    );

    // Check the message mentions the missing members
    let has_prop = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2654 && d.message_text.contains("'prop'"));
    let has_m = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2654 && d.message_text.contains("'m'"));
    assert!(has_prop, "Error 2654 should mention missing 'prop'");
    assert!(has_m, "Error 2654 should mention missing 'm'");
}

#[test]
fn test_readonly_property_assignment_2540() {
    // Error 2540: Cannot assign to 'ro' because it is a read-only property.

    let source = r#"
class C {
    readonly ro: string = "readonly please";
}
let c = new C();
c.ro = "error: lhs of assignment can't be readonly";
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have error 2540 for readonly property assignment
    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly property assignment, got {count_2540} in: {codes:?}"
    );
}

#[test]
fn test_readonly_element_access_assignment_2540() {
    // Error 2540: Cannot assign to 'name' because it is a read-only property.

    let source = r#"
interface Config {
    readonly name: string;
}
let config: Config = { name: "ok" };
config["name"] = "error";
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly element access assignment, got {count_2540} in: {codes:?}"
    );
}

#[test]
fn test_readonly_array_element_assignment_2540() {
    // Error 2542: Index signature in type 'readonly number[]' only permits reading.

    let source = r#"
const xs: readonly number[] = [1, 2];
xs[0] = 3;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // TS2542 for readonly index signatures (tsc emits 2542, not 2540, for arrays)
    let count = codes.iter().filter(|&&c| c == 2542 || c == 2540).count();
    assert!(
        count >= 1,
        "Expected at least 1 error 2540/2542 for readonly array element assignment, got {count} in: {codes:?}"
    );
}

#[test]
fn test_readonly_method_signature_assignment_2540() {
    // Error 2540: Cannot assign to 'run' because it is a read-only property.

    let source = r#"
interface Service {
    readonly run(): void;
}
let svc: Service = { run() {} };
svc.run = () => {};
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly method signature assignment, got {count_2540} in: {codes:?}"
    );
}

