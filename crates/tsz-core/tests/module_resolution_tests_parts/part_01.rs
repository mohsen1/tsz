#[test]
fn test_import_equals_require_new_expression_no_ts2304() {
    // Test that `new Backbone.Model()` works when Backbone is from import = require
    let source = r#"
import Backbone = require("./backbone");
const m = new Backbone.Model();
"#;
    let module_source = r#"
export class Model {
    public name: string = "";
}
"#;
    let diags = check_with_module_sources(source, "main.ts", vec![("./backbone", module_source)]);
    let ts2304_errors: Vec<_> = diags.iter().filter(|(c, _)| *c == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should not emit TS2304 for 'new Backbone.Model()' with import = require, got: {ts2304_errors:?}"
    );
}

// TODO: Emit TS2304/TS2339 for extends with non-existent property on import=require namespace.
#[test]
fn test_import_equals_require_extends_nonexistent_still_errors() {
    // Negative test: extends with non-existent export should still produce an error
    let source = r#"
import Backbone = require("./backbone");
class Bad extends Backbone.NonExistent {
    x: number = 0;
}
"#;
    let module_source = r#"
export class Model {
    public name: string = "";
}
"#;
    let diags = check_with_module_sources(source, "main.ts", vec![("./backbone", module_source)]);
    // Should have some error (TS2304 for unresolved name or TS2339 for missing property)
    assert!(
        !diags.is_empty(),
        "Should emit error for extends Backbone.NonExistent, got no diagnostics"
    );
}
