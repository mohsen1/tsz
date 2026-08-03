use super::*;

#[test]
fn test_collect_module_specifiers_finds_dynamic_imports() {
    let text = r#"import("./foo").then(x => x);"#;
    let path = Path::new("test.mts");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./foo".to_string()),
        "Should find dynamic import specifier './foo', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_finds_plain_require_calls() {
    let text = r#"const data = require("./data.json");"#;
    let path = Path::new("test.js");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./data.json".to_string()),
        "Should find require specifier './data.json', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_finds_require_with_whitespace_before_paren() {
    let text = r#"const data = require ("./data.json");"#;
    let path = Path::new("test.js");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./data.json".to_string()),
        "Should find spaced require specifier './data.json', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_finds_jsdoc_import_tags() {
    let text = r#"
// @ts-check
/** @import { "a,b" as CommaName } from "./dep" */
/** @type {CommaName} */
const value = "x";
"#;
    let path = Path::new("test.js");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./dep".to_string()),
        "Should find JSDoc @import specifier './dep', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_jsdoc_import_tag_parses_resolution_mode() {
    use tsz::module_resolver::ImportingModuleKind;
    let text = r#"
// @ts-check
/** @import { Esm } from "pkg" with { "resolution-mode": "import" } */
/** @import { Cjs } from "pkg" with { 'resolution-mode': 'require' } */
/** @import { Plain } from "pkg" */
/** @type {Esm} */
const value = "x";
"#;
    let path = Path::new("test.js");
    let requests = collect_module_requests_from_text(path, text);

    // All three target "pkg"; the attribute clause drives the per-request mode.
    let modes: Vec<Option<ImportingModuleKind>> = requests
        .iter()
        .filter(|(specifier, _, _, _)| specifier == "pkg")
        .map(|(_, _, mode, _)| *mode)
        .collect();

    assert!(
        modes.contains(&Some(ImportingModuleKind::Esm)),
        "expected an ESM-mode request for the `resolution-mode: import` tag, got: {modes:?}"
    );
    assert!(
        modes.contains(&Some(ImportingModuleKind::CommonJs)),
        "expected a CommonJS-mode request for the `resolution-mode: require` tag, got: {modes:?}"
    );
    assert!(
        modes.contains(&None),
        "expected a mode-less request for the plain `@import` tag, got: {modes:?}"
    );
}

#[test]
fn test_collect_module_specifiers_require_has_correct_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"const data = require("./data.json");"#;
    let file_name = "test.js".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let requires: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::CjsRequire)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        requires.contains(&"./data.json"),
        "Should find CommonJS require, got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_export_import_require_has_correct_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"export import dep = require("./dep");"#;
    let file_name = "test.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let requires: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::CjsRequire)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        requires.contains(&"./dep"),
        "Should find exported CommonJS require, got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_dynamic_import_has_correct_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"import("./foo").then(x => x);"#;
    let file_name = "test.mts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::DynamicImport)
        .collect();
    assert_eq!(
        dynamic_imports.len(),
        1,
        "Should find exactly one DynamicImport, got: {specifiers:?}"
    );
    assert_eq!(dynamic_imports[0].0, "./foo");
}

#[test]
fn test_collect_module_specifiers_import_defer_has_dynamic_import_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"import.defer("./foo.js").then(x => x);"#;
    let file_name = "test.mts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::DynamicImport)
        .collect();
    assert_eq!(
        dynamic_imports.len(),
        1,
        "Should find exactly one import.defer DynamicImport, got: {specifiers:?}"
    );
    assert_eq!(dynamic_imports[0].0, "./foo.js");
}

#[test]
fn test_module_specifier_detects_type_json_import_attribute() {
    let text = r#"import data from "./data.json" with { type: "json" };"#;
    let file_name = "test.mts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let (specifier, specifier_idx, _, _) = specifiers
        .iter()
        .find(|(specifier, _, _, _)| specifier == "./data.json")
        .expect("expected JSON import specifier");

    assert_eq!(specifier, "./data.json");
    assert!(
        module_specifier_has_type_json_import_attribute(&arena, *specifier_idx),
        "Expected the JSON module specifier to carry a type=json import attribute"
    );
}

