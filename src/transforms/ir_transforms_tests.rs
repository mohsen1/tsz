//! Tests for IR-based transforms

use crate::transforms::ir::*;
use crate::transforms::ir_printer::IRPrinter;

#[test]
fn test_ir_enum_numeric() {
    let enum_ir = IRNode::EnumIIFE {
        name: "E".to_string(),
        members: vec![
            EnumMember {
                name: "A".to_string(),
                value: EnumMemberValue::Auto(0),
            },
            EnumMember {
                name: "B".to_string(),
                value: EnumMemberValue::Auto(1),
            },
        ],
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
        name: "S".to_string(),
        members: vec![
            EnumMember {
                name: "A".to_string(),
                value: EnumMemberValue::String("alpha".to_string()),
            },
            EnumMember {
                name: "B".to_string(),
                value: EnumMemberValue::String("beta".to_string()),
            },
        ],
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
        name_parts: vec!["MyNamespace".to_string()],
        body: vec![
            IRNode::func_decl("foo", vec![], vec![IRNode::ret(Some(IRNode::number("42")))]),
            IRNode::ExportAssignment {
                name: "foo".to_string(),
            },
        ],
        is_exported: false,
        attach_to_exports: false,
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
        name_parts: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        body: vec![],
        is_exported: false,
        attach_to_exports: false,
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
    let nodes = vec![
        IRNode::UseStrict,
        IRNode::EsesModuleMarker,
    ];

    let output1 = IRPrinter::emit_to_string(&nodes[0]);
    assert_eq!(output1, "\"use strict\";");

    let output2 = IRPrinter::emit_to_string(&nodes[1]);
    assert!(output2.contains("Object.defineProperty(exports, \"__esModule\""));
}

#[test]
fn test_ir_require_statement() {
    let require = IRNode::RequireStatement {
        var_name: "module_1".to_string(),
        module_spec: "./myModule".to_string(),
    };

    let output = IRPrinter::emit_to_string(&require);
    assert!(output.contains("var module_1 = require(\"./myModule\");"));
}

#[test]
fn test_ir_import_statements() {
    // Default import
    let default_import = IRNode::DefaultImport {
        var_name: "myDefault".to_string(),
        module_var: "module_1".to_string(),
    };
    let output = IRPrinter::emit_to_string(&default_import);
    assert!(output.contains("var myDefault = module_1.default;"));

    // Named import
    let named_import = IRNode::NamedImport {
        var_name: "foo".to_string(),
        module_var: "module_1".to_string(),
        import_name: "foo".to_string(),
    };
    let output = IRPrinter::emit_to_string(&named_import);
    assert!(output.contains("var foo = module_1.foo;"));

    // Namespace import
    let namespace_import = IRNode::NamespaceImport {
        var_name: "ns".to_string(),
        module_var: "module_1".to_string(),
    };
    let output = IRPrinter::emit_to_string(&namespace_import);
    assert!(output.contains("var ns = __importStar(module_1);"));
}

#[test]
fn test_ir_export_assignment() {
    let export = IRNode::ExportAssignment {
        name: "myFunction".to_string(),
    };

    let output = IRPrinter::emit_to_string(&export);
    assert!(output.contains("exports.myFunction = myFunction;"));
}

#[test]
fn test_ir_reexport_property() {
    let reexport = IRNode::ReExportProperty {
        export_name: "foo".to_string(),
        module_var: "module_1".to_string(),
        import_name: "bar".to_string(),
    };

    let output = IRPrinter::emit_to_string(&reexport);
    assert!(output.contains("Object.defineProperty(exports, \"foo\""));
    assert!(output.contains("get: function () { return module_1.bar;"));
}

#[test]
fn test_ir_namespace_export() {
    let export = IRNode::NamespaceExport {
        namespace: "MyNamespace".to_string(),
        name: "myFunction".to_string(),
    };

    let output = IRPrinter::emit_to_string(&export);
    assert!(output.contains("MyNamespace.myFunction = myFunction;"));
}
