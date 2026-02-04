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

use crate::binder::{BinderState, SymbolId};
use crate::checker::TypeCache;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeInterner;
use crate::solver::visitor;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

/// Tracks how a symbol is used - as a type, a value, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageKind {
    bits: u8,
}

impl UsageKind {
    pub const NONE: UsageKind = UsageKind { bits: 0 };
    pub const TYPE: UsageKind = UsageKind { bits: 1 };
    pub const VALUE: UsageKind = UsageKind { bits: 2 };

    #[inline]
    pub const fn is_type(self) -> bool {
        self.bits & Self::TYPE.bits != 0
    }

    #[inline]
    pub const fn is_value(self) -> bool {
        self.bits & Self::VALUE.bits != 0
    }
}

impl std::ops::BitOr for UsageKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        UsageKind {
            bits: self.bits | rhs.bits,
        }
    }
}

impl std::ops::BitOrAssign for UsageKind {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

/// Usage analyzer for determining which symbols are referenced in exported declarations.
pub struct UsageAnalyzer<'a> {
    /// AST arena for walking explicit type annotations
    arena: &'a NodeArena,
    /// Binder state for symbol resolution (node_symbols)
    binder: &'a BinderState,
    /// Type cache for inferred types and def_to_symbol mapping
    type_cache: &'a TypeCache,
    /// Type interner for type operations
    type_interner: &'a TypeInterner,
    /// Map of symbols to their usage kind (Type, Value, or Both)
    used_symbols: FxHashMap<SymbolId, UsageKind>,
    /// Visited AST nodes (for cycle detection)
    visited_nodes: FxHashSet<NodeIndex>,
    /// Visited TypeIds (for cycle detection)
    visited_types: FxHashSet<crate::solver::TypeId>,
    /// The current file's arena (for distinguishing local vs foreign symbols)
    current_arena: Arc<NodeArena>,
    /// Set of symbols from other modules that need imports
    foreign_symbols: FxHashSet<SymbolId>,
    /// Context flag: true when we're in a value position (expression, typeof)
    in_value_pos: bool,
}

