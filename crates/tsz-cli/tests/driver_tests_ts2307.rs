//! TS2307 Path Mapping Validation Tests
//!
//! These tests verify that TS2307 errors are properly emitted when path mappings
//! configured in tsconfig.json don't resolve to actual files.

use crate::config::{
    CompilerOptions, ModuleResolutionKind, ResolvedCompilerOptions, resolve_compiler_options,
};
use crate::driver_resolution::{ModuleResolutionCache, resolve_module_specifier};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tsz::emitter::ModuleKind;

/// Helper function to create a test directory structure
fn create_test_structure() -> TempDir {
    TempDir::new().unwrap()
}

/// Helper function to create compiler options with path mappings
fn create_options_with_paths(
    base_url: PathBuf,
    paths: Vec<(String, Vec<String>)>,
) -> ResolvedCompilerOptions {
    let mut path_mappings = FxHashMap::default();
    for (pattern, targets) in paths {
        path_mappings.insert(pattern, targets);
    }

    let compiler_options = CompilerOptions {
        base_url: Some(base_url.to_string_lossy().into_owned()),
        module_resolution: Some("node16".to_string()),
        paths: Some(path_mappings),
        ..Default::default()
    };

    let mut resolved =
        resolve_compiler_options(Some(&compiler_options)).expect("valid test compiler options");
    resolved.module_resolution = Some(ModuleResolutionKind::Node16);
    resolved
}

fn resolve_for_test(
    from_file: &Path,
    module_specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let known_files = FxHashSet::default();
    resolve_module_specifier(
        from_file,
        module_specifier,
        options,
        base_dir,
        cache,
        &known_files,
    )
}

#[cfg(test)]
mod ts2307_path_mapping_tests {
    use super::*;

    #[test]
    fn test_path_mapping_to_nonexistent_file_emits_error() {
        // This test verifies that when a path mapping is configured but the target
        // file doesn't exist, resolve_module_specifier returns None (which triggers TS2307)

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create a path mapping: "@utils/*" -> "./utils/*"
        let paths = vec![("@utils/*".to_string(), vec!["./utils/*".to_string()])];

        let options = create_options_with_paths(base_url.clone(), paths);
        let mut cache = ModuleResolutionCache::default();

        // Try to resolve a module that matches the path mapping pattern
        // but the file doesn't exist
        let result = resolve_for_test(
            &base_url.join("src/index.ts"),
            "@utils/helper",
            &options,
            &base_url,
            &mut cache,
        );

        // Should return None (indicating TS2307 should be emitted)
        assert!(
            result.is_none(),
            "Path mapping to non-existent file should return None to trigger TS2307"
        );
    }

    #[test]
    fn test_path_mapping_to_existing_file_resolves() {
        // This test verifies that when a path mapping is configured AND the target
        // file exists, resolve_module_specifier returns the file path

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create the target file
        let utils_dir = temp_dir.path().join("utils");
        std::fs::create_dir_all(&utils_dir).unwrap();
        let helper_file = utils_dir.join("helper.ts");
        std::fs::write(&helper_file, "export function foo() {}").unwrap();

        // Create a path mapping: "@utils/*" -> "./utils/*"
        let paths = vec![("@utils/*".to_string(), vec!["./utils/*".to_string()])];

        let options = create_options_with_paths(base_url.clone(), paths);
        let mut cache = ModuleResolutionCache::default();

        // Try to resolve a module that matches the path mapping
        let result = resolve_for_test(
            &base_url.join("src/index.ts"),
            "@utils/helper",
            &options,
            &base_url,
            &mut cache,
        );

        // Should return Some(path) indicating successful resolution
        assert!(
            result.is_some(),
            "Path mapping to existing file should resolve successfully"
        );

        let resolved_path = result.unwrap();
        assert!(
            resolved_path.ends_with("utils/helper.ts") || resolved_path.ends_with("utils/helper"), // Extension might be added
            "Resolved path should point to the utils/helper.ts file"
        );
    }

    #[test]
    fn test_no_path_mapping_falls_through_to_node_modules() {
        // This test verifies that when NO path mapping matches, the resolver
        // falls through to other strategies (node_modules, etc.)

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create a path mapping for a different pattern
        let paths = vec![("@other/*".to_string(), vec!["./other/*".to_string()])];

        let options = create_options_with_paths(base_url.clone(), paths);
        let mut cache = ModuleResolutionCache::default();

        // Try to resolve a module that doesn't match the path mapping
        let _result = resolve_for_test(
            &base_url.join("src/index.ts"),
            "lodash", // Bare specifier, should try node_modules
            &options,
            &base_url,
            &mut cache,
        );

        // Should attempt node_modules resolution (likely return None in test environment)
        // The important thing is it doesn't return early with None from path mapping
        // We can't easily test the full behavior without setting up node_modules
    }

