use super::*;

#[test]
fn directive_line_basic_forms() {
    let d = parse_directive_line("// @strict: true").unwrap();
    assert_eq!(d.key, "strict");
    assert_eq!(d.value, "true");
    assert_eq!(d.raw_value, " true");
    assert_eq!(&"// @strict: true"[..d.prefix_len], "// @strict:");

    let d = parse_directive_line("  //@Filename:  a.ts ").unwrap();
    assert!(d.key_is("filename"));
    assert_eq!(d.key_lower(), "filename");
    assert_eq!(d.value, "a.ts");
}

#[test]
fn directive_line_prefix_reconstructs_line() {
    for line in [
        "// @filename: a.ts",
        "\t //@FileName:b/c.ts  ",
        "// @lib : es5,dom",
    ] {
        let d = parse_directive_line(line).unwrap();
        assert_eq!(format!("{}{}", &line[..d.prefix_len], d.raw_value), line);
    }
}

#[test]
fn flag_directive_line_forms() {
    assert_eq!(parse_flag_directive_line("// @ts-check"), Some("ts-check"));
    assert_eq!(
        parse_flag_directive_line("  //@ts-nocheck  "),
        Some("ts-nocheck")
    );
    assert_eq!(parse_flag_directive_line("// @ts-check: true"), None);
    assert_eq!(parse_flag_directive_line("// @ts-check x"), None);
    assert_eq!(parse_flag_directive_line("// @strict: true"), None);
}

#[test]
fn filename_directive_is_case_insensitive() {
    let filename = |line| {
        parse_directive_line(line)
            .filter(|d| d.key_is("filename"))
            .map(|d| d.value)
    };
    assert_eq!(filename("// @filename: a.ts"), Some("a.ts"));
    assert_eq!(filename("// @Filename: b.ts"), Some("b.ts"));
    assert_eq!(filename("//@FILENAME: /x/c.ts"), Some("/x/c.ts"));
    assert_eq!(filename("// @file: a.ts"), None);
}

#[test]
fn list_and_bool_value_helpers() {
    assert_eq!(
        split_list_values("es5, dom ,,es2015.core").collect::<Vec<_>>(),
        vec!["es5", "dom", "es2015.core"]
    );
    assert_eq!(first_list_value("es2015,es5"), "es2015");
    assert_eq!(first_list_value(""), "");
    assert_eq!(parse_bool_value("true, false"), Some(true));
    assert_eq!(parse_bool_value("false;"), Some(false));
    assert_eq!(parse_bool_value("TRUE"), None);
}

#[test]
fn parse_test_file_splits_files_and_options() {
    let content = "\u{FEFF}// @strict: true\n// @target: es5\n// @strict: false\n// @ts-check\n// @ts-nocheck\nfunction foo() {}\n";
    let parsed = parse_test_file(content);
    assert_eq!(
        parsed.option_order,
        vec!["strict", "target", "checkjs"],
        "first-seen order with lowercased keys"
    );
    assert_eq!(
        parsed.options.get("strict").map(String::as_str),
        Some("false"),
        "last duplicate wins"
    );
    assert_eq!(
        parsed.options.get("checkjs").map(String::as_str),
        Some("false")
    );
    assert!(parsed.filenames.is_empty());
}

#[test]
fn parse_test_file_strips_one_harness_trailing_semicolon() {
    let parsed = parse_test_file("// @declaration: true;\nconst x = 1;");
    assert_eq!(
        parsed.options.get("declaration").map(String::as_str),
        Some("true")
    );
}

#[test]
fn parse_test_file_multi_file_sections() {
    let content = "// @module: esnext\n// @Filename: a.ts\nexport const a = 1;\n// @ts-check\n// @filename: dir/b.ts\nimport { a } from './a';\na;\n";
    let parsed = parse_test_file(content);
    assert_eq!(
        parsed.options.get("module").map(String::as_str),
        Some("esnext")
    );
    assert_eq!(
        parsed.options.get("checkjs").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        parsed.filenames,
        vec![
            (
                "a.ts".to_string(),
                "export const a = 1;\n// @ts-check".to_string()
            ),
            (
                "dir/b.ts".to_string(),
                "import { a } from './a';\na;".to_string()
            ),
        ],
        "ts-check kept as content inside filename sections; directive lines removed"
    );
}

