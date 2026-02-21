//! Helper functions for the namespace ES5 IR transformer.
//!
//! Contains utility functions for namespace body analysis, modifier checking,
//! variable declaration conversion, export rewriting, and parameter renaming.

use super::*;

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if a namespace body (`MODULE_BLOCK`) contains any value declarations.
/// Value declarations are: variables, functions, classes, enums, sub-namespaces.
/// Type-only declarations (interfaces, type aliases) don't count.
pub(super) fn body_has_value_declarations(arena: &NodeArena, body_idx: NodeIndex) -> bool {
    let Some(body_node) = arena.get(body_idx) else {
        return false;
    };

    let Some(block_data) = arena.get_module_block(body_node) else {
        return false;
    };

    let Some(stmts) = block_data.statements.as_ref() else {
        return false;
    };

    for &stmt_idx in &stmts.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION =>
            {
                return true;
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                // Recursively check nested namespaces when they contain runtime members.
                if let Some(ns_data) = arena.get_module(stmt_node)
                    && body_has_value_declarations(arena, ns_data.body)
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Check if the exported declaration is a value declaration
                if let Some(export_data) = arena.get_export_decl(stmt_node)
                    && let Some(inner_node) = arena.get(export_data.export_clause)
                {
                    match inner_node.kind {
                        k if k == syntax_kind_ext::VARIABLE_STATEMENT
                            || k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION =>
                        {
                            return true;
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            if let Some(ns_data) = arena.get_module(inner_node)
                                && body_has_value_declarations(arena, ns_data.body)
                            {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    false
}

/// Check if an IR node is a comment (standalone or trailing).
/// Used to determine if a namespace body has only comments and no actual code.
pub(super) fn is_comment_node(node: &IRNode) -> bool {
    matches!(node, IRNode::Raw(s) if s.starts_with("//") || s.starts_with("/*"))
        || matches!(node, IRNode::TrailingComment(_))
}

/// Check if a node is a namespace-like declaration (`MODULE_DECLARATION` or
/// `EXPORT_DECLARATION` wrapping `MODULE_DECLARATION`). These have block bodies
/// whose internal comments are handled by the sub-emitter.
pub(super) fn is_namespace_like(arena: &NodeArena, node: &tsz_parser::parser::node::Node) -> bool {
    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
        return true;
    }
    if node.kind == syntax_kind_ext::EXPORT_DECLARATION
        && let Some(export_data) = arena.get_export_decl(node)
        && let Some(inner) = arena.get(export_data.export_clause)
    {
        return inner.kind == syntax_kind_ext::MODULE_DECLARATION;
    }
    false
}

pub(super) fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

pub(super) fn has_modifier(arena: &NodeArena, modifiers: &Option<NodeList>, kind: u16) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == kind
            {
                return true;
            }
        }
    }
    false
}

pub(super) fn has_declare_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::DeclareKeyword as u16)
}

pub(super) fn has_export_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::ExportKeyword as u16)
}

/// Convert function parameters to IR parameters (without type annotations)
pub(super) fn convert_function_parameters(arena: &NodeArena, params: &NodeList) -> Vec<IRParam> {
    params
        .nodes
        .iter()
        .filter_map(|&p| {
            let param = arena.get_parameter_at(p)?;
            let name = get_identifier_text(arena, param.name)?;
            let rest = param.dot_dot_dot_token;
            // Convert default value if present
            let default_value = (param.initializer.is_some())
                .then(|| Box::new(AstToIr::new(arena).convert_expression(param.initializer)));
            Some(IRParam {
                name,
                rest,
                default_value,
            })
        })
        .collect()
}

/// Convert function body to IR statements (without type annotations)
pub(super) fn convert_function_body(arena: &NodeArena, body_idx: NodeIndex) -> Vec<IRNode> {
    let Some(body_node) = arena.get(body_idx) else {
        return vec![];
    };

    // Handle both Block and syntax_kind_ext::BLOCK
    if body_node.kind == syntax_kind_ext::BLOCK
        && let Some(block) = arena.get_block(body_node)
    {
        return block
            .statements
            .nodes
            .iter()
            .map(|&s| AstToIr::new(arena).convert_statement(s))
            .collect();
    }

    // Fallback for unsupported body types
    vec![]
}

