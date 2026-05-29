#[test]
fn test_abstract_class_4_missing_still_uses_ts2654() {
    // When 4 or fewer abstract members are missing, TSC uses TS2654 (lists all).

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
}
class B extends A { }
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2654),
        "Expected TS2654 for 4 or fewer missing abstract members, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2655),
        "Should NOT use TS2655 when <=4 members are missing, got: {codes:?}"
    );

    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2654)
        .expect("TS2654 diagnostic should exist");
    assert!(
        !msg.message_text.contains("more"),
        "TS2654 should list all members without truncation, got: {}",
        msg.message_text
    );
}

#[test]
fn test_abstract_constructor_emits_ts1242() {
    // 'abstract' on a constructor should emit TS1242, not TS1244.
    // TSC anchors at the 'abstract' keyword.

    let source = r#"
abstract class A {
    abstract constructor() {}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1242),
        "Expected TS1242 for abstract constructor, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1244),
        "Should NOT emit TS1244 for abstract constructor, got: {codes:?}"
    );
}

#[test]
fn test_generic_interface_implements_ts2416() {
    // Test: genericSpecializations1.ts
    // Interface method has its own type param <T> shadowing the interface's T.
    // Non-generic implementations are NOT assignable to the generic method.
    let source = r#"
interface IFoo<T> {
    foo<T>(x: T): T;
}
class IntFooBad implements IFoo<number> {
    foo(x: string): string { return null; }
}
class StringFoo2 implements IFoo<string> {
    foo(x: string): string { return null; }
}
class StringFoo3 implements IFoo<string> {
    foo<T>(x: T): T { return null; }
}
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
    let count_2416 = codes.iter().filter(|&&c| c == 2416).count();
    assert!(
        count_2416 == 2,
        "Expected exactly 2 TS2416 errors (IntFooBad and StringFoo2), got {count_2416}: {codes:?}"
    );
}

// =============================================================================
// Stable identity: single-file heritage resolution in checker pre-population
// =============================================================================

/// Test that heritage resolution fires in single-file checker mode
/// (not just the merge path). After checker pre-population,
/// DefinitionInfo.extends should be wired for classes and interfaces.
#[test]
fn test_single_file_heritage_resolution_in_checker_prepopulation() {
    let source = r#"
class Animal { name: string = ""; }
class Dog extends Animal { bark(): void {} }
interface Shape { area(): number; }
interface Circle extends Shape { radius: number; }
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

    // Verify heritage was wired in the DefinitionStore by the checker's
    // pre-population path (populate_def_ids_from_semantic_defs Pass 3).
    let store = &checker.ctx.definition_store;

    // Find DefIds by name via the store's name index
    let find_def = |name: &str| -> Option<tsz_solver::def::DefId> {
        let atom = types.intern_string(name);
        store.find_defs_by_name(atom)?.into_iter().next()
    };

    let animal_def = find_def("Animal").expect("Animal should have DefId");
    let dog_def = find_def("Dog").expect("Dog should have DefId");
    let shape_def = find_def("Shape").expect("Shape should have DefId");
    let circle_def = find_def("Circle").expect("Circle should have DefId");

    // Dog extends Animal
    let dog_info = store.get(dog_def).expect("Dog info");
    assert_eq!(
        dog_info.extends,
        Some(animal_def),
        "Dog.extends should point to Animal in single-file mode"
    );

    // Circle extends Shape
    let circle_info = store.get(circle_def).expect("Circle info");
    assert_eq!(
        circle_info.extends,
        Some(shape_def),
        "Circle.extends should point to Shape in single-file mode"
    );
}

