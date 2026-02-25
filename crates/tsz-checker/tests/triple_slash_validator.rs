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
    // Verify path offset points to the value start (after the opening quote)
    // `/// <reference path="./types.d.ts" />` — value starts at byte offset 21
    assert_eq!(refs[0].2, 21); // offset of value start in line
    assert_eq!(refs[1].2, 21); // same structure, single quotes
}

#[test]
fn test_extract_reference_paths_offset() {
    // Column positions should point at the value after the opening quote
    let source =
        "/// <reference path='filedoesnotexist.ts'/>\n/// <reference path=\"other.d.ts\"/>\n";
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 2);

    // "/// <reference path='" = 21 chars, so value starts at offset 21
    assert_eq!(refs[0].0, "filedoesnotexist.ts");
    assert_eq!(refs[0].2, 21); // quote_offset: byte offset of value start

    assert_eq!(refs[1].0, "other.d.ts");
    assert_eq!(refs[1].2, 21);
}

#[test]
fn test_extract_reference_paths_with_leading_whitespace() {
    let source = "    /// <reference path='test.ts'/>\n";
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].0, "test.ts");
    // 4 spaces + "/// <reference path='" = 4 + 21 = 25
    assert_eq!(refs[0].2, 25);
}

#[test]
fn test_extract_no_references() {
    let source = "const x = 1;\n// regular comment\n";
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 0);
}

#[test]
fn test_extract_quoted_attr_basic() {
    assert_eq!(
        extract_quoted_attr(r#"path="./file.ts""#, "path"),
        Some("./file.ts".to_string())
    );
    assert_eq!(
        extract_quoted_attr(r"path='./file.ts'", "path"),
        Some("./file.ts".to_string())
    );
    assert_eq!(
        extract_quoted_attr(r#"  path  =  "./file.ts"  "#, "path"),
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

#[test]
fn test_triple_slash_in_block_comment_ignored() {
    // Triple-slash references inside block comments should NOT be parsed.
    let source = r#"/*
/// <reference path="non-existing-file.d.ts" />
*/
void 0;"#;
    let refs = extract_reference_paths(source);
    assert_eq!(
        refs.len(),
        0,
        "references inside block comments should be ignored"
    );

    let types = extract_reference_types(source);
    assert_eq!(types.len(), 0);

    let amd = extract_amd_module_names(source);
    assert_eq!(amd.len(), 0);
}

#[test]
fn test_triple_slash_after_block_comment_still_works() {
    let source = r#"/* comment */
/// <reference path="real-file.d.ts" />
void 0;"#;
    let refs = extract_reference_paths(source);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].0, "real-file.d.ts");
}
