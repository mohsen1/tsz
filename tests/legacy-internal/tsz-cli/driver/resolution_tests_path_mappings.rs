use super::*;

#[test]
fn test_resolve_module_specifier_classic_path_mapping_falls_back_to_root() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert(
        "*".to_string(),
        vec!["*".to_string(), "generated/*".to_string()],
    );
    let compiler_options = CompilerOptions {
        base_url: Some("c:/root".to_string()),
        paths: Some(raw_paths),
        module: Some("amd".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");
    tracing::debug!(
        "resolved options: base_url={:?} paths={:?} resolution={:?}",
        options.base_url,
        options
            .paths
            .as_ref()
            .map(|paths| paths.iter().map(|m| m.pattern.clone()).collect::<Vec<_>>()),
        options.effective_module_resolution()
    );

    let base = PathBuf::from("/tmp/tsz-test-absolute");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("c:/root/folder2/file1.ts"));
    known_files.insert(base.join("c:/root/generated/folder3/file2.ts"));
    known_files.insert(base.join("c:/root/shared/components/file3.ts"));
    known_files.insert(base.join("c:/file4.ts"));
    known_files.insert(base.join("c:/root/folder1/file1.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &base.join("c:/root/folder1/file1.ts"),
        "file4",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(base.join("c:/file4.ts")),
        "classic path-mapping fallback should resolve file4 to c:/file4.ts"
    );
}

#[test]
fn test_resolve_module_specifier_paths_without_base_url_use_project_base() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert("foo/*".to_string(), vec!["./dist/*".to_string()]);
    raw_paths.insert("baz/*.ts".to_string(), vec!["./types/*.d.ts".to_string()]);
    let compiler_options = CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("bundler".to_string()),
        module: Some("es2015".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let base = PathBuf::from("/tmp/tsz-test-paths-without-baseurl");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("dist/bar.ts"));
    known_files.insert(base.join("types/main.d.ts"));

    let mut cache = ModuleResolutionCache::default();
    let foo = resolve_module_specifier(
        &base.join("test.ts"),
        "foo/bar.ts",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(foo, Some(base.join("dist/bar.ts")));

    let baz = resolve_module_specifier(
        &base.join("test.ts"),
        "baz/main.ts",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(baz, Some(base.join("types/main.d.ts")));
}

#[test]
fn test_path_mapping_selection_cache_preserves_sorted_precedence() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert("*".to_string(), vec!["fallback/*".to_string()]);
    raw_paths.insert("@scope/pkg/*".to_string(), vec!["wildcard/*".to_string()]);
    raw_paths.insert(
        "@scope/pkg/foo".to_string(),
        vec!["exact/foo.ts".to_string()],
    );
    for i in 0..64 {
        raw_paths.insert(format!("@scope/pkg-{i}/*"), vec![format!("pkg-{i}/*")]);
    }

    let compiler_options = CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("bundler".to_string()),
        module: Some("es2015".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let base = PathBuf::from("/tmp/tsz-test-path-mapping-cache");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("exact/foo.ts"));
    known_files.insert(base.join("wildcard/foo.ts"));
    known_files.insert(base.join("fallback/@scope/pkg/foo.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &base.join("src/main.ts"),
        "@scope/pkg/foo",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(resolved, Some(base.join("exact/foo.ts")));
    let cached = cache
        .path_mapping_by_specifier
        .get("@scope/pkg/foo")
        .and_then(Option::as_ref)
        .expect("path mapping selection should be cached");
    assert_eq!(
        options.paths.as_ref().unwrap()[cached.0].pattern,
        "@scope/pkg/foo",
        "exact mapping should win over wildcard mappings before caching"
    );

    let resolved_again = resolve_module_specifier(
        &base.join("src/other.ts"),
        "@scope/pkg/foo",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(resolved_again, Some(base.join("exact/foo.ts")));
    assert_eq!(cache.path_mapping_by_specifier.len(), 1);
}

#[test]
fn test_resolve_module_specifier_root_dirs_overlay() {
    let base = PathBuf::from("/tmp/tsz-test-rootdirs");
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        root_dirs: vec![base.join("src"), base.join("generated")],
        ..Default::default()
    };

    let mut known_files = FxHashSet::default();
    known_files.insert(base.join("generated/generated.ts"));
    let mut cache = ModuleResolutionCache::default();

    let resolved = resolve_module_specifier(
        &base.join("src/main.ts"),
        "./generated",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(resolved, Some(base.join("generated/generated.ts")));
}

#[test]
fn test_resolve_module_specifier_classic_path_mapping_absolute_target_fallback() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert(
        "*".to_string(),
        vec!["*".to_string(), "c:/shared/*".to_string()],
    );
    raw_paths.insert(
        "templates/*".to_string(),
        vec!["generated/src/templates/*".to_string()],
    );

    let compiler_options = CompilerOptions {
        base_url: Some("c:/root/src".to_string()),
        paths: Some(raw_paths),
        module: Some("amd".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(PathBuf::from("c:/root/src/file3.d.ts"));
    known_files.insert(PathBuf::from("c:/shared/module1.d.ts"));
    known_files.insert(PathBuf::from("c:/root/generated/src/templates/module2.ts"));
    known_files.insert(PathBuf::from("c:/module3.d.ts"));
    known_files.insert(PathBuf::from("c:/root/src/file1.ts"));
    known_files.insert(PathBuf::from("c:/root/generated/src/project/file2.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &PathBuf::from("c:/root/src/file1.ts"),
        "module3",
        &options,
        &PathBuf::from("c:/root/src"),
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(PathBuf::from("c:/module3.d.ts")),
        "absolute path mapping fallback should prefer shared module declarations"
    );
}
