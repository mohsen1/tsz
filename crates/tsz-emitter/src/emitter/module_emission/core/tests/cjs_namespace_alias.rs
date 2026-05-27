use tsz_common::ScriptTarget;

use super::emit_commonjs_with_target;

#[test]
fn commonjs_export_clause_namespace_alias_folds_iife_alias() {
    let source = r#"namespace m {
    export var x = 10;
}

export { m as instantiatedModule };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("})(m || (exports.instantiatedModule = m = {}));"),
        "Namespace export aliases should fold the exported name into the IIFE tail.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.m ="),
        "The folded export should use the alias, not the local namespace name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.instantiatedModule = m;"),
        "The later export clause should not emit a duplicate assignment after IIFE folding.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_es5_export_clause_namespace_alias_folds_iife_alias() {
    let source = r#"namespace m {
    export var x = 10;
}

export { m as instantiatedModule };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES5);

    assert!(
        output.contains("})(m || (exports.instantiatedModule = m = {}));"),
        "ES5 namespace transform should carry the export-clause alias into the IIFE tail.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.m ="),
        "ES5 namespace transform should not fold through the local namespace name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.instantiatedModule = m;"),
        "ES5 namespace transform should mark the IIFE fold as handling the later export clause.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_export_clause_empty_namespace_is_type_only() {
    let source = r#"namespace m {
    export var x = 10;
}

namespace uninstantiated {}

export { m as instantiatedModule };
export { uninstantiated };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("exports.instantiatedModule"),
        "The instantiated namespace alias should remain a runtime export.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.uninstantiated"),
        "Empty non-instantiated namespaces should not produce CommonJS runtime exports.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_export_clause_namespace_alias_fold_keeps_other_aliases() {
    let source = r#"namespace m {
    export var x = 10;
}

export { m as firstAlias };
export { m as secondAlias };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("})(m || (exports.secondAlias = exports.firstAlias = m = {}));"),
        "Namespace aliases should fold into the IIFE tail in source order.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.secondAlias = m;"),
        "Aliases folded into the IIFE tail should not emit later duplicate assignments.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_exported_namespace_and_alias_fold_through_direct_export() {
    let source = r#"export namespace M {
    export var x;
}

export { M as M1 };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("})(M || (exports.M1 = exports.M = M = {}));"),
        "A direct namespace export plus alias should fold both export bindings into the IIFE tail.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_exported_import_alias_reexport_reads_live_export_binding() {
    let source = r#"export namespace M {
    export var x;
}
export import a = M.x;

export { a as a1 };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("exports.a = M.x;"),
        "The direct import alias export should initialize the live export binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.a1 = exports.a;"),
        "A renamed export of an already-exported import alias should read through exports.a.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.a1 = a;"),
        "The renamed export should not read the erased local alias.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_merged_namespace_declaration_does_not_duplicate_export() {
    // Two `export namespace X` blocks with the same name both map to the
    // same CJS identifier.  The IIFE tail must fold to a single
    // `exports.X = X = {}` assignment, not `exports.X = exports.X = X = {}`.
    let source = r#"export namespace X {
    export namespace Y {
        class A {}
    }
}
export namespace X {
    export namespace Y {
        export class B {}
    }
}
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("(exports.X = X = {})"),
        "Merged namespace should fold to a single exports.X assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.X = exports.X"),
        "Merged namespace must not produce a duplicate exports.X chain.\nOutput:\n{output}"
    );
}
