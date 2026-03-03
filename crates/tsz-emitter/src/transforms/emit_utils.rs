use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Get identifier text from a node index, returning `None` if the node is not an identifier.
pub(crate) fn identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

/// Get identifier text from a node index, returning an empty string on failure.
pub(crate) fn identifier_text_or_empty(arena: &NodeArena, idx: NodeIndex) -> String {
    identifier_text(arena, idx).unwrap_or_default()
}

/// Get enum member name from a node index (identifier or string literal).
///
/// Returns the identifier's escaped text or the literal's text, or an empty string
/// if the node is neither.
pub(crate) fn enum_member_name(arena: &NodeArena, idx: NodeIndex) -> String {
    let Some(node) = arena.get(idx) else {
        return String::new();
    };
    if let Some(ident) = arena.get_identifier(node) {
        return ident.escaped_text.clone();
    }
    if let Some(lit) = arena.get_literal(node) {
        return lit.text.clone();
    }
    String::new()
}

/// Check if an export declaration has any runtime (non-type-only) value that needs to be emitted.
///
/// Returns `false` for `export type { ... }`, re-exports of only type-only specifiers,
/// and exports of declarations that are purely types (interfaces, type aliases, const enums,
/// declare-prefixed classes/functions/variables/modules).
pub(crate) fn export_decl_has_runtime_value(
    arena: &NodeArena,
    export_decl: &tsz_parser::parser::node::ExportDeclData,
    preserve_const_enums: bool,
) -> bool {
    if export_decl.is_type_only {
        return false;
    }

    if export_decl.is_default_export {
        return true;
    }

    if export_decl.export_clause.is_none() {
        return true;
    }

    let Some(clause_node) = arena.get(export_decl.export_clause) else {
        return false;
    };

    if let Some(named) = arena.get_named_imports(clause_node) {
        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        return false;
    }

    if export_clause_is_type_only(arena, clause_node, preserve_const_enums) {
        return false;
    }

    true
}

/// Check if an export clause (the declaration after `export`) is type-only.
///
/// Returns `true` for interfaces, type aliases, const/declare enums (when not preserved),
/// and declare-prefixed classes, functions, variables, and modules.
pub(crate) fn export_clause_is_type_only(
    arena: &NodeArena,
    clause_node: &Node,
    preserve_const_enums: bool,
) -> bool {
    match clause_node.kind {
        k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            let Some(enum_decl) = arena.get_enum(clause_node) else {
                return false;
            };
            arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                || (arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                    && !preserve_const_enums)
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            let Some(class_decl) = arena.get_class(clause_node) else {
                return false;
            };
            arena.has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            let Some(func_decl) = arena.get_function(clause_node) else {
                return false;
            };
            arena.has_modifier(&func_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            let Some(var_decl) = arena.get_variable(clause_node) else {
                return false;
            };
            arena.has_modifier(&var_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            let Some(module_decl) = arena.get_module(clause_node) else {
                return false;
            };
            arena.has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        _ => false,
    }
}

/// Check if a module body contains any runtime (non-type-only) statements,
/// meaning the module is "instantiated" and needs to be emitted.
///
/// Recursively walks dotted namespaces (e.g., `namespace Foo.Bar`) to find
/// the innermost `MODULE_BLOCK` and checks each statement.
pub(crate) fn is_instantiated_module(arena: &NodeArena, module_body: NodeIndex) -> bool {
    let Some(body_node) = arena.get(module_body) else {
        return false;
    };

    // If body is another MODULE_DECLARATION (dotted namespace like Foo.Bar),
    // recurse into the inner module
    if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
        if let Some(inner_module) = arena.get_module(body_node) {
            return is_instantiated_module(arena, inner_module.body);
        }
        return false;
    }

    // MODULE_BLOCK: check if any statement is a value declaration
    if let Some(block) = arena.get_module_block(body_node)
        && let Some(ref stmts) = block.statements
    {
        for &stmt_idx in &stmts.nodes {
            if let Some(stmt_node) = arena.get(stmt_idx)
                && !is_type_only_module_statement(arena, stmt_node)
            {
                return true;
            }
        }
    }

    false
}

/// Check if a statement inside a module body is purely a type declaration
/// (interface, type alias, type-only import/export, const/declare enum,
/// declare/non-instantiated module).
pub(crate) fn is_type_only_module_statement(arena: &NodeArena, node: &Node) -> bool {
    match node.kind {
        k if k == syntax_kind_ext::INTERFACE_DECLARATION
            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
        {
            true
        }
        k if k == syntax_kind_ext::IMPORT_DECLARATION => {
            if let Some(import_decl) = arena.get_import_decl(node)
                && let Some(clause_node) = arena.get(import_decl.import_clause)
                && let Some(clause) = arena.get_import_clause(clause_node)
            {
                return clause.is_type_only;
            }
            false
        }
        // `import x = require("...")` (external module reference) doesn't instantiate.
        // But `import X = Namespace` (internal alias) DOES instantiate.
        k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
            if let Some(import_decl) = arena.get_import_decl(node) {
                // Check if it's type-only first
                if import_decl.is_type_only {
                    return true;
                }
                // External module reference: `require("...")` produces a string literal
                // as module_specifier. Entity name aliases produce identifiers/qualified names.
                if let Some(ref_node) = arena.get(import_decl.module_specifier) {
                    return ref_node.kind == SyntaxKind::StringLiteral as u16;
                }
            }
            false
        }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            if let Some(export_decl) = arena.get_export_decl(node) {
                if export_decl.is_type_only {
                    return true;
                }
                if let Some(inner_node) = arena.get(export_decl.export_clause) {
                    return is_type_only_module_statement(arena, inner_node);
                }
            }
            false
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enum_decl) = arena.get_enum(node) {
                return arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                    || arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword);
            }
            false
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            if let Some(module_decl) = arena.get_module(node) {
                return arena.has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
                    || !is_instantiated_module(arena, module_decl.body);
            }
            true
        }
        _ => false,
    }
}

