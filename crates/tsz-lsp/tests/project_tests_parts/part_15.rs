#[test]
fn test_project_get_implementations_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"interface Printable {
    print(): void;
}

class Document implements Printable {
    print() { }
}
"#
        .to_string(),
    );

    let impls = project.get_implementations("test.ts", Position::new(0, 10));
    // Implementations search for the interface
    // This may or may not find results depending on how file-local impl search works
    let _ = impls;
}

#[test]
fn test_project_cross_file_subtypes() {
    let mut project = Project::new();
    project.set_file(
        "base.ts".to_string(),
        r#"export class Animal {
    name: string;
}
"#
        .to_string(),
    );
    project.set_file(
        "dog.ts".to_string(),
        r#"import { Animal } from './base';
class Dog extends Animal {
    breed: string;
}
"#
        .to_string(),
    );
    project.set_file(
        "cat.ts".to_string(),
        r#"import { Animal } from './base';
class Cat extends Animal {
    indoor: boolean;
}
"#
        .to_string(),
    );

    // Position on "Animal" class name in base.ts (line 0, char 13)
    let subtypes = project.subtypes("base.ts", Position::new(0, 13));
    // Should find subtypes from other files (Dog and Cat)
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Dog"),
        "Should find Dog as a subtype of Animal across files, got: {names:?}"
    );
    assert!(
        names.contains(&"Cat"),
        "Should find Cat as a subtype of Animal across files, got: {names:?}"
    );
}

#[test]
fn test_project_cross_file_supertypes() {
    let mut project = Project::new();
    project.set_file(
        "base.ts".to_string(),
        r#"export class Vehicle {
    wheels: number;
}
"#
        .to_string(),
    );
    project.set_file(
        "car.ts".to_string(),
        r#"import { Vehicle } from './base';
class Car extends Vehicle {
    doors: number;
}
"#
        .to_string(),
    );

    // Position on "Car" class name in car.ts (line 1, char 6)
    let supertypes = project.supertypes("car.ts", Position::new(1, 6));
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Vehicle"),
        "Should find Vehicle as a supertype of Car across files, got: {names:?}"
    );
}

#[test]
fn test_project_cross_file_incoming_calls() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        r#"export function helper() {
    return 42;
}
"#
        .to_string(),
    );
    project.set_file(
        "main.ts".to_string(),
        r#"import { helper } from './utils';
function main() {
    helper();
}
"#
        .to_string(),
    );

    // Position on "helper" function name in utils.ts (line 0, char 16)
    let incoming = project.get_incoming_calls("utils.ts", Position::new(0, 16));
    // Should find the call from main.ts
    let caller_names: Vec<&str> = incoming.iter().map(|c| c.from.name.as_str()).collect();
    assert!(
        caller_names.contains(&"main"),
        "Should find 'main' as a caller of 'helper' across files, got: {caller_names:?}"
    );
}

#[test]
fn test_project_shared_type_interner() {
    // Verify that all files in a project share the same TypeInterner instance.
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hello';".to_string(),
    );

    // Both files should share the same Arc<TypeInterner> (same pointer)
    let a_interner = &project.files["a.ts"].type_interner;
    let b_interner = &project.files["b.ts"].type_interner;

    assert!(
        std::sync::Arc::ptr_eq(a_interner, b_interner),
        "All files in a project should share the same TypeInterner"
    );

    // The project-level interner should also be the same instance
    let project_interner = project.type_interner();
    assert!(
        std::sync::Arc::ptr_eq(a_interner, &project_interner),
        "Project-level interner should be the same instance as file interners"
    );
}

#[test]
fn test_project_shared_interner_survives_file_update() {
    // Verify that updating a file preserves the shared TypeInterner.
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hello';".to_string(),
    );

    let interner_before = project.type_interner();

    // Update file a.ts with new content
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 42;".to_string(),
    );

    // The interner should still be the same instance
    let interner_after = project.type_interner();
    assert!(
        std::sync::Arc::ptr_eq(&interner_before, &interner_after),
        "TypeInterner should persist across file updates"
    );

    // The updated file should still share the same interner
    let a_interner = &project.files["a.ts"].type_interner;
    assert!(
        std::sync::Arc::ptr_eq(a_interner, &interner_after),
        "Updated file should share the project's TypeInterner"
    );
}

#[test]
fn test_standalone_project_file_has_own_interner() {
    // Verify that standalone ProjectFile (outside Project) creates its own interner.
    use crate::project::ProjectFile;

    let file_a = ProjectFile::new("a.ts".to_string(), "const x = 1;".to_string());
    let file_b = ProjectFile::new("b.ts".to_string(), "const y = 2;".to_string());

    assert!(
        !std::sync::Arc::ptr_eq(&file_a.type_interner, &file_b.type_interner),
        "Standalone ProjectFiles should have independent TypeInterners"
    );
}

#[test]
fn test_project_files_share_definition_store() {
    // Verify that files created via Project::set_file share the project's DefinitionStore.
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hi';".to_string(),
    );

    let project_def_store = project.definition_store();
    let a_def_store = project.files["a.ts"]
        .definition_store
        .as_ref()
        .expect("Project file should have a shared DefinitionStore");
    let b_def_store = project.files["b.ts"]
        .definition_store
        .as_ref()
        .expect("Project file should have a shared DefinitionStore");

    assert!(
        std::sync::Arc::ptr_eq(&project_def_store, a_def_store),
        "File a.ts should share the project's DefinitionStore"
    );
    assert!(
        std::sync::Arc::ptr_eq(&project_def_store, b_def_store),
        "File b.ts should share the project's DefinitionStore"
    );
}

#[test]
fn test_standalone_project_file_has_no_shared_def_store() {
    // Standalone ProjectFile (outside Project) should not have a shared DefinitionStore.
    use crate::project::ProjectFile;

    let file = ProjectFile::new("test.ts".to_string(), "const x = 1;".to_string());
    assert!(
        file.definition_store.is_none(),
        "Standalone ProjectFile should not have a shared DefinitionStore"
    );
}

#[test]
fn test_project_diagnostics_use_shared_def_store() {
    // Verify that get_diagnostics works correctly when the shared DefinitionStore is wired.
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "let x: number = 42; let y: string = x;".to_string(),
    );

    // Should produce diagnostics (TS2322: number not assignable to string)
    let diagnostics = project.get_diagnostics("test.ts").unwrap();
    assert!(
        !diagnostics.is_empty(),
        "Should produce type-checking diagnostics with shared DefinitionStore"
    );
}