impl<'a> UsageAnalyzer<'a> {
    /// Create a new usage analyzer.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        type_cache: &'a TypeCache,
        type_interner: &'a TypeInterner,
        current_arena: Arc<NodeArena>,
    ) -> Self {
        Self {
            arena,
            binder,
            type_cache,
            type_interner,
            used_symbols: FxHashMap::default(),
            visited_nodes: FxHashSet::default(),
            visited_types: FxHashSet::default(),
            current_arena,
            foreign_symbols: FxHashSet::default(),
            in_value_pos: false,
        }
    }

    /// Analyze all exported declarations in a source file.
    ///
    /// Returns the map of SymbolIds to their usage kinds that are referenced in the public API.
    pub fn analyze(&mut self, root_idx: NodeIndex) -> &FxHashMap<SymbolId, UsageKind> {
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
            // Export declarations - check if clause contains a declaration to analyze
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Check if export_clause contains a declaration we need to analyze
                if let Some(export_node) = self.arena.get(stmt_idx) {
                    if let Some(export) = self.arena.get_export_decl(export_node) {
                        if !export.export_clause.is_none() {
                            if let Some(clause_node) = self.arena.get(export.export_clause) {
                                match clause_node.kind {
                                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                                        self.analyze_function_declaration(export.export_clause);
                                    }
                                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                                        self.analyze_class_declaration(export.export_clause);
                                    }
                                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                                        self.analyze_interface_declaration(export.export_clause);
                                    }
                                    k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                                        self.analyze_type_alias_declaration(export.export_clause);
                                    }
                                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                                        self.analyze_enum_declaration(export.export_clause);
                                    }
                                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                                        self.analyze_variable_statement(export.export_clause);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
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

        // CRITICAL: Also walk the inferred type of the function itself
        // This catches imported types via the type system even when
        // there's an explicit type annotation
        self.walk_inferred_type(func_idx);

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
        let Some(_ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Walk parameters
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
    ///
    /// Sets `in_value_pos = false` since we're in a type position.
    fn analyze_type_node(&mut self, type_idx: NodeIndex) {
        if !self.visited_nodes.insert(type_idx) {
            return;
        }

        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        // We're in a type position, so set in_value_pos to false
        // Save the previous value to restore it later
        let old_in_value_pos = self.in_value_pos;
        self.in_value_pos = false;

        match type_node.kind {
            // Type references - extract the symbol
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    self.analyze_entity_name(type_ref.type_name);
                    // CRITICAL: Walk type arguments to catch generic types like Promise<User>
                    if let Some(ref type_args) = type_ref.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
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

            // Type query (typeof X) - CRITICAL: marks X as VALUE usage
            // Even though typeof appears in a type position, it requires the value to exist
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(type_query) = self.arena.get_type_query(type_node) {
                    // Set in_value_pos = true for typeof expressions
                    self.in_value_pos = true;
                    self.analyze_entity_name(type_query.expr_name);
                    self.in_value_pos = false; // Restore after

                    // TODO: Walk type arguments
                }
            }

            // Import type (import("./module").Type)
            // Note: Handler added but commented out until parser exposes ImportType data
            // k if k == syntax_kind_ext::IMPORT_TYPE => {
            //     if let Some(import_type) = self.arena.get_import_type(type_node) {
            //         // Handle qualifier (e.g., the ".Bar" in import("./foo").Bar)
            //         if !import_type.qualifier.is_none() {
            //             self.analyze_entity_name(import_type.qualifier);
            //         } else {
            //             // If no qualifier, the node itself is the module reference
            //             if let Some(&sym_id) = self.binder.node_symbols.get(&type_idx.0) {
            //                 self.mark_symbol_used(sym_id, crate::declaration_emitter::usage_analyzer::UsageKind::TYPE);
            //             }
            //         }
            //
            //         // Handle type arguments: import("./foo").Bar<T>
            //         if let Some(ref type_args) = import_type.type_arguments {
            //             for &arg_idx in &type_args.nodes {
            //                 self.analyze_type_node(arg_idx);
            //             }
            //         }
            //     }
            // }

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

        // Restore the previous in_value_pos
        self.in_value_pos = old_in_value_pos;
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
                eprintln!(
                    "[DEBUG] analyze_entity_name: found Identifier, name_idx={:?}",
                    name_idx
                );
                // Found the leftmost identifier - mark as used
                if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0) {
                    eprintln!("[DEBUG] analyze_entity_name: found sym_id={:?}", sym_id);
                    let kind = if self.in_value_pos {
                        UsageKind::VALUE
                    } else {
                        UsageKind::TYPE
                    };
                    self.mark_symbol_used(sym_id, kind);
                } else {
                    eprintln!(
                        "[DEBUG] analyze_entity_name: no symbol found for name_idx={:?}",
                        name_idx
                    );
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
        eprintln!("[DEBUG] walk_inferred_type: node_idx={:?}", node_idx);
        if let Some(&type_id) = self.type_cache.node_types.get(&node_idx.0) {
            eprintln!("[DEBUG] walk_inferred_type: found type_id={:?}", type_id);
            self.walk_type_id(type_id);
        } else {
            eprintln!(
                "[DEBUG] walk_inferred_type: NO TYPE FOUND for node_idx={:?}",
                node_idx
            );
        }
    }

    /// Walk a TypeId to extract all referenced symbols.
    ///
    /// Uses collect_all_types() to get all TypeIds, then extracts DefIds/SymbolIds.
    fn walk_type_id(&mut self, type_id: crate::solver::TypeId) {
        if !self.visited_types.insert(type_id) {
            return;
        }

        eprintln!("[DEBUG] walk_type_id: type_id={:?}", type_id);

        // Collect all types reachable from this TypeId
        let all_types = visitor::collect_all_types(self.type_interner, type_id);

        eprintln!("[DEBUG] walk_type_id: collected {} types", all_types.len());
        eprintln!("[DEBUG] walk_type_id: all_types = {:?}", all_types);

        // Print TypeKeys for debugging
        for &tid in &all_types {
            if let Some(key) = self.type_interner.lookup(tid) {
                eprintln!("[DEBUG] walk_type_id: TypeId({:?}) = {:?}", tid, key);
            }
        }

        // Extract DefIds/SymbolIds from each type
        for other_type_id in all_types {
            // Extract Lazy(DefId)
            if let Some(def_id) = visitor::lazy_def_id(self.type_interner, other_type_id) {
                eprintln!("[DEBUG] walk_type_id: found Lazy(DefId={:?})", def_id);
                if let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id) {
                    self.mark_symbol_used(
                        sym_id,
                        crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                } else {
                    eprintln!(
                        "[DEBUG] walk_type_id: def_id={:?} NOT in def_to_symbol",
                        def_id
                    );
                }
            }

            // Extract Enum(DefId, _)
            if let Some((def_id, _)) = visitor::enum_components(self.type_interner, other_type_id) {
                eprintln!("[DEBUG] walk_type_id: found Enum(def_id={:?})", def_id);
                if let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id) {
                    self.mark_symbol_used(
                        sym_id,
                        crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                }
            }

            // Extract TypeQuery(SymbolRef) - marks as value usage
            if let Some(sym_ref) = visitor::type_query_symbol(self.type_interner, other_type_id) {
                let sym_id = crate::binder::SymbolId(sym_ref.0);
                self.mark_symbol_used(
                    sym_id,
                    crate::declaration_emitter::usage_analyzer::UsageKind::VALUE,
                );
            }

            // Extract UniqueSymbol(SymbolRef)
            if let Some(sym_ref) = visitor::unique_symbol_ref(self.type_interner, other_type_id) {
                let sym_id = crate::binder::SymbolId(sym_ref.0);
                self.mark_symbol_used(
                    sym_id,
                    crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                );
            }

            // Extract ModuleNamespace(SymbolRef) - marks namespace import as used
            if let Some(type_key) = self.type_interner.lookup(other_type_id) {
                if let crate::solver::TypeKey::ModuleNamespace(sym_ref) = type_key {
                    let sym_id = crate::binder::SymbolId(sym_ref.0);
                    self.mark_symbol_used(
                        sym_id,
                        crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                }
            }

            // Extract Object nominal symbols (Class instances)
            // This handles cases like x: MyClass where the type is an ObjectShape
            if let Some(shape_id) = visitor::object_shape_id(self.type_interner, other_type_id)
                .or_else(|| visitor::object_with_index_shape_id(self.type_interner, other_type_id))
            {
                let shape = self.type_interner.object_shape(shape_id);
                eprintln!(
                    "[DEBUG] walk_type_id: ObjectShapeId={:?}, symbol={:?}",
                    shape_id, shape.symbol
                );
                if let Some(sym_id) = shape.symbol {
                    self.mark_symbol_used(
                        sym_id,
                        crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                }
            }

            // Extract Callable nominal symbols (Class constructors/statics)
            // This handles cases like typeof MyClass, constructor signatures
            if let Some(shape_id) = visitor::callable_shape_id(self.type_interner, other_type_id) {
                let shape = self.type_interner.callable_shape(shape_id);
                if let Some(sym_id) = shape.symbol {
                    self.mark_symbol_used(
                        sym_id,
                        crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                }
            }
        }
    }

    /// Mark a symbol as used in the public API.
    ///
    /// Categorizes symbols as:
    /// - Global/lib symbols: Ignored (don't need imports)
    /// - Local symbols: Added to used_symbols (for elision logic)
    /// - Foreign symbols: Added to both used_symbols AND foreign_symbols (for import generation)
    fn mark_symbol_used(&mut self, sym_id: SymbolId, usage_kind: UsageKind) {
        eprintln!(
            "[DEBUG] mark_symbol_used: sym_id={:?}, usage_kind={:?}",
            sym_id, usage_kind
        );
        // Check if this is a lib/global symbol
        if self.binder.lib_symbol_ids.contains(&sym_id) {
            eprintln!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is lib symbol - skipping",
                sym_id
            );
            // Global symbol - don't track for imports
            return;
        }

        // Check if this symbol is from the current file
        let is_local = self
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| Arc::ptr_eq(arena, &self.current_arena))
            .unwrap_or(false);

        eprintln!(
            "[DEBUG] mark_symbol_used: sym_id={:?} is_local={}",
            sym_id, is_local
        );

        // Add to used_symbols with bitwise OR to handle symbols used as both types and values
        self.used_symbols
            .entry(sym_id)
            .and_modify(|kind| *kind |= usage_kind)
            .or_insert(usage_kind);

        // If it's from another file, track as foreign (for import generation)
        if !is_local {
            eprintln!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is FOREIGN - adding to foreign_symbols",
                sym_id
            );
            self.foreign_symbols.insert(sym_id);
        }
    }

    /// Get the set of foreign symbols that need imports.
    pub fn get_foreign_symbols(&self) -> &FxHashSet<SymbolId> {
        &self.foreign_symbols
    }
}
