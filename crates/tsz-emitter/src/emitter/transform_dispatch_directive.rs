use super::IdentifierId;
use crate::context::transform::ModuleFormat;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;

pub(super) enum EmitDirective {
    Identity,
    ES5Class {
        class_node: NodeIndex,
    },
    ES5ClassExpression {
        class_node: NodeIndex,
    },
    ES5Namespace {
        namespace_node: NodeIndex,
        should_declare_var: bool,
    },
    ES5Enum {
        enum_node: NodeIndex,
    },
    CommonJSExport {
        names: Arc<[IdentifierId]>,
        is_default: bool,
        inner: Box<Self>,
    },
    CommonJSExportDefaultExpr,
    CommonJSExportDefaultClassES5 {
        class_node: NodeIndex,
    },
    ES5ArrowFunction {
        arrow_node: NodeIndex,
        captures_this: bool,
        captures_arguments: bool,
        class_alias: Option<Arc<str>>,
    },
    ES5AsyncFunction {
        function_node: NodeIndex,
    },
    ES5GeneratorFunction {
        function_node: NodeIndex,
    },
    ES5ForOf {
        for_of_node: NodeIndex,
    },
    ES5ObjectLiteral {
        object_literal: NodeIndex,
    },
    ES5ArrayLiteral {
        array_literal: NodeIndex,
    },
    ES5CallSpread {
        call_expr: NodeIndex,
    },
    ES5NewSpread {
        new_expr: NodeIndex,
    },
    ES5VariableDeclarationList {
        decl_list: NodeIndex,
    },
    ES5FunctionParameters {
        function_node: NodeIndex,
    },
    ES5TemplateLiteral,
    SubstituteThis {
        capture_name: Arc<str>,
    },
    SubstituteArguments,
    ES5SuperCall,
    TC39Decorators {
        class_node: NodeIndex,
        function_name: Option<String>,
    },
    ModuleWrapper {
        format: ModuleFormat,
        dependencies: Arc<[String]>,
    },
    Chain(Vec<Self>),
}
