/// UMD global conflict: when two modules both `export as namespace Alpha`,
/// the first one encountered in file order should win (matching tsc behavior).
/// Previously, `globals.set()` overwrote with the last file, causing false
/// TS2322 when accessing properties from the first namespace.
#[test]
fn test_umd_global_conflict_first_in_wins() {
    let files = vec![
        (
            "v1/index.d.ts".to_string(),
            r#"
export as namespace Alpha;
export var x: string;
"#
            .to_string(),
        ),
        (
            "v2/index.d.ts".to_string(),
            r#"
export as namespace Alpha;
export var y: number;
"#
            .to_string(),
        ),
        (
            "global.ts".to_string(),
            r#"
const p: string = Alpha.x;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|f| f.file_name == "global.ts")
        .expect("expected global.ts result");
    let errors: Vec<(u32, &str)> = file
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors for UMD global conflict (first in wins). Got: {errors:#?}"
    );
}

/// Cross-module optional interface property must include `undefined` in its type.
///
/// When an interface has `server?: IServer`, the property type should be
/// `IServer | undefined`. Passing this to a parameter that expects `IServer`
/// should emit TS2345. This tests that cross-module type resolution preserves
/// the optional flag on interface properties.
#[test]
fn test_cross_module_optional_interface_property_emits_ts2345() {
    let files = vec![
        (
            "server.ts".to_string(),
            r#"
export interface IServer {}
export interface IWorkspace {
    toAbsolutePath(server: IServer, extra?: string): string;
}
export interface IConfiguration {
    workspace: IWorkspace;
    server?: IServer;
}
"#
            .to_string(),
        ),
        (
            "consumer.ts".to_string(),
            r#"
import * as server from './server';
function run(configuration: server.IConfiguration) {
    var absoluteWorkspacePath = configuration.workspace.toAbsolutePath(configuration.server);
}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let consumer = result
        .file_results
        .iter()
        .find(|file| file.file_name == "consumer.ts")
        .expect("expected consumer.ts result");

    let has_ts2345 = consumer.diagnostics.iter().any(|diag| diag.code == 2345);
    assert!(
        has_ts2345,
        "Expected TS2345 for passing optional IServer|undefined to IServer parameter. \
         Cross-module optional interface property type must include undefined. \
         Actual diagnostics: {:#?}",
        consumer.diagnostics
    );
}
