use std::path::Path;

const CONFORMANCE_SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts"];

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn has_conformance_source_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| CONFORMANCE_SOURCE_EXTENSIONS.contains(&ext))
}

fn has_declaration_extension(path: &Path) -> bool {
    let normalized = normalized_path(path).to_ascii_lowercase();
    normalized.ends_with(".d.ts")
        || normalized.ends_with(".d.mts")
        || normalized.ends_with(".d.cts")
}

pub fn skipped_conformance_source_reason(path: &Path) -> Option<&'static str> {
    let normalized = normalized_path(path);
    if format!("/{normalized}").contains("/fourslash/") {
        return Some("fourslash");
    }
    if normalized.contains("APISample") || normalized.contains("APILibCheck") {
        return Some("api-sample");
    }
    if has_declaration_extension(path) {
        return Some("declaration");
    }
    None
}

pub fn is_conformance_source_file(path: &Path) -> bool {
    has_conformance_source_extension(path) && skipped_conformance_source_reason(path).is_none()
}

pub fn matches_path_filter(_path: &Path, _filter: Option<&str>) -> bool {
    let Some(filter) = _filter else {
        return true;
    };
    normalized_path(_path).contains(filter)
}

#[cfg(test)]
mod tests {
    use super::{
        is_conformance_source_file, matches_path_filter, skipped_conformance_source_reason,
    };
    use std::path::Path;

    #[test]
    fn test_matches_path_filter_normalizes_windows_separators() {
        assert!(matches_path_filter(
            Path::new(r"cases\moduleResolution\foo.ts"),
            Some("moduleResolution")
        ));
    }

    #[test]
    fn conformance_sources_include_script_and_module_extensions() {
        for path in [
            "tests/cases/compiler/foo.ts",
            "tests/cases/compiler/foo.tsx",
            "tests/cases/compiler/foo.js",
            "tests/cases/compiler/foo.jsx",
            "tests/cases/compiler/foo.mts",
            "tests/cases/compiler/foo.cts",
        ] {
            assert!(is_conformance_source_file(Path::new(path)), "{path}",);
        }
    }

    #[test]
    fn conformance_sources_exclude_upstream_harness_specific_files() {
        for path in [
            "tests/cases/fourslash/quickInfo.ts",
            "tests/cases/compiler/APISample_compile.ts",
            "tests/cases/compiler/APILibCheck.ts",
            "tests/cases/compiler/lib.d.ts",
            "tests/cases/compiler/lib.d.mts",
            "tests/cases/compiler/lib.d.cts",
        ] {
            assert!(
                skipped_conformance_source_reason(Path::new(path)).is_some(),
                "{path}",
            );
            assert!(!is_conformance_source_file(Path::new(path)), "{path}",);
        }
    }
}
