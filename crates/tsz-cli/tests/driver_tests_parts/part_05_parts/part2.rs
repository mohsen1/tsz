#[test]
fn compile_with_cache_invalidates_dependents() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);

    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);
}

#[test]
fn invalidate_paths_with_dependents_symbols_keeps_unrelated_cache() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const local = 1; export const uses = value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(before > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    let after = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(after > 0);
    assert!(after < before);
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(0), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export { value } from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_import_equals() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import util = require('./util'); export const local = util.value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_namespace_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * as util from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_star_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}
