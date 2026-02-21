use std::fs;
use std::path::Path;

fn collect_typedata_aliases(source: &str) -> Vec<String> {
    let mut aliases = vec!["TypeData".to_string()];

    for statement in source.split(';') {
        let compact = statement.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some((_, alias_part)) = compact.split_once("TypeData as ") {
            let alias = alias_part
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            if !alias.is_empty() {
                aliases.push(alias);
            }
        }

        let trimmed = compact.trim_start();
        if let Some(rest) = trimmed.strip_prefix("type ")
            && let Some((alias, rhs)) = rest.split_once('=')
            && rhs.contains("TypeData")
        {
            let alias = alias.trim();
            if !alias.is_empty() {
                aliases.push(alias.to_string());
            }
        }
    }

    aliases.sort();
    aliases.dedup();
    aliases
}

fn compact_without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn has_raw_typedata_intern(source: &str, aliases: &[String]) -> bool {
    let compact = compact_without_whitespace(source);
    if compact.contains(".intern(TypeData::")
        || compact.contains(".intern(crate::types::TypeData::")
        || compact.contains(".intern(tsz_solver::TypeData::")
    {
        return true;
    }

    aliases
        .iter()
        .filter(|alias| alias.as_str() != "TypeData")
        .any(|alias| compact.contains(&format!(".intern({alias}::")))
}

#[test]
fn test_typedata_alias_scanner_detects_grouped_import_alias() {
    let source = r#"
use crate::types::{OtherType, TypeData as TD, Value};
type LocalAlias = crate::types::TypeData;
"#;
    let aliases = collect_typedata_aliases(source);
    assert!(aliases.iter().any(|alias| alias == "TD"));
    assert!(aliases.iter().any(|alias| alias == "LocalAlias"));
}

#[test]
fn test_multiline_intern_detection_catches_alias_based_raw_construction() {
    let source = r#"
use crate::types::{TypeData as TD};

fn bad(interner: &mut crate::intern::TypeInterner) {
    interner
        .intern(
            TD::ThisType,
        );
}
"#;
    let aliases = collect_typedata_aliases(source);
    assert!(has_raw_typedata_intern(source, &aliases));
}

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
        let aliases = collect_typedata_aliases(&source);
        if has_raw_typedata_intern(&source, &aliases) {
            violations.push(path.display().to_string());
        }
    }

    assert!(
        violations.is_empty(),
        "solver TypeData construction via .intern(TypeData::...) should be done only in intern.rs; violations: {}",
        violations.join(", ")
    );
}
