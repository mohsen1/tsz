use super::matches_path_filter;
use std::path::Path;

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