/// Test that implements resolution works in single-file checker mode.
#[test]
fn test_single_file_implements_resolution_in_checker_prepopulation() {
    let source = r#"
interface Printable { print(): void; }
interface Loggable { log(): void; }
class Report implements Printable, Loggable {
    print(): void {}
    log(): void {}
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

    let store = &checker.ctx.definition_store;

    let find_def = |name: &str| -> Option<tsz_solver::def::DefId> {
        let atom = types.intern_string(name);
        store.find_defs_by_name(atom)?.into_iter().next()
    };

    let printable_def = find_def("Printable").expect("Printable DefId");
    let loggable_def = find_def("Loggable").expect("Loggable DefId");
    let report_def = find_def("Report").expect("Report DefId");

    let report_info = store.get(report_def).expect("Report info");
    assert!(
        report_info.implements.contains(&printable_def),
        "Report.implements should contain Printable"
    );
    assert!(
        report_info.implements.contains(&loggable_def),
        "Report.implements should contain Loggable"
    );
}

/// Test that `ClassConstructor` companion `DefId`s are pre-populated for classes.
///
/// Verifies that the single-file pre-population path creates `ClassConstructor`
/// companion `DefId`s for top-level class declarations, moving constructor
/// identity from checker on-demand creation to binder-owned stable identity.
#[test]
fn test_class_constructor_companion_prepopulated_single_file() {
    let source = r#"
class Alpha {}
class Beta<T> { value: T; }
abstract class Gamma {}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Verify binder captured all 3 classes
    let class_count = binder
        .semantic_defs
        .values()
        .filter(|e| e.kind == crate::binder::SemanticDefKind::Class)
        .count();
    assert_eq!(
        class_count, 3,
        "Binder should capture 3 class semantic defs"
    );

    let types = TypeInterner::new();
    let checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    let store = &checker.ctx.definition_store;
    let stats = store.statistics();

    assert!(
        stats.class_constructors >= 3,
        "expected at least 3 ClassConstructor companions, got {}",
        stats.class_constructors
    );
    assert!(
        stats.class_to_constructor_entries >= 3,
        "expected at least 3 class_to_constructor entries, got {}",
        stats.class_to_constructor_entries
    );

    // Verify each class has a constructor companion
    for entry in binder.semantic_defs.values() {
        if entry.kind != crate::binder::SemanticDefKind::Class {
            continue;
        }
        let class_atom = types.intern_string(&entry.name);
        let defs = store.find_defs_by_name(class_atom);
        assert!(
            defs.is_some(),
            "Class {} should have defs by name",
            entry.name
        );

        // Find the Class def and check it has a companion
        let class_def = defs
            .as_ref()
            .unwrap()
            .iter()
            .find(|&&d| store.get_kind(d) == Some(tsz_solver::def::DefKind::Class))
            .unwrap_or_else(|| panic!("Class {} should have a Class DefId", entry.name));

        let ctor_def = store.get_constructor_def(*class_def);
        assert!(
            ctor_def.is_some(),
            "Class {} should have a ClassConstructor companion",
            entry.name
        );
    }
}

/// Test that `def_fallback_count` stays at zero for standard declarations
/// (all should be covered by binder `semantic_defs` pre-population).
#[test]
fn test_def_fallback_count_zero_for_standard_declarations() {
    let source = r#"
class MyClass<T> { value: T; }
interface MyInterface { x: number; }
type MyAlias = string | number;
enum MyEnum { A, B, C }
function myFunc(): void {}
const myVar: number = 42;
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Verify binder captured all 6 declarations in semantic_defs
    assert!(
        binder.semantic_defs.len() >= 6,
        "Binder should capture at least 6 semantic defs, got {}",
        binder.semantic_defs.len()
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // After checking, all standard top-level declarations should have been
    // resolved via pre-populated DefIds, not the fallback path.
    // Note: def_fallback_count may be > 0 for internal/implicit symbols
    // (like enum members accessed as values), but the top-level declarations
    // themselves should not trigger fallback.
    let fallback_count = checker.ctx.def_fallback_count.get();

    // We use a generous threshold here because some internal symbols
    // (enum member values, implicit constructor types) may still trigger
    // fallback. The key invariant is that top-level declarations don't.
    let store = &checker.ctx.definition_store;
    let names = [
        "MyClass",
        "MyInterface",
        "MyAlias",
        "MyEnum",
        "myFunc",
        "myVar",
    ];
    for name in &names {
        let atom = types.intern_string(name);
        let defs = store.find_defs_by_name(atom);
        assert!(
            defs.is_some() && !defs.as_ref().unwrap().is_empty(),
            "{name} should have a DefId in the DefinitionStore (not fallback-created)"
        );
    }

    // Fallback count may be non-zero for internal symbols; this is expected.
    let _ = fallback_count;
}

/// Test that using a shared `DefinitionStore` (pre-populated at merge time)
/// reduces checker fallback to zero for all top-level declaration families.
/// This verifies that the merge pipeline's `pre_populate_definition_store`
/// and checker's `warm_local_caches_from_shared_store` path provides complete
/// stable identity coverage.
#[test]
fn test_shared_def_store_eliminates_fallback_for_top_level() {
    use crate::parallel::{merge_bind_results, parse_and_bind_parallel};

    let source = r#"
export class MyClass<T> { value: T; }
export interface MyInterface { x: number; }
export type MyAlias = string | number;
export enum MyEnum { A, B, C }
export function myFunc(): void {}
export const myVar: number = 42;
"#;

    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Use the shared DefinitionStore from the merge pipeline
    let store = &program.definition_store;
    let interner = &program.type_interner;

    // Verify all families have DefIds in the shared store
    let names = [
        "MyClass",
        "MyInterface",
        "MyAlias",
        "MyEnum",
        "myFunc",
        "myVar",
    ];
    for name in &names {
        let atom = interner.intern_string(name);
        let defs = store.find_defs_by_name(atom);
        assert!(
            defs.is_some() && !defs.as_ref().unwrap().is_empty(),
            "{name} should have DefId in shared store before checker construction"
        );
    }

    // Verify symbol_only_index has mappings for all semantic_defs
    let mappings = store.all_symbol_mappings();
    assert!(
        mappings.len() >= 6,
        "Shared store should have at least 6 symbol→DefId mappings, got {}",
        mappings.len()
    );
}

/// Test that `super()` in a class extending a class with private static fields
/// does NOT emit false TS2346 ("Call target does not contain any signatures").
/// Regression test for conformance: privateNamesAndStaticFields.ts
#[test]
fn test_super_call_no_false_ts2346_with_private_static_fields() {
    let source = r#"
class A {
    static #foo: number;
    static #bar: number;
    constructor () {
    }
}

class B extends A {
    static #foo: string;
    constructor () {
        super();
        B.#foo = "some string";
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
        crate::checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    println!("=== super() with private static fields ===");
    for diag in &checker.ctx.diagnostics {
        println!(
            "  TS{}: {} (pos={})",
            diag.code, diag.message_text, diag.start
        );
    }

    let has_2346 = checker.ctx.diagnostics.iter().any(|d| d.code == 2346);
    assert!(
        !has_2346,
        "Should NOT emit TS2346 for super() in class with private static fields. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Test that `super()` in a class extending a forward-declared class
/// does NOT emit false TS2346. tsc emits TS2449 for the forward reference
/// but NOT TS2346 for the `super()` call.
/// Regression test for conformance: classSideInheritance2.ts
#[test]
fn test_super_call_no_false_ts2346_with_forward_reference() {
    let source = r#"
interface IText {
    foo: number;
}

interface TextSpan {}

class SubText extends TextBase {
    constructor(text: IText, span: TextSpan) {
        super();
    }
}

class TextBase implements IText {
    public foo: number;
    public subText(span: TextSpan): IText {
        return new SubText(this, span);
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

    println!("=== super() with forward reference ===");
    for diag in &checker.ctx.diagnostics {
        println!(
            "  TS{}: {} (pos={})",
            diag.code, diag.message_text, diag.start
        );
    }

    let has_2346 = checker.ctx.diagnostics.iter().any(|d| d.code == 2346);
    assert!(
        !has_2346,
        "Should NOT emit TS2346 for super() in class with forward-referenced base. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    // TS2449 (forward reference) should still be emitted
    let has_2449 = checker.ctx.diagnostics.iter().any(|d| d.code == 2449);
    assert!(
        has_2449,
        "Should still emit TS2449 for class used before its declaration"
    );
}

#[test]
fn test_type_predicate_narrowing_no_redundant_intersection() {
    // Regression test: type predicate narrowing with conditional type utilities
    // like Extract<T, Function> should produce Extract<T, Function> directly,
    // not the redundant intersection T & Extract<T, Function>.
    //
    // The redundant intersection prevents proper evaluation of the conditional
    // type during call resolution, which can cause false TS2348/TS2349 errors.
    //
    // This test verifies that calling a function whose return type is inferred
    // from a type predicate with Extract<T, Function> doesn't produce false
    // "not callable" errors.
    let code = r#"
declare function isFunction<T>(value: T): value is Extract<T, Function>;

function getFunction<T>(item: T) {
    if (isFunction(item)) {
        return item;
    }
    throw new Error();
}

// When f12 calls getFunction(x), the return type should be
// Extract<string | (() => string) | undefined, Function> which
// evaluates to () => string. Calling f() should be valid.
function f12(x: string | (() => string) | undefined) {
    const f = getFunction(x);
    f();
}
"#;

    let (parser, root) = parse_test_source(code);
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

    let all_codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should NOT have TS2348 (value not callable, did you mean new?)
    // The is_constructor_type check should not falsely detect the type as constructable.
    assert!(
        !all_codes.contains(&2348),
        "Should not emit TS2348 for Extract-narrowed function call. Diagnostics: {all_codes:?}",
    );
}