#[test]
fn test_collect_module_requests_from_text_carries_type_json_attribute() {
    let path = Path::new("test.mts");
    let requests = collect_module_requests_from_text(
        path,
        r#"import data from "./data.json" with { type: "json" };"#,
    );
    let (_, _, _, has_type_json_attribute) = requests
        .iter()
        .find(|(specifier, _, _, _)| specifier == "./data.json")
        .expect("expected JSON import request");

    assert!(
        *has_type_json_attribute,
        "Expected source-discovery module requests to retain type=json import attributes"
    );
}

#[test]
fn test_collect_module_requests_from_text_skips_non_relative_ambient_declaration_names() {
    let path = Path::new("types.d.ts");
    let requests = collect_module_requests_from_text(
        path,
        r#"
declare module "*.css" {}
declare module "virtual:asset" {}
declare module "./augment" {}
declare module "pkg" {
  export { T } from "dep";
}
"#,
    );
    let specifiers: Vec<_> = requests
        .iter()
        .map(|(specifier, _, _, _)| specifier.as_str())
        .collect();

    assert!(
        !specifiers.contains(&"*.css"),
        "ambient wildcard declarations are not source dependencies: {specifiers:?}"
    );
    assert!(
        !specifiers.contains(&"virtual:asset"),
        "bare ambient declarations are not source dependencies: {specifiers:?}"
    );
    assert!(
        !specifiers.contains(&"pkg"),
        "ambient declaration names are not source dependencies: {specifiers:?}"
    );
    assert!(
        specifiers.contains(&"./augment"),
        "relative module augmentation names can target concrete files: {specifiers:?}"
    );
    assert!(
        specifiers.contains(&"dep"),
        "real re-exports inside ambient module bodies remain dependencies: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_for_check_skips_declaration_file_ambient_names() {
    let text = r#"
declare module "*.css" {}
declare module "virtual:asset" {}
declare module "./augment" {}
declare module "pkg" {
  export { T } from "dep";
}
"#;
    let mut parser = tsz::parser::ParserState::new("types.d.ts".to_string(), text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();

    let specifiers: Vec<_> = collect_module_specifiers_for_check(&arena, source_file, true)
        .into_iter()
        .map(|(specifier, _, _, _)| specifier)
        .collect();

    assert!(
        !specifiers.iter().any(|specifier| specifier == "*.css"),
        "ambient wildcard declarations are not driver lookups: {specifiers:?}"
    );
    assert!(
        !specifiers
            .iter()
            .any(|specifier| specifier == "virtual:asset"),
        "ambient bare declarations are not driver lookups: {specifiers:?}"
    );
    assert!(
        !specifiers.iter().any(|specifier| specifier == "pkg"),
        "ambient declaration names are not driver lookups in declaration files: {specifiers:?}"
    );
    assert!(
        specifiers.iter().any(|specifier| specifier == "./augment"),
        "relative augmentation names still need source-file-specific lookup: {specifiers:?}"
    );
    assert!(
        specifiers.iter().any(|specifier| specifier == "dep"),
        "real re-exports inside ambient module bodies remain dependencies: {specifiers:?}"
    );
}

#[test]
fn test_collect_declaration_file_augmentation_targets_for_untyped_check_finds_bare_names() {
    // The dedicated TS2665-only collector picks up exactly what the
    // general-purpose `collect_module_specifiers_for_check` above deliberately
    // excludes for a `.d.ts` host: bare (non-relative) augmentation names.
    // This collector feeds a side-channel resolution that only ever writes
    // `untyped_module_paths`, so it does not need to distinguish wildcard
    // ambient patterns or relative names the way the general pipeline does —
    // callers only care about "does this look like a real augmentation
    // target."
    let text = r#"
declare module "*.css" {}
declare module "./augment" {}
declare module "pkg" {
  export { T } from "dep";
}
"#;
    let mut parser = tsz::parser::ParserState::new("types.d.ts".to_string(), text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();

    let targets: Vec<_> =
        collect_declaration_file_augmentation_targets_for_untyped_check(&arena, source_file)
            .into_iter()
            .map(|(specifier, _)| specifier)
            .collect();

    assert!(
        targets.iter().any(|specifier| specifier == "pkg"),
        "a bare augmentation name needs resolution for TS2665: {targets:?}"
    );
    assert!(
        targets.iter().any(|specifier| specifier == "*.css"),
        "the collector itself does not filter wildcard patterns — the caller \
         resolves every entry through ModuleResolver, which classifies a \
         pattern like any other unresolvable specifier: {targets:?}"
    );
    assert!(
        !targets.iter().any(|specifier| specifier == "./augment"),
        "relative augmentation names resolve through the general pipeline \
         already (TS2664), not this untyped-only side channel: {targets:?}"
    );
}

#[test]
fn test_collect_declaration_file_augmentation_targets_for_untyped_check_ignores_non_ambient() {
    // A non-`declare` module block (a real `namespace`/module body) and a
    // relative name are both out of scope for this collector.
    let text = r#"
export {};
namespace NotAmbient {
  export const x = 1;
}
"#;
    let mut parser = tsz::parser::ParserState::new("types.d.ts".to_string(), text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();

    let targets =
        collect_declaration_file_augmentation_targets_for_untyped_check(&arena, source_file);
    assert!(
        targets.is_empty(),
        "a non-ambient namespace is not an augmentation target: {targets:?}"
    );
}

#[test]
fn test_collect_module_specifiers_for_check_keeps_bare_source_augmentation_targets() {
    let text = r#"
export {};
declare module "pkg" {
  export const value: number;
}
"#;
    let mut parser = tsz::parser::ParserState::new("source.ts".to_string(), text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();

    let external_specifiers: Vec<_> =
        collect_module_specifiers_for_check(&arena, source_file, true)
            .into_iter()
            .map(|(specifier, _, _, _)| specifier)
            .collect();
    assert!(
        external_specifiers
            .iter()
            .any(|specifier| specifier == "pkg"),
        "external source augmentations need lookup for TS2664: {external_specifiers:?}"
    );

    let script_specifiers: Vec<_> = collect_module_specifiers_for_check(&arena, source_file, false)
        .into_iter()
        .map(|(specifier, _, _, _)| specifier)
        .collect();
    assert!(
        !script_specifiers.iter().any(|specifier| specifier == "pkg"),
        "script ambient declarations should not become driver lookups: {script_specifiers:?}"
    );
}

#[test]
fn simple_module_request_scanner_collects_static_imports_and_reexports() {
    let requests = collect_simple_module_requests_from_text(
        r#"
import "./setup";
import type { Widget } from "./types";
import view from "./view";
export { Widget } from "./types";
export * from "./shared";
export interface LocalOnly {}
"#,
    )
    .expect("simple static module syntax should not need the source-discovery parser");

    let actual: Vec<_> = requests
        .iter()
        .map(|(specifier, kind, _, has_type_json)| (specifier.as_str(), *kind, *has_type_json))
        .collect();
    assert_eq!(
        actual,
        vec![
            (
                "./setup",
                tsz::module_resolver::ImportKind::EsmImport,
                false
            ),
            (
                "./types",
                tsz::module_resolver::ImportKind::EsmImport,
                false
            ),
            ("./view", tsz::module_resolver::ImportKind::EsmImport, false),
            (
                "./types",
                tsz::module_resolver::ImportKind::EsmReExport,
                false
            ),
            (
                "./shared",
                tsz::module_resolver::ImportKind::EsmReExport,
                false
            ),
        ]
    );
}

#[test]
fn simple_module_request_scanner_preserves_opposite_quote_specifier_values() {
    let requests = collect_simple_module_requests_from_text(
        r#"
import quotedSingle from "'pkg'";
export { quotedDouble } from '"pkg"';
"#,
    )
    .expect("opposite quote characters are literal specifier contents");

    let actual: Vec<_> = requests
        .iter()
        .map(|(specifier, kind, _, has_type_json)| (specifier.as_str(), *kind, *has_type_json))
        .collect();
    assert_eq!(
        actual,
        vec![
            ("'pkg'", tsz::module_resolver::ImportKind::EsmImport, false),
            (
                r#""pkg""#,
                tsz::module_resolver::ImportKind::EsmReExport,
                false,
            ),
        ]
    );
}

#[test]
fn simple_module_request_scanner_falls_back_for_escaped_static_specifiers() {
    let text = r#"
import "./d\u0065p";
export { value } from "./r\u0065exp";
"#;
    assert!(
        collect_simple_module_requests_from_text(text).is_none(),
        "escaped module specifiers must fall back so the parser can decode them"
    );

    let requests = collect_module_requests_from_text(std::path::Path::new("index.ts"), text);
    let actual: Vec<_> = requests
        .iter()
        .map(|(specifier, kind, _, _)| (specifier.as_str(), *kind))
        .collect();
    assert_eq!(
        actual,
        vec![
            ("./dep", tsz::module_resolver::ImportKind::EsmImport),
            ("./reexp", tsz::module_resolver::ImportKind::EsmReExport),
        ]
    );
}

#[test]
fn simple_module_request_scanner_handles_ambient_modules_conservatively() {
    let simple = collect_simple_module_requests_from_text(
        r#"
declare module "*.css" {}
declare module "virtual:asset" {}
declare module "./augment" {}
"#,
    )
    .expect("ambient declarations without body imports can stay on the scanner path");
    let specifiers: Vec<_> = simple
        .iter()
        .map(|(specifier, _, _, _)| specifier.as_str())
        .collect();
    assert_eq!(specifiers, vec!["./augment"]);

    assert!(
        collect_simple_module_requests_from_text(
            r#"
declare module "pkg" {
  export { T } from "dep";
}
"#
        )
        .is_none(),
        "real dependencies inside ambient module bodies fall back to the parser path"
    );
}

#[test]
fn simple_module_request_scanner_falls_back_for_mode_sensitive_forms() {
    assert!(
        collect_simple_module_requests_from_text(
            r#"import data from "./data.json" with { type: "json" };"#
        )
        .is_none()
    );
    assert!(
        collect_simple_module_requests_from_text(r#"const loader = import("./lazy");"#).is_none()
    );
    assert!(collect_simple_module_requests_from_text(r#"require("./cjs");"#).is_none());
}

#[test]
fn test_collect_module_specifiers_mixed_import_kinds() {
    use tsz::module_resolver::ImportKind;
    let text = r#"
import { foo } from "./static-import";
import("./dynamic-import");
export { bar } from "./re-export";
"#;
    let file_name = "test.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let static_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        static_imports.contains(&"./static-import"),
        "Should find static import, got: {static_imports:?}"
    );

    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::DynamicImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        dynamic_imports.contains(&"./dynamic-import"),
        "Should find dynamic import, got: {dynamic_imports:?}"
    );

    let re_exports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmReExport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        re_exports.contains(&"./re-export"),
        "Should find re-export, got: {re_exports:?}"
    );
}

#[test]
fn test_collect_module_specifiers_finds_re_exports_inside_ambient_module_blocks() {
    use tsz::module_resolver::ImportKind;
    let text = r#"
declare module "baz" {
  export { T } from "foo";
}
"#;
    let file_name = "index.d.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let re_exports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmReExport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        re_exports.contains(&"foo"),
        "Should find ambient module re-export, got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_resolution_mode_override() {
    use tsz::module_resolver::ImportingModuleKind;

    let text = r#"import type { Foo } from "pkg" with { "resolution-mode": "import" };"#;
    let file_name = "test.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    assert_eq!(
        specifiers.len(),
        1,
        "Expected exactly one import: {specifiers:?}"
    );
    assert_eq!(specifiers[0].0, "pkg");
    assert_eq!(specifiers[0].3, Some(ImportingModuleKind::Esm));
}

#[test]
fn test_collect_module_specifiers_finds_import_type_dependencies() {
    use tsz::module_resolver::ImportKind;

    let text = r#"export type SomeType = import("./inner").SomeType;"#;
    let file_name = "index.d.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();

    assert!(
        import_types.contains(&"./inner"),
        "Should find import type dependency './inner', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_import_type_resolution_mode_override() {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind};

    let text =
        r#"export type SomeType = import("pkg", { with: { "resolution-mode": "require" } }).Foo;"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .collect();

    assert_eq!(
        import_types.len(),
        1,
        "Expected one import type, got: {specifiers:?}"
    );
    assert_eq!(import_types[0].0, "pkg");
    assert_eq!(import_types[0].3, Some(ImportingModuleKind::CommonJs));
}

#[test]
fn test_collect_module_specifiers_finds_typeof_import_dependencies() {
    use tsz::module_resolver::ImportKind;

    let text = r#"const parserRef: typeof import("csv-parse") = null as any;"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();

    assert!(
        import_types.contains(&"csv-parse"),
        "Should find bare typeof import dependency 'csv-parse', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_typeof_import_resolution_mode_override() {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind};

    let text = r#"type Parser = typeof import("pkg", { with: { "resolution-mode": "require" } });"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .collect();

    assert_eq!(
        import_types.len(),
        1,
        "Expected one typeof import, got: {specifiers:?}"
    );
    assert_eq!(import_types[0].0, "pkg");
    assert_eq!(import_types[0].3, Some(ImportingModuleKind::CommonJs));
}