/// Get the text of a module specifier (string literal) node.
///
/// Returns `None` if the index is null or the node is not a string literal.
pub(crate) fn module_specifier_text(arena: &NodeArena, specifier: NodeIndex) -> Option<String> {
    if specifier.is_none() {
        return None;
    }
    let node = arena.get(specifier)?;
    let literal = arena.get_literal(node)?;
    Some(literal.text.clone())
}

/// Check if a property member (property assignment, method, or accessor) has a computed name.
pub(crate) fn is_computed_property_member(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    let name_idx = match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            arena.get_property_assignment(node).map(|p| p.name)
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(node).map(|m| m.name)
        }
        k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(node).map(|a| a.name)
        }
        _ => None,
    };

    if let Some(name_idx) = name_idx
        && let Some(name_node) = arena.get(name_idx)
    {
        return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
    }
    false
}

/// Get the text content of a string literal node.
///
/// Returns `None` if the index is null, the node doesn't exist, or the node
/// is not a `StringLiteral`.
pub(crate) fn string_literal_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::StringLiteral as u16 {
        arena.get_literal(node).map(|s| s.text.clone())
    } else {
        None
    }
}

/// Sanitize a module specifier string for use as a JavaScript variable name.
///
/// Strips leading `./` and `../` prefixes, replaces non-identifier characters
/// with `_`, and ensures the result is a valid identifier (non-empty, starts
/// with a letter/`_`/`$`).
pub(crate) fn sanitize_module_name(module_spec: &str) -> String {
    let raw = module_spec
        .trim_start_matches("./")
        .trim_start_matches("../");
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        sanitized.push_str("module");
    }
    let starts_with_invalid_ident = sanitized
        .chars()
        .next()
        .is_some_and(|c| !(c == '_' || c == '$' || c.is_ascii_alphabetic()));
    if starts_with_invalid_ident {
        sanitized.insert(0, '_');
    }
    sanitized
}

/// Check if a node is a spread element or spread assignment.
pub(crate) fn is_spread_element(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };
    node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT || node.kind == syntax_kind_ext::SPREAD_ELEMENT
}

/// Generate the next temporary variable name (`_a`, `_b`, ... `_z`, `_a`, ...) from a counter.
pub(crate) fn next_temp_var_name(counter: &mut u32) -> String {
    let name = format!("_{}", (b'a' + (*counter % 26) as u8) as char);
    *counter += 1;
    name
}

/// Check if a block body (given by `NodeIndex`) has an empty statement list.
pub(crate) fn block_is_empty(arena: &NodeArena, body: NodeIndex) -> bool {
    let Some(body_node) = arena.get(body) else {
        return false;
    };
    let Some(block) = arena.get_block(body_node) else {
        return false;
    };
    block.statements.nodes.is_empty()
}

