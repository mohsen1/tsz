use std::path::PathBuf;

use tsz_conformance::cache::cache_key;

#[test]
fn test_cache_key() {
    let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
    let path = PathBuf::from("/repo/TypeScript/tests/cases/compiler/foo.ts");
    assert_eq!(
        cache_key(&path, &test_dir),
        Some("compiler/foo.ts".to_string())
    );
}

#[test]
fn test_cache_key_nested() {
    let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
    let path = PathBuf::from("/repo/TypeScript/tests/cases/conformance/types/intersection/bar.ts");
    assert_eq!(
        cache_key(&path, &test_dir),
        Some("conformance/types/intersection/bar.ts".to_string())
    );
}

#[test]
fn test_cache_key_outside_test_dir() {
    let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
    let path = PathBuf::from("/somewhere/else/foo.ts");
    assert_eq!(cache_key(&path, &test_dir), None);
}
