use super::*;

#[test]
fn test_expand_option_variants_cartesian() {
    let mut options = HashMap::new();
    options.insert(
        "moduleresolution".to_string(),
        "node16, nodenext".to_string(),
    );
    options.insert("module".to_string(), "commonjs, node16".to_string());
    options.insert("traceResolution".to_string(), "true".to_string());

    let expanded = expand_option_variants(&options);
    assert_eq!(expanded.len(), 4);

    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    for opts in expanded {
        let mr = opts.get("moduleresolution").cloned().unwrap_or_default();
        let m = opts.get("module").cloned().unwrap_or_default();
        let tr = opts.get("traceResolution").cloned().unwrap_or_default();
        seen.insert((mr, m, tr));
    }

    assert!(seen.contains(&(
        "node16".to_string(),
        "commonjs".to_string(),
        "true".to_string()
    )));
    assert!(seen.contains(&(
        "node16".to_string(),
        "node16".to_string(),
        "true".to_string()
    )));
    assert!(seen.contains(&(
        "nodenext".to_string(),
        "commonjs".to_string(),
        "true".to_string()
    )));
    assert!(seen.contains(&(
        "nodenext".to_string(),
        "node16".to_string(),
        "true".to_string()
    )));
}

#[test]
fn test_filter_incompatible_module_resolution_variants() {
    let mut options = HashMap::new();
    options.insert(
        "moduleresolution".to_string(),
        "node16, nodenext".to_string(),
    );
    options.insert(
        "module".to_string(),
        "commonjs, node16, nodenext".to_string(),
    );

    let variants = expand_option_variants(&options);
    let filtered = filter_incompatible_module_resolution_variants(variants);
    assert_eq!(filtered.len(), 2);

    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for opts in filtered {
        let mr = opts.get("moduleresolution").cloned().unwrap_or_default();
        let m = opts.get("module").cloned().unwrap_or_default();
        seen.insert((mr, m));
    }

    assert!(seen.contains(&("node16".to_string(), "node16".to_string())));
    assert!(seen.contains(&("nodenext".to_string(), "nodenext".to_string())));
}

#[test]
fn test_parse_simple_directives() {
    let content = r#"
// @strict: true
// @target: es5
function foo() {}
"#;
    let parsed = parse_test_file(content).unwrap();
    assert_eq!(
        parsed.directives.options.get("strict"),
        Some(&"true".to_string())
    );
    assert_eq!(
        parsed.directives.options.get("target"),
        Some(&"es5".to_string())
    );
}

#[test]
fn test_parse_multi_value_directive() {
    let content = "// @lib: es6, dom\nfunction foo() {}";
    let parsed = parse_test_file(content).unwrap();
    assert_eq!(
        parsed.directives.options.get("lib"),
        Some(&"es6, dom".to_string())
    );
}

#[test]
fn test_directive_keys_are_lowercased() {
    let content = "// @Target: ES6\n// @Strict: true\nfunction foo() {}";
    let parsed = parse_test_file(content).unwrap();
    assert_eq!(
        parsed.directives.options.get("target"),
        Some(&"ES6".to_string())
    );
    assert_eq!(
        parsed.directives.options.get("strict"),
        Some(&"true".to_string())
    );
    // Original casing keys should NOT exist
    assert!(!parsed.directives.options.contains_key("Target"));
    assert!(!parsed.directives.options.contains_key("Strict"));
}

#[test]
fn test_bom_directive_on_first_line() {
    // UTF-8 BOM followed by directive on first line
    let content = "\u{FEFF}// @strict: true\n// @module: es2015\nfunction foo() {}";
    let parsed = parse_test_file(content).unwrap();
    assert_eq!(
        parsed.directives.options.get("strict"),
        Some(&"true".to_string()),
        "First-line directive after BOM should be parsed"
    );
    assert_eq!(
        parsed.directives.options.get("module"),
        Some(&"es2015".to_string()),
    );
}

#[test]
fn test_parse_multi_file() {
    let content = r#"
// @filename: file1.ts
export const x: number = 1;
// @filename: file2.ts
import { x } from './file1';
console.log(x);
"#;
    let parsed = parse_test_file(content).unwrap();
    assert_eq!(parsed.directives.filenames.len(), 2);
    assert_eq!(parsed.directives.filenames[0].0, "file1.ts");
    assert_eq!(parsed.directives.filenames[1].0, "file2.ts");
}
