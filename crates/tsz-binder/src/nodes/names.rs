//! AST name collection utilities and modifier helpers for the binder.
//!
//! Provides functions for:
//! - Extracting property/identifier names from AST nodes
//! - Collecting binding identifiers from destructuring patterns
//! - Collecting file-scope names and hoisted names
//! - Collecting symbol nodes for statements/imports/exports
//! - Checking modifier keywords (abstract, static, export, private, declare, const)

use rustc_hash::FxHashSet;
use std::borrow::Cow;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use crate::state::BinderState;

impl BinderState {
    /// Get property name from a node index.
    /// Handles Identifiers, `StringLiterals`, and `NumericLiterals` (normalized).
    pub(crate) fn get_property_name(arena: &NodeArena, idx: NodeIndex) -> Option<Cow<'_, str>> {
        if let Some(node) = arena.get(idx) {
            if let Some(id) = arena.get_identifier(node) {
                return Some(Cow::Borrowed(&id.escaped_text));
            }
            if let Some(lit) = arena.get_literal(node) {
                if node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                    && let Some(val) = lit.value
                {
                    return Some(Cow::Owned(val.to_string()));
                }
                return Some(Cow::Borrowed(&lit.text));
            }
            // Handle computed property names with literal expressions, e.g. ["bar"]
            // Only resolve when the expression is a string/numeric literal, NOT an
            // identifier (which would be a late-bound computed property like [f1]).
            if let Some(computed) = arena.get_computed_property(node)
                && let Some(expr_node) = arena.get(computed.expression)
                && arena.get_literal(expr_node).is_some()
            {
                return Self::get_property_name(arena, computed.expression);
            }
        }
        None
    }

    /// Get identifier name from a node index.
    pub(crate) fn get_identifier_name(arena: &NodeArena, idx: NodeIndex) -> Option<&str> {
        if let Some(node) = arena.get(idx)
            && let Some(id) = arena.get_identifier(node)
        {
            return Some(&id.escaped_text);
        }
        None
    }

    /// Extract names from a heritage clause list (`extends` / `implements`).
    ///
    /// Given the `heritage_clauses: Option<NodeList>` from a class or interface
    /// declaration, walks all heritage clause types and extracts the simple
    /// identifier names. Property-access expressions like `ns.Base` are stored
    /// as dot-separated strings (e.g., `"ns.Base"`).
    ///
    /// Returns an empty `Vec` if there are no heritage clauses or no extractable names.
    /// Collect heritage clause names split by clause kind.
    ///
    /// Returns `(extends_names, implements_names)` so the binder can record
    /// which heritage references are `extends` vs `implements`. This enables
    /// pre-population to wire `DefinitionInfo.extends` and `.implements`
    /// directly, moving class/interface heritage identity from checker-side
    /// type resolution to binder-owned stable identity.
    pub(crate) fn collect_heritage_clause_names_split(
        arena: &NodeArena,
        heritage_clauses: Option<&NodeList>,
    ) -> (Vec<String>, Vec<String>) {
        let clauses = match heritage_clauses {
            Some(c) => c,
            None => return (Vec::new(), Vec::new()),
        };
        let mut extends_names = Vec::new();
        let mut implements_names = Vec::new();
        for &clause_idx in &clauses.nodes {
            let clause_node = match arena.get(clause_idx) {
                Some(n) => n,
                None => continue,
            };
            let heritage_data = match arena.get_heritage_clause(clause_node) {
                Some(d) => d,
                None => continue,
            };
            // ExtendsKeyword = 96 (from tsz_scanner::SyntaxKind)
            let is_extends = heritage_data.token == 96;
            for &type_idx in &heritage_data.types.nodes {
                let type_node = match arena.get(type_idx) {
                    Some(n) => n,
                    None => continue,
                };
                // Heritage clause types are ExpressionWithTypeArguments wrapping an expression.
                let expr_idx = if let Some(expr_data) = arena.get_expr_type_args(type_node) {
                    expr_data.expression
                } else {
                    type_idx
                };
                if let Some(name) = Self::extract_heritage_expression_name(arena, expr_idx) {
                    if is_extends {
                        extends_names.push(name);
                    } else {
                        implements_names.push(name);
                    }
                }
            }
        }
        (extends_names, implements_names)
    }

    /// Extract a name from a heritage expression node.
    ///
    /// Handles simple identifiers (`Foo`) and property-access chains (`ns.Base`).
    fn extract_heritage_expression_name(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        // Simple identifier
        if let Some(id) = arena.get_identifier(node) {
            return Some(id.escaped_text.clone());
        }
        // Property access expression: expression.name (e.g., ns.Base)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
        {
            let lhs = Self::extract_heritage_expression_name(arena, access.expression)?;
            let rhs_node = arena.get(access.name_or_argument)?;
            let rhs_id = arena.get_identifier(rhs_node)?;
            return Some(format!("{}.{}", lhs, rhs_id.escaped_text));
        }
        None
    }

    /// Get the declared name from a declaration node (`ClassDeclaration`,
    /// `FunctionDeclaration`, etc.) by looking up its `name` child identifier.
    pub(crate) fn get_declaration_name(arena: &NodeArena, idx: NodeIndex) -> Option<&str> {
        let node = arena.get(idx)?;
        // Try class/function declarations which have a `name` field
        if let Some(class) = arena.get_class(node) {
            return Self::get_identifier_name(arena, class.name);
        }
        if let Some(func) = arena.get_function(node) {
            return Self::get_identifier_name(arena, func.name);
        }
        None
    }

    pub(crate) fn collect_binding_identifiers(
        arena: &NodeArena,
        idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        if idx.is_none() {
            return;
        }

        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                out.push(idx);
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(binding) = arena.get_binding_element(node) {
                    Self::collect_binding_identifiers(arena, binding.name, out);
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = arena.get_binding_pattern(node) {
                    for &elem in &pattern.elements.nodes {
                        if elem.is_none() {
                            continue;
                        }
                        Self::collect_binding_identifiers(arena, elem, out);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn collect_file_scope_names_for_statements(
        &self,
        arena: &NodeArena,
        statements: &[NodeIndex],
        out: &mut FxHashSet<String>,
    ) {
        for &stmt_idx in statements {
            self.collect_file_scope_names_for_statement(arena, stmt_idx, out);
        }
    }

    pub(crate) fn collect_file_scope_names_for_statement(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node) {
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        Self::collect_variable_decl_names(arena, decl_list_idx, true, out);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && let Some(name) = Self::get_identifier_name(arena, func.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node)
                    && let Some(name) = Self::get_identifier_name(arena, class.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = arena.get_interface(node)
                    && let Some(name) = Self::get_identifier_name(arena, iface.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = arena.get_type_alias(node)
                    && let Some(name) = Self::get_identifier_name(arena, alias.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node)
                    && let Some(name) = Self::get_identifier_name(arena, enum_decl.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node)
                    && let Some(name) = Self::get_identifier_name(arena, module.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                Self::collect_import_names(arena, node, out);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import) = arena.get_import_decl(node)
                    && let Some(name) = Self::get_identifier_name(arena, import.import_clause)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = arena.get_export_decl(node) {
                    if export.export_clause.is_none() {
                        return;
                    }
                    let Some(clause_node) = arena.get(export.export_clause) else {
                        return;
                    };
                    if Self::is_declaration(clause_node.kind) {
                        self.collect_file_scope_names_for_statement(
                            arena,
                            export.export_clause,
                            out,
                        );
                    } else if clause_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name) = Self::get_identifier_name(arena, export.export_clause)
                    {
                        out.insert(name.to_string());
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                self.collect_hoisted_file_scope_names(arena, idx, out);
            }
            _ => {}
        }
    }

    pub(crate) fn collect_hoisted_file_scope_names(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node)
                    && let Some(&decl_list_idx) = var_stmt.declarations.nodes.first()
                {
                    Self::collect_variable_decl_names(arena, decl_list_idx, false, out);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && let Some(name) = Self::get_identifier_name(arena, func.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_hoisted_file_scope_names(arena, stmt_idx, out);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = arena.get_if_statement(node) {
                    self.collect_hoisted_file_scope_from_node(arena, if_stmt.then_statement, out);
                    if if_stmt.else_statement.is_some() {
                        self.collect_hoisted_file_scope_from_node(
                            arena,
                            if_stmt.else_statement,
                            out,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                if let Some(loop_data) = arena.get_loop(node) {
                    self.collect_hoisted_file_scope_from_node(arena, loop_data.statement, out);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn collect_hoisted_file_scope_from_node(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = arena.get_block(node)
        {
            for &stmt_idx in &block.statements.nodes {
                self.collect_hoisted_file_scope_names(arena, stmt_idx, out);
            }
        }
    }

    pub(crate) fn collect_variable_decl_names(
        arena: &NodeArena,
        decl_list_idx: NodeIndex,
        include_block_scoped: bool,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = arena.get_variable(node) else {
            return;
        };
        let is_var = (u32::from(node.flags) & (node_flags::LET | node_flags::CONST)) == 0;
        if !include_block_scoped && !is_var {
            return;
        }

        for &decl_idx in &list.declarations.nodes {
            if let Some(decl) = arena.get_variable_declaration_at(decl_idx) {
                if let Some(name) = Self::get_identifier_name(arena, decl.name) {
                    out.insert(name.to_string());
                } else {
                    let mut names = Vec::new();
                    Self::collect_binding_identifiers(arena, decl.name, &mut names);
                    for ident_idx in names {
                        if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                            out.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_import_names(
        arena: &NodeArena,
        node: &Node,
        out: &mut FxHashSet<String>,
    ) {
        if let Some(import) = arena.get_import_decl(node)
            && let Some(clause_node) = arena.get(import.import_clause)
            && let Some(clause) = arena.get_import_clause(clause_node)
        {
            if clause.name.is_some()
                && let Some(name) = Self::get_identifier_name(arena, clause.name)
            {
                out.insert(name.to_string());
            }
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(name) = Self::get_identifier_name(arena, clause.named_bindings) {
                        out.insert(name.to_string());
                    }
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    for &spec_idx in &named.elements.nodes {
                        if let Some(spec_node) = arena.get(spec_idx)
                            && let Some(spec) = arena.get_specifier(spec_node)
                        {
                            let local_ident = if spec.name.is_none() {
                                spec.property_name
                            } else {
                                spec.name
                            };
                            if let Some(name) = Self::get_identifier_name(arena, local_ident) {
                                out.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_statement_symbol_nodes(
        &self,
        arena: &NodeArena,
        statements: &[NodeIndex],
        out: &mut Vec<NodeIndex>,
    ) {
        for &stmt_idx in statements {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = arena.get_variable(node) {
                        for &decl_list_idx in &var_stmt.declarations.nodes {
                            Self::collect_variable_decl_symbol_nodes(arena, decl_list_idx, out);
                        }
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::MODULE_DECLARATION =>
                {
                    out.push(stmt_idx);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                    Self::collect_import_symbol_nodes(arena, node, out);
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    out.push(stmt_idx);
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    self.collect_export_symbol_nodes(arena, node, out);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn collect_variable_decl_symbol_nodes(
        arena: &NodeArena,
        decl_list_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(list) = arena.get_variable_at(decl_list_idx) else {
            return;
        };

        for &decl_idx in &list.declarations.nodes {
            out.push(decl_idx);
            if let Some(decl) = arena.get_variable_declaration_at(decl_idx) {
                if let Some(_name) = Self::get_identifier_name(arena, decl.name) {
                    out.push(decl.name);
                } else {
                    let mut names = Vec::new();
                    Self::collect_binding_identifiers(arena, decl.name, &mut names);
                    out.extend(names);
                }
            }
        }
    }

    pub(crate) fn collect_import_symbol_nodes(
        arena: &NodeArena,
        node: &Node,
        out: &mut Vec<NodeIndex>,
    ) {
        if let Some(import) = arena.get_import_decl(node)
            && let Some(clause_node) = arena.get(import.import_clause)
            && let Some(clause) = arena.get_import_clause(clause_node)
        {
            if clause.name.is_some() {
                out.push(clause.name);
            }
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    out.push(clause.named_bindings);
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    for &spec_idx in &named.elements.nodes {
                        out.push(spec_idx);
                        if let Some(spec_node) = arena.get(spec_idx)
                            && let Some(spec) = arena.get_specifier(spec_node)
                        {
                            let local_ident = if spec.name.is_none() {
                                spec.property_name
                            } else {
                                spec.name
                            };
                            if local_ident.is_some() {
                                out.push(local_ident);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_export_symbol_nodes(
        &self,
        arena: &NodeArena,
        node: &Node,
        out: &mut Vec<NodeIndex>,
    ) {
        if let Some(export) = arena.get_export_decl(node) {
            if export.export_clause.is_none() {
                return;
            }
            let Some(clause_node) = arena.get(export.export_clause) else {
                return;
            };
            if let Some(named) = arena.get_named_imports(clause_node) {
                for &spec_idx in &named.elements.nodes {
                    out.push(spec_idx);
                }
            } else if Self::is_declaration(clause_node.kind) {
                self.collect_statement_symbol_nodes(arena, &[export.export_clause], out);
            } else if clause_node.kind == SyntaxKind::Identifier as u16 {
                out.push(export.export_clause);
            }
        }
    }

    /// Check if modifiers list contains the 'abstract' keyword.
    pub(crate) fn has_abstract_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::AbstractKeyword)
    }

    /// Check if modifiers list contains the 'static' keyword.
    pub(crate) fn has_static_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::StaticKeyword)
    }

    /// Check if modifiers list contains the 'export' keyword.
    pub(crate) fn has_export_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::ExportKeyword)
    }

    /// Check if modifiers list contains the 'private' keyword.
    pub(crate) fn has_private_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::PrivateKeyword)
    }

    /// Check if modifiers list contains the 'const' keyword.
    pub(crate) fn has_const_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::ConstKeyword)
    }

    /// Check if modifiers list contains the 'protected' keyword.
    pub(crate) fn has_protected_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::ProtectedKeyword)
    }

    /// Check if modifiers list contains the 'declare' keyword.
    ///
    /// Used to capture ambient declaration status in `SemanticDefEntry.is_declare`.
    /// Declarations with `declare` have no runtime representation.
    pub(crate) fn has_declare_modifier(arena: &NodeArena, modifiers: Option<&NodeList>) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// Check if modifiers include a parameter property keyword
    /// (public, private, protected, or readonly).
    pub(crate) fn has_parameter_property_modifier(
        arena: &NodeArena,
        modifiers: Option<&NodeList>,
    ) -> bool {
        arena.has_modifier_ref(modifiers, SyntaxKind::PublicKeyword)
            || arena.has_modifier_ref(modifiers, SyntaxKind::PrivateKeyword)
            || arena.has_modifier_ref(modifiers, SyntaxKind::ProtectedKeyword)
            || arena.has_modifier_ref(modifiers, SyntaxKind::ReadonlyKeyword)
    }
}