pub(super) fn collect_runtime_exported_var_names(
    arena: &NodeArena,
    body_idx: NodeIndex,
) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();

    let Some(body_node) = arena.get(body_idx) else {
        return names;
    };
    let Some(block_data) = arena.get_module_block(body_node) else {
        return names;
    };
    let Some(stmts) = block_data.statements.as_ref() else {
        return names;
    };

    for &stmt_idx in &stmts.nodes {
        collect_runtime_exported_var_names_in_stmt(arena, stmt_idx, &mut names);
    }

    names
}

pub(super) fn collect_runtime_exported_var_names_in_stmt(
    arena: &NodeArena,
    stmt_idx: NodeIndex,
    names: &mut std::collections::HashSet<String>,
) {
    let Some(stmt_node) = arena.get(stmt_idx) else {
        return;
    };

    let collect_from_var_statement =
        |node: &Node, names: &mut std::collections::HashSet<String>| {
            if let Some(var_data) = arena.get_variable(node) {
                for &decl_list_idx in &var_data.declarations.nodes {
                    if let Some(decl_list_node) = arena.get(decl_list_idx)
                        && let Some(decl_list) = arena.get_variable(decl_list_node)
                    {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = arena.get(decl_idx)
                                && let Some(decl) = arena.get_variable_declaration(decl_node)
                                && let Some(name_node) = arena.get(decl.name)
                            {
                                let Some(name) = arena
                                    .get_identifier(name_node)
                                    .map(|id| id.escaped_text.clone())
                                else {
                                    continue;
                                };
                                if decl.initializer.is_none() {
                                    continue;
                                }
                                names.insert(name);
                            }
                        }
                    }
                }
            }
        };

    match stmt_node.kind {
        kind if kind == syntax_kind_ext::VARIABLE_STATEMENT => {
            collect_from_var_statement(stmt_node, names);
        }
        kind if kind == syntax_kind_ext::EXPORT_DECLARATION => {
            if let Some(export_data) = arena.get_export_decl(stmt_node)
                && let Some(inner_node) = arena.get(export_data.export_clause)
                && inner_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            {
                collect_from_var_statement(inner_node, names);
            }
        }
        _ => {}
    }
}

/// Convert exported variable declarations directly to namespace property assignments.
/// Instead of `var X = init; NS.X = X;`, emits `NS.X = init;` (matching tsc).
pub(super) fn convert_exported_variable_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
    ns_name: &str,
) -> Vec<IRNode> {
    let mut result = Vec::new();
    let mut assignment_targets: Vec<(String, IRNode)> = Vec::new();

    for &decl_list_idx in &declarations.nodes {
        if let Some(decl_list_node) = arena.get(decl_list_idx)
            && let Some(decl_list) = arena.get_variable(decl_list_node)
        {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = arena.get(decl_idx)
                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                    && let Some(name) = get_identifier_text(arena, decl.name)
                    && decl.initializer.is_some()
                {
                    let value = AstToIr::new(arena).convert_expression(decl.initializer);
                    assignment_targets.push((name, value));
                }
                // No initializer: tsc omits the assignment entirely in namespaces
            }
        }
    }

    if assignment_targets.is_empty() {
        return result;
    }

    if assignment_targets.len() == 1 {
        let (name, value) = assignment_targets.remove(0);
        return vec![IRNode::NamespaceExport {
            namespace: ns_name.to_string(),
            name,
            value: Box::new(value),
        }];
    }

    let parts: Vec<String> = assignment_targets
        .into_iter()
        .map(|(name, value)| format!("{}.{name} = {}", ns_name, IRPrinter::emit_to_string(&value)))
        .collect();
    result.push(IRNode::Raw(format!("{};", parts.join(", "))));

    result
}

