use std::path::Path;

use tsz_conformance::test_filter::matches_path_filter;

#[test]
fn test_matches_path_filter() {
    assert!(matches_path_filter(
        Path::new("cases/moduleResolution/foo.ts"),
        None
    ));
    assert!(matches_path_filter(
        Path::new("cases/moduleResolution/foo.ts"),
        Some("moduleResolution")
    ));
    assert!(!matches_path_filter(
        Path::new("cases/other/foo.ts"),
        Some("moduleResolution")
    ));
}
