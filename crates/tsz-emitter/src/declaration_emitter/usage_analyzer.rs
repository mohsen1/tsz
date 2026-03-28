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
//! The semantic walk uses solver visitors to traverse referenced types and maps
//! `DefId` -> `SymbolId` via `TypeResolver`.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::visitor;

use crate::type_cache_view::TypeCacheView;

/// Tracks how a symbol is used - as a type, a value, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageKind {
    bits: u8,
}

impl UsageKind {
    pub const NONE: Self = Self { bits: 0 };
    pub const TYPE: Self = Self { bits: 1 };
    pub const VALUE: Self = Self { bits: 2 };

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
        Self {
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
    /// Binder state for symbol resolution (`node_symbols`)
    binder: &'a BinderState,
    /// Type cache view for inferred types and `def_to_symbol` mapping
    type_cache: &'a TypeCacheView,
    /// Type interner for type operations
    type_interner: &'a TypeInterner,
    /// Map of import name -> `SymbolId` for resolving type references
    import_name_map: &'a FxHashMap<String, SymbolId>,
    /// Map of symbols to their usage kind (Type, Value, or Both)
    used_symbols: FxHashMap<SymbolId, UsageKind>,
    /// Visited AST nodes (for cycle detection)
    visited_nodes: FxHashSet<NodeIndex>,
    /// Visited `TypeIds` (for cycle detection)
    visited_types: FxHashSet<tsz_solver::TypeId>,
    /// Memoized transitive symbol usages per `TypeId`.
    type_symbol_cache: FxHashMap<tsz_solver::TypeId, Arc<[(SymbolId, UsageKind)]>>,
    /// `TypeIds` currently being memoized (cycle guard).
    memoizing_types: FxHashSet<tsz_solver::TypeId>,
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
        type_cache: &'a TypeCacheView,
        type_interner: &'a TypeInterner,
        current_arena: Arc<NodeArena>,
        import_name_map: &'a FxHashMap<String, SymbolId>,
    ) -> Self {
        Self {
            arena,
            binder,
            type_cache,
            type_interner,
            import_name_map,
            used_symbols: FxHashMap::default(),
            visited_nodes: FxHashSet::default(),
            visited_types: FxHashSet::default(),
            type_symbol_cache: FxHashMap::default(),
            memoizing_types: FxHashSet::default(),
            current_arena,
            foreign_symbols: FxHashSet::default(),
            in_value_pos: false,
        }
    }