/// Convert variable declarations to proper IR (`VarDecl` nodes)
pub(super) fn convert_variable_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
    empty_decl_keyword: &str,
) -> Vec<IRNode> {
    let mut result = Vec::new();

    for &decl_list_idx in &declarations.nodes {
        let decl_list_node = arena.get(decl_list_idx);
        if let (Some(decl_list_node), Some(decl_list)) =
            (decl_list_node, arena.get_variable_at(decl_list_idx))
        {
            let mut emitted_any = false;
            let keyword = if decl_list_node.flags == 0 {
                empty_decl_keyword
            } else {
                declaration_keyword_from_flags(decl_list_node.flags)
            };
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl) = arena.get_variable_declaration_at(decl_idx)
                    && let Some(name) = get_identifier_text(arena, decl.name)
                {
                    // Use AstToIr for eager lowering of initializers
                    // This converts expressions to proper IR (NumericLiteral, CallExpr, etc.)
                    let initializer = (decl.initializer.is_some()).then(|| {
                        Box::new(AstToIr::new(arena).convert_expression(decl.initializer))
                    });

                    result.push(IRNode::VarDecl { name, initializer });
                    emitted_any = true;
                }
            }

            if !emitted_any && decl_list.declarations.nodes.is_empty() {
                // Preserve declaration-shape recovery output such as `var ;` / `let;`.
                if keyword == "var" {
                    result.push(IRNode::Raw("var ;".to_string()));
                } else {
                    result.push(IRNode::Raw(format!("{keyword};")));
                }
            }
        }
    }

    result
}

const fn declaration_keyword_from_flags(flags: u16) -> &'static str {
    if (flags & node_flags::LET as u16) != 0 {
        "let"
    } else {
        // TypeScript emits `const` declarations as `var` in emitted JS output.
        "var"
    }
}

pub(super) fn declaration_keyword_from_var_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
) -> &'static str {
    for &decl_list_idx in &declarations.nodes {
        let Some(decl_list_node) = arena.get(decl_list_idx) else {
            continue;
        };

        if arena.get_variable_at(decl_list_idx).is_some() {
            let keyword = declaration_keyword_from_flags(decl_list_node.flags);
            if keyword == "let" {
                return "let";
            }
        }
    }

    "var"
}

// =============================================================================
// Namespace IIFE parameter collision detection and renaming
// =============================================================================

/// Collect all member names declared in the namespace body IR.
/// These are names that would clash with the IIFE parameter if they match the namespace name.
pub(super) fn collect_body_member_names(body: &[IRNode]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for node in body {
        collect_member_names_from_node(node, &mut names);
    }
    names
}

/// Recursively collect declared names from IR nodes
pub(super) fn collect_member_names_from_node(
    node: &IRNode,
    names: &mut std::collections::HashSet<String>,
) {
    match node {
        IRNode::ES5ClassIIFE { name, .. }
        | IRNode::FunctionDecl { name, .. }
        | IRNode::VarDecl { name, .. }
        | IRNode::EnumIIFE { name, .. } => {
            names.insert(name.clone());
        }
        IRNode::Sequence(items) => {
            for item in items {
                collect_member_names_from_node(item, names);
            }
        }
        _ => {}
    }
}

