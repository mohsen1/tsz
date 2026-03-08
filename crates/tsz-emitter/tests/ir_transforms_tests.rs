//! Tests for IR-based transforms

use crate::transforms::ir::*;
use crate::transforms::ir_printer::IRPrinter;

#[test]
fn test_ir_enum_numeric() {
    let enum_ir = IRNode::EnumIIFE {
        name: "E".into(),
        members: vec![
            EnumMember {
                name: "A".into(),
                value: EnumMemberValue::Auto(0),
                leading_comment: None,
                trailing_comment: None,
            },
            EnumMember {
                name: "B".into(),
                value: EnumMemberValue::Auto(1),
                leading_comment: None,
                trailing_comment: None,
            },
        ],
        namespace_export: None,
    };

    let output = IRPrinter::emit_to_string(&enum_ir);
    assert!(output.contains("var E;"));
    assert!(output.contains("(function (E)"));
    assert!(output.contains("E[E[\"A\"] = 0] = \"A\""));
    assert!(output.contains("E[E[\"B\"] = 1] = \"B\""));
}

#[test]
fn test_ir_enum_string() {
    let enum_ir = IRNode::EnumIIFE {
        name: "S".into(),
        members: vec![
            EnumMember {
                name: "A".into(),
                value: EnumMemberValue::String("alpha".into()),
                leading_comment: None,
                trailing_comment: None,
            },
            EnumMember {
                name: "B".into(),
                value: EnumMemberValue::String("beta".into()),
                leading_comment: None,
                trailing_comment: None,
            },
        ],
        namespace_export: None,
    };

    let output = IRPrinter::emit_to_string(&enum_ir);
    assert!(output.contains("var S;"));
    assert!(output.contains("S[\"A\"] = \"alpha\""));
    assert!(output.contains("S[\"B\"] = \"beta\""));
    // Should NOT have reverse mapping for string enums
    assert!(!output.contains("S[S["));
}

#[test]
fn test_ir_namespace_iife() {
    let namespace_ir = IRNode::NamespaceIIFE {
        name: "MyNamespace".into(),
        name_parts: vec!["MyNamespace".into()],
        body: vec![
            IRNode::func_decl("foo", vec![], vec![IRNode::ret(Some(IRNode::number("42")))]),
            IRNode::NamespaceExport {
                namespace: "MyNamespace".into(),
                name: "foo".into(),
                value: Box::new(IRNode::id("foo")),
            },
        ],
        is_exported: false,
        attach_to_exports: false,
        should_declare_var: true,
        parent_name: None,
        param_name: None,
        skip_sequence_indent: false,
    };

    let output = IRPrinter::emit_to_string(&namespace_ir);
    assert!(output.contains("var MyNamespace;"));
    assert!(output.contains("(function (MyNamespace)"));
    assert!(output.contains("function foo()"));
    assert!(output.contains("MyNamespace.foo = foo"));
}

#[test]
fn test_ir_namespace_qualified() {
    let namespace_ir = IRNode::NamespaceIIFE {
        name: "A".into(),
        name_parts: vec!["A".into(), "B".into(), "C".into()],
        body: vec![],
        is_exported: false,
        attach_to_exports: false,
        should_declare_var: true,
        parent_name: None,
        param_name: None,
        skip_sequence_indent: false,
    };

    let output = IRPrinter::emit_to_string(&namespace_ir);
    assert!(output.contains("var A;"));
    assert!(output.contains("(function (A)"));
    assert!(output.contains("var B;"));
    assert!(output.contains("(function (B)"));
    assert!(output.contains("var C;"));
    assert!(output.contains("(function (C)"));
}

#[test]
fn test_ir_commonjs_preamble() {
    let nodes = [IRNode::UseStrict, IRNode::EsesModuleMarker];

    let output1 = IRPrinter::emit_to_string(&nodes[0]);
    assert_eq!(output1, "\"use strict\";");

    let output2 = IRPrinter::emit_to_string(&nodes[1]);
    assert!(output2.contains("Object.defineProperty(exports, \"__esModule\""));
}

#[test]
fn test_ir_require_statement() {
    let require = IRNode::RequireStatement {
        var_name: "module_1".into(),
        module_spec: "./myModule".into(),
    };

    let output = IRPrinter::emit_to_string(&require);
    assert!(output.contains("var module_1 = require(\"./myModule\");"));
}

#[test]
fn test_ir_import_statements() {
    // Default import
    let default_import = IRNode::DefaultImport {
        var_name: "myDefault".into(),
        module_var: "module_1".into(),
    };
    let output = IRPrinter::emit_to_string(&default_import);
    assert!(output.contains("var myDefault = module_1.default;"));

    // Named import
    let named_import = IRNode::NamedImport {
        var_name: "foo".into(),
        module_var: "module_1".into(),
        import_name: "foo".into(),
    };
    let output = IRPrinter::emit_to_string(&named_import);
    assert!(output.contains("var foo = module_1.foo;"));

    // Namespace import
    let namespace_import = IRNode::NamespaceImport {
        var_name: "ns".into(),
        module_var: "module_1".into(),
    };
    let output = IRPrinter::emit_to_string(&namespace_import);
    assert!(output.contains("var ns = __importStar(module_1);"));
}

#[test]
fn test_ir_export_assignment() {
    let export = IRNode::ExportAssignment {
        name: "myFunction".into(),
    };

    let output = IRPrinter::emit_to_string(&export);
    assert!(output.contains("exports.myFunction = myFunction;"));
}

#[test]
fn test_ir_reexport_property() {
    let reexport = IRNode::ReExportProperty {
        export_name: "foo".into(),
        module_var: "module_1".into(),
        import_name: "bar".into(),
    };

    let output = IRPrinter::emit_to_string(&reexport);
    assert!(output.contains("Object.defineProperty(exports, \"foo\""));
    assert!(output.contains("get: function () { return module_1.bar;"));
}

#[test]
fn test_ir_namespace_export() {
    let export = IRNode::NamespaceExport {
        namespace: "MyNamespace".into(),
        name: "myFunction".into(),
        value: Box::new(IRNode::Identifier("myFunction".into())),
    };

    let output = IRPrinter::emit_to_string(&export);
    assert!(output.contains("MyNamespace.myFunction = myFunction;"));
}
