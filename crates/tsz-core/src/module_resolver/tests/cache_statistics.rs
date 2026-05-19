//! Module resolver cache visibility and accounting tests.

use std::path::{Path, PathBuf};

use super::super::*;
use crate::module_resolver_helpers::PackageJson;

#[test]
fn test_resolver_cache_statistics_cover_owned_caches() {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let before = resolver.cache_statistics();

    assert_eq!(before.total_entries(), 0);
    assert_eq!(before.estimated_size_bytes(), 0);
    assert_eq!(resolver.cache_estimated_size_bytes(), 0);

    resolver.resolution_cache.insert(
        (
            PathBuf::from("/repo/src"),
            "./dep".to_string(),
            ImportingModuleKind::CommonJs,
            ImportKind::CjsRequire,
        ),
        Ok(ResolvedModule {
            resolved_path: PathBuf::from("/repo/src/dep.ts"),
            resolved_using_ts_extension: false,
            is_external: false,
            package_name: None,
            original_specifier: "./dep".to_string(),
            extension: ModuleExtension::Ts,
        }),
    );
    resolver
        .package_type_cache
        .borrow_mut()
        .insert(PathBuf::from("/repo"), Some(PackageType::Module));
    resolver.package_json_cache.borrow_mut().insert(
        PathBuf::from("/repo/package.json"),
        Ok(PackageJson::default()),
    );
    resolver.skip_fallback_cache.borrow_mut().insert(
        (
            PathBuf::from("/repo/src"),
            "pkg".to_string(),
            ImportingModuleKind::Esm,
        ),
        true,
    );
    resolver
        .node_modules_dir_cache
        .borrow_mut()
        .insert(PathBuf::from("/repo/node_modules"), true);

    let after = resolver.cache_statistics();
    assert_eq!(after.resolution_cache_entries, 1);
    assert_eq!(after.package_type_cache_entries, 1);
    assert_eq!(after.package_json_cache_entries, 1);
    assert_eq!(after.skip_fallback_cache_entries, 1);
    assert_eq!(after.node_modules_dir_cache_entries, 1);
    assert_eq!(after.total_entries(), 5);
    assert!(after.estimated_size_bytes() > before.estimated_size_bytes());
    assert!(resolver.cache_estimated_size_bytes() >= after.estimated_size_bytes());

    let resolved = resolver
        .resolve_with_kind(
            "./dep",
            Path::new("/repo/src/main.ts"),
            Span::new(0, 7),
            ImportKind::CjsRequire,
        )
        .expect("seeded resolution cache should return inserted result");
    assert_eq!(resolved.resolved_path, PathBuf::from("/repo/src/dep.ts"));
    assert_eq!(
        resolver
            .get_package_type_for_dir(Path::new("/repo"))
            .expect("seeded package type cache should return inserted result"),
        PackageType::Module
    );
    assert!(
        resolver
            .read_package_json(Path::new("/repo/package.json"))
            .is_ok()
    );
    assert!(resolver.should_skip_fallback_on_not_found(
        "pkg",
        Path::new("/repo/src"),
        ImportingModuleKind::Esm
    ));

    let counters = resolver.cache_statistics();
    assert_eq!(counters.resolution_cache_hits, 1);
    assert_eq!(counters.package_type_cache_hits, 1);
    assert_eq!(counters.package_json_cache_hits, 1);
    assert_eq!(counters.skip_fallback_cache_hits, 1);

    resolver.clear_cache();
    let cleared = resolver.cache_statistics();
    assert_eq!(cleared.total_entries(), 0);
    assert_eq!(cleared.resolution_cache_hits, 0);
    assert_eq!(cleared.package_type_cache_hits, 0);
    assert_eq!(cleared.package_json_cache_hits, 0);
    assert_eq!(cleared.skip_fallback_cache_hits, 0);
}