/// Generate a unique parameter name by appending `_1`, `_2`, etc.
/// Ensures the generated name doesn't collide with any existing member name.
pub(super) fn generate_unique_param_name(
    ns_name: &str,
    member_names: &std::collections::HashSet<String>,
) -> String {
    let mut suffix = 1;
    loop {
        let candidate = format!("{ns_name}_{suffix}");
        if !member_names.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Rename namespace references in body IR nodes.
/// Updates `NamespaceExport.namespace` and nested `NamespaceIIFE.parent_name`
/// from `old_name` to `new_name`.
pub(super) fn rename_namespace_refs_in_body(body: &mut [IRNode], old_name: &str, new_name: &str) {
    for node in body.iter_mut() {
        rename_namespace_refs_in_node(node, old_name, new_name);
    }
}

/// Recursively rename namespace references in a single IR node
pub(super) fn rename_namespace_refs_in_node(node: &mut IRNode, old_name: &str, new_name: &str) {
    match node {
        IRNode::NamespaceExport { namespace, .. } => {
            if namespace == old_name {
                *namespace = new_name.to_string();
            }
        }
        IRNode::NamespaceIIFE { parent_name, .. } => {
            if let Some(parent) = parent_name
                && parent == old_name
            {
                *parent = new_name.to_string();
            }
        }
        IRNode::Sequence(items) => {
            for item in items.iter_mut() {
                rename_namespace_refs_in_node(item, old_name, new_name);
            }
        }
        _ => {}
    }
}

/// Detect collision between namespace name and body member names,
/// and if found, rename the body's namespace references and return the new parameter name.
pub(super) fn detect_and_apply_param_rename(body: &mut [IRNode], ns_name: &str) -> Option<String> {
    let member_names = collect_body_member_names(body);
    member_names.contains(ns_name).then(|| {
        let renamed = generate_unique_param_name(ns_name, &member_names);
        rename_namespace_refs_in_body(body, ns_name, &renamed);
        renamed
    })
}

pub(super) fn rewrite_exported_var_refs(
    node: &mut IRNode,
    ns_name: &str,
    names: &std::collections::HashSet<String>,
) {
    match node {
        IRNode::Identifier(name) => {
            if names.contains(name) {
                let property = name.clone();
                *node = IRNode::PropertyAccess {
                    object: Box::new(IRNode::Identifier(ns_name.to_string())),
                    property,
                };
            }
        }
        IRNode::BinaryExpr { left, right, .. }
        | IRNode::LogicalOr { left, right }
        | IRNode::LogicalAnd { left, right } => {
            rewrite_exported_var_refs(left, ns_name, names);
            rewrite_exported_var_refs(right, ns_name, names);
        }
        IRNode::PrefixUnaryExpr { operand, .. } | IRNode::PostfixUnaryExpr { operand, .. } => {
            rewrite_exported_var_refs(operand, ns_name, names);
        }
        IRNode::CallExpr {
            callee, arguments, ..
        }
        | IRNode::NewExpr {
            callee, arguments, ..
        } => {
            rewrite_exported_var_refs(callee, ns_name, names);
            for arg in arguments {
                rewrite_exported_var_refs(arg, ns_name, names);
            }
        }
        IRNode::PropertyAccess { object, .. } => rewrite_exported_var_refs(object, ns_name, names),
        IRNode::ElementAccess { object, index } => {
            rewrite_exported_var_refs(object, ns_name, names);
            rewrite_exported_var_refs(index, ns_name, names);
        }
        IRNode::ConditionalExpr {
            condition,
            when_true,
            when_false,
            ..
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(when_true, ns_name, names);
            rewrite_exported_var_refs(when_false, ns_name, names);
        }
        IRNode::Parenthesized(inner) | IRNode::SpreadElement(inner) => {
            rewrite_exported_var_refs(inner, ns_name, names)
        }
        IRNode::CommaExpr(exprs) | IRNode::ArrayLiteral(exprs) => {
            for expr in exprs.iter_mut() {
                rewrite_exported_var_refs(expr, ns_name, names);
            }
        }
        IRNode::ObjectLiteral { properties, .. } => {
            for prop in properties.iter_mut() {
                if let IRPropertyKey::Computed(key) = &mut prop.key {
                    rewrite_exported_var_refs(key, ns_name, names);
                }
                rewrite_exported_var_refs(&mut prop.value, ns_name, names);
            }
        }
        IRNode::FunctionExpr { body, .. }
        | IRNode::FunctionDecl { body, .. }
        | IRNode::NamespaceIIFE { body, .. }
        | IRNode::ES5ClassIIFE { body, .. } => {
            for stmt in body {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::VarDecl {
            initializer: Some(initializer),
            ..
        } => {
            rewrite_exported_var_refs(initializer, ns_name, names);
        }
        IRNode::VarDeclList(items) => {
            for item in items {
                rewrite_exported_var_refs(item, ns_name, names);
            }
        }
        IRNode::ExpressionStatement(expr)
        | IRNode::ReturnStatement(Some(expr))
        | IRNode::ThrowStatement(expr) => {
            rewrite_exported_var_refs(expr, ns_name, names);
        }
        IRNode::AwaiterCall {
            this_arg,
            generator_body,
            ..
        } => {
            rewrite_exported_var_refs(this_arg, ns_name, names);
            rewrite_exported_var_refs(generator_body, ns_name, names);
        }
        IRNode::IfStatement {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(then_branch, ns_name, names);
            if let Some(else_branch) = else_branch {
                rewrite_exported_var_refs(else_branch, ns_name, names);
            }
        }
        IRNode::Block(statements) | IRNode::Sequence(statements) => {
            for stmt in statements {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::ForStatement {
            initializer,
            condition,
            incrementor,
            body,
        } => {
            if let Some(init) = initializer {
                rewrite_exported_var_refs(init, ns_name, names);
            }
            if let Some(cond) = condition {
                rewrite_exported_var_refs(cond, ns_name, names);
            }
            if let Some(inc) = incrementor {
                rewrite_exported_var_refs(inc, ns_name, names);
            }
            rewrite_exported_var_refs(body, ns_name, names);
        }
        IRNode::WhileStatement { condition, body }
        | IRNode::DoWhileStatement {
            body: condition,
            condition: body,
            ..
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(body, ns_name, names);
        }
        IRNode::TryStatement {
            try_block,
            catch_clause,
            finally_block,
        } => {
            rewrite_exported_var_refs(try_block, ns_name, names);
            if let Some(catch) = catch_clause {
                if let Some(param) = catch.param.as_ref() {
                    let _ = param;
                }
                for stmt in &mut catch.body {
                    rewrite_exported_var_refs(stmt, ns_name, names);
                }
            }
            if let Some(finally_block) = finally_block {
                rewrite_exported_var_refs(finally_block, ns_name, names);
            }
        }
        IRNode::LabeledStatement { statement, .. } => {
            rewrite_exported_var_refs(statement, ns_name, names);
        }
        IRNode::SwitchStatement { expression, cases } => {
            rewrite_exported_var_refs(expression, ns_name, names);
            for case in cases {
                if let Some(ref mut test) = case.test {
                    rewrite_exported_var_refs(test, ns_name, names);
                }
                for stmt in &mut case.statements {
                    rewrite_exported_var_refs(stmt, ns_name, names);
                }
            }
        }
        IRNode::NamespaceExport { value, .. } => {
            rewrite_exported_var_refs(value, ns_name, names);
        }
        IRNode::PrototypeMethod { function, .. } | IRNode::StaticMethod { function, .. } => {
            rewrite_exported_var_refs(function, ns_name, names);
        }
        IRNode::DefineProperty {
            target, descriptor, ..
        } => {
            rewrite_exported_var_refs(target, ns_name, names);
            if let Some(getter) = &mut descriptor.get {
                rewrite_exported_var_refs(getter, ns_name, names);
            }
            if let Some(setter) = &mut descriptor.set {
                rewrite_exported_var_refs(setter, ns_name, names);
            }
        }
        IRNode::EnumIIFE { members, .. } => {
            for member in members {
                if let EnumMemberValue::Computed(expr) = &mut member.value {
                    rewrite_exported_var_refs(expr, ns_name, names);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn collect_qualified_name_parts(
    arena: &NodeArena,
    name_idx: NodeIndex,
) -> Option<Vec<String>> {
    let node = arena.get(name_idx)?;

    if node.kind == SyntaxKind::Identifier as u16 {
        if let Some(id) = arena.get_identifier(node) {
            return Some(vec![id.escaped_text.clone()]);
        }
        return None;
    }

    if node.kind == syntax_kind_ext::QUALIFIED_NAME
        && let Some(qn) = arena.qualified_names.get(node.data_index as usize)
    {
        let mut left = collect_qualified_name_parts(arena, qn.left)?;
        let right = collect_qualified_name_parts(arena, qn.right)?;
        left.extend(right);
        return Some(left);
    }

    None
}

pub(super) fn namespace_body_by_name(
    arena: &NodeArena,
    target_parts: &[String],
) -> Option<NodeIndex> {
    if target_parts.is_empty() {
        return None;
    }

    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            continue;
        };

        if let Some((parts, body_idx)) =
            collect_module_decl_parts_for_body_lookup(arena, NodeIndex(idx as u32))
            && parts == target_parts
        {
            return Some(body_idx);
        }
    }

    None
}

pub(super) fn collect_module_decl_parts_for_body_lookup(
    arena: &NodeArena,
    ns_idx: NodeIndex,
) -> Option<(Vec<String>, NodeIndex)> {
    let mut parts = Vec::new();
    let mut current_idx = ns_idx;

    loop {
        let node = arena.get(current_idx)?;
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            break;
        }

        let ns_data = arena.get_module(node)?;

        if let Some(name_node) = arena.get(ns_data.name)
            && let Some(id) = arena.get_identifier(name_node)
        {
            parts.push(id.escaped_text.clone());
        }

        let body_node = arena.get(ns_data.body)?;
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            current_idx = ns_data.body;
        } else {
            return Some((parts, ns_data.body));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some((parts, current_idx))
    }
}
