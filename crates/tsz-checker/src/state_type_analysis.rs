//! Type Analysis & Compatibility Module
//!
//! Extracted from state.rs: Methods for type analysis including qualified name
//! resolution, symbol type computation, type queries, and contextual literal type analysis.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

type TypeParamPushResult = (
    Vec<tsz_solver::TypeParamInfo>,
    Vec<(String, Option<TypeId>)>,
);

impl<'a> CheckerState<'a> {
    /// Resolve a qualified name (A.B.C) to its type.
    ///
    /// This function handles qualified type names like `Namespace.SubType`, `Module.Interface`,
    /// or deeply nested names like `A.B.C`. It resolves each segment and looks up the final member.
    ///
    /// ## Resolution Strategy:
    /// 1. **Recursively resolve left side**: For `A.B.C`, first resolve `A.B`
    /// 2. **Get member type**: Look up rightmost member in left type's exports
    /// 3. **Handle symbol merging**: Supports merged class+namespace, enum+namespace, etc.
    ///
    /// ## Qualified Name Forms:
    /// - `Module.Type` - Type from module
    /// - `Namespace.Interface` - Interface from namespace
    /// - `A.B.C` - Deeply nested qualified name
    /// - `Class.StaticMember` - Static class member
    ///
    /// ## Symbol Resolution:
    /// - Checks exports of left side's symbol
    /// - Handles merged symbols (class+namespace, function+namespace)
    /// - Falls back to property access if not found in exports
    ///
    /// ## Error Reporting:
    /// - TS2694: Namespace has no exported member
    /// - Returns ERROR type if resolution fails
    ///
    /// ## Lib Binders:
    /// - Collects lib binders for cross-arena symbol lookup
    /// - Fixes TS2694 false positives for lib.d.ts types
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Module members
    /// namespace Utils {
    ///   export interface Helper {}
    /// }
    /// let h: Utils.Helper;  // resolve_qualified_name("Utils.Helper")
    ///
    /// // Deep nesting
    /// namespace A {
    ///   export namespace B {
    ///     export interface C {}
    ///   }
    /// }
    /// let x: A.B.C;  // resolve_qualified_name("A.B.C")
    ///
    /// // Static class members
    /// class Container {
    ///   static class Inner {}
    /// }
    /// let y: Container.Inner;  // resolve_qualified_name("Container.Inner")
    ///
    /// // Merged symbols
    /// function Model() {}
    /// namespace Model {
    ///   export interface Options {}
    /// }
    /// let opts: Model.Options;  // resolve_qualified_name("Model.Options")
    /// ```
    pub(crate) fn resolve_qualified_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
            return TypeId::ERROR; // Missing qualified name data - propagate error
        };

        // Resolve the left side (could be Identifier or another QualifiedName)
        let left_type = if let Some(left_node) = self.ctx.arena.get(qn.left) {
            if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                self.resolve_qualified_name(qn.left)
            } else if left_node.kind == SyntaxKind::Identifier as u16 {
                let left_name = self
                    .ctx
                    .arena
                    .get_identifier(left_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();

                match self.resolve_identifier_symbol_in_type_position(qn.left) {
                    TypeSymbolResolution::Type(sym_id) => self.type_reference_symbol_type(sym_id),
                    TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => {
                        if !self.is_unresolved_import_symbol(qn.left) && !left_name.is_empty() {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node_msg(
                                qn.left,
                                diagnostic_codes::CANNOT_FIND_NAMESPACE,
                                &[left_name.as_str()],
                            );
                        }
                        TypeId::ERROR
                    }
                }
            } else {
                TypeId::ERROR // Unknown node kind - propagate error
            }
        } else {
            TypeId::ERROR // Missing left node - propagate error
        };

        if left_type == TypeId::ANY || left_type == TypeId::ERROR {
            return TypeId::ERROR; // Propagate error from left side
        }

        // Get the right side name (B in A.B)
        let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right) {
            if let Some(id) = self.ctx.arena.get_identifier(right_node) {
                id.escaped_text.clone()
            } else {
                return TypeId::ERROR; // Missing identifier data - propagate error
            }
        } else {
            return TypeId::ERROR; // Missing right node - propagate error
        };

        // Collect lib binders for cross-arena symbol lookup (fixes TS2694 false positives)
        let lib_binders = self.get_lib_binders();

        // First, try to resolve the left side as a symbol and check its exports.
        // This handles merged class+namespace, function+namespace, and enum+namespace symbols.
        let mut left_sym_for_missing = None;
        let mut left_module_specifier: Option<String> = None;
        let member_sym_id_from_symbol = if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
        {
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(qn.left)
            {
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    left_sym_for_missing = Some(sym_id);
                    left_module_specifier = symbol.import_module.clone();
                    self.resolve_symbol_export(symbol, &right_name, &lib_binders)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // If found via symbol resolution, use it
        if let Some(member_sym_id) = member_sym_id_from_symbol {
            if let Some(member_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(member_sym_id, &lib_binders)
            {
                let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                if !is_namespace
                    && (self.alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                        || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                    && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
                {
                    self.error_value_only_type_at(&right_name, qn.right);
                    return TypeId::ERROR;
                }
            }
            let mut member_type = self.type_reference_symbol_type(member_sym_id);
            if let Some(module_specifier) = left_module_specifier.as_deref() {
                member_type =
                    self.apply_module_augmentations(module_specifier, &right_name, member_type);
            }
            return member_type;
        }

        if let Some(left_sym_id) = left_sym_for_missing
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym_id, &lib_binders)
            && symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::CLASS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM
                    | symbol_flags::INTERFACE)
                != 0
        {
            let namespace_name = self
                .entity_name_text(qn.left)
                .unwrap_or_else(|| symbol.escaped_name.clone());
            self.error_namespace_no_export(&namespace_name, &right_name, qn.right);
            return TypeId::ERROR;
        }

        // Otherwise, fall back to type-based lookup for pure namespace/module types
        // Look up the member in the left side's exports
        // Supports both Lazy(DefId) and Enum types
        let fallback_sym_id = self.ctx.resolve_type_to_symbol_id(left_type);

        if let Some(fallback_sym) = fallback_sym_id
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(fallback_sym, &lib_binders)
        {
            // Use the helper to resolve the member from exports, members, or re-exports
            if let Some(member_sym_id) =
                self.resolve_symbol_export(symbol, &right_name, &lib_binders)
            {
                // Check value-only, but skip for namespaces since they can be used
                // to navigate to types (e.g., Outer.Inner.Type)
                if let Some(member_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(member_sym_id, &lib_binders)
                {
                    let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                    if !is_namespace
                        && (self
                            .alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                            || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                        && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
                    {
                        self.error_value_only_type_at(&right_name, qn.right);
                        return TypeId::ERROR;
                    }
                }
                let mut member_type = self.type_reference_symbol_type(member_sym_id);
                if let Some(module_specifier) = left_module_specifier.as_deref() {
                    member_type =
                        self.apply_module_augmentations(module_specifier, &right_name, member_type);
                }
                return member_type;
            }

            // Not found - report TS2694
            let namespace_name = self
                .entity_name_text(qn.left)
                .unwrap_or_else(|| symbol.escaped_name.clone());
            self.error_namespace_no_export(&namespace_name, &right_name, qn.right);
            return TypeId::ERROR;
        }

        // Left side wasn't a reference to a namespace/module
        // This is likely an error - the left side should resolve to a namespace
        // Emit an appropriate error for the unresolved qualified name
        // We don't emit TS2304 here because the left side might have already emitted an error
        // Returning ERROR prevents cascading errors while still indicating failure
        if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
            && !self.is_unresolved_import_symbol(qn.left)
            && let Some(ident) = self.ctx.arena.get_identifier(left_node)
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                qn.left,
                diagnostic_codes::CANNOT_FIND_NAMESPACE,
                &[ident.escaped_text.as_str()],
            );
        }
        TypeId::ERROR
    }

    /// Resolve a member from a symbol's exports, members, or re-exports.
    ///
    /// This helper implements the common pattern of looking up a member in:
    /// 1. Direct exports
    /// 2. Members (for classes with static members)
    /// 3. Re-exports (for imported namespaces)
    ///
    /// Returns `Some(member_sym_id)` if found, `None` otherwise.
    fn resolve_symbol_export(
        &mut self,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<tsz_binder::SymbolId> {
        // Try direct exports first
        if let Some(ref exports) = symbol.exports
            && let Some(member_id) = exports.get(member_name)
        {
            return Some(member_id);
        }

        // For classes, also check members (for static members in type queries)
        // This handles `typeof C.staticMember` where C is a class
        if symbol.flags & symbol_flags::CLASS != 0
            && let Some(ref members) = symbol.members
            && let Some(member_id) = members.get(member_name)
        {
            return Some(member_id);
        }

        if symbol.flags & symbol_flags::MODULE != 0 {
            if let Some(member_id) =
                self.resolve_module_export_from_declarations(symbol, member_name)
            {
                return Some(member_id);
            }
            if let Some(sym_id) = self.ctx.binder.file_locals.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                && sym.is_exported
            {
                return Some(sym_id);
            }
        }

        // If not found in direct exports, check for re-exports
        // The member might be re-exported from another module
        if let Some(ref module_specifier) = symbol.import_module {
            if (symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return None;
            }
            if let Some(reexported_sym_id) =
                self.resolve_reexported_member(module_specifier, member_name, lib_binders)
            {
                return Some(reexported_sym_id);
            }
        }

        None
    }

    fn resolve_module_export_from_declarations(
        &self,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                continue;
            };
            if module.body.is_none() {
                continue;
            }
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&module.body.0)
                && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                && let Some(sym_id) = scope.table.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                && sym.is_exported
            {
                return Some(sym_id);
            }
            let Some(module_block) = self.ctx.arena.get_module_block_at(module.body) else {
                continue;
            };
            let Some(statements) = &module_block.statements else {
                continue;
            };

            for &stmt_idx in &statements.nodes {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    let name = if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                        self.ctx
                            .arena
                            .get_type_alias(stmt_node)
                            .and_then(|alias| self.ctx.arena.get(alias.name))
                            .and_then(|node| self.ctx.arena.get_identifier(node))
                            .map(|ident| ident.escaped_text.clone())
                    } else {
                        self.ctx
                            .arena
                            .get_interface(stmt_node)
                            .and_then(|iface| self.ctx.arena.get(iface.name))
                            .and_then(|node| self.ctx.arena.get_identifier(node))
                            .map(|ident| ident.escaped_text.clone())
                    };
                    if let Some(name) = name
                        && name == member_name
                        && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&stmt_idx.0)
                    {
                        return Some(sym_id);
                    }
                }
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }
                let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export_decl.export_clause.is_none() {
                    continue;
                }
                let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                    continue;
                };

                match clause_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
                    | syntax_kind_ext::MODULE_DECLARATION => {
                        let name = match clause_node.kind {
                            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                                .ctx
                                .arena
                                .get_function(clause_node)
                                .and_then(|func| self.ctx.arena.get(func.name))
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                                .ctx
                                .arena
                                .get_class(clause_node)
                                .and_then(|class| self.ctx.arena.get(class.name))
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                                .ctx
                                .arena
                                .get_interface(clause_node)
                                .and_then(|iface| self.ctx.arena.get(iface.name))
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                                .ctx
                                .arena
                                .get_type_alias(clause_node)
                                .and_then(|alias| self.ctx.arena.get(alias.name))
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            k if k == syntax_kind_ext::ENUM_DECLARATION => self
                                .ctx
                                .arena
                                .get_enum(clause_node)
                                .and_then(|enm| self.ctx.arena.get(enm.name))
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                                .ctx
                                .arena
                                .get_module(clause_node)
                                .and_then(|module| self.ctx.arena.get(module.name))
                                .and_then(|node| {
                                    self.ctx
                                        .arena
                                        .get_identifier(node)
                                        .map(|ident| ident.escaped_text.clone())
                                        .or_else(|| {
                                            self.ctx
                                                .arena
                                                .get_literal(node)
                                                .map(|lit| lit.text.clone())
                                        })
                                }),
                            _ => None,
                        };
                        if let Some(name) = name
                            && name == member_name
                            && let Some(&sym_id) = self
                                .ctx
                                .binder
                                .node_symbols
                                .get(&export_decl.export_clause.0)
                        {
                            return Some(sym_id);
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(clause_node) {
                            // VariableStatement holds VariableDeclarationList nodes.
                            // Walk list -> declaration to recover exported namespace vars.
                            for &list_idx in &var_stmt.declarations.nodes {
                                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                                    continue;
                                };
                                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                                    continue;
                                };
                                for &decl_idx in &decl_list.declarations.nodes {
                                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                        continue;
                                    };
                                    let Some(decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    else {
                                        continue;
                                    };
                                    let Some(name_node) = self.ctx.arena.get(decl.name) else {
                                        continue;
                                    };
                                    let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    else {
                                        continue;
                                    };
                                    if ident.escaped_text == member_name
                                        && let Some(&sym_id) =
                                            self.ctx.binder.node_symbols.get(&decl_idx.0)
                                    {
                                        return Some(sym_id);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Get type from a union type node (A | B).
    ///
    /// Parses a union type expression and creates a Union type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty union → NEVER (the empty type)
    /// - Single member → the member itself (no union wrapper)
    /// - Multiple members → Union type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - This handles nested typeof expressions and type references
    /// - Type arguments are recursively resolved
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type StringOrNumber = string | number;
    /// // Creates Union(STRING, NUMBER)
    ///
    /// type ThreeTypes = string | number | boolean;
    /// // Creates Union(STRING, NUMBER, BOOLEAN)
    ///
    /// type Nested = (string | number) | boolean;
    /// // Normalized to Union(STRING, NUMBER, BOOLEAN)
    /// ```
    /// Get type from a type query node (typeof X).
    ///
    /// Creates a `TypeQuery` type that captures the type of a value, enabling type-level
    /// queries and conditional type logic.
    ///
    /// ## Resolution Strategy:
    /// 1. **Value symbol resolved** (typeof value):
    ///    - Without type args: Return the actual type directly
    ///    - With type args: Create `TypeQuery` type for deferred resolution
    ///    - Exception: ANY/ERROR types still create `TypeQuery` for proper error handling
    ///
    /// 2. **Type symbol only**: Emit TS2504 error (type cannot be used as value)
    ///
    /// 3. **Unknown identifier**:
    ///    - Known global value → return ANY (allows property access)
    ///    - Unresolved import → return ANY (TS2307 already emitted)
    ///    - Otherwise → emit TS2304 error and return ERROR
    ///
    /// 4. **Missing member** (typeof obj.prop): Emit appropriate error
    ///
    /// 5. **Fallback**: Hash the name for forward compatibility
    ///
    /// ## Type Arguments:
    /// - If present, creates TypeApplication(base, args)
    /// - Used in generic type queries: `typeof Array<string>`
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// let x = 42;
    /// type T1 = typeof x;  // number
    ///
    /// function foo(): string { return "hello"; }
    /// type T2 = typeof foo;  // () => string
    ///
    /// class MyClass {
    ///   prop = 123;
    /// }
    /// type T3 = typeof MyClass;  // typeof MyClass (constructor type)
    /// type T4 = MyClass;  // MyClass (instance type)
    ///
    /// // Type query with type arguments (advanced)
    /// type T5 = typeof Array<string>;  // typeof Array with type args
    /// ```
    pub(crate) fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::SymbolRef;
        trace!(idx = idx.0, "ENTER get_type_from_type_query");

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR; // Missing type query data - propagate error
        };

        if self.is_import_type_query(type_query.expr_name) {
            trace!("get_type_from_type_query: is import type query");
            return TypeId::ANY;
        }

        let name_text = self.entity_name_text(type_query.expr_name);
        let is_identifier = self
            .ctx
            .arena
            .get(type_query.expr_name)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();
        let has_type_args = type_query
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());
        let factory = self.ctx.types.factory();
        let flow_type_for_query_expr = |state: &mut Self| {
            let prev_skip = state.ctx.skip_flow_narrowing;
            state.ctx.skip_flow_narrowing = false;
            let ty = state.get_type_of_node(type_query.expr_name);
            state.ctx.skip_flow_narrowing = prev_skip;
            ty
        };

        if !has_type_args && let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
            // Handle QualifiedName (e.g. `typeof x.p`) by resolving as value property access.
            // QualifiedName in typeof context means value.property, not namespace.member,
            // so we can't send it through get_type_of_node which dispatches to resolve_qualified_name.
            if expr_node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
                if let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) {
                    let left_idx = qn.left;
                    let right_idx = qn.right;
                    // Resolve the left side as a value expression (with flow narrowing)
                    let prev_skip = self.ctx.skip_flow_narrowing;
                    self.ctx.skip_flow_narrowing = false;
                    let left_type = self.get_type_of_node(left_idx);
                    self.ctx.skip_flow_narrowing = prev_skip;
                    trace!(left_type = ?left_type, "type_query qualified: left_type");
                    if left_type == TypeId::ANY {
                        // globalThis resolves to ANY since it's a synthetic global.
                        // `typeof globalThis.foo` should also be ANY (no TS2304).
                        if let Some(left_node) = self.ctx.arena.get(left_idx)
                            && let Some(ident) = self.ctx.arena.get_identifier(left_node)
                            && ident.escaped_text == "globalThis"
                        {
                            return TypeId::ANY;
                        }
                    }
                    if left_type != TypeId::ANY && left_type != TypeId::ERROR {
                        // Look up the right side as a property on the left type
                        if let Some(right_node) = self.ctx.arena.get(right_idx)
                            && let Some(ident) = self.ctx.arena.get_identifier(right_node)
                        {
                            let prop_name = ident.escaped_text.clone();
                            let object_type = self.resolve_type_for_property_access(left_type);
                            trace!(object_type = ?object_type, prop_name = %prop_name, "type_query qualified: property access");
                            use tsz_solver::operations_property::PropertyAccessResult;
                            match self.resolve_property_access_with_env(object_type, &prop_name) {
                                PropertyAccessResult::Success { type_id, .. }
                                    if type_id != TypeId::ANY && type_id != TypeId::ERROR =>
                                {
                                    return type_id;
                                }
                                _ => {
                                    // Property access returned any/error or failed entirely.
                                    // Fall through to binder-based resolution below.
                                }
                            }
                        }
                    }
                    // Fall back: resolve via binder symbol exports for namespace members
                    if let Some(sym_id) = self.resolve_qualified_symbol(type_query.expr_name) {
                        let member_type = self.get_type_of_symbol(sym_id);
                        trace!(sym_id = ?sym_id, member_type = ?member_type, "type_query qualified: resolved via binder exports");
                        if member_type != TypeId::ERROR {
                            return member_type;
                        }
                    }
                }
            } else if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || expr_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                || expr_node.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16
            {
                // Prefer flow-aware value-space type at the query site.
                // This keeps `typeof expr` aligned with control-flow narrowing.
                // BUT skip Lazy types - those indicate circular reference (e.g., `typeof A`
                // inside class A's body). Lazy types resolve to the instance type via
                // resolve_lazy, but typeof needs the constructor type. Fall through to
                // create a TypeQuery(SymbolRef) which resolves correctly.
                let expr_type = flow_type_for_query_expr(self);
                let is_lazy =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, expr_type).is_some();
                if expr_type != TypeId::ANY && expr_type != TypeId::ERROR && !is_lazy {
                    return expr_type;
                }
            }
        }

        let base = if let Some(sym_id) =
            self.resolve_value_symbol_for_lowering(type_query.expr_name)
        {
            trace!("=== get_type_from_type_query ===");
            trace!(name = ?name_text, sym_id, "get_type_from_type_query");

            // Always compute the symbol type to ensure it's in the type environment
            // This is important for Application resolution and TypeQuery resolution during subtype checking
            let resolved = self.get_type_of_symbol(tsz_binder::SymbolId(sym_id));
            trace!(resolved = ?resolved, "resolved type");

            if !has_type_args {
                // Prefer flow-aware type at the query site for `typeof expr` in narrowed scopes
                // (e.g. inside `if (x.p === "A")`, `typeof x.p` should be `"A"`).
                // Skip Lazy types - they indicate circular reference and would resolve to
                // the instance type instead of the constructor type needed for typeof.
                let flow_resolved = flow_type_for_query_expr(self);
                let flow_is_lazy =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, flow_resolved)
                        .is_some();
                if flow_resolved != TypeId::ANY && flow_resolved != TypeId::ERROR && !flow_is_lazy {
                    trace!(flow_resolved = ?flow_resolved, "=> returning flow-resolved type directly");
                    return flow_resolved;
                }
                let resolved_is_lazy =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved).is_some();
                if resolved != TypeId::ANY && resolved != TypeId::ERROR && !resolved_is_lazy {
                    // Fall back to symbol type when flow result is unavailable.
                    trace!("=> returning symbol-resolved type directly");
                    return resolved;
                }
            }

            // For type arguments or when resolved is ANY/ERROR, use TypeQuery
            let typequery_type = factory.type_query(SymbolRef(sym_id));
            trace!(typequery_type = ?typequery_type, "=> returning TypeQuery type");
            typequery_type
        } else if self
            .resolve_type_symbol_for_lowering(type_query.expr_name)
            .is_some()
        {
            let name = name_text.as_deref().unwrap_or("<unknown>");
            self.error_type_only_value_at(name, type_query.expr_name);
            return TypeId::ERROR;
        } else if let Some(name) = name_text {
            if is_identifier {
                // Handle global intrinsics that may not have symbols in the binder
                // (e.g., `typeof undefined`, `typeof NaN`, `typeof Infinity`, `typeof globalThis`)
                match name.as_str() {
                    "undefined" => return TypeId::UNDEFINED,
                    "NaN" | "Infinity" => return TypeId::NUMBER,
                    // globalThis is a synthetic symbol in tsc whose exports are all globals.
                    // typeof globalThis should resolve to a type with all global members.
                    // For now, return ANY to suppress false TS2304/TS2552 errors.
                    // TODO: Create a proper object type with global members.
                    "globalThis" => return TypeId::ANY,
                    _ => {}
                }
                if self.is_known_global_value_name(&name) {
                    // Emit TS2318/TS2583 for missing global type in typeof context
                    // TS2583 for ES2015+ types, TS2304 for other globals
                    use tsz_binder::lib_loader;
                    if lib_loader::is_es2015_plus_type(&name) {
                        self.error_cannot_find_global_type(&name, type_query.expr_name);
                    } else {
                        self.error_cannot_find_name_at(&name, type_query.expr_name);
                    }
                    return TypeId::ERROR;
                }
                // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                if self.is_unresolved_import_symbol(type_query.expr_name) {
                    return TypeId::ANY;
                }
                self.error_cannot_find_name_at(&name, type_query.expr_name);
                return TypeId::ERROR;
            }
            if let Some(missing_idx) = self.missing_type_query_left(type_query.expr_name)
                && let Some(missing_name) = self
                    .ctx
                    .arena
                    .get(missing_idx)
                    .and_then(|node| self.ctx.arena.get_identifier(node))
                    .map(|ident| ident.escaped_text.clone())
            {
                // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                if self.is_unresolved_import_symbol(missing_idx) {
                    return TypeId::ANY;
                }
                self.error_cannot_find_name_at(&missing_name, missing_idx);
                return TypeId::ERROR;
            }
            if self.report_type_query_missing_member(type_query.expr_name) {
                return TypeId::ERROR;
            }
            // Not found - fall back to hash (for forward compatibility)
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            name.hash(&mut hasher);
            let symbol_id = hasher.finish() as u32;
            factory.type_query(SymbolRef(symbol_id))
        } else {
            return TypeId::ERROR; // No name text - propagate error
        };

        let factory = self.ctx.types.factory();
        if let Some(args) = &type_query.type_arguments
            && !args.nodes.is_empty()
        {
            let type_args = args
                .nodes
                .iter()
                .map(|&idx| self.get_type_from_type_node(idx))
                .collect();
            return factory.application(base, type_args);
        }

        base
    }

    fn is_import_type_query(&self, expr_name: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_name) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call_expr) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(callee) = self.ctx.arena.get(call_expr.expression) else {
            return false;
        };
        callee.kind == tsz_scanner::SyntaxKind::ImportKeyword as u16
    }

    // Get type of a JSX opening element.
    // NOTE: get_type_of_jsx_opening_element, get_jsx_namespace_type,
    // get_intrinsic_elements_type, get_jsx_element_type moved to jsx_checker.rs

    // NOTE: get_type_from_type_node_in_type_literal, get_type_from_type_reference_in_type_literal,
    // extract_params_from_signature_in_type_literal, get_type_from_type_literal
    // moved to type_literal_checker.rs

    /// Push type parameters into scope for generic type resolution.
    ///
    /// This is a critical function for handling generic types (classes, interfaces,
    /// functions, type aliases). It makes type parameters available for use within
    /// the generic type's body and returns information for later scope restoration.
    ///
    /// ## Two-Pass Algorithm:
    /// 1. **First pass**: Adds all type parameters to scope WITHOUT constraints
    ///    - This allows self-referential constraints like `T extends Box<T>`
    ///    - Creates unconstrained `TypeParameter` entries
    /// 2. **Second pass**: Resolves constraints and defaults with all params in scope
    ///    - Now all type parameters are visible for constraint resolution
    ///    - Updates the scope with constrained `TypeParameter` entries
    ///
    /// ## Returns:
    /// - `Vec<TypeParamInfo>`: Type parameter info with constraints and defaults
    /// - `Vec<(String, Option<TypeId>)>`: Restoration data for `pop_type_parameters`
    ///
    /// ## Constraint Validation:
    /// - Emits TS2315 if constraint type is error
    /// - Emits TS2314 if default doesn't satisfy constraint
    /// - Uses UNKNOWN for invalid constraints
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Simple type parameter
    /// function identity<T>(value: T): T { return value; }
    /// // push_type_parameters adds T to scope
    ///
    /// // Type parameter with constraint
    /// interface Comparable<T> {
    ///   compare(other: T): number;
    /// }
    /// function max<T extends Comparable<T>>(a: T, b: T): T {
    ///   // T is in scope with constraint Comparable<T>
    ///   return a.compare(b) > 0 ? a : b;
    /// }
    ///
    /// // Type parameter with default
    /// interface Box<T = string> {
    ///   value: T;
    /// }
    /// // T has default type string
    ///
    /// // Self-referential constraint (requires two-pass algorithm)
    /// type Box<T extends Box<T>> = T;
    /// // First pass: T added to scope unconstrained
    /// // Second pass: Constraint Box<T> resolved (T now in scope)
    ///
    /// // Multiple type parameters
    /// interface Map<K, V> {
    ///   get(key: K): V | undefined;
    ///   set(key: K, value: V): void;
    /// }
    /// ```
    pub(crate) fn push_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> TypeParamPushResult {
        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        // Recursion depth check: prevent stack overflow from circular type parameter
        // references (e.g. interface I<T extends I<T>> {} or circular generic defaults)
        if !self.ctx.enter_recursion() {
            return (Vec::new(), Vec::new());
        }

        let mut params = Vec::new();
        let mut updates = Vec::new();
        let mut param_indices = Vec::new();
        let mut seen_names = FxHashSet::default();

        // First pass: Add all type parameters to scope WITHOUT resolving constraints
        // This allows self-referential constraints like T extends Box<T>
        let factory = self.ctx.types.factory();

        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());

            // Check for duplicate type parameter names (TS2300)
            if !seen_names.insert(name.clone()) {
                self.error_at_node_msg(
                    data.name,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name],
                );
            }

            let atom = self.ctx.types.intern_string(&name);

            // Create unconstrained type parameter initially
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
            param_indices.push(param_idx);
        }

        // Second pass: Now resolve constraints and defaults with all type parameters in scope
        for &param_idx in &param_indices {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);

            let constraint = if data.constraint != NodeIndex::NONE {
                // Check for circular constraint: T extends T
                // First get the constraint type ID
                let constraint_type = self.get_type_from_type_node(data.constraint);

                // Check if the constraint references the same type parameter
                let is_circular =
                    if let Some(&param_type_id) = self.ctx.type_parameter_scope.get(&name) {
                        // Check if constraint_type is the same as the type parameter
                        // or if it's a TypeReference that resolves to this type parameter
                        self.is_same_type_parameter(constraint_type, param_type_id, &name)
                    } else {
                        false
                    };

                if is_circular {
                    // TS2313: Type parameter 'T' has a circular constraint
                    self.error_at_node_msg(
                        data.constraint,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                        &[&name],
                    );
                    Some(TypeId::UNKNOWN)
                } else {
                    // Note: Even if constraint_type is ERROR, we don't emit an error here
                    // because the error for the unresolved type was already emitted by get_type_from_type_node.
                    // This prevents duplicate error messages.
                    Some(constraint_type)
                }
            } else {
                None
            };

            let default = if data.default != NodeIndex::NONE {
                let default_type = self.get_type_from_type_node(data.default);
                // Validate that default satisfies constraint if present
                if let Some(constraint_type) = constraint
                    && default_type != TypeId::ERROR
                    && !crate::query_boundaries::generic_checker::contains_type_parameters(
                        self.ctx.types,
                        constraint_type,
                    )
                    && !crate::query_boundaries::generic_checker::contains_type_parameters(
                        self.ctx.types,
                        default_type,
                    )
                    && !self.is_assignable_to(default_type, constraint_type)
                {
                    let type_str = self.format_type(default_type);
                    let constraint_str = self.format_type(constraint_type);
                    self.error_at_node_msg(
                        data.default,
                        crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                        &[&type_str, &constraint_str],
                    );
                }
                if default_type == TypeId::ERROR {
                    None
                } else {
                    Some(default_type)
                }
            } else {
                None
            };

            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default,
                is_const: false,
            };
            params.push(info.clone());

            // UPDATE: Create a new TypeParameter with constraints and update the scope
            // This ensures that when function parameters reference these type parameters,
            // they get the constrained version, not the unconstrained placeholder
            let constrained_type_id = factory.type_param(info);
            self.ctx
                .type_parameter_scope
                .insert(name.clone(), constrained_type_id);
        }

        // Third pass: Detect indirect circular constraints (e.g., T extends U, U extends T)
        // Build a constraint graph among type parameters in this list and detect cycles.
        self.check_indirect_circular_constraints(&params, &param_indices);

        self.ctx.leave_recursion();
        (params, updates)
    }

    /// Detect indirect circular constraints among type parameters.
    ///
    /// For each type parameter, if its constraint is another type parameter in the same
    /// list, follow the chain. If we reach the original parameter, emit TS2313.
    /// Direct self-references (T extends T) are already caught in the second pass.
    fn check_indirect_circular_constraints(
        &mut self,
        params: &[tsz_solver::TypeParamInfo],
        param_indices: &[NodeIndex],
    ) {
        // Build a map: param name (Atom) -> index in params list
        let mut name_to_idx: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let param_names: Vec<String> = params
            .iter()
            .map(|p| self.ctx.types.resolve_atom(p.name))
            .collect();
        for (i, name) in param_names.iter().enumerate() {
            name_to_idx.insert(name.clone(), i);
        }

        // For each param, check if its constraint forms an indirect cycle
        for (i, param) in params.iter().enumerate() {
            let Some(constraint_type) = param.constraint else {
                continue;
            };

            // Get the name of the constraint if it's a type parameter
            let constraint_info =
                tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, constraint_type);
            let Some(constraint_info) = constraint_info else {
                continue;
            };
            let constraint_name = self
                .ctx
                .types
                .resolve_atom(constraint_info.name)
                .to_string();

            // Skip direct self-references (already caught)
            if constraint_name == param_names[i] {
                continue;
            }

            // Only follow if constraint is another param in the same list
            let Some(&next_idx) = name_to_idx.get(&constraint_name) else {
                continue;
            };

            // Follow the chain to detect if it cycles back to param i.
            // Only report if the chain leads back to the starting parameter itself,
            // not if it merely reaches some other cycle.
            let mut current = next_idx;
            let mut steps = 0;
            let max_steps = params.len();

            let is_in_cycle = loop {
                if current == i {
                    break true;
                }
                steps += 1;
                if steps > max_steps {
                    break false;
                }

                // Follow the constraint of the current param
                let Some(next_constraint) = params[current].constraint else {
                    break false;
                };
                let next_info = tsz_solver::type_queries::get_type_parameter_info(
                    self.ctx.types,
                    next_constraint,
                );
                let Some(next_info) = next_info else {
                    break false;
                };
                let next_name = self.ctx.types.resolve_atom(next_info.name).to_string();
                let Some(&next) = name_to_idx.get(&next_name) else {
                    break false;
                };
                current = next;
            };

            if is_in_cycle {
                let node_idx = param_indices[i];
                if let Some(node) = self.ctx.arena.get(node_idx)
                    && let Some(data) = self.ctx.arena.get_type_parameter(node)
                    && data.constraint != NodeIndex::NONE
                {
                    self.error_at_node_msg(
                        data.constraint,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                        &[&param_names[i]],
                    );
                }
            }
        }
    }

    /// Check if a constraint type is the same as a type parameter (circular constraint).
    ///
    /// This detects cases like `T extends T` where the type parameter references itself
    /// in its own constraint.
    pub(crate) fn is_same_type_parameter(
        &self,
        constraint_type: TypeId,
        param_type_id: TypeId,
        param_name: &str,
    ) -> bool {
        // Direct match
        if constraint_type == param_type_id {
            return true;
        }

        // Check if constraint is a TypeParameter with the same name
        if let Some(info) =
            tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, constraint_type)
        {
            // Check if the type parameter name matches
            let name_str = self.ctx.types.resolve_atom(info.name);
            if name_str == param_name {
                return true;
            }
        }

        false
    }

    /// Get type of a symbol with caching and circular reference detection.
    ///
    /// This is the main entry point for resolving the type of symbols (variables,
    /// functions, classes, interfaces, type aliases, etc.). All type resolution
    /// ultimately flows through this function.
    ///
    /// ## Caching:
    /// - Symbol types are cached in `ctx.symbol_types` by symbol ID
    /// - Subsequent calls for the same symbol return the cached type
    /// - Cache is populated on first successful resolution
    ///
    /// ## Fuel Management:
    /// - Consumes fuel on each call to prevent infinite loops
    /// - Returns ERROR if fuel is exhausted (prevents type checker timeout)
    ///
    /// ## Circular Reference Detection:
    /// - Tracks currently resolving symbols in `ctx.symbol_resolution_set`
    /// - Returns ERROR if a circular reference is detected
    /// - Uses a stack to track resolution depth
    ///
    /// ## Type Environment Population:
    /// - After resolution, populates the type environment for generic type expansion
    /// - For classes: Handles instance type with type parameters specially
    /// - For generic types: Stores both the type and its type parameters
    /// - Skips ANY/ERROR types (don't populate environment for errors)
    ///
    /// ## Symbol Dependency Tracking:
    /// - Records symbol dependencies for incremental type checking
    /// - Pushes/pops from dependency stack during resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// let x = 42;              // get_type_of_symbol(x) → number
    /// function foo(): void {}  // get_type_of_symbol(foo) → () => void
    /// class C {}               // get_type_of_symbol(C) → typeof C (constructor)
    /// interface I {}           // get_type_of_symbol(I) → I (interface type)
    /// type T = string;         // get_type_of_symbol(T) → string
    /// ```
    pub fn get_type_of_symbol(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_solver::SymbolRef;

        let factory = self.ctx.types.factory();
        self.record_symbol_dependency(sym_id);

        // Check cache first
        if let Some(&cached) = self.ctx.symbol_types.get(&sym_id) {
            trace!(
                sym_id = sym_id.0,
                type_id = cached.0,
                file = self.ctx.file_name.as_str(),
                "(cached) get_type_of_symbol"
            );
            return cached;
        }

        // Check fuel - return ERROR if exhausted to prevent timeout
        if !self.ctx.consume_fuel() {
            // Cache ERROR so we don't keep trying to resolve this symbol
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR;
        }

        // Check for circular reference
        if self.ctx.symbol_resolution_set.contains(&sym_id) {
            // CRITICAL: For named entities (Interface, Class, TypeAlias, Enum), return Lazy placeholder
            // instead of ERROR. This allows circular dependencies to work correctly.
            //
            // For example: `interface User { filtered: Filtered } type Filtered = { [K in keyof User]: ... }`
            // When Filtered evaluates `keyof User` and User is still being checked, we return Lazy(User)
            // instead of ERROR, allowing the type system to defer evaluation.
            //
            // For other symbols (variables, functions, etc.), we still return ERROR to prevent infinite loops.
            let symbol = self.ctx.binder.get_symbol(sym_id);
            if let Some(symbol) = symbol {
                let flags = symbol.flags;
                if flags
                    & (symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    != 0
                {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    let lazy_type = factory.lazy(def_id);
                    // Don't cache the Lazy type - we want to retry when the circular reference is broken
                    return lazy_type;
                }
            }

            // For non-named entities, cache ERROR to prevent repeated deep recursion
            // This is key for fixing timeout issues with circular class inheritance
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR; // Circular reference - propagate error
        }

        // Check recursion depth to prevent stack overflow
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= self.ctx.max_symbol_resolution_depth {
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR; // Depth exceeded - prevent stack overflow
        }
        self.ctx.symbol_resolution_depth.set(depth + 1);

        // Push onto resolution stack
        self.ctx.symbol_resolution_stack.push(sym_id);
        self.ctx.symbol_resolution_set.insert(sym_id);

        // CRITICAL: Pre-cache a placeholder to break deep recursion chains
        // This prevents stack overflow in circular class inheritance by ensuring
        // that when we try to resolve this symbol again mid-resolution, we get
        // the cached value immediately instead of recursing deeper.
        // We'll overwrite this with the real result later (line 815).
        //
        // For named entities (Interface, Class, TypeAlias, Enum), use a Lazy type
        // as the placeholder instead of ERROR. This allows circular dependencies
        // like `interface User { filtered: Filtered } type Filtered = { [K in keyof User]: ... }`
        // to work correctly, since keyof Lazy(User) can defer evaluation instead of failing.
        let symbol = self.ctx.binder.get_symbol(sym_id);
        let placeholder = if let Some(symbol) = symbol {
            let flags = symbol.flags;
            if flags
                & (symbol_flags::INTERFACE
                    | symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE)
                != 0
            {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                factory.lazy(def_id)
            } else {
                TypeId::ERROR
            }
        } else {
            TypeId::ERROR
        };
        trace!(
            sym_id = sym_id.0,
            placeholder = placeholder.0,
            is_lazy =
                tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, placeholder).is_some(),
            file = self.ctx.file_name.as_str(),
            "get_type_of_symbol: inserted placeholder"
        );
        self.ctx.symbol_types.insert(sym_id, placeholder);

        self.push_symbol_dependency(sym_id, true);
        let (result, type_params) = self.compute_type_of_symbol(sym_id);
        self.pop_symbol_dependency();

        // Pop from resolution stack
        self.ctx.symbol_resolution_stack.pop();
        self.ctx.symbol_resolution_set.remove(&sym_id);

        // Decrement recursion depth
        self.ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get() - 1);

        // Cache result
        self.ctx.symbol_types.insert(sym_id, result);
        trace!(
            sym_id = sym_id.0,
            type_id = result.0,
            file = self.ctx.file_name.as_str(),
            "get_type_of_symbol"
        );

        // Also populate the type environment for Application expansion
        // IMPORTANT: We use the type_params returned by compute_type_of_symbol
        // because those are the same TypeIds used when lowering the type body.
        // Calling get_type_params_for_symbol would create fresh TypeIds that don't match.
        if result != TypeId::ANY && result != TypeId::ERROR {
            // For class symbols, we need to cache BOTH the constructor type (for value position)
            // and the instance type (for type position with typeof/TypeQuery resolution).
            let class_env_entry = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                if symbol.flags & symbol_flags::CLASS != 0 {
                    self.class_instance_type_with_params_from_symbol(sym_id)
                } else {
                    None
                }
            });

            // Use try_borrow_mut to avoid panic if type_env is already borrowed.
            // This can happen during recursive type resolution (e.g., class inheritance).
            // If we can't borrow, skip the cache update - the type is still computed correctly.
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                // Get the DefId if one exists (Phase 4.3 migration)
                let def_id = self.ctx.symbol_to_def.borrow().get(&sym_id).copied();

                // For CLASS symbols:
                // - `result` is the constructor type (Callable with construct signatures)
                // - `instance_type` is the instance type (Object with properties)
                //
                // We cache the CONSTRUCTOR type in the type environment so that:
                // - `typeof Animal` resolves to the constructor type
                // - `Animal` used as a value resolves to the constructor type
                //
                // The instance type is still available via `class_instance_type_from_symbol`
                // for type position contexts where it's needed.
                if let Some((instance_type, _instance_params)) = &class_env_entry {
                    // This is a CLASS symbol - cache the constructor type (result)
                    // NOT the instance type. The instance type is used for class
                    // type position (e.g., `a: Animal`), not value position.
                    if type_params.is_empty() {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                            // Also register the instance type so resolve_lazy returns it
                            // in type position (e.g., `{new(): Foo}` where Foo is a class)
                            env.insert_class_instance_type(def_id, *instance_type);
                        }
                    } else {
                        env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, type_params);
                            // Also register the instance type for class
                            env.insert_class_instance_type(def_id, *instance_type);
                        }
                    }
                } else if type_params.is_empty() {
                    // Check if resolve_lib_type_by_name already registered type params
                    // for this DefId. This happens for lib interfaces like Promise<T>,
                    // Array<T> where compute_type_of_symbol returns empty params but
                    // the lib resolution path registered them via ctx.insert_def_type_params.
                    let lib_params = def_id.and_then(|d| self.ctx.get_def_type_params(d));
                    if let Some(params) = lib_params {
                        env.insert_with_params(SymbolRef(sym_id.0), result, params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, params);
                        }
                    } else {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                        }
                    }
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                }

                // Register numeric enums for Rule #7 (Open Numeric Enums)
                if let Some(def_id) = def_id {
                    self.maybe_register_numeric_enum(&mut env, sym_id, def_id);
                }

                // Register enum parent relationships for Task #17 (Enum Type Resolution)
                if let Some(def_id) = def_id
                    && let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
                {
                    use tsz_binder::symbol_flags;
                    if (symbol.flags & symbol_flags::ENUM_MEMBER) != 0 {
                        let parent_sym_id = symbol.parent;
                        if let Some(&parent_def_id) =
                            self.ctx.symbol_to_def.borrow().get(&parent_sym_id)
                        {
                            env.register_enum_parent(def_id, parent_def_id);
                        }
                    }
                }
            } else {
                let sym_name = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map_or("<unknown>", |s| s.escaped_name.as_str());
                tracing::warn!(
                    sym_id = sym_id.0,
                    sym_name = sym_name,
                    type_id = result.0,
                    type_params_count = type_params.len(),
                    "type_env try_borrow_mut FAILED - skipping insertion"
                );
            }
        }

        result
    }

    /// Check if a symbol is a numeric enum and register it in the `TypeEnvironment`.
    ///
    /// This is used for Rule #7 (Open Numeric Enums) where number types are
    /// assignable to/from numeric enums.
    fn maybe_register_numeric_enum(
        &self,
        env: &mut tsz_solver::TypeEnvironment,
        sym_id: SymbolId,
        def_id: tsz_solver::def::DefId,
    ) {
        // Check if the symbol is an enum
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.flags & symbol_flags::ENUM == 0 {
            return;
        }

        // Get the enum declaration to check if it's numeric
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            match symbol.declarations.first() {
                Some(&idx) => idx,
                None => return,
            }
        };

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // Check enum members to determine if it's numeric
        let mut saw_string = false;
        let mut saw_numeric = false;

        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            if !member.initializer.is_none() {
                let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                    continue;
                };
                match init_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => saw_string = true,
                    k if k == SyntaxKind::NumericLiteral as u16 => saw_numeric = true,
                    _ => {}
                }
            } else {
                // Members without initializers are auto-incremented numbers
                saw_numeric = true;
            }
        }

        // Register as numeric enum if it's numeric (not string-only)
        if saw_numeric && !saw_string {
            env.register_numeric_enum(def_id);
        }
    }

    /// Get a symbol from the current binder, lib binders, or other file binders.
    /// This ensures we can resolve symbols from lib.d.ts and other files.
    pub(crate) fn get_symbol_globally(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        // 1. Check current file
        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        // 2. Check lib files (lib.d.ts, etc.)
        for lib in &self.ctx.lib_contexts {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        // 3. Check other files in the project (multi-file mode)
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    /// Get a symbol, preferring the cross-file binder for known cross-file `SymbolIds`.
    ///
    /// Unlike `get_symbol_globally` (which checks the local binder first and may find
    /// a WRONG symbol due to `SymbolId` collisions), this method checks
    /// `cross_file_symbol_targets` FIRST. If the `SymbolId` is known to belong to another
    /// file, the target file's binder is used directly, avoiding the collision.
    ///
    /// Falls back to `get_symbol_globally` for non-cross-file symbols.
    pub(crate) fn get_cross_file_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        // Check if this is a known cross-file symbol
        let file_idx = self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .get(&sym_id)
            .copied();
        if let Some(file_idx) = file_idx
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        // Fall back to global search
        self.get_symbol_globally(sym_id)
    }

    /// Delegate symbol resolution to a checker using the correct arena.
    ///
    /// When a symbol's arena differs from the current arena (cross-file symbol),
    /// we create a child checker with the correct arena and delegate the resolution.
    /// This ensures symbols are resolved in their original context.
    ///
    /// ## Returns:
    /// - `Some((type_id, params))`: Delegation occurred, use this result
    /// - `None`: Symbol is in the local arena, proceed with local computation
    ///
    /// ## Critical Behavior:
    /// - Removes the "in-progress" ERROR marker from cache before delegation
    /// - Shares the parent's cache via `with_parent_cache` (fixes Cache Isolation Bug)
    /// - Copies `lib_contexts` for global symbol resolution (Array, Promise, etc.)
    /// - Copies resolution sets for cross-file cycle detection
    pub(crate) fn delegate_cross_arena_symbol_resolution(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        // Fast path: if this is a known cross-file symbol, skip the namespace guard
        // (which would check the wrong symbol in the current binder) and go straight
        // to cross-file delegation.
        //
        // TYPE_ALIAS + value merge fix: When a user-defined type alias (e.g., `type Proxy<T>`)
        // has the same name as a global value (`declare var Proxy: ProxyConstructor`), the
        // merged symbol has both TYPE_ALIAS and value flags, and symbol_arenas may point to
        // the lib arena. Delegating to the lib arena loses the type alias declaration (which
        // lives in the user arena), causing property access on the instantiated type to fail.
        // If the type alias declaration exists in the current arena, handle it locally.
        {
            let sym_found = self.get_symbol_globally(sym_id);
            let has_type_alias = sym_found.is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0);
            if has_type_alias {
                let symbol = sym_found.unwrap();
                tracing::debug!(
                    sym_id = sym_id.0,
                    name = %symbol.escaped_name,
                    num_decls = symbol.declarations.len(),
                    arena_len = self.ctx.arena.len(),
                    "delegate_cross_arena: checking TYPE_ALIAS in current arena"
                );
                let has_type_alias_in_current_arena = symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                // Verify the name matches to prevent NodeIndex collisions:
                                // A lib NodeIndex may accidentally map to a different
                                // TYPE_ALIAS_DECLARATION in the user arena.
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name_node = self.ctx.arena.get(type_alias.name)?;
                                let ident = self.ctx.arena.get_identifier(name_node)?;
                                let name = self.ctx.arena.resolve_identifier_text(ident);
                                Some(name == symbol.escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                });
                tracing::debug!(
                    sym_id = sym_id.0,
                    name = %symbol.escaped_name,
                    has_type_alias_in_current_arena,
                    "delegate_cross_arena: TYPE_ALIAS check result"
                );
                if has_type_alias_in_current_arena {
                    return None; // Handle locally, don't delegate to lib arena
                }
            }
        }
        let is_known_cross_file = self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .contains_key(&sym_id);

        if !is_known_cross_file
            && let Some(symbol) = self.get_symbol_globally(sym_id)
            && (symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
        {
            return None;
        }

        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        if delegate_arena.is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && let Some(symbol) = self.get_symbol_globally(sym_id)
        {
            // For INTERFACE symbols whose primary arena is already the current arena,
            // do NOT scan per-declaration arenas for delegation. Interfaces split across
            // multiple lib files (e.g., RegExp in es5 + es2015.symbol.wellknown) cause
            // ping-pong between arenas until the depth limit, resulting in ERROR.
            // The INTERFACE block in compute_type_of_symbol handles multi-arena merging
            // correctly via resolve_lib_type_by_name.
            if symbol.flags & symbol_flags::INTERFACE == 0 {
                let mut decl_candidates = symbol.declarations.clone();
                if !symbol.value_declaration.is_none() {
                    decl_candidates.push(symbol.value_declaration);
                }

                for decl_idx in decl_candidates {
                    if decl_idx.is_none() {
                        continue;
                    }
                    if let Some(arena) = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        && !std::ptr::eq(arena.as_ref(), self.ctx.arena)
                    {
                        delegate_arena = Some(arena.as_ref());
                        break;
                    }
                }
            }
        }

        // Check cross-file symbol target mapping as fallback.
        // When resolve_cross_file_export returns a SymbolId from another file's binder,
        // it records the target file index. Use that to find the correct arena AND binder.
        let mut cross_file_idx: Option<usize> = None;
        let needs_cross_file_delegation = delegate_arena
            .is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .cross_file_symbol_targets
                .borrow()
                .get(&sym_id)
                .is_some_and(|&file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = *self
                .ctx
                .cross_file_symbol_targets
                .borrow()
                .get(&sym_id)
                .unwrap();
            cross_file_idx = Some(file_idx);
        }

        // Check if we have a valid delegate arena (either from symbol_arenas/declaration_arenas
        // or from cross_file_symbol_targets).
        let should_delegate = if needs_cross_file_delegation {
            true
        } else {
            delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena))
        };

        if should_delegate {
            // Guard against deep cross-arena recursion to prevent stack overflow.
            // Uses shared thread-local counter across all delegation points.
            if !Self::enter_cross_arena_delegation() {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            }

            // Also check the per-checker recursion guard
            if !self.ctx.enter_recursion() {
                Self::leave_cross_arena_delegation();
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            }

            // Remove the in-progress ERROR marker before delegating to child checker.
            // The parent pre-caches ERROR as a cycle-detection marker and we don't
            // want the child checker to observe that placeholder.
            self.ctx.symbol_types.remove(&sym_id);

            // Re-fetch the arena reference after mutable operations above.
            // For cross-file symbols, use the target file's arena and binder.
            let (symbol_arena, delegate_binder) = if let Some(file_idx) = cross_file_idx {
                let arena = self.ctx.get_arena_for_file(file_idx as u32);
                let binder = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .unwrap_or(self.ctx.binder);
                (arena, binder)
            } else {
                // Non-cross-file delegation: use the already-computed arena.
                // Safe to re-fetch since the data hasn't changed.
                let arena = delegate_arena.unwrap_or(self.ctx.arena);
                (arena, self.ctx.binder)
            };

            // Box the child checker to keep it on the heap — nested delegations for
            // interdependent lib types (Array → ReadonlyArray → Iterator → ...) can
            // create deep call stacks, and CheckerState is too large to stack-allocate
            // at every level without risking stack overflow.
            let mut checker = Box::new(CheckerState::with_parent_cache(
                symbol_arena,
                delegate_binder,
                self.ctx.types,
                self.ctx.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
            ));
            // Copy lib contexts for global symbol resolution (Array, Promise, etc.)
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            // Copy cross-file symbol targets so nested resolutions work
            if !self.ctx.cross_file_symbol_targets.borrow().is_empty() {
                *checker.ctx.cross_file_symbol_targets.borrow_mut() =
                    self.ctx.cross_file_symbol_targets.borrow().clone();
            }
            // Copy all_arenas and all_binders for nested cross-file resolution
            checker.ctx.all_arenas = self.ctx.all_arenas.clone();
            checker.ctx.all_binders = self.ctx.all_binders.clone();
            checker.ctx.resolved_module_paths = self.ctx.resolved_module_paths.clone();
            checker.ctx.current_file_idx = cross_file_idx.unwrap_or(self.ctx.current_file_idx);
            // Copy symbol resolution state to detect cross-file cycles, but exclude
            // the current symbol (which the parent added) since this checker will
            // add it again during get_type_of_symbol
            for &id in &self.ctx.symbol_resolution_set {
                if id != sym_id {
                    checker.ctx.symbol_resolution_set.insert(id);
                }
            }
            // Copy class_instance_resolution_set to detect circular class inheritance
            for &id in &self.ctx.class_instance_resolution_set {
                checker.ctx.class_instance_resolution_set.insert(id);
            }
            // Copy class_constructor_resolution_set to detect circular constructor resolution
            for &id in &self.ctx.class_constructor_resolution_set {
                checker.ctx.class_constructor_resolution_set.insert(id);
            }
            // Use get_type_of_symbol to ensure proper cycle detection.
            let result = checker.get_type_of_symbol(sym_id);

            // DO NOT merge child's symbol_types back to the parent.
            // Cross-arena child checkers share the parent's binder (including node_symbols
            // which maps the parent file's node indices to symbol IDs) but operate on a
            // different arena. This causes node index collisions: a lib node at index N
            // can be confused with the parent file's node at index N, contaminating the
            // symbol cache (e.g., setting an ALIAS import symbol to STRING because the
            // same node index maps to a StringKeyword in the lib arena).
            // The delegated symbol's result is returned directly and cached by the caller
            // in get_type_of_symbol, so no merge-back is needed for correctness.

            // Merge child's DefId→SymbolId mappings to parent.
            // The child creates DefIds (in the shared DefinitionStore) for enum/class/etc.
            // symbols. These DefIds are embedded in TypeKeys in the shared TypeStore.
            // The parent needs DefId→SymbolId mappings to resolve these types
            // (e.g., for enum property access via resolve_namespace_value_member).
            // NOTE: symbol_to_def is NOT merged because SymbolIds are binder-local;
            // the same SymbolId maps to different symbols in different binders.
            {
                let child_d2s = checker.ctx.def_to_symbol.borrow();
                let mut parent_d2s = self.ctx.def_to_symbol.borrow_mut();
                for (&def_id, &sym_id) in child_d2s.iter() {
                    parent_d2s.entry(def_id).or_insert(sym_id);
                }
            }

            // Merge child's def_type_params to parent.
            // Generic type aliases (e.g., `type Constructor<T = {}> = new (...args: any[]) => T`)
            // register their type parameters in def_type_params. When the parent later tries to
            // expand Application(Lazy(DefId), Args) via CompatChecker, it needs these type params.
            {
                let child_params = checker.ctx.def_type_params.borrow();
                let mut parent_params = self.ctx.def_type_params.borrow_mut();
                for (def_id, params) in child_params.iter() {
                    parent_params
                        .entry(*def_id)
                        .or_insert_with(|| params.clone());
                }
            }

            // Merge child's type_env def entries (type alias bodies and params) to parent.
            // The child registers type alias bodies via type_env.insert_def_with_params.
            // The parent's CompatChecker needs these to expand Application types like
            // Constructor<{}> → new (...args: any[]) => {}.
            {
                let child_env = checker.ctx.type_env.borrow();
                let mut parent_env = self.ctx.type_env.borrow_mut();
                child_env.merge_defs_into(&mut parent_env);
            }

            self.ctx.leave_recursion();
            Self::leave_cross_arena_delegation();
            return Some((result, Vec::new()));
        }

        None
    }

    /// Delegate class instance type resolution to a child checker with the correct arena.
    ///
    /// When a class symbol's declaration is not in the current file's arena (cross-file case),
    /// this creates a child checker using the symbol's home arena and computes the instance
    /// type there, where the class declaration node is accessible.
    pub(crate) fn delegate_cross_arena_class_instance_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        // Find the symbol's home arena
        let delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        let symbol_arena = delegate_arena.filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))?;

        // Guard against deep cross-arena recursion
        if !Self::enter_cross_arena_delegation() {
            return None;
        }

        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        let mut checker = Box::new(CheckerState::with_parent_cache(
            symbol_arena,
            self.ctx.binder,
            self.ctx.types,
            self.ctx.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        for &id in &self.ctx.class_instance_resolution_set {
            checker.ctx.class_instance_resolution_set.insert(id);
        }
        for &id in &self.ctx.symbol_resolution_set {
            if id != sym_id {
                checker.ctx.symbol_resolution_set.insert(id);
            }
        }
        for &id in &self.ctx.class_constructor_resolution_set {
            checker.ctx.class_constructor_resolution_set.insert(id);
        }

        let result = checker.class_instance_type_with_params_from_symbol(sym_id);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        result
    }

    /// Detect and record cross-file `SymbolIds`.
    ///
    /// In multi-file mode, the driver copies target file's `module_exports` into
    /// the local binder, so `SymbolIds` may be from another file's binder. We
    /// detect this by checking if the `SymbolId` maps to a symbol with the expected
    /// name in the current binder. If not, we search `all_binders` to find the
    /// correct source file.
    pub(crate) fn record_cross_file_symbol_if_needed(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
        module_name: &str,
    ) {
        // Skip if already recorded
        if self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .contains_key(&sym_id)
        {
            return;
        }

        // Check if the SymbolId maps to the expected name in the current binder.
        // If it does, this is a local symbol and no cross-file tracking needed.
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name.as_str() == expected_name
        {
            return;
        }

        // The SymbolId doesn't match in the current binder — it's cross-file.
        // Try resolve_import_target first (most reliable).
        if let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, target_file_idx);
            return;
        }

        // Fallback: search all binders for one where this SymbolId has the expected name.
        if let Some(binders) = &self.ctx.all_binders {
            for (idx, binder) in binders.iter().enumerate() {
                if let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx
                        .cross_file_symbol_targets
                        .borrow_mut()
                        .insert(sym_id, idx);
                    return;
                }
            }
        }
    }
}
