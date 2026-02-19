use std::fs;
use std::path::Path;

#[test]
fn test_direct_typedata_construction_is_quarantined_to_intern() {
    fn is_rs_source_file(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("rs")
    }

    fn collect_solver_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|_| panic!("failed to read solver source directory {}", dir.display()));

        for entry in entries {
            let entry = entry.expect("failed to read solver source directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_solver_rs_files_recursive(&path, files);
                continue;
            }
            if is_rs_source_file(&path) {
                files.push(path);
            }
        }
    }

    let solver_src_dir = Path::new("src");
    let mut source_files = Vec::new();
    collect_solver_rs_files_recursive(solver_src_dir, &mut source_files);

    let mut violations = Vec::new();
    for path in source_files {
        if path.ends_with("src/intern.rs")
            || path.ends_with("src/intern_intersection.rs")
            || path.ends_with("src/intern_normalize.rs")
            || path.ends_with("src/intern_template.rs")
        {
            continue;
        }
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains(".intern(TypeData::") {
                violations.push(format!("{}:{}", path.display(), line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "solver TypeData construction via .intern(TypeData::...) should be done only in intern.rs; violations: {}",
        violations.join(", ")
    );
}
