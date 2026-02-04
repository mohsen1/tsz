//! Usage Analyzer for Import/Export Elision
//!
//! Analyzes exported declarations to determine which imports are actually used
//! in the public API surface. This prevents "Module not found" errors by eliding
//! unused imports from .d.ts files, matching TypeScript's behavior.
//!
//! # Architecture
//!
//! Uses a **Hybrid Walk** strategy:
//! 1. **AST Walk**: For explicit type annotations (e.g., `x: SomeType`)
//! 2. **Semantic Walk**: For inferred types using `TypeId` analysis
//!
//! The semantic walk leverages `collect_all_types()` from the solver to extract
//! all referenced types, then maps `DefId` -> `SymbolId` via `TypeResolver`.

use crate::binder::SymbolId;
use crate::checker::context::CheckerContext;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::visitor;
use rustc_hash::FxHashSet;

/// Usage analyzer for determining which symbols are referenced in exported declarations.
pub struct UsageAnalyzer<'a, 'ctx> {
    /// AST arena for walking explicit type annotations
    arena: &'a NodeArena,
    /// Checker context for accessing TypeResolver and node_types
    ctx: &'ctx CheckerContext<'ctx>,
    /// Set of symbols used in the exported API surface
    used_symbols: FxHashSet<SymbolId>,
    /// Visited AST nodes (for cycle detection)
    visited_nodes: FxHashSet<NodeIndex>,
    /// Visited TypeIds (for cycle detection)
    visited_types: FxHashSet<crate::solver::TypeId>,
}

impl<'a, 'ctx> UsageAnalyzer<'a, 'ctx> {
    /// Create a new usage analyzer.
    pub fn new(arena: &'a NodeArena, ctx: &'ctx CheckerContext<'ctx>) -> Self {
        Self {
            arena,
            ctx,
            used_symbols: FxHashSet::default(),
            visited_nodes: FxHashSet::default(),
            visited_types: FxHashSet::default(),
        }
    }

    /// Analyze all exported declarations in a source file.
    ///
    /// Returns the set of SymbolIds that are referenced in the public API.
    pub fn analyze(&mut self, root_idx: NodeIndex) -> &FxHashSet<SymbolId> {
        let Some(root_node) = self.arena.get(root_idx) else {
            return &self.used_symbols;
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return &self.used_symbols;
        };

        // Walk all statements to find exported declarations
        for &stmt_idx in &source_file.statements.nodes {
            self.analyze_statement(stmt_idx);
        }

        &self.used_symbols
    }

