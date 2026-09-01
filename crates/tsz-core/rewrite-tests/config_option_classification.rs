use std::fs;
use std::path::Path;

use tempfile::TempDir;
use tsz::config::{ProjectRequest, ProjectSelection, resolve_project};
use tsz::diagnostics::Diagnostic;
use tsz::host::SystemHost;
use tsz::{DeferredCompilerOption, DeferredCompilerOptionValue};

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture path has a parent")).unwrap();
    fs::write(path, text).unwrap();
}

fn resolve(root: &Path) -> tsz::config::ResolvedProject {
    let host = SystemHost::new(root);
    resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    )
}

fn assert_diagnostic(
    diagnostic: &Diagnostic,
    file: &str,
    start: usize,
    length: usize,
    code: u32,
    message: &str,
) {
    assert_eq!(
        (
            diagnostic.file.as_str(),
            diagnostic.start,
            diagnostic.length,
            diagnostic.code,
            diagnostic.message_text.as_str(),
        ),
        (file, start as u32, length as u32, code, message),
    );
}

#[test]
fn config_option_registry_reports_unknown_typo_and_wrong_type_at_authored_spans() {
    let fixture = TempDir::new().unwrap();
    let config = concat!(
        "{\n",
        "  \"compilerOptions\": {\n",
        "    \"stric\": true,\n",
        "    \"noemit\": true,\n",
        "    \"strict\": \"yes\",\n",
        "    \"lib\": \"es2025\",\n",
        "    \"target\": false,\n",
        "    \"rootDir\": false,\n",
        "    \"noEmitHelpers\": \"yes\",\n",
        "    \"noEmitHelpers\": true,\n",
        "    \"moduleResolution\": false,\n",
        "    \"moduleResolution\": \"node16\",\n",
        "    \"jsxFactory\": true,\n",
        "    \"jsxFactory\": \"h\"\n",
        "  },\n",
        "  \"files\": [\"case.ts\"]\n",
        "}\n",
    );
    write(fixture.path(), "tsconfig.json", config);
    write(fixture.path(), "case.ts", "export const value = 1;\n");

    let project = resolve(fixture.path());
    assert_eq!(project.diagnostics.len(), 9, "{:#?}", project.diagnostics);
    let cases = [
        ("\"stric\"", 5023, "Unknown compiler option 'stric'."),
        (
            "\"noemit\"",
            5025,
            "Unknown compiler option 'noemit'. Did you mean 'noEmit'?",
        ),
        (
            "\"yes\"",
            5024,
            "Compiler option 'strict' requires a value of type boolean.",
        ),
        (
            "\"es2025\"",
            5024,
            "Compiler option 'lib' requires a value of type Array.",
        ),
        (
            "false",
            5024,
            "Compiler option 'target' requires a value of type enum.",
        ),
        (
            "false",
            5024,
            "Compiler option 'rootDir' requires a value of type string.",
        ),
        (
            "\"yes\"",
            5024,
            "Compiler option 'noEmitHelpers' requires a value of type boolean.",
        ),
        (
            "false",
            5024,
            "Compiler option 'moduleResolution' requires a value of type enum.",
        ),
        (
            "true",
            5024,
            "Compiler option 'jsxFactory' requires a value of type string.",
        ),
    ];
    let mut from = 0;
    for (diagnostic, (needle, code, message)) in project.diagnostics.iter().zip(cases) {
        let relative = config[from..].find(needle).unwrap();
        let start = from + relative;
        assert_diagnostic(
            diagnostic,
            "tsconfig.json",
            start,
            needle.len(),
            code,
            message,
        );
        from = start + needle.len();
    }
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::NoEmitHelpers),
        Some(&DeferredCompilerOptionValue::Boolean(true)),
    );
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::ModuleResolution),
        Some(&DeferredCompilerOptionValue::String("node16".to_string())),
    );
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::JsxFactory),
        Some(&DeferredCompilerOptionValue::String("h".to_string())),
    );
}

#[test]
fn strictness_options_are_typed_preserved_inputs_instead_of_unknown_options() {
    let fixture = TempDir::new().unwrap();
    write(
        fixture.path(),
        "tsconfig.json",
        concat!(
            "{\n",
            "  \"compilerOptions\": {\n",
            "    \"noImplicitThis\": false,\n",
            "    \"strictBindCallApply\": false,\n",
            "    \"strictFunctionTypes\": false,\n",
            "    \"useUnknownInCatchVariables\": false\n",
            "  },\n",
            "  \"files\": [\"case.ts\"]\n",
            "}\n",
        ),
    );
    write(fixture.path(), "case.ts", "export const value = 1;\n");

    let project = resolve(fixture.path());
    assert!(project.diagnostics.is_empty(), "{:#?}", project.diagnostics);
    for option in [
        DeferredCompilerOption::NoImplicitThis,
        DeferredCompilerOption::StrictBindCallApply,
        DeferredCompilerOption::StrictFunctionTypes,
        DeferredCompilerOption::UseUnknownInCatchVariables,
    ] {
        assert_eq!(
            project.options.deferred_options.get(&option),
            Some(&DeferredCompilerOptionValue::Boolean(false)),
            "{option:?}",
        );
    }
}

