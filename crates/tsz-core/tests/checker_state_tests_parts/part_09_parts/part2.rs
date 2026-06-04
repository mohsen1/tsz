/// TS Unsoundness #44: Interface merging with method overloads
///
/// When interfaces merge, methods with the same name become overloads.
#[test]
fn test_interface_merging_method_overloads() {
    let source = r#"
interface Calculator {
    add(a: number, b: number): number;
}

interface Calculator {
    add(a: string, b: string): string;
    multiply(a: number, b: number): number;
}

// Merged interface has both overloads of add and multiply
declare const calc: Calculator;

const numResult: number = calc.add(1, 2);
const strResult: string = calc.add("a", "b");
const product: number = calc.multiply(3, 4);
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Interface Merging Method Overloads Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface merging with overloads should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Interface extending and merging
///
/// Interfaces can both extend other interfaces and merge with
/// other declarations of the same name.
///
/// NOTE: Currently ignored - interface extending and merging is not fully implemented.
#[test]
fn test_interface_extend_and_merge() {
    let source = r#"
interface Named {
    name: string;
}

interface Person extends Named {
    age: number;
}

// Merge more properties into Person
interface Person {
    email: string;
}

// Person now has name (from Named), age, and email
const person: Person = {
    name: "Alice",
    age: 30,
    email: "alice@example.com"
};

const n: string = person.name;
const a: number = person.age;
const e: string = person.email;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Interface Extend and Merge Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface extend and merge should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Namespace and interface merging
///
/// Namespaces can merge with interfaces to add static members.
///
/// EXPECTED FAILURE: Namespace-interface merging for value-space access
/// is not yet implemented. Currently expects 2 errors.
#[test]
fn test_namespace_interface_merging() {
    let source = r##"
interface Color {
    r: number;
    g: number;
    b: number;
}

namespace Color {
    export function fromHex(hex: string): Color {
        return { r: 0, g: 0, b: 0 };
    }
    export const RED: Color = { r: 255, g: 0, b: 0 };
}

// Use as interface type
const myColor: Color = { r: 100, g: 150, b: 200 };

// Use namespace members (these should work but currently fail)
const red: Color = Color.RED;
const fromString: Color = Color.fromHex("#FF0000");
"##;

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

    let error_count = checker.ctx.diagnostics.len();

    // Now expects 0 errors: both interface member access (myColor.r, etc.) and
    // namespace value access (Color.RED, Color.fromHex) work correctly after
    // fixing interface+namespace merge type resolution.
    assert_eq!(
        error_count, 0,
        "Expected 0 errors for namespace-interface merging: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Class and namespace merging
///
/// Classes can merge with namespaces to add static properties/methods.
///
/// NOTE: Currently ignored - class-namespace merging is not fully implemented.
/// The merging doesn't correctly handle type checking for merged static members.
#[test]
fn test_class_namespace_merging() {
    let source = r#"
class Album {
    title: string;
    constructor(title: string) {
        this.title = title;
    }
}

namespace Album {
    export interface Track {
        name: string;
        duration: number;
    }
    export function create(title: string): Album {
        return new Album(title);
    }
}

// Use class as type and constructor
const album: Album = new Album("Best Of");

// Use namespace members
const track: Album.Track = { name: "Song 1", duration: 180 };
const created: Album = Album.create("New Album");
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Class Namespace Merging Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Class and namespace merging should work: {:?}",
        checker.ctx.diagnostics
    );
}