#[test]
fn parse_test_file_discards_preamble_before_first_filename() {
    let content = "// @target: es2015\n\n// @filename: a.ts\nnamespace foo {}\n";
    let parsed = parse_test_file(content);
    assert_eq!(
        parsed.filenames,
        vec![("a.ts".to_string(), "namespace foo {}".to_string())]
    );
}

#[test]
fn parse_test_file_accepts_cr_only_line_endings() {
    let content = "\u{FEFF}//@target: es6\r\r// newlines are <CR>\r`\r\\r`";
    let parsed = parse_test_file(content);
    assert_eq!(
        parsed.options.get("target").map(String::as_str),
        Some("es6")
    );
    assert!(
        parsed.filenames.is_empty(),
        "no virtual files declared in the CR-only fixture"
    );
}

#[test]
fn parse_test_file_drops_other_flag_lines() {
    let content = "// @filename: a.ts\n// @internal\nexport const a = 1;\n";
    let parsed = parse_test_file(content);
    assert_eq!(
        parsed.filenames,
        vec![("a.ts".to_string(), "export const a = 1;".to_string())]
    );
}

mod spec_vectors {
    use super::super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct SpecVectors {
        directive_lines: Vec<DirectiveCase>,
        flag_lines: Vec<FlagCase>,
        list_values: Vec<ListCase>,
        bool_values: Vec<BoolCase>,
    }

    #[derive(Deserialize)]
    struct DirectiveCase {
        line: String,
        key: Option<String>,
        #[serde(default)]
        value: Option<String>,
    }

    #[derive(Deserialize)]
    struct FlagCase {
        line: String,
        name: Option<String>,
    }

    #[derive(Deserialize)]
    struct ListCase {
        value: String,
        list: Vec<String>,
        first: String,
    }

    #[derive(Deserialize)]
    struct BoolCase {
        value: String,
        bool: Option<bool>,
    }

    fn load() -> SpecVectors {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/test-directives/spec-vectors.json"
        );
        let text = std::fs::read_to_string(path).expect("spec-vectors.json readable");
        serde_json::from_str(&text).expect("spec-vectors.json well-formed")
    }

    #[test]
    fn canonical_parser_matches_spec_vectors() {
        let vectors = load();

        for case in &vectors.directive_lines {
            let parsed = parse_directive_line(&case.line);
            match (&case.key, parsed) {
                (Some(key), Some(d)) => {
                    assert_eq!(d.key_lower(), *key, "key for line {:?}", case.line);
                    assert_eq!(
                        Some(d.value.to_string()),
                        case.value,
                        "value for line {:?}",
                        case.line
                    );
                }
                (None, None) => {}
                (expected, actual) => panic!(
                    "line {:?}: expected key {:?}, parsed {:?}",
                    case.line, expected, actual
                ),
            }
        }

        for case in &vectors.flag_lines {
            assert_eq!(
                parse_flag_directive_line(&case.line).map(str::to_string),
                case.name,
                "flag for line {:?}",
                case.line
            );
        }

        for case in &vectors.list_values {
            assert_eq!(
                split_list_values(&case.value).collect::<Vec<_>>(),
                case.list.iter().map(String::as_str).collect::<Vec<_>>(),
                "list for value {:?}",
                case.value
            );
            assert_eq!(
                first_list_value(&case.value),
                case.first,
                "first for value {:?}",
                case.value
            );
        }

        for case in &vectors.bool_values {
            assert_eq!(
                parse_bool_value(&case.value),
                case.bool,
                "bool for value {:?}",
                case.value
            );
        }
    }
}