    #[test]
    fn test_exports_js_target_does_not_substitute_dts() {
        let temp_dir = create_test_structure();
        let base_dir = temp_dir.path().to_path_buf();

        let pkg_dir = base_dir.join("node_modules").join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::create_dir_all(base_dir.join("src")).unwrap();

        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name":"pkg","version":"0.0.1","exports":"./entrypoint.js"}"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("entrypoint.d.ts"), "export {};").unwrap();
        std::fs::write(base_dir.join("src/index.ts"), "import * as p from 'pkg';").unwrap();

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            resolve_package_json_exports: true,
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node16,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node16,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut cache = ModuleResolutionCache::default();
        let result = resolve_for_test(
            &base_dir.join("src/index.ts"),
            "pkg",
            &options,
            &base_dir,
            &mut cache,
        );

        assert!(
            result.is_none(),
            "exports target with .js should not substitute to .d.ts"
        );
    }

    #[test]
    fn test_relative_import_not_affected() {
        // This test verifies that relative imports still work correctly

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create a file
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let helper_file = src_dir.join("helper.ts");
        std::fs::write(&helper_file, "export function foo() {}").unwrap();

        let options = ResolvedCompilerOptions {
            base_url: Some(base_url.clone()),
            module_resolution: Some(ModuleResolutionKind::Node16),
            ..Default::default()
        };
        let mut cache = ModuleResolutionCache::default();

        // Try to resolve a relative import
        let result = resolve_for_test(
            &src_dir.join("index.ts"),
            "./helper",
            &options,
            &base_url,
            &mut cache,
        );

        // Should resolve successfully
        assert!(
            result.is_some(),
            "Relative import to existing file should resolve successfully"
        );
    }

    #[test]
    fn test_relative_import_to_nonexistent_emits_error() {
        // This test verifies that relative imports to non-existent files return None

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let options = ResolvedCompilerOptions {
            base_url: Some(base_url.clone()),
            module_resolution: Some(ModuleResolutionKind::Node16),
            ..Default::default()
        };
        let mut cache = ModuleResolutionCache::default();

        // Try to resolve a relative import to a non-existent file
        let result = resolve_for_test(
            &src_dir.join("index.ts"),
            "./nonexistent",
            &options,
            &base_url,
            &mut cache,
        );

        // Should return None (triggering TS2307)
        assert!(
            result.is_none(),
            "Relative import to non-existent file should return None to trigger TS2307"
        );
    }

    #[test]
    fn test_wildcard_path_mapping_substitution() {
        // This test verifies that wildcards in path mappings are correctly substituted

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create multiple target files
        let utils_dir = temp_dir.path().join("utils");
        std::fs::create_dir_all(&utils_dir).unwrap();

        for name in &["helper", "utils", "constants"] {
            let file = utils_dir.join(format!("{name}.ts"));
            std::fs::write(&file, format!("export function {name}() {{}}")).unwrap();
        }

        // Create a path mapping with wildcard: "@utils/*" -> "./utils/*"
        let paths = vec![("@utils/*".to_string(), vec!["./utils/*".to_string()])];

        let options = create_options_with_paths(base_url.clone(), paths);
        let mut cache = ModuleResolutionCache::default();

        // Test that the wildcard is correctly substituted
        for module_name in &["@utils/helper", "@utils/utils", "@utils/constants"] {
            let result = resolve_for_test(
                &base_url.join("src/index.ts"),
                module_name,
                &options,
                &base_url,
                &mut cache,
            );

            assert!(
                result.is_some(),
                "Path mapping with wildcard should resolve: {module_name}"
            );
        }
    }

    #[test]
    fn test_multiple_path_mapping_targets() {
        // This test verifies path mappings with multiple target patterns

        let temp_dir = create_test_structure();
        let base_url = temp_dir.path().to_path_buf();

        // Create files in both locations
        let src_dir = temp_dir.path().join("src");
        let lib_dir = temp_dir.path().join("lib");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&lib_dir).unwrap();

        let src_helper = src_dir.join("helper.ts");
        let lib_helper = lib_dir.join("helper.ts");
        std::fs::write(&src_helper, "// src version").unwrap();
        std::fs::write(&lib_helper, "// lib version").unwrap();

        // Create a path mapping with multiple targets
        let paths = vec![(
            "*".to_string(),
            vec!["./src/*".to_string(), "./lib/*".to_string()],
        )];

        let options = create_options_with_paths(base_url.clone(), paths);
        let mut cache = ModuleResolutionCache::default();

        // Should resolve to one of the targets (implementation-dependent which one)
        let result = resolve_for_test(
            &base_url.join("index.ts"),
            "helper",
            &options,
            &base_url,
            &mut cache,
        );

        assert!(
            result.is_some(),
            "Path mapping with multiple targets should resolve"
        );
    }
}

// Add these tests to the existing driver_tests.rs module
