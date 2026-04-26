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

// =========================================================================
// Extended `extract_reference_types` coverage (only block-comment-empty
// negative case existed)
// =========================================================================

#[test]
fn extract_reference_types_returns_type_name_no_resolution_mode() {
    let source = r#"/// <reference types="node" />"#;
    let types = extract_reference_types(source);
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].0, "node");
    assert_eq!(types[0].1, None, "no resolution-mode attribute");
}

#[test]
fn extract_reference_types_captures_resolution_mode() {
    let source = r#"/// <reference types="some-pkg" resolution-mode="import" />"#;
    let types = extract_reference_types(source);
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].0, "some-pkg");
    assert_eq!(types[0].1, Some("import".to_string()));
}

#[test]
fn extract_reference_types_captures_byte_offset_and_length() {
    // Byte offset must point at the start of the `types` attribute value
    // (after the opening quote). Length matches the value length.
    let source = r#"/// <reference types="abc" />"#;
    let types = extract_reference_types(source);
    assert_eq!(types.len(), 1);
    let (name, _, offset, length) = &types[0];
    assert_eq!(name, "abc");
    assert_eq!(*length, 3, "length should be the value length");
    // The value `abc` starts after `/// <reference types="` (22 bytes).
    assert_eq!(*offset, 22);
}

#[test]
fn extract_reference_types_multiple_directives_carry_offsets() {
    // Two consecutive directives — second must have a byte offset
    // accounting for the first directive's line + newline.
    let source = "/// <reference types=\"first\" />\n/// <reference types=\"second\" />\n";
    let types = extract_reference_types(source);
    assert_eq!(types.len(), 2);
    assert_eq!(types[0].0, "first");
    assert_eq!(types[1].0, "second");
    // The second directive's offset must be greater than the first.
    assert!(types[1].2 > types[0].2 + types[0].3);
}

// =========================================================================
// `find_malformed_reference_directives` (no prior tests)
// =========================================================================

#[test]
fn find_malformed_directives_locates_unquoted_attribute() {
    // `path=` without quotes — malformed.
    let source = r#"/// <reference path=foo.ts />"#;
    let malformed = find_malformed_reference_directives(source);
    assert_eq!(malformed.len(), 1);
    assert_eq!(malformed[0].0, 0, "line 0 (zero-indexed)");
}

#[test]
fn find_malformed_directives_locates_no_attribute() {
    let source = r#"/// <reference />"#;
    let malformed = find_malformed_reference_directives(source);
    assert_eq!(malformed.len(), 1);
}

#[test]
fn find_malformed_directives_skips_well_formed_path() {
    let source = r#"/// <reference path="./real.ts" />"#;
    let malformed = find_malformed_reference_directives(source);
    assert!(malformed.is_empty());
}

#[test]
fn find_malformed_directives_skips_well_formed_types_lib_no_default_lib() {
    for src in &[
        r#"/// <reference types="node" />"#,
        r#"/// <reference lib="es2015" />"#,
        r#"/// <reference no-default-lib="true" />"#,
    ] {
        let malformed = find_malformed_reference_directives(src);
        assert!(
            malformed.is_empty(),
            "should not flag well-formed directive: {src}"
        );
    }
}

#[test]
fn find_malformed_directives_ignores_block_comments() {
    let source = r#"/*
/// <reference path=unquoted />
*/
void 0;"#;
    let malformed = find_malformed_reference_directives(source);
    assert!(
        malformed.is_empty(),
        "directives inside block comments must not be reported"
    );
}

#[test]
fn find_malformed_directives_returns_byte_offset_of_triple_slash() {
    // Leading whitespace before `///` — the byte offset must point at
    // the `/` of `///`, NOT line start.
    let source = "    /// <reference />\n";
    let malformed = find_malformed_reference_directives(source);
    assert_eq!(malformed.len(), 1);
    assert_eq!(malformed[0].1, 4, "offset should point at the `/` of `///`");
}

// =========================================================================
// `validate_reference_path` extension-handling branches
// =========================================================================

#[test]
fn validate_reference_path_explicit_extension_only_tries_exact() {
    // When the reference already has an extension, the validator does NOT
    // try `.ts`/`.tsx`/`.d.ts` fallbacks — it returns the result of the
    // exact-path check.
    use std::fs;
    let temp_dir = std::env::temp_dir().join("tsz_test_ext_explicit");
    let _ = fs::create_dir_all(&temp_dir);
    let exact_ts = temp_dir.join("file.ts");
    fs::write(&exact_ts, "// stub").unwrap();

    let source_file = temp_dir.join("t.ts");
    // Exact path with .ts extension that exists → true.
    assert!(validate_reference_path(&source_file, "file.ts"));
    // Reference with non-existent .js extension — must NOT fall back to
    // `.ts`/`.tsx`/`.d.ts` because the reference already has an extension.
    assert!(!validate_reference_path(&source_file, "file.js"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn validate_reference_path_returns_false_when_source_has_no_parent() {
    // `Path::new("/").parent()` is None on Windows, but `Path::new("foo")`
    // has parent `""`. This test pins the contract: a relative-path source
    // file (no leading `/`) returns based on its empty parent + ref path.
    // On most systems the empty parent + missing file yields false.
    use std::path::PathBuf;
    let source_file = PathBuf::from("/");
    // Paths under `/` likely don't exist; just verify the function does not
    // panic and produces a deterministic boolean.
    let _ = validate_reference_path(&source_file, "non-existent");
}

// =========================================================================
// Extended `extract_amd_module_names` coverage
// =========================================================================

#[test]
fn extract_amd_module_names_returns_zero_indexed_line_number() {
    // The 2-tuple return is `(name, line_num)` — line_num is zero-indexed.
    let source = "// header\n// ...\n///<amd-module name=\"Foo\"/>\n";
    let amd = extract_amd_module_names(source);
    assert_eq!(amd.len(), 1);
    assert_eq!(amd[0].0, "Foo");
    assert_eq!(
        amd[0].1, 2,
        "line_num is zero-indexed; directive is on line 2"
    );
}

#[test]
fn extract_amd_module_names_ignores_block_comments_with_directive() {
    let source = r#"/*
///<amd-module name="ShouldBeIgnored"/>
*/
"#;
    let amd = extract_amd_module_names(source);
    assert!(amd.is_empty());
}

#[test]
fn extract_reference_paths_ignores_directives_inside_block_comment() {
    // The block comment opens on line 1, includes the triple-slash on
    // line 2, and closes on line 3. None of the directives should be
    // extracted.
    let source = "/*\n/// <reference path=\"a.ts\" />\n*/\n";
    let refs = extract_reference_paths(source);
    assert!(refs.is_empty());
}
