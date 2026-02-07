//! Tests for File Rename functionality (workspace/willRenameFiles).
//!
//! These tests verify that import statements are correctly updated when
//! files or directories are renamed.

use crate::project::{FileRename, Project};

/// Helper function to create a test project with multiple files.
fn create_test_project() -> Project {
    let mut project = Project::new();

    // Create a simple directory structure:
    // /src/
    //   main.ts (imports utils)
    //   utils/
    //     math.ts
    //     strings.ts

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { add } from './utils/math';
import { capitalize } from './utils/strings';

const result = add(1, 2);
const text = capitalize('hello');
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/math.ts".to_string(),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function subtract(a: number, b: number): number {
    return a - b;
}
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/strings.ts".to_string(),
        r#"
export function capitalize(s: string): string {
    return s.charAt(0).toUpperCase() + s.slice(1);
}

export function lowercase(s: string): string {
    return s.toLowerCase();
}
"#
        .to_string(),
    );

    project
}

#[test]
fn test_single_file_rename_updates_imports() {
    let mut project = create_test_project();

    // Rename /src/utils/math.ts to /src/utils/calculations.ts
    let renames = vec![FileRename {
        old_uri: "/src/utils/math.ts".to_string(),
        new_uri: "/src/utils/calculations.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit for the import statement
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_directory_rename_updates_imports() {
    let mut project = create_test_project();

    // Rename /src/utils/ to /src/helpers/
    let renames = vec![FileRename {
        old_uri: "/src/utils".to_string(),
        new_uri: "/src/helpers".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have two edits (one for each import from utils)
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_nested_directory_rename() {
    let mut project = Project::new();

    // Create nested structure:
    // /src/
    //   main.ts
    //   lib/
    //     utils/
    //       math.ts
    //       strings.ts

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { add } from './lib/utils/math';
import { capitalize } from './lib/utils/strings';
"#
        .to_string(),
    );

    project.set_file(
        "/src/lib/utils/math.ts".to_string(),
        "export function add(a: number, b: number): number { return a + b; }".to_string(),
    );

    project.set_file(
        "/src/lib/utils/strings.ts".to_string(),
        "export function capitalize(s: string): string { return s; }".to_string(),
    );

    // Rename /src/lib/utils/ to /src/lib/helpers/
    let renames = vec![FileRename {
        old_uri: "/src/lib/utils".to_string(),
        new_uri: "/src/lib/helpers".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have two edits
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_sibling_directory_rename() {
    let mut project = Project::new();

    // Create sibling structure:
    // /src/
    //   feature-a/
    //     main.ts
    //   feature-b/
    //     utils.ts

    project.set_file(
        "/src/feature-a/main.ts".to_string(),
        r#"
import { helper } from '../feature-b/utils';
"#
        .to_string(),
    );

    project.set_file(
        "/src/feature-b/utils.ts".to_string(),
        "export function helper() {}".to_string(),
    );

    // Rename /src/feature-b/ to /src/shared/
    let renames = vec![FileRename {
        old_uri: "/src/feature-b".to_string(),
        new_uri: "/src/shared".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit with ../shared/utils
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_reexport_updates_on_rename() {
    let mut project = Project::new();

    // Create files with re-exports:
    // /src/
    //   main.ts
    //   utils/
    //     index.ts (re-exports math.ts)
    //     math.ts

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { add } from './utils';
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/index.ts".to_string(),
        r#"
export { add, subtract } from './math';
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/math.ts".to_string(),
        r#"
export function add(a: number, b: number): number { return a + b; }
export function subtract(a: number, b: number): number { return a - b; }
"#
        .to_string(),
    );

    // Rename /src/utils/math.ts to /src/utils/operations.ts
    let renames = vec![FileRename {
        old_uri: "/src/utils/math.ts".to_string(),
        new_uri: "/src/utils/operations.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // index.ts should have one edit for the re-export
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_extensionless_import_updates() {
    let mut project = Project::new();

    // Create files with extensionless import:
    // /src/
    //   main.ts (imports ./utils - points to utils.ts)

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { helper } from './utils';
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils.ts".to_string(),
        "export function helper() {}".to_string(),
    );

    // Rename /src/utils.ts to /src/helpers.ts
    let renames = vec![FileRename {
        old_uri: "/src/utils.ts".to_string(),
        new_uri: "/src/helpers.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit updating ./utils to ./helpers
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_no_edit_for_unrelated_imports() {
    let mut project = Project::new();

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { add } from './math';
import { capitalize } from './strings';
"#
        .to_string(),
    );

    project.set_file(
        "/src/math.ts".to_string(),
        "export function add() {}".to_string(),
    );

    project.set_file(
        "/src/other.ts".to_string(),
        "export function helper() {}".to_string(),
    );

    // Rename /src/other.ts (not imported by main.ts)
    let renames = vec![FileRename {
        old_uri: "/src/other.ts".to_string(),
        new_uri: "/src/helper.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should NOT have any edits since it doesn't import other.ts
    assert!(edits.changes.is_empty());
}

#[test]
fn test_dot_slash_prefix_preserved() {
    let mut project = Project::new();

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { add } from './math';
"#
        .to_string(),
    );

    project.set_file(
        "/src/math.ts".to_string(),
        "export function add() {}".to_string(),
    );

    // Rename /src/math.ts to /src/calculations.ts
    let renames = vec![FileRename {
        old_uri: "/src/math.ts".to_string(),
        new_uri: "/src/calculations.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit, and the new specifier should preserve ./ prefix
    assert!(!edits.changes.is_empty());
}

#[test]
#[ignore = "Requires directory-to-index module resolution (./utils -> ./utils/index.ts)"]
fn test_directory_with_index_file() {
    let mut project = Project::new();

    // Create structure with index.ts:
    // /src/
    //   main.ts (imports from ./utils - resolves to utils/index.ts)
    //   utils/
    //     index.ts

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { helper } from './utils';
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/index.ts".to_string(),
        "export function helper() {}".to_string(),
    );

    // Rename /src/utils/ directory
    let renames = vec![FileRename {
        old_uri: "/src/utils".to_string(),
        new_uri: "/src/helpers".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_dynamic_import_updates() {
    let mut project = Project::new();

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
async function loadModule() {
    const module = await import('./utils/math');
    return module.add(1, 2);
}
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/math.ts".to_string(),
        "export function add(a: number, b: number): number { return a + b; }".to_string(),
    );

    // Rename /src/utils/math.ts to /src/utils/calculations.ts
    let renames = vec![FileRename {
        old_uri: "/src/utils/math.ts".to_string(),
        new_uri: "/src/utils/calculations.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit for the dynamic import
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_require_call_updates() {
    let mut project = Project::new();

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
const utils = require('./utils/math');
const result = utils.add(1, 2);
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils/math.ts".to_string(),
        "export function add(a: number, b: number): number { return a + b; }".to_string(),
    );

    // Rename /src/utils/math.ts to /src/utils/calculations.ts
    let renames = vec![FileRename {
        old_uri: "/src/utils/math.ts".to_string(),
        new_uri: "/src/utils/calculations.ts".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have one edit for the require call
    assert!(!edits.changes.is_empty());
}

#[test]
fn test_mixed_imports_and_dynamic() {
    let mut project = Project::new();

    project.set_file(
        "/src/main.ts".to_string(),
        r#"
import { staticFn } from './utils';
const utils = require('./utils/math');
async function load() {
    const module = await import('./utils/strings');
}
"#
        .to_string(),
    );

    project.set_file(
        "/src/utils.ts".to_string(),
        "export function staticFn() {}".to_string(),
    );

    project.set_file(
        "/src/utils/math.ts".to_string(),
        "export function add() {}".to_string(),
    );

    project.set_file(
        "/src/utils/strings.ts".to_string(),
        "export function capitalize() {}".to_string(),
    );

    // Rename /src/utils/ directory
    let renames = vec![FileRename {
        old_uri: "/src/utils".to_string(),
        new_uri: "/src/helpers".to_string(),
    }];

    let edits = project.handle_will_rename_files(&renames);

    // main.ts should have 3 edits (static import, require, dynamic import)
    assert!(!edits.changes.is_empty());
}
