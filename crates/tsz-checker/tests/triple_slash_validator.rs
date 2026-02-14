use super::*;

#[test]
fn test_extract_reference_paths() {
    let source = r#"
/// <reference path="./types.d.ts" />
/// <reference path='./other.ts' />
const x = 1;
"#;
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].0, "./types.d.ts");
    assert_eq!(refs[1].0, "./other.ts");
}

#[test]
fn test_extract_no_references() {
    let source = "const x = 1;\n// regular comment\n";
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 0);
}

#[test]
fn test_extract_quoted_path() {
    assert_eq!(
        extract_quoted_path(r#"path="./file.ts""#),
        Some("./file.ts".to_string())
    );
    assert_eq!(
        extract_quoted_path(r#"path='./file.ts'"#),
        Some("./file.ts".to_string())
    );
    assert_eq!(
        extract_quoted_path(r#"  path  =  "./file.ts"  "#),
        Some("./file.ts".to_string())
    );
}

#[test]
fn test_validate_extensionless_references() {
    use std::fs;

    // Create a temporary directory with test files
    let temp_dir = std::env::temp_dir().join("tsz_test_refs");
    let _ = fs::create_dir_all(&temp_dir);

    // Create test files
    let a_ts = temp_dir.join("a.ts");
    let b_dts = temp_dir.join("b.d.ts");
    let c_ts = temp_dir.join("c.ts");

    fs::write(&a_ts, "var aa = 1;").unwrap();
    fs::write(&b_dts, "declare var bb: number;").unwrap();
    fs::write(&c_ts, "var cc = 1;").unwrap();

    let source_file = temp_dir.join("t.ts");

    // Test extension-less references
    assert!(
        validate_reference_path(&source_file, "a"),
        "Should find a.ts"
    );
    assert!(
        validate_reference_path(&source_file, "b"),
        "Should find b.d.ts"
    );
    assert!(
        validate_reference_path(&source_file, "c"),
        "Should find c.ts"
    );
    assert!(
        !validate_reference_path(&source_file, "missing"),
        "Should not find missing file"
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_extract_amd_module_names() {
    let source = r#"
///<amd-module name="FirstModuleName"/>
///<amd-module name='SecondModuleName'/>
class Foo {}
"#;
    let amd_modules = extract_amd_module_names(source);
    assert_eq!(amd_modules.len(), 2);
    assert_eq!(amd_modules[0].0, "FirstModuleName");
    assert_eq!(amd_modules[1].0, "SecondModuleName");
}

#[test]
fn test_extract_amd_module_names_no_duplicates() {
    let source = r#"
///<amd-module name="ModuleName"/>
class Foo {}
"#;
    let amd_modules = extract_amd_module_names(source);
    assert_eq!(amd_modules.len(), 1);
    assert_eq!(amd_modules[0].0, "ModuleName");
}