    /// Analyze a single statement to find exported declarations.
    fn analyze_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            // Exported declarations
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.analyze_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.analyze_class_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.analyze_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.analyze_type_alias_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.analyze_enum_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.analyze_variable_statement(stmt_idx);
            }
            // Export declarations (re-exports are always used)
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Re-exports don't need analysis - they're part of the API
            }
            _ => {}
        }
    }

    /// Analyze a function declaration.
    fn analyze_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = func.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk parameters
        for &param_idx in &func.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type (explicit or inferred)
        if !func.type_annotation.is_none() {
            self.analyze_type_node(func.type_annotation);
        } else {
            // No explicit annotation - use inferred type from node_types
            self.walk_inferred_type(func_idx);
        }
    }

    /// Analyze a class declaration.
    fn analyze_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = class.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk heritage clauses (extends, implements)
        if let Some(ref heritage) = class.heritage_clauses {
            self.analyze_heritage_clauses(heritage);
        }

        // Walk ALL members (including private - they can have type annotations referencing external types)
        for &member_idx in &class.members.nodes {
            self.analyze_class_member(member_idx);
        }
    }

    /// Analyze a class member.
    fn analyze_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.analyze_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.analyze_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.analyze_constructor(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.analyze_accessor(member_idx);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.analyze_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Analyze a property declaration.
    fn analyze_property_declaration(&mut self, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };

        // Walk type annotation (explicit or inferred)
        if !prop.type_annotation.is_none() {
            self.analyze_type_node(prop.type_annotation);
        } else {
            self.walk_inferred_type(prop_idx);
        }

        // For computed properties, also walk the inferred type to catch symbols in the name expression
        // This handles cases like [Symbol.iterator]: Type where Symbol needs to be marked as used
        // Check the name node to see if it's a computed property
        if let Some(name_node) = self.arena.get(prop.name) {
            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                self.walk_inferred_type(prop_idx);
            }
        }
    }

    /// Analyze a method declaration.
    fn analyze_method_declaration(&mut self, method_idx: NodeIndex) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = method.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk parameters
        for &param_idx in &method.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type
        if !method.type_annotation.is_none() {
            self.analyze_type_node(method.type_annotation);
        } else {
            self.walk_inferred_type(method_idx);
        }
    }

    /// Analyze a constructor.
    fn analyze_constructor(&mut self, ctor_idx: NodeIndex) {
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Walk parameters
        for &param_idx in &ctor.parameters.nodes {
            self.analyze_parameter(param_idx);
        }
    }

    /// Analyze an accessor (getter/setter).
    fn analyze_accessor(&mut self, accessor_idx: NodeIndex) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        // Walk parameters
        for &param_idx in &accessor.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type (for getters)
        if !accessor.type_annotation.is_none() {
            self.analyze_type_node(accessor.type_annotation);
        }
    }

    /// Analyze an index signature.
    fn analyze_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        // Walk parameter type
        for &param_idx in &sig.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type
        if !sig.type_annotation.is_none() {
            self.analyze_type_node(sig.type_annotation);
        }
    }

    /// Analyze an interface declaration.
    fn analyze_interface_declaration(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = iface.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk heritage clauses
        if let Some(ref heritage) = iface.heritage_clauses {
            self.analyze_heritage_clauses(heritage);
        }

        // Walk members
        for &member_idx in &iface.members.nodes {
            self.analyze_interface_member(member_idx);
        }
    }

    /// Analyze an interface member.
    fn analyze_interface_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    if !sig.type_annotation.is_none() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Walk type parameters
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    // Walk parameters
                    if let Some(ref params) = sig.parameters {
                        for &param_idx in &params.nodes {
                            self.analyze_parameter(param_idx);
                        }
                    }
                    // Walk return type
                    if !sig.type_annotation.is_none() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
            {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Walk type parameters
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    // Walk parameters
                    if let Some(ref params) = sig.parameters {
                        for &param_idx in &params.nodes {
                            self.analyze_parameter(param_idx);
                        }
                    }
                    // Walk return type
                    if !sig.type_annotation.is_none() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.analyze_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Analyze a type alias declaration.
    fn analyze_type_alias_declaration(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = alias.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk the aliased type
        self.analyze_type_node(alias.type_node);
    }

    /// Analyze an enum declaration.
    fn analyze_enum_declaration(&mut self, _enum_idx: NodeIndex) {
        // Enum declarations don't reference other types in their signature
    }

    /// Analyze a variable statement.
    fn analyze_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                for &decl_idx in &decl_list.declarations.nodes {
                    self.analyze_variable_declaration(decl_idx);
                }
            }
        }
    }

    /// Analyze a variable declaration.
    fn analyze_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        // Walk type annotation
        if !decl.type_annotation.is_none() {
            self.analyze_type_node(decl.type_annotation);
        } else {
            self.walk_inferred_type(decl_idx);
        }
    }

    /// Analyze heritage clauses (extends/implements).
    fn analyze_heritage_clauses(&mut self, clauses: &crate::parser::NodeList) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            for &type_idx in &heritage.types.nodes {
                self.analyze_type_node(type_idx);
            }
        }
    }

    /// Analyze a type parameter.
    fn analyze_type_parameter(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_type_parameter(param_node) else {
            return;
        };

        // Walk constraint
        if !param.constraint.is_none() {
            self.analyze_type_node(param.constraint);
        }

        // Walk default type
        if !param.default.is_none() {
            self.analyze_type_node(param.default);
        }
    }

    /// Analyze a parameter.
    fn analyze_parameter(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return;
        };

        // Walk type annotation
        if !param.type_annotation.is_none() {
            self.analyze_type_node(param.type_annotation);
        } else {
            self.walk_inferred_type(param_idx);
        }
    }

    /// Analyze a type node (AST walk for explicit types).
    fn analyze_type_node(&mut self, type_idx: NodeIndex) {
        if !self.visited_nodes.insert(type_idx) {
            return;
        }

        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        match type_node.kind {
            // Type references - extract the symbol
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    self.analyze_entity_name(type_ref.type_name);
                }
            }

            // Expression with type arguments (heritage clauses)
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(type_node) {
                    self.analyze_entity_name(expr.expression);
                    if let Some(ref type_args) = expr.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
                }
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.arena.get_array_type(type_node) {
                    self.analyze_type_node(arr.element_type);
                }
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                if let Some(union) = self.arena.get_composite_type(type_node) {
                    for &type_idx in &union.types.nodes {
                        self.analyze_type_node(type_idx);
                    }
                }
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(inter) = self.arena.get_composite_type(type_node) {
                    for &type_idx in &inter.types.nodes {
                        self.analyze_type_node(type_idx);
                    }
                }
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(type_node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.analyze_type_node(elem_idx);
                    }
                }
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if let Some(ref type_params) = func.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    for &param_idx in &func.parameters.nodes {
                        self.analyze_parameter(param_idx);
                    }
                    self.analyze_type_node(func.type_annotation);
                }
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.arena.get_type_literal(type_node) {
                    for &member_idx in &lit.members.nodes {
                        self.analyze_interface_member(member_idx);
                    }
                }
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(paren) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(paren.type_node);
                }
            }

            // Type query (typeof X) - marks X as value usage
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(type_query) = self.arena.get_type_query(type_node) {
                    self.analyze_entity_name(type_query.expr_name);
                    // TODO: Walk type arguments
                }
            }

            // Type operator (keyof, readonly, etc.)
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(type_op) = self.arena.get_type_operator(type_node) {
                    self.analyze_type_node(type_op.type_node);
                }
            }

            // Indexed access type (T[K])
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed_access) = self.arena.get_indexed_access_type(type_node) {
                    self.analyze_type_node(indexed_access.object_type);
                    self.analyze_type_node(indexed_access.index_type);
                }
            }

            // Mapped type
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(type_node) {
                    self.analyze_type_parameter(mapped_type.type_parameter);
                    self.analyze_type_node(mapped_type.type_node);
                    if !mapped_type.name_type.is_none() {
                        self.analyze_type_node(mapped_type.name_type);
                    }
                }
            }

            // Conditional type
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(conditional) = self.arena.get_conditional_type(type_node) {
                    self.analyze_type_node(conditional.check_type);
                    self.analyze_type_node(conditional.extends_type);
                    self.analyze_type_node(conditional.true_type);
                    self.analyze_type_node(conditional.false_type);
                }
            }

            // Infer type
            k if k == syntax_kind_ext::INFER_TYPE => {
                // Infer type doesn't reference external symbols
            }

            // Keyword types (no external references)
            k if k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 => {}

            // Literal types
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16 => {}

            _ => {}
        }
    }

    /// Analyze an entity name to extract the leftmost symbol.
    ///
    /// For `A.B.C`, we need to mark `A` as used (otherwise `import * as A` gets elided).
    fn analyze_entity_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                // Found the leftmost identifier - mark as used
                if let Some(sym_id) = self.ctx.binder.get_node_symbol(name_idx) {
                    self.mark_symbol_used(sym_id);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(name_node) {
                    // Recurse to find leftmost identifier
                    self.analyze_entity_name(name.left);
                    // Also mark right side (for cases like `namespace.Class`)
                    self.analyze_entity_name(name.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(name_node) {
                    self.analyze_entity_name(access.expression);
                    self.analyze_entity_name(access.name_or_argument);
                }
            }
            _ => {}
        }
    }

    /// Walk an inferred type from the type cache.
    ///
    /// This is the semantic walk - uses TypeId analysis via collect_all_types.
    fn walk_inferred_type(&mut self, node_idx: NodeIndex) {
        // Look up the inferred TypeId for this node
        if let Some(&type_id) = self.ctx.node_types.get(&node_idx.0) {
            self.walk_type_id(type_id);
        }
    }

    /// Walk a TypeId to extract all referenced symbols.
    ///
    /// Uses collect_all_types() to get all TypeIds, then extracts DefIds/SymbolIds.
    fn walk_type_id(&mut self, type_id: crate::solver::TypeId) {
        if !self.visited_types.insert(type_id) {
            return;
        }

        // Collect all types reachable from this TypeId
        let all_types = visitor::collect_all_types(self.ctx.types, type_id);

        // Extract DefIds/SymbolIds from each type
        for other_type_id in all_types {
            // Extract Lazy(DefId)
            if let Some(def_id) = visitor::lazy_def_id(self.ctx.types, other_type_id) {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    self.mark_symbol_used(sym_id);
                }
            }

            // Extract Enum(DefId, _)
            if let Some((def_id, _)) = visitor::enum_components(self.ctx.types, other_type_id) {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    self.mark_symbol_used(sym_id);
                }
            }

            // Extract TypeQuery(SymbolRef) - marks as value usage
            if let Some(sym_ref) = visitor::type_query_symbol(self.ctx.types, other_type_id) {
                let sym_id = crate::binder::SymbolId(sym_ref.0);
                self.mark_symbol_used(sym_id);
            }

            // Extract UniqueSymbol(SymbolRef)
            if let Some(sym_ref) = visitor::unique_symbol_ref(self.ctx.types, other_type_id) {
                let sym_id = crate::binder::SymbolId(sym_ref.0);
                self.mark_symbol_used(sym_id);
            }

            // Extract ModuleNamespace(SymbolRef) - marks namespace import as used
            if let Some(type_key) = self.ctx.types.lookup(other_type_id) {
                if let crate::solver::TypeKey::ModuleNamespace(sym_ref) = type_key {
                    let sym_id = crate::binder::SymbolId(sym_ref.0);
                    self.mark_symbol_used(sym_id);
                }
            }
        }
    }

    /// Mark a symbol as used in the public API.
    fn mark_symbol_used(&mut self, sym_id: SymbolId) {
        self.used_symbols.insert(sym_id);
    }
}