#[test]
fn removed_options_keep_typed_values_and_use_the_ts7_key_or_value_span() {
    let fixture = TempDir::new().unwrap();
    let config = concat!(
        "{\n",
        "  \"compilerOptions\": {\n",
        "    \"baseUrl\": \"./root\",\n",
        "    \"downlevelIteration\": false,\n",
        "    \"outFile\": \"./bundle.js\",\n",
        "    \"alwaysStrict\": false,\n",
        "    \"esModuleInterop\": false\n",
        "  },\n",
        "  \"files\": [\"case.ts\"]\n",
        "}\n",
    );
    write(fixture.path(), "tsconfig.json", config);
    write(fixture.path(), "case.ts", "export const value = 1;\n");

    let project = resolve(fixture.path());
    assert_eq!(project.diagnostics.len(), 5, "{:#?}", project.diagnostics);
    let expected = [
        (
            "\"baseUrl\"",
            5102,
            concat!(
                "Option 'baseUrl' has been removed. Please remove it from your configuration.\n",
                "  Use '\"paths\": {\"*\": [\"./root/*\"]}' instead."
            ),
        ),
        (
            "\"downlevelIteration\"",
            5102,
            "Option 'downlevelIteration' has been removed. Please remove it from your configuration.",
        ),
        (
            "\"outFile\"",
            5102,
            "Option 'outFile' has been removed. Please remove it from your configuration.",
        ),
        (
            "false",
            5108,
            "Option 'alwaysStrict=false' has been removed. Please remove it from your configuration.",
        ),
        (
            "false",
            5108,
            "Option 'esModuleInterop=false' has been removed. Please remove it from your configuration.",
        ),
    ];
    let mut from = 0;
    for (diagnostic, (needle, code, message)) in project.diagnostics.iter().zip(expected) {
        let relative = config[from..].find(needle).unwrap();
        let start = from + relative;
        assert_diagnostic(
            diagnostic,
            "tsconfig.json",
            start,
            needle.len(),
            code,
            message,
        );
        from = start + needle.len();
    }
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::BaseUrl),
        Some(&DeferredCompilerOptionValue::Path(
            fixture.path().join("root")
        )),
    );
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::AlwaysStrict),
        Some(&DeferredCompilerOptionValue::Boolean(false)),
    );
}

#[test]
fn extends_and_duplicates_only_replace_with_valid_typed_values() {
    let fixture = TempDir::new().unwrap();
    let base = concat!(
        "{\n",
        "  \"compilerOptions\": {\n",
        "    \"noEmti\": true,\n",
        "    \"strict\": false,\n",
        "    \"noEmit\": true,\n",
        "    \"alwaysStrict\": false,\n",
        "    \"baseUrl\": \"./base\",\n",
        "    \"moduleResolution\": \"node16\"\n",
        "  }\n",
        "}\n",
    );
    let child = concat!(
        "{\n",
        "  \"extends\": \"./base.json\",\n",
        "  \"compilerOptions\": {\n",
        "    \"strict\": true,\n",
        "    \"strict\": 0,\n",
        "    \"noEmit\": \"invalid\",\n",
        "    \"alwaysStrict\": true,\n",
        "    \"baseUrl\": \"./child\",\n",
        "    \"baseUrl\": false,\n",
        "    \"moduleResolution\": \"bundler\"\n",
        "  },\n",
        "  \"files\": [\"case.ts\"]\n",
        "}\n",
    );
    write(fixture.path(), "base.json", base);
    write(fixture.path(), "tsconfig.json", child);
    write(fixture.path(), "case.ts", "export const value = 1;\n");

    let project = resolve(fixture.path());
    assert!(project.options.strict);
    assert!(project.options.no_emit);
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::AlwaysStrict),
        Some(&DeferredCompilerOptionValue::Boolean(true)),
    );
    assert_eq!(
        project
            .options
            .deferred_options
            .get(&DeferredCompilerOption::ModuleResolution),
        Some(&DeferredCompilerOptionValue::String("bundler".to_string())),
    );
    assert_eq!(project.diagnostics.len(), 5, "{:#?}", project.diagnostics);
    assert_diagnostic(
        &project.diagnostics[0],
        "base.json",
        base.find("\"noEmti\"").unwrap(),
        "\"noEmti\"".len(),
        5023,
        "Unknown compiler option 'noEmti'.",
    );
    let child_cases = [
        (
            "0",
            5024,
            "Compiler option 'strict' requires a value of type boolean.",
        ),
        (
            "\"invalid\"",
            5024,
            "Compiler option 'noEmit' requires a value of type boolean.",
        ),
        (
            "\"baseUrl\"",
            5102,
            concat!(
                "Option 'baseUrl' has been removed. Please remove it from your configuration.\n",
                "  Use '\"paths\": {\"*\": [\"./child/*\"]}' instead."
            ),
        ),
        (
            "false",
            5024,
            "Compiler option 'baseUrl' requires a value of type string.",
        ),
    ];
    let mut from = 0;
    for (diagnostic, (needle, code, message)) in project.diagnostics[1..].iter().zip(child_cases) {
        let relative = child[from..].find(needle).unwrap();
        let start = from + relative;
        assert_diagnostic(
            diagnostic,
            "tsconfig.json",
            start,
            needle.len(),
            code,
            message,
        );
        from = start + needle.len();
    }
    assert!(project.diagnostics.iter().all(|diagnostic| {
        diagnostic.message_text
            != "Option 'alwaysStrict=false' has been removed. Please remove it from your configuration."
    }));
}
