use std::path::Path;

pub fn matches_path_filter(_path: &Path, _filter: Option<&str>) -> bool {
    let Some(filter) = _filter else {
        return true;
    };
    let normalized = _path.to_string_lossy().replace('\\', "/");
    normalized.contains(filter)
}

#[cfg(test)]
mod tests {
    use super::matches_path_filter;
    use std::path::Path;

    #[test]
    fn test_matches_path_filter_normalizes_windows_separators() {
        assert!(matches_path_filter(
            Path::new(r"cases\moduleResolution\foo.ts"),
            Some("moduleResolution")
        ));
    }
}
