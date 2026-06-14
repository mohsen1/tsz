#[test]
fn compile_import_alias_indexer_does_not_leak_instance_side_into_namespace_static_side() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true
          },
          "include": [
            "*.ts", "*.tsx", "*.js", "*.jsx",
            "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
          ],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_backbone.ts"),
        r#"export class Model {
    public someData: string;
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_moduleA.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_moduleB.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_main.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
import moduleA = require("./extendingClassFromAliasAndUsageInIndexer_moduleA");
import moduleB = require("./extendingClassFromAliasAndUsageInIndexer_moduleB");
interface IHasVisualizationModel {
    VisualizationModel: typeof Backbone.Model;
}
var moduleATyped: IHasVisualizationModel = moduleA;
var moduleMap: { [key: string]: IHasVisualizationModel } = {
    "moduleA": moduleA,
    "moduleB": moduleB
};
var moduleName: string;
var visModel = new moduleMap[moduleName].VisualizationModel();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();

    assert_eq!(
        codes,
        vec![2454, 2564],
        "Expected only TS2454 and TS2564 for alias indexer usage. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 from instance-side leakage into module namespace static side. Diagnostics: {:?}",
        result.diagnostics
    );
}