    /// Analyze all exported declarations in a source file.
    ///
    /// Returns the map of `SymbolIds` to their usage kinds that are referenced in the public API.
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
            // Exported declarations - only analyze if they have the Export modifier
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(stmt_node)
                    && self
                        .arena
                        .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_function_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && self
                        .arena
                        .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_class_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                // Interfaces are implicitly exported unless in a namespace
                // For now, analyze all interfaces at module level
                self.analyze_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(stmt_node)
                    && self
                        .arena
                        .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_type_alias_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_data) = self.arena.get_enum(stmt_node)
                    && self
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_enum_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                    && self
                        .arena
                        .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_variable_statement(stmt_idx);
                }
            }
            // Export declarations - check if clause contains a declaration to analyze
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_node) = self.arena.get(stmt_idx)
                    && let Some(export) = self.arena.get_export_decl(export_node)
                    && export.export_clause.is_some()
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                {
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
                        k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                            self.analyze_import_equals_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            self.analyze_module_declaration(export.export_clause);
                        }
                        // Named exports: export { x, y as z }
                        // Mark each specifier's local name as used
                        k if k == syntax_kind_ext::NAMED_EXPORTS => {
                            self.analyze_named_exports(export.export_clause);
                        }
                        // Identifier reference: export default <Identifier>
                        // Mark the referenced declaration as used so it's included in .d.ts.
                        // We look up via file_locals rather than node_symbols because
                        // node_symbols maps this reference to the export symbol, not the
                        // underlying declaration symbol.
                        k if k == SyntaxKind::Identifier as u16 => {
                            if let Some(ident) = self.arena.get_identifier(clause_node)
                                && let Some(sym_id) =
                                    self.binder.file_locals.get(&ident.escaped_text)
                            {
                                self.mark_symbol_used(sym_id, UsageKind::VALUE | UsageKind::TYPE);
                            }
                        }
                        // Default export with expression: export default new A()
                        // Unwrap new/call to find the constructor reference.
                        k if k == syntax_kind_ext::NEW_EXPRESSION
                            || k == syntax_kind_ext::CALL_EXPRESSION =>
                        {
                            let callee =
                                self.unwrap_export_default_expression(export.export_clause);
                            self.analyze_entity_name(callee);
                        }
                        _ => {}
                    }
                }
            }
            // Export assignment: export default expr
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.analyze_export_assignment(stmt_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.analyze_module_declaration(stmt_idx);
            }
            _ => {}
        }
    }

    fn analyze_module_declaration(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        if let Some(body_node) = self.arena.get(module.body) {
            if let Some(module_block) = self.arena.get_module_block(body_node) {
                if let Some(ref stmts) = module_block.statements {
                    for &stmt_idx in &stmts.nodes {
                        self.analyze_statement(stmt_idx);
                    }
                }
            } else if let Some(_nested_module) = self.arena.get_module(body_node) {
                self.analyze_module_declaration(module.body);
            }
        }
    }

    fn analyze_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Mark the RHS namespace/type/value as used by this declaration.
        if import.module_specifier.is_some() {
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(import.module_specifier);
            self.in_value_pos = old;
        }
    }

    /// Analyze named exports: `export { x, y as z }`.
    /// Marks each specifier's local binding as used so non-exported declarations
    /// referenced by the export clause survive into .d.ts output.
    fn analyze_named_exports(&mut self, clause_idx: NodeIndex) {
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return;
        };
        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return;
        };
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            // The local name is `property_name` if it exists, otherwise `name`
            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            // Mark the local symbol as used (both type and value, since
            // we don't know which side of the export is being consumed)
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(local_name_idx);
            self.in_value_pos = old;
            // Also mark as type usage
            self.analyze_entity_name(local_name_idx);
        }
    }

    /// Analyze export assignment: `export default expr`.
    fn analyze_export_assignment(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(export_assign) = self.arena.get_export_assignment(stmt_node) else {
            return;
        };
        // Mark the expression as used (could be a type or value reference)
        if export_assign.expression.is_some() {
            let expr_idx = self.unwrap_export_default_expression(export_assign.expression);
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(expr_idx);
            self.analyze_local_import_equals_dependency(expr_idx);
            self.in_value_pos = old;
            // Also type usage
            self.analyze_entity_name(expr_idx);
            self.analyze_local_import_equals_dependency(expr_idx);
        }
    }

    /// Unwrap `new X()` and `X()` expressions to find the constructor/callee
    /// reference for dependency tracking in default exports.
    fn unwrap_export_default_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return expr_idx;
        };
        // `export default new A()` → track `A`
        if (expr_node.kind == syntax_kind_ext::NEW_EXPRESSION
            || expr_node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && let Some(call) = self.arena.get_call_expr(expr_node)
        {
            return call.expression;
        }
        expr_idx
    }

    fn analyze_local_import_equals_dependency(&mut self, name_idx: NodeIndex) {
        let mut seen_symbols = FxHashSet::default();
        self.analyze_local_import_equals_dependency_inner(name_idx, &mut seen_symbols);
    }

    fn analyze_local_import_equals_dependency_inner(
        &mut self,
        name_idx: NodeIndex,
        seen_symbols: &mut FxHashSet<SymbolId>,
    ) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.arena.get_identifier(name_node) else {
            return;
        };
        // Look up the symbol by name: first in file_locals, then in scope tables
        // (needed for import aliases inside namespaces).
        let sym_id = if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text) {
            sym_id
        } else {
            let mut found = None;
            for scope in &self.binder.scopes {
                if let Some(sym_id) = scope.table.get(&ident.escaped_text) {
                    found = Some(sym_id);
                    break;
                }
            }
            let Some(sym_id) = found else {
                return;
            };
            sym_id
        };
        if !seen_symbols.insert(sym_id) {
            return;
        }
        let declarations = {
            let Some(symbol) = self.binder.symbols.get(sym_id) else {
                return;
            };

            let mut declarations = symbol.declarations.clone();
            if symbol.value_declaration.is_some()
                && !declarations.contains(&symbol.value_declaration)
            {
                declarations.push(symbol.value_declaration);
            }
            declarations
        };

        for decl_idx in declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                // Mark the import alias symbol itself as used so it survives
                // elision in the .d.ts output.
                self.mark_symbol_used(sym_id, UsageKind::TYPE | UsageKind::VALUE);
                self.analyze_import_equals_declaration(decl_idx);
                if let Some(import) = self.arena.get_import_decl(decl_node)
                    && import.module_specifier.is_some()
                {
                    self.analyze_local_import_equals_dependency_inner(
                        import.module_specifier,
                        seen_symbols,
                    );
                }
            }
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
        if func.type_annotation.is_some() {
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

        // Private properties emit as `private x;` without type — skip type dependencies.
        // Computed property names are still tracked since the name IS emitted.
        let is_private = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(prop.name);

        if !is_private {
            // Walk type annotation (explicit or inferred)
            if prop.type_annotation.is_some() {
                self.analyze_type_node(prop.type_annotation);
            } else {
                self.walk_inferred_type(prop_idx);
            }
        }

        // For computed properties, analyze the name expression to mark referenced symbols
        // (e.g., `const symb = Symbol(); class C { [symb]: boolean }` — symb needs to be tracked)
        self.analyze_computed_property_name(prop.name);

        // Also walk the inferred type for computed properties (non-private only)
        if !is_private
            && let Some(name_node) = self.arena.get(prop.name)
            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
        {
            self.walk_inferred_type(prop_idx);
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

        // Track symbols referenced in computed property names
        self.analyze_computed_property_name(method.name);

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
        if method.type_annotation.is_some() {
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

        // Private constructors don't emit parameters in .d.ts — skip dependency tracking
        if self
            .arena
            .has_modifier(&ctor.modifiers, SyntaxKind::PrivateKeyword)
        {
            return;
        }

        // Walk parameters (public and protected constructors)
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

        // Track symbols referenced in computed property names
        self.analyze_computed_property_name(accessor.name);

        // Private accessors emit without types — skip type deps
        if self
            .arena
            .has_modifier(&accessor.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(accessor.name)
        {
            return;
        }

        // Walk parameters (setter parameter types)
        for &param_idx in &accessor.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type (for getters)
        if accessor.type_annotation.is_some() {
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
        if sig.type_annotation.is_some() {
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
                    if sig.type_annotation.is_some() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                    // Track symbols referenced in computed property names
                    self.analyze_computed_property_name(sig.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Track symbols referenced in computed property names
                    self.analyze_computed_property_name(sig.name);
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
                    if sig.type_annotation.is_some() {
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
                    if sig.type_annotation.is_some() {
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
    const fn analyze_enum_declaration(&mut self, _enum_idx: NodeIndex) {
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
        if decl.type_annotation.is_some() {
            self.analyze_type_node(decl.type_annotation);
        } else {
            self.walk_inferred_type_or_related(&[decl_idx, decl.name]);
        }

        if decl.initializer.is_some()
            && self.initializer_preserves_value_reference(decl.initializer)
        {
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(decl.initializer);
            self.analyze_local_import_equals_dependency(decl.initializer);
            self.in_value_pos = false;
            self.analyze_entity_name(decl.initializer);
            self.analyze_local_import_equals_dependency(decl.initializer);
            self.in_value_pos = old;
        }

        // When there is no explicit type annotation, the declaration emitter
        // may use the initializer's referenced name as the emitted type (e.g.
        // `var d: X` for `var d = new X()`, or `typeof b` for `var b2 = b`).
        // We must mark import alias dependencies from the initializer so that
        // non-exported `import =` aliases are preserved in the .d.ts.
        if decl.type_annotation.is_none() && decl.initializer.is_some() {
            // Unwrap `new X()` / `X()` to get the callee, or use the
            // initializer directly if it's a plain identifier/expression.
            let callee = self.unwrap_export_default_expression(decl.initializer);
            self.analyze_entity_name(callee);
            self.analyze_local_import_equals_dependency(callee);
            // If the initializer itself was different (i.e. it IS a plain
            // identifier, not a new/call), also track it directly.
            if callee != decl.initializer {
                self.analyze_entity_name(decl.initializer);
                self.analyze_local_import_equals_dependency(decl.initializer);
            }
        }
    }

    /// Analyze heritage clauses (extends/implements).
    fn analyze_heritage_clauses(&mut self, clauses: &tsz_parser::parser::NodeList) {
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
        if param.constraint.is_some() {
            self.analyze_type_node(param.constraint);
        }

        // Walk default type
        if param.default.is_some() {
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
        if param.type_annotation.is_some() {
            self.analyze_type_node(param.type_annotation);
        } else {
            self.walk_inferred_type_or_related(&[param_idx, param.name, param.initializer]);
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
            // Some explicit type positions, especially heritage clauses in error
            // recovery, surface a bare entity name instead of a wrapped TypeReference.
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::QUALIFIED_NAME
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                self.analyze_entity_name(type_idx);
            }

            // Type references - extract the symbol
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    // First try AST walk via analyze_entity_name
                    self.analyze_entity_name(type_ref.type_name);

                    // Fallback: If AST walk didn't find the symbol, try semantic walk via TypeId
                    // This handles imported types where node_symbols doesn't have entries
                    debug!(
                        "[DEBUG] TYPE_REFERENCE: looking up type_cache.node_types for type_idx={:?}",
                        type_idx
                    );
                    if let Some(&type_id) = self.type_cache.node_types.get(&type_idx.0) {
                        debug!(
                            "[DEBUG] TYPE_REFERENCE: found type_id={:?}, walking it",
                            type_id
                        );
                        self.walk_type_id(type_id);
                    } else {
                        debug!(
                            "[DEBUG] TYPE_REFERENCE: no type_id found for type_idx={:?}",
                            type_idx
                        );
                    }

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
                    // Also track import alias dependencies so that non-exported
                    // `import =` aliases referenced via `typeof` are preserved.
                    self.analyze_local_import_equals_dependency(type_query.expr_name);
                    self.in_value_pos = false; // Restore after

                    // Walk type arguments (e.g., typeof X<A, B>)
                    if let Some(ref type_args) = type_query.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
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
                    if mapped_type.name_type.is_some() {
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

            // Type predicate (x is T)
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(predicate) = self.arena.get_type_predicate(type_node) {
                    self.analyze_type_node(predicate.type_node);
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

            // Constructor type: new (...) => T
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
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

            // Optional type: T? (in tuples)
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(wrapped.type_node);
                }
            }

            // Rest type: ...T (in tuples)
            k if k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(wrapped.type_node);
                }
            }

            // Named tuple member: name: T
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(named) = self.arena.get_named_tuple_member(type_node) {
                    self.analyze_type_node(named.type_node);
                }
            }

            // Template literal type: `hello${T}world`
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(tlt) = self.arena.get_template_literal_type(type_node) {
                    for &span_idx in &tlt.template_spans.nodes {
                        // Spans reuse TemplateSpanData — expression field holds the type
                        if let Some(span_node) = self.arena.get(span_idx)
                            && let Some(span) = self.arena.get_template_span(span_node)
                        {
                            self.analyze_type_node(span.expression);
                        }
                    }
                }
            }

            // Import type: import("mod").T — handled by walk_inferred_type
            k if k == syntax_kind_ext::IMPORT_TYPE => {}

            _ => {}
        }

        // Restore the previous in_value_pos
        self.in_value_pos = old_in_value_pos;
    }

    /// Check if a member name is a private identifier (`#foo`).
    fn member_has_private_identifier_name(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
    }

    /// Analyze the expression inside a computed property name (e.g., `[symb]`).
    /// This ensures that symbols referenced in computed names are tracked as used,
    /// so their declarations (e.g., `const symb: unique symbol`) are emitted in .d.ts.
    fn analyze_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return;
        };
        // The expression inside [] may be an identifier, property access, etc.
        let old_in_value_pos = self.in_value_pos;
        self.in_value_pos = true;
        self.analyze_entity_name(computed.expression);
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
                // Found the leftmost identifier - mark as used
                let kind = if self.in_value_pos {
                    UsageKind::VALUE
                } else {
                    UsageKind::TYPE
                };
                if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0) {
                    self.mark_symbol_used(sym_id, kind);
                }
                // Also mark the file-local/import symbol by name, since
                // references and declarations may have different SymbolIds.
                if let Some(ident) = self.arena.get_identifier(name_node) {
                    if let Some(&sym_id) = self.import_name_map.get(&ident.escaped_text) {
                        self.mark_symbol_used(sym_id, kind);
                    }
                    if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text) {
                        self.mark_symbol_used(sym_id, kind);
                    }
                    // Also check namespace/module scope tables, since the
                    // symbol may live in a parent namespace scope rather than
                    // file-level locals. This is needed so that non-exported
                    // namespace members referenced by exported members are
                    // properly marked as used (and thus emitted + triggering
                    // `export {};` scope markers).
                    for scope in &self.binder.scopes {
                        if let Some(sym_id) = scope.table.get(&ident.escaped_text) {
                            self.mark_symbol_used(sym_id, kind);
                        }
                    }
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

    fn initializer_preserves_value_reference(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .value_reference_symbol(expr_idx)
                .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id)),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.arena.get_access_expr(expr_node) else {
                    return false;
                };
                self.value_reference_symbol(access.name_or_argument)
                    .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id))
                    || self
                        .value_reference_symbol(expr_idx)
                        .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id))
            }
            _ => false,
        }
    }

    fn symbol_needs_typeof(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };

        (symbol.has_any_flags(
            tsz_binder::symbol_flags::FUNCTION
                | tsz_binder::symbol_flags::CLASS
                | tsz_binder::symbol_flags::ENUM
                | tsz_binder::symbol_flags::VALUE_MODULE
                | tsz_binder::symbol_flags::METHOD,
        ) || self.is_namespace_import_alias_symbol(sym_id))
            && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
    }

    fn is_namespace_import_alias_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && symbol.import_name.is_none()
    }

    fn value_reference_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let expr_node = self.arena.get(expr_idx)?;

        if expr_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(&sym_id) = self.binder.node_symbols.get(&expr_idx.0) {
                return Some(sym_id);
            }

            let ident = self.arena.get_identifier(expr_node)?;
            return self
                .import_name_map
                .get(&ident.escaped_text)
                .copied()
                .or_else(|| self.binder.file_locals.get(&ident.escaped_text));
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(expr_node)?;
            return self
                .binder
                .node_symbols
                .get(&expr_idx.0)
                .copied()
                .or_else(|| {
                    self.binder
                        .node_symbols
                        .get(&access.name_or_argument.0)
                        .copied()
                });
        }

        self.binder.node_symbols.get(&expr_idx.0).copied()
    }

    /// Walk an inferred type from the type cache.
    ///
    /// This is the semantic walk over inferred `TypeId`s.
    fn walk_inferred_type(&mut self, node_idx: NodeIndex) {
        // Look up the inferred TypeId for this node
        debug!("[DEBUG] walk_inferred_type: node_idx={:?}", node_idx);
        if let Some(&type_id) = self.type_cache.node_types.get(&node_idx.0) {
            debug!("[DEBUG] walk_inferred_type: found type_id={:?}", type_id);
            self.walk_type_id(type_id);
        } else {
            debug!(
                "[DEBUG] walk_inferred_type: NO TYPE FOUND for node_idx={:?}",
                node_idx
            );
        }
    }

    fn walk_inferred_type_if_present(&mut self, node_idx: NodeIndex) -> bool {
        if let Some(&type_id) = self.type_cache.node_types.get(&node_idx.0) {
            self.walk_type_id(type_id);
            return true;
        }
        false
    }

    fn walk_inferred_type_or_related(&mut self, node_ids: &[NodeIndex]) {
        for &node_idx in node_ids {
            if !node_idx.is_some() {
                continue;
            }

            if self.walk_inferred_type_if_present(node_idx) {
                return;
            }

            let Some(node) = self.arena.get(node_idx) else {
                continue;
            };

            for related_idx in self.get_node_type_related_nodes(node) {
                if related_idx.is_some() && self.walk_inferred_type_if_present(related_idx) {
                    return;
                }
            }
        }
    }

    fn get_node_type_related_nodes(&self, node: &tsz_parser::parser::node::Node) -> Vec<NodeIndex> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    if decl.type_annotation.is_some() {
                        related.push(decl.type_annotation);
                    }
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(decl) = self.arena.get_property_decl(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    if decl.type_annotation.is_some() {
                        related.push(decl.type_annotation);
                    }
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if param.initializer.is_some() {
                        vec![param.initializer]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn add_symbol_usage(
        usages: &mut FxHashMap<SymbolId, UsageKind>,
        sym_id: SymbolId,
        usage_kind: UsageKind,
    ) {
        usages
            .entry(sym_id)
            .and_modify(|kind| *kind |= usage_kind)
            .or_insert(usage_kind);
    }

    fn collect_direct_symbol_usages(
        &self,
        type_id: tsz_solver::TypeId,
        usages: &mut FxHashMap<SymbolId, UsageKind>,
    ) {
        if let Some(def_id) = visitor::lazy_def_id(self.type_interner, type_id)
            && let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id)
        {
            Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
        }

        if let Some((def_id, _)) = visitor::enum_components(self.type_interner, type_id)
            && let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id)
        {
            Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
        }

        if let Some(sym_ref) = visitor::type_query_symbol(self.type_interner, type_id) {
            Self::add_symbol_usage(usages, tsz_binder::SymbolId(sym_ref.0), UsageKind::VALUE);
        }

        if let Some(sym_ref) = visitor::unique_symbol_ref(self.type_interner, type_id) {
            Self::add_symbol_usage(usages, tsz_binder::SymbolId(sym_ref.0), UsageKind::TYPE);
        }

        if let Some(sym_ref) = visitor::module_namespace_symbol_ref(self.type_interner, type_id) {
            Self::add_symbol_usage(usages, tsz_binder::SymbolId(sym_ref.0), UsageKind::TYPE);
        }

        if let Some(shape_id) = visitor::object_shape_id(self.type_interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.type_interner, type_id))
        {
            let shape = self.type_interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
            }
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.type_interner, type_id) {
            let shape = self.type_interner.callable_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
            }
        }
    }

    fn collect_symbol_usages_for_type(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) -> Arc<[(SymbolId, UsageKind)]> {
        if let Some(cached) = self.type_symbol_cache.get(&type_id) {
            return cached.clone();
        }

        if !self.memoizing_types.insert(type_id) {
            return self
                .type_symbol_cache
                .get(&type_id)
                .cloned()
                .unwrap_or_else(|| Arc::from([]));
        }

        let mut usages = FxHashMap::default();
        self.collect_direct_symbol_usages(type_id, &mut usages);

        let mut result = Self::freeze_symbol_usages(&usages);
        self.type_symbol_cache.insert(type_id, result.clone());

        let mut children = Vec::new();
        visitor::for_each_child_by_id(self.type_interner, type_id, |child| {
            children.push(child);
        });

        for child in children {
            if child == type_id {
                continue;
            }
            for &(sym_id, usage_kind) in self.collect_symbol_usages_for_type(child).iter() {
                Self::add_symbol_usage(&mut usages, sym_id, usage_kind);
            }
        }

        self.memoizing_types.remove(&type_id);

        result = Self::freeze_symbol_usages(&usages);
        self.type_symbol_cache.insert(type_id, result.clone());
        result
    }

    fn freeze_symbol_usages(
        usages: &FxHashMap<SymbolId, UsageKind>,
    ) -> Arc<[(SymbolId, UsageKind)]> {
        let mut frozen: Vec<(SymbolId, UsageKind)> = usages
            .iter()
            .map(|(&sym_id, &usage_kind)| (sym_id, usage_kind))
            .collect();
        frozen.sort_unstable_by_key(|(sym_id, usage_kind)| (sym_id.0, usage_kind.bits));
        Arc::from(frozen)
    }

    /// Walk a `TypeId` to extract all referenced symbols.
    fn walk_type_id(&mut self, type_id: tsz_solver::TypeId) {
        if !self.visited_types.insert(type_id) {
            return;
        }

        for &(sym_id, usage_kind) in self.collect_symbol_usages_for_type(type_id).iter() {
            self.mark_symbol_used(sym_id, usage_kind);
        }
    }

    /// Mark a symbol as used in the public API.
    ///
    /// Categorizes symbols as:
    /// - Global/lib symbols: Ignored (don't need imports)
    /// - Local symbols: Added to `used_symbols` (for elision logic)
    /// - Foreign symbols: Added to both `used_symbols` AND `foreign_symbols` (for import generation)
    fn mark_symbol_used(&mut self, sym_id: SymbolId, usage_kind: UsageKind) {
        debug!(
            "[DEBUG] mark_symbol_used: sym_id={:?}, usage_kind={:?}",
            sym_id, usage_kind
        );
        // Check if this is a lib/global symbol
        if self.binder.lib_symbol_ids.contains(&sym_id) {
            debug!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is lib symbol - skipping",
                sym_id
            );
            // Global symbol - don't track for imports
            return;
        }

        // Check if this symbol is from the current file by checking if any of its
        // declarations are in the current arena using the declaration_arenas map
        let is_local = self.binder.symbols.get(sym_id).is_some_and(|symbol| {
            // Check if any declaration is in the current file's arena
            symbol.declarations.iter().any(|&decl_idx| {
                self.binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .and_then(|v| v.first())
                    .is_some_and(|arena| Arc::ptr_eq(arena, &self.current_arena))
            })
        });

        debug!(
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
            debug!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is FOREIGN - adding to foreign_symbols",
                sym_id
            );
            self.foreign_symbols.insert(sym_id);
        }
    }

    /// Get the set of foreign symbols that need imports.
    pub const fn get_foreign_symbols(&self) -> &FxHashSet<SymbolId> {
        &self.foreign_symbols
    }
}