/// Check if a node is an `AwaitExpression`.
pub(crate) fn is_await_expression(arena: &NodeArena, idx: NodeIndex) -> bool {
    arena
        .get(idx)
        .is_some_and(|n| n.kind == syntax_kind_ext::AWAIT_EXPRESSION)
}

/// Check if a parameter's default initializer is a top-level `await` expression.
pub(crate) fn param_initializer_has_top_level_await(
    arena: &NodeArena,
    param_idx: NodeIndex,
) -> bool {
    let Some(param_node) = arena.get(param_idx) else {
        return false;
    };
    let Some(param) = arena.get_parameter(param_node) else {
        return false;
    };
    if param.initializer.is_none() {
        return false;
    }
    let Some(init_node) = arena.get(param.initializer) else {
        return false;
    };
    init_node.kind == syntax_kind_ext::AWAIT_EXPRESSION
}

/// Find the name of the first parameter whose default initializer is an `await` expression.
///
/// Used by async function lowering to detect the pattern `async function f(x = await y)`
/// which requires special handling (TS2524-related workaround).
pub(crate) fn first_await_default_param_name(
    arena: &NodeArena,
    params: &[NodeIndex],
) -> Option<String> {
    for &param_idx in params {
        let Some(param_node) = arena.get(param_idx) else {
            continue;
        };
        let Some(param) = arena.get_parameter(param_node) else {
            continue;
        };
        if param.initializer.is_none() {
            continue;
        }
        let Some(init_node) = arena.get(param.initializer) else {
            continue;
        };
        if init_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
            continue;
        }
        let Some(name_node) = arena.get(param.name) else {
            continue;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            continue;
        }
        let name = identifier_text_or_empty(arena, param.name);
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Check whether `name` is a valid JavaScript identifier name.
///
/// Returns `true` if `name` starts with `_`, `$`, or an alphabetic char
/// and continues with `_`, `$`, or alphanumeric chars.
pub(crate) fn is_valid_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first == '$' || first.is_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
}

/// Extract a property key from an AST node, using `convert_computed` to transform
/// computed property expressions into IR nodes.
///
/// Handles identifiers, string/numeric literals, and computed property names.
/// The `convert_computed` closure is only called for `ComputedPropertyName` nodes,
/// allowing each caller to use its own expression converter.
pub(crate) fn get_property_key(
    arena: &NodeArena,
    idx: NodeIndex,
    convert_computed: impl FnOnce(NodeIndex) -> Option<crate::transforms::ir::IRNode>,
) -> Option<crate::transforms::ir::IRPropertyKey> {
    use crate::transforms::ir::IRPropertyKey;

    let node = arena.get(idx)?;

    if node.kind == SyntaxKind::Identifier as u16 {
        let ident = arena.get_identifier(node)?;
        return Some(IRPropertyKey::Identifier(ident.escaped_text.clone()));
    }
    if node.kind == SyntaxKind::StringLiteral as u16 {
        let lit = arena.get_literal(node)?;
        return Some(IRPropertyKey::StringLiteral(lit.text.clone()));
    }
    if node.kind == SyntaxKind::NumericLiteral as u16 {
        let lit = arena.get_literal(node)?;
        return Some(IRPropertyKey::NumericLiteral(lit.text.clone()));
    }
    if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
        let computed = arena.get_computed_property(node)?;
        let expr = convert_computed(computed.expression)?;
        return Some(IRPropertyKey::Computed(Box::new(expr)));
    }

    None
}

/// Extract the expression `NodeIndex` of the base class from heritage clauses.
///
/// Walks the heritage clause list, finds the `extends` clause, and returns the
/// expression index of the first type. Callers then convert the expression using
/// their own expression-to-IR conversion method.
///
/// Returns `None` if no `extends` clause is present.
pub(crate) fn get_extends_expression_index(
    arena: &NodeArena,
    heritage_clauses: &Option<tsz_parser::parser::NodeList>,
) -> Option<NodeIndex> {
    let clauses = heritage_clauses.as_ref()?;

    for &clause_idx in &clauses.nodes {
        let clause_node = arena.get(clause_idx)?;
        if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
            continue;
        }
        let heritage_data = arena.get_heritage_clause(clause_node)?;
        if heritage_data.token != SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        let first_type_idx = *heritage_data.types.nodes.first()?;
        let type_node = arena.get(first_type_idx)?;

        // Try as ExpressionWithTypeArguments (e.g., `extends Base` or `extends Base<T>`)
        if let Some(expr_data) = arena.get_expr_type_args(type_node) {
            return Some(expr_data.expression);
        }
        // Fall back to the type node itself (e.g., a plain identifier)
        return Some(first_type_idx);
    }

    None
}

/// Convert an operator token kind (`u16`) to its string representation.
///
/// Covers all binary, unary, assignment, and compound-assignment operators.
/// Returns `""` for unrecognized token kinds.
pub(crate) const fn operator_to_str(op: u16) -> &'static str {
    match op {
        k if k == SyntaxKind::PlusToken as u16 => "+",
        k if k == SyntaxKind::MinusToken as u16 => "-",
        k if k == SyntaxKind::AsteriskToken as u16 => "*",
        k if k == SyntaxKind::SlashToken as u16 => "/",
        k if k == SyntaxKind::PercentToken as u16 => "%",
        k if k == SyntaxKind::AsteriskAsteriskToken as u16 => "**",
        k if k == SyntaxKind::PlusPlusToken as u16 => "++",
        k if k == SyntaxKind::MinusMinusToken as u16 => "--",
        k if k == SyntaxKind::LessThanToken as u16 => "<",
        k if k == SyntaxKind::GreaterThanToken as u16 => ">",
        k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
        k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
        k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
        k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
        k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
        k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
        k if k == SyntaxKind::EqualsToken as u16 => "=",
        k if k == SyntaxKind::PlusEqualsToken as u16 => "+=",
        k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
        k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
        k if k == SyntaxKind::SlashEqualsToken as u16 => "/=",
        k if k == SyntaxKind::PercentEqualsToken as u16 => "%=",
        k if k == SyntaxKind::AmpersandToken as u16 => "&",
        k if k == SyntaxKind::BarToken as u16 => "|",
        k if k == SyntaxKind::CaretToken as u16 => "^",
        k if k == SyntaxKind::TildeToken as u16 => "~",
        k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
        k if k == SyntaxKind::BarBarToken as u16 => "||",
        k if k == SyntaxKind::ExclamationToken as u16 => "!",
        k if k == SyntaxKind::QuestionQuestionToken as u16 => "??",
        k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
        k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
        k if k == SyntaxKind::InstanceOfKeyword as u16 => "instanceof",
        k if k == SyntaxKind::InKeyword as u16 => "in",
        k if k == SyntaxKind::TypeOfKeyword as u16 => "typeof ",
        k if k == SyntaxKind::VoidKeyword as u16 => "void ",
        k if k == SyntaxKind::DeleteKeyword as u16 => "delete ",
        k if k == SyntaxKind::CommaToken as u16 => ",",
        k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**=",
        k if k == SyntaxKind::AmpersandEqualsToken as u16 => "&=",
        k if k == SyntaxKind::BarEqualsToken as u16 => "|=",
        k if k == SyntaxKind::CaretEqualsToken as u16 => "^=",
        k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<=",
        k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>=",
        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => ">>>=",
        k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => "&&=",
        k if k == SyntaxKind::BarBarEqualsToken as u16 => "||=",
        k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => "??=",
        _ => "",
    }
}

/// Scan forward from `start` past whitespace and comments to find the actual
/// token start. Used because AST node positions include leading trivia.
///
/// Handles single-line (`//`) and multi-line (`/* */`) comments.
/// Returns `start` unchanged if `source_text` is `None`.
pub(crate) fn skip_trivia_forward(source_text: Option<&str>, start: u32, end: u32) -> u32 {
    let Some(text) = source_text else {
        return start;
    };
    let bytes = text.as_bytes();
    let mut pos = start as usize;
    let end = std::cmp::min(end as usize, bytes.len());
    while pos < end {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
            b'/' if pos + 1 < end && bytes[pos + 1] == b'/' => {
                // Single-line comment: skip to end of line
                pos += 2;
                while pos < end && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                    pos += 1;
                }
            }
            b'/' if pos + 1 < end && bytes[pos + 1] == b'*' => {
                // Multi-line comment: skip to */
                pos += 2;
                while pos + 1 < end {
                    if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                        pos += 2;
                        break;
                    }
                    pos += 1;
                }
            }
            _ => break,
        }
    }
    pos as u32
}

#[cfg(test)]
#[path = "../../tests/emit_utils.rs"]
mod tests;
