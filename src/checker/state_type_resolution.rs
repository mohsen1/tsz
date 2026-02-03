//! Type Reference Resolution Module
//!
//! Extracted from state.rs: Methods for resolving type references, cross-file
//! exports, and constructor type operations on CheckerState.

use crate::binder::{SymbolId, symbol_flags};
use crate::checker::state::CheckerState;
use crate::checker::symbol_resolver::TypeSymbolResolution;
use crate::interner::Atom;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::solver::def::DefId;
use crate::solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get type from a type reference node (e.g., "number", "string", "MyType").
    pub(crate) fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        // Fuel check: prevent infinite loops in circular type references
        if !self.ctx.consume_fuel() {
            return TypeId::ERROR;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // Get the TypeRefData from the arena
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type ref data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        // Check if type_name is a qualified name (A.B)
        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && name_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            if has_type_args {
                let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                    TypeSymbolResolution::Type(sym_id) => sym_id,
                    TypeSymbolResolution::ValueOnly(_) => {
                        let name = self
                            .entity_name_text(type_name_idx)
                            .unwrap_or_else(|| "<unknown>".to_string());
                        self.error_value_only_type_at(&name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => {
                        let _ = self.resolve_qualified_name(type_name_idx);
                        return TypeId::ERROR;
                    }
                };
                if let Some(args) = &type_ref.type_arguments {
                    if self.should_resolve_recursive_type_alias(sym_id, args) {
                        // Ensure the base type symbol is resolved first so its type params
                        // are available in the type_env for Application expansion
                        let _ = self.get_type_of_symbol(sym_id);
                    }
                    for &arg_idx in &args.nodes {
                        let _ = self.get_type_from_type_node(arg_idx);
                    }
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Phase 2: Use DefId resolver to prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver = |node_idx: NodeIndex| -> Option<DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(SymbolId(sym_id)))
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = crate::solver::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let type_id = lowering.lower_type(idx);
                // Phase 2: Still post-process to create DefIds for types that don't have them yet
                return self.ctx.maybe_create_lazy_from_resolved(type_id);
            }
            // No type arguments provided - check if this generic type requires them
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_qualified_symbol_in_type_position(type_name_idx)
            {
                let required_count = self.count_required_type_params(sym_id);
                if required_count > 0 {
                    let name = self
                        .entity_name_text(type_name_idx)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    self.error_generic_type_requires_type_arguments_at(&name, required_count, idx);
                }
            }
            return self.resolve_qualified_name(type_name_idx);
        }

        // Get the identifier for the type name
        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.as_str();
            let has_libs = self.ctx.has_lib_loaded();
            let is_known_global = self.is_known_global_type_name(name);

            if has_type_args {
                let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                let type_param = self.lookup_type_parameter(name);
                let type_resolution =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx);
                let sym_id = match type_resolution {
                    TypeSymbolResolution::Type(sym_id) => Some(sym_id),
                    TypeSymbolResolution::ValueOnly(_) => {
                        self.error_value_only_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => None,
                };
                // TS2318: Array<T> with noLib should emit "Cannot find global type 'Array'"
                if is_builtin_array && !has_libs && sym_id.is_none() {
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    // Only try resolving from lib binders if lib files are loaded (noLib is false)
                    if has_libs {
                        // Try resolving from lib binders before falling back to UNKNOWN
                        // First check if the global type exists via binder's get_global_type
                        let lib_binders = self.get_lib_binders();
                        if let Some(_global_sym) = self
                            .ctx
                            .binder
                            .get_global_type_with_libs(name, &lib_binders)
                        {
                            // Global type symbol exists in lib binders - try to resolve it
                            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                                // Successfully resolved - create a TypeApplication if there are type arguments
                                if let Some(args) = &type_ref.type_arguments {
                                    if !args.nodes.is_empty() {
                                        // Collect type argument IDs
                                        let type_args: Vec<TypeId> = args
                                            .nodes
                                            .iter()
                                            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                            .collect();
                                        // Create a TypeApplication to instantiate the generic type
                                        return self.ctx.types.application(type_id, type_args);
                                    }
                                }
                                return type_id;
                            }
                            // Symbol exists but failed to resolve - this is an error condition
                            // The type is declared but we couldn't get its TypeId, which shouldn't happen
                            // Fall through to emit error below
                        }
                        // Fall back to resolve_lib_type_by_name for cases where type may exist
                        // but get_global_type_with_libs doesn't find it
                        if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                            // Successfully resolved via alternate path - create TypeApplication if there are type arguments
                            if let Some(args) = &type_ref.type_arguments {
                                if !args.nodes.is_empty() {
                                    // Collect type argument IDs
                                    let type_args: Vec<TypeId> = args
                                        .nodes
                                        .iter()
                                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                        .collect();
                                    // Create a TypeApplication to instantiate the generic type
                                    return self.ctx.types.application(type_id, type_args);
                                }
                            }
                            return type_id;
                        }
                    }
                    // When has_lib_loaded() is false (noLib is true), the above block is skipped
                    // and falls through to the is_known_global_type_name check below,
                    // which emits TS2318 via error_cannot_find_global_type
                    if is_known_global {
                        return self.handle_missing_global_type_with_args(
                            name,
                            type_ref,
                            type_name_idx,
                        );
                    }
                    if name == "await" {
                        self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_name_idx) {
                        return TypeId::ANY;
                    }
                    self.error_cannot_find_name_at(name, type_name_idx);
                    return TypeId::ERROR;
                }
                if !is_builtin_array
                    && let Some(sym_id) = sym_id
                    && let Some(args) = &type_ref.type_arguments
                    && self.should_resolve_recursive_type_alias(sym_id, args)
                {
                    // Ensure the base type symbol is resolved first so its type params
                    // are available in the type_env for Application expansion
                    let _ = self.get_type_of_symbol(sym_id);
                }
                // Also ensure type arguments are resolved and in type_env
                // This is needed so that when we evaluate the Application, we can
                // resolve Ref types in the arguments
                if let Some(args) = &type_ref.type_arguments {
                    for &arg_idx in &args.nodes {
                        // Recursively get type from the arg - this will add any referenced
                        // symbols to type_env
                        let _ = self.get_type_from_type_node(arg_idx);
                    }
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Phase 2: Use DefId resolver to prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver = |node_idx: NodeIndex| -> Option<DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(SymbolId(sym_id)))
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = crate::solver::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let type_id = lowering.lower_type(idx);
                // Phase 2: Still post-process to create DefIds for types that don't have them yet
                return self.ctx.maybe_create_lazy_from_resolved(type_id);
            }

            if name == "Array" || name == "ReadonlyArray" {
                if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
                    return type_id;
                }
                // Array/ReadonlyArray not found - check if lib files are loaded
                // When --noLib is used, emit TS2318 instead of silently creating Array type
                if !self.ctx.has_lib_loaded() {
                    // No lib files loaded - emit TS2318 for missing global type
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                // Lib files are loaded but Array not found - this shouldn't happen normally
                // Fall back to creating Array type for graceful degradation
                let elem_type = type_ref
                    .type_arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first().copied())
                    .map(|idx| self.get_type_from_type_node(idx))
                    .unwrap_or(TypeId::ERROR);
                let array_type = self.ctx.types.array(elem_type);
                if name == "ReadonlyArray" {
                    return self
                        .ctx
                        .types
                        .intern(crate::solver::TypeKey::ReadonlyType(array_type));
                }
                return array_type;
            }

            // Check for built-in types (primitive keywords)
            match name {
                "number" => return TypeId::NUMBER,
                "string" => return TypeId::STRING,
                "boolean" => return TypeId::BOOLEAN,
                "void" => return TypeId::VOID,
                "any" => return TypeId::ANY,
                "never" => return TypeId::NEVER,
                "unknown" => return TypeId::UNKNOWN,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "object" => return TypeId::OBJECT,
                "bigint" => return TypeId::BIGINT,
                "symbol" => return TypeId::SYMBOL,
                _ => {}
            }

            // Check if this is a type parameter (generic type like T in function<T>)
            if let Some(type_param) = self.lookup_type_parameter(name) {
                return type_param;
            }

            if name != "Array" && name != "ReadonlyArray" {
                match self.resolve_identifier_symbol_in_type_position(type_name_idx) {
                    TypeSymbolResolution::Type(sym_id) => {
                        // TS2314: Check if this generic type requires type arguments
                        let type_params = self.get_type_params_for_symbol(sym_id);
                        let required_count = type_params.iter().filter(|p| p.default.is_none()).count();

                        if required_count > 0 {
                            self.error_generic_type_requires_type_arguments_at(
                                name,
                                required_count,
                                idx,
                            );
                            // Continue to resolve - we still want type inference to work
                        }

                        // Apply default type arguments if no explicit args were provided
                        // This handles cases like: type Box<T = string> = ...; let x: Box;
                        if type_ref.type_arguments.as_ref().map_or(true, |args| args.nodes.is_empty()) {
                            // No explicit type arguments provided
                            let has_defaults = type_params.iter().any(|p| p.default.is_some());

                            if has_defaults {
                                // Collect default type arguments
                                let default_args: Vec<TypeId> = type_params
                                    .iter()
                                    .map(|p| p.default.unwrap_or(TypeId::UNKNOWN))
                                    .collect();

                                // Create a Lazy type with DefId for proper type parameter substitution
                                let def_id = self.ctx.get_or_create_def_id(sym_id);
                                let base_type_id = self.ctx.types.intern(crate::solver::TypeKey::Lazy(def_id));

                                // Create TypeApplication with defaults
                                return self.ctx.types.application(base_type_id, default_args);
                            }
                        }
                    }
                    TypeSymbolResolution::ValueOnly(_) => {
                        self.error_value_only_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => {}
                }
            }

            if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
                return type_id;
            }
            if name == "await" {
                self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                return TypeId::ERROR;
            }
            if self.is_known_global_type_name(name) {
                // TS2318/TS2583: Emit error for missing global type
                // The type is a known global type but was not found in lib contexts
                self.error_cannot_find_global_type(name, type_name_idx);
                return TypeId::ERROR;
            }
            // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
            if self.is_unresolved_import_symbol(type_name_idx) {
                return TypeId::ANY;
            }
            self.error_cannot_find_name_at(name, type_name_idx);
            return TypeId::ERROR;
        }

        // Unknown type name node kind - propagate error
        TypeId::ERROR
    }

    pub(crate) fn handle_missing_global_type_with_args(
        &mut self,
        name: &str,
        type_ref: &crate::parser::node::TypeRefData,
        type_name_idx: NodeIndex,
    ) -> TypeId {
        if self.is_mapped_type_utility(name) {
            if let Some(args) = &type_ref.type_arguments {
                for &arg_idx in &args.nodes {
                    let _ = self.get_type_from_type_node(arg_idx);
                }
            }
            return TypeId::ANY;
        }

        self.error_cannot_find_global_type(name, type_name_idx);

        if self.is_promise_like_name(name)
            && let Some(args) = &type_ref.type_arguments
        {
            let type_args: Vec<TypeId> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                .collect();
            if !type_args.is_empty() {
                return self.ctx.types.application(TypeId::PROMISE_BASE, type_args);
            }
        }

        if let Some(args) = &type_ref.type_arguments {
            for &arg_idx in &args.nodes {
                let _ = self.get_type_from_type_node(arg_idx);
            }
        }
        TypeId::ERROR
    }

    pub(crate) fn should_resolve_recursive_type_alias(
        &self,
        sym_id: SymbolId,
        type_args: &crate::parser::NodeList,
    ) -> bool {
        if !self.ctx.symbol_resolution_set.contains(&sym_id) {
            return true;
        }
        if self.ctx.symbol_resolution_stack.last().copied() != Some(sym_id) {
            return true;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        // Check if this is a type alias (original behavior)
        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            return self.type_args_match_alias_params(sym_id, type_args);
        }

        // For classes and interfaces, allow recursive references in type parameter constraints
        // Don't force eager resolution - this prevents false cycle detection for patterns like:
        // class C<T extends C<T>>
        // interface I<T extends I<T>>
        if symbol.flags & (symbol_flags::CLASS | symbol_flags::INTERFACE) != 0 {
            // Only resolve if we're not in a direct self-reference scenario
            // The symbol_resolution_stack check above handles direct recursion
            return false;
        }

        // For other symbol types, use type args matching
        self.type_args_match_alias_params(sym_id, type_args)
    }

    pub(crate) fn type_args_match_alias_params(
        &self,
        sym_id: SymbolId,
        type_args: &crate::parser::NodeList,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(type_params) = &type_alias.type_parameters else {
            return false;
        };
        if type_params.nodes.len() != type_args.nodes.len() {
            return false;
        }

        for (&param_idx, &arg_idx) in type_params.nodes.iter().zip(type_args.nodes.iter()) {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(param_name) = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|node| self.ctx.arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };

            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if arg_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let Some(arg_ref) = self.ctx.arena.get_type_ref(arg_node) else {
                    return false;
                };
                if arg_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|list| !list.nodes.is_empty())
                {
                    return false;
                }
                let Some(arg_name_node) = self.ctx.arena.get(arg_ref.type_name) else {
                    return false;
                };
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_name_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else if arg_node.kind == SyntaxKind::Identifier as u16 {
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    pub(crate) fn class_instance_type_from_symbol(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        self.class_instance_type_with_params_from_symbol(sym_id)
            .map(|(instance_type, _)| instance_type)
    }

    pub(crate) fn class_instance_type_with_params_from_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<crate::solver::TypeParamInfo>)> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        // Check if we're already resolving this class - return fallback to break cycle.
        // NOTE: We don't insert here because get_class_instance_type_inner will handle it.
        // The check here is just to catch cycles from callers who go through this function.
        if self.ctx.class_instance_resolution_set.contains(&sym_id) {
            // Already resolving this class - return a Lazy(DefId) fallback to break cycle.
            // Like Ref(SymbolRef), this resolves to ERROR during mid-resolution since the
            // class body isn't registered in TypeEnvironment yet. Once resolution completes
            // and register_resolved_type is called, the DefId becomes resolvable.
            let fallback = self.ctx.create_lazy_type_ref(sym_id);
            return Some((fallback, Vec::new()));
        }

        let (params, updates) = self.push_type_parameters(&class.type_parameters);
        let instance_type = self.get_class_instance_type(decl_idx, class);
        self.pop_type_parameters(updates);
        Some((instance_type, params))
    }

    pub(crate) fn type_reference_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        use crate::solver::TypeLowering;

        // Recursion depth check: prevents stack overflow from circular
        // interface/class type references (e.g. I<T extends I<T>>)
        if !self.ctx.enter_recursion() {
            return TypeId::ERROR;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            // For merged class+namespace symbols, return the constructor type (with namespace exports)
            // instead of the instance type. This allows accessing namespace members via Foo.Bar.
            if symbol.flags & symbol_flags::CLASS != 0
                && symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) == 0
                && let Some(instance_type) = self.class_instance_type_from_symbol(sym_id)
            {
                self.ctx.leave_recursion();
                return instance_type;
            }
            if symbol.flags & symbol_flags::INTERFACE != 0 {
                if !symbol.declarations.is_empty() {
                    // IMPORTANT: Use the correct arena for the symbol - lib types use a different arena
                    let symbol_arena = self
                        .ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .map(|arena| arena.as_ref())
                        .unwrap_or(self.ctx.arena);

                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        symbol_arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type =
                        lowering.lower_interface_declarations(&symbol.declarations);
                    let result =
                        self.merge_interface_heritage_types(&symbol.declarations, interface_type);
                    self.ctx.leave_recursion();
                    return result;
                }
                if !symbol.value_declaration.is_none() {
                    let result = self.get_type_of_interface(symbol.value_declaration);
                    self.ctx.leave_recursion();
                    return result;
                }
            }

            // For type aliases, resolve the body type using the correct arena
            if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                let decl_idx = if !symbol.value_declaration.is_none() {
                    symbol.value_declaration
                } else {
                    symbol
                        .declarations
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE)
                };
                if !decl_idx.is_none() {
                    // Get the correct arena for the symbol (lib arena or current arena)
                    let symbol_arena = self
                        .ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .map(|arena| arena.as_ref())
                        .unwrap_or(self.ctx.arena);

                    // Use the correct arena to get the node
                    if let Some(node) = symbol_arena.get(decl_idx)
                        && let Some(type_alias) = symbol_arena.get_type_alias(node)
                    {
                        let type_param_bindings = self.get_type_param_bindings();
                        let type_resolver =
                            |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                        let value_resolver =
                            |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);

                        let lowering = TypeLowering::with_resolvers(
                            symbol_arena,
                            self.ctx.types,
                            &type_resolver,
                            &value_resolver,
                        )
                        .with_type_param_bindings(type_param_bindings);

                        self.ctx.leave_recursion();
                        return lowering.lower_type(type_alias.type_node);
                    }
                }
            }
        }
        let result = self.get_type_of_symbol(sym_id);
        self.ctx.leave_recursion();
        result
    }

    /// Like `type_reference_symbol_type` but also returns the type parameters used.
    ///
    /// This is critical for Application type evaluation: when instantiating a generic
    /// type, we need the body type AND the type parameters to be built from the SAME
    /// call to `push_type_parameters`, so the TypeIds in the body match those in the
    /// substitution. Otherwise, substitution fails because the TypeIds don't match.
    pub(crate) fn type_reference_symbol_type_with_params(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<crate::solver::TypeParamInfo>) {
        use crate::solver::TypeLowering;

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            // For classes, use class_instance_type_with_params_from_symbol which
            // returns both the instance type AND the type params used to build it
            if symbol.flags & symbol_flags::CLASS != 0
                && symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) == 0
            {
                if let Some((instance_type, params)) =
                    self.class_instance_type_with_params_from_symbol(sym_id)
                {
                    return (instance_type, params);
                }
            }

            // For interfaces, lower with type parameters and return both
            if symbol.flags & symbol_flags::INTERFACE != 0 {
                if !symbol.declarations.is_empty() {
                    // Get type parameters from first declaration
                    let first_decl = symbol
                        .declarations
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    let type_params_list = if !first_decl.is_none() {
                        self.ctx
                            .arena
                            .get(first_decl)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .and_then(|iface| iface.type_parameters.clone())
                    } else {
                        None
                    };

                    // Push type params, lower interface, pop type params
                    let (params, updates) = self.push_type_parameters(&type_params_list);

                    let symbol_arena = self
                        .ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .map(|arena| arena.as_ref())
                        .unwrap_or(self.ctx.arena);

                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        symbol_arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type =
                        lowering.lower_interface_declarations(&symbol.declarations);
                    let merged =
                        self.merge_interface_heritage_types(&symbol.declarations, interface_type);

                    self.pop_type_parameters(updates);
                    return (merged, params);
                }
            }

            // For type aliases, get body type and params together
            if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                let decl_idx = if !symbol.value_declaration.is_none() {
                    symbol.value_declaration
                } else {
                    symbol
                        .declarations
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE)
                };
                if !decl_idx.is_none()
                    && let Some(node) = self.ctx.arena.get(decl_idx)
                    && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
                {
                    let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                    let alias_type = self.get_type_from_type_node(type_alias.type_node);
                    self.pop_type_parameters(updates);
                    return (alias_type, params);
                }
            }
        }

        // Fallback: get type of symbol and params separately
        let body_type = self.get_type_of_symbol(sym_id);
        let type_params = self.get_type_params_for_symbol(sym_id);
        (body_type, type_params)
    }

    // NOTE: merge_namespace_exports_into_constructor, merge_namespace_exports_into_function,
    // resolve_reexported_member moved to namespace_checker.rs

    /// Resolve a named type reference to its TypeId.
    ///
    /// This is a core function for resolving type names like `User`, `Array`, `Promise`,
    /// etc. to their actual type representations. It handles multiple resolution strategies.
    ///
    /// ## Resolution Strategy (in order):
    /// 1. **Type Parameters**: Check if name is a type parameter in current scope
    /// 2. **Global Augmentations**: Check if name is declared in `declare global` blocks
    /// 3. **Local Symbols**: Resolve to interface/class/type alias in current file
    /// 4. **Lib Types**: Fall back to lib.d.ts and library contexts
    ///
    /// ## Type Parameter Lookup:
    /// - Checks current type parameter scope first
    /// - Allows generic type parameters to shadow global types
    ///
    /// ## Global Augmentations:
    /// - Merges user's global declarations with lib.d.ts
    /// - Ensures augmentation properly extends base types
    ///
    /// ## Lib Context Resolution:
    /// - Searches through loaded library contexts
    /// - Handles built-in types (Object, Array, Promise, etc.)
    /// - Merges multiple declarations (interface merging)
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Type parameter lookup
    /// function identity<T>(value: T): T {
    ///   // resolve_named_type_reference("T") → type parameter T
    ///   return value;
    /// }
    ///
    /// // Local interface
    /// interface User {}
    /// // resolve_named_type_reference("User") → User interface type
    ///
    /// // Global type (from lib.d.ts)
    /// let arr: Array<string>;
    /// // resolve_named_type_reference("Array") → Array global type
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProp: string;
    ///   }
    /// }
    /// // resolve_named_type_reference("Window") → merged Window type
    ///
    /// // Type alias
    /// type UserId = number;
    /// // resolve_named_type_reference("UserId") → number
    /// ```
    pub(crate) fn resolve_named_type_reference(
        &mut self,
        name: &str,
        name_idx: NodeIndex,
    ) -> Option<TypeId> {
        if let Some(type_id) = self.lookup_type_parameter(name) {
            return Some(type_id);
        }
        // Check if this is a global augmentation (interface declared in `declare global` block)
        // If so, use resolve_lib_type_by_name to merge with lib.d.ts declarations
        let is_global_augmentation = self.ctx.binder.global_augmentations.contains_key(name);
        if is_global_augmentation {
            // For global augmentations, we must use resolve_lib_type_by_name to get
            // the proper merge of lib.d.ts + user augmentation
            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                return Some(type_id);
            }
        }
        if let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position(name_idx)
        {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Fall back to lib contexts for global type resolution
        // BUT only if lib files are actually loaded (noLib is false)
        if self.ctx.has_lib_loaded() {
            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                return Some(type_id);
            }
        }
        None
    }

    /// Resolve an export from another file using cross-file resolution.
    ///
    /// This method uses `all_binders` and `resolved_module_paths` to look up an export
    /// from a different file in multi-file mode. Returns the SymbolId of the export
    /// if found, or None if cross-file resolution is not available or the export is not found.
    ///
    /// This is the core of Phase 1.1: ModuleResolver ↔ Checker Integration.
    pub(crate) fn resolve_cross_file_export(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<crate::binder::SymbolId> {
        // First, try to resolve the module specifier to a target file index
        let target_file_idx = self.ctx.resolve_import_target(module_specifier)?;

        // Get the target file's binder
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;

        // Look up the export in the target binder's module_exports
        // The module_exports map is keyed by both file name and specifier,
        // so we try the file name first (which is more reliable)
        // Try to find the export in the target binder's module_exports
        // The module_exports is keyed by file paths and specifiers
        // Only check first entry which should be the file's exports
        if let Some((_file_key, exports_table)) = target_binder.module_exports.iter().next() {
            if let Some(sym_id) = exports_table.get(export_name) {
                return Some(sym_id);
            }
        }

        // Fall back to checking file_locals in the target binder
        target_binder.file_locals.get(export_name)
    }

    /// Resolve a namespace import (import * as ns) from another file using cross-file resolution.
    ///
    /// Returns a SymbolTable containing all exports from the target module.
    pub(crate) fn resolve_cross_file_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<crate::binder::SymbolTable> {
        let target_file_idx = self.ctx.resolve_import_target(module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;

        // Try to find exports in the target binder's module_exports
        // First, try the specifier itself
        if let Some(exports) = target_binder.module_exports.get(module_specifier) {
            return Some(exports.clone());
        }

        // Try iterating through module_exports to find matching file
        if let Some((_, exports_table)) = target_binder.module_exports.iter().next() {
            return Some(exports_table.clone());
        }

        None
    }

    /// Emit TS2307 error for a module that cannot be found.
    ///
    /// This function emits a "Cannot find module" error with the module specifier
    /// and attempts to report the error at the import declaration node if available.
    pub(crate) fn emit_module_not_found_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Only emit if report_unresolved_imports is enabled
        // (CLI driver handles module resolution in multi-file mode)
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        // IMPORTANT: Mark as emitted BEFORE calling self.error() to prevent race conditions
        // where multiple code paths check the set simultaneously
        let module_key = module_specifier.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return; // Already emitted - skip duplicate
        }
        self.ctx
            .modules_with_ts2307_emitted
            .insert(module_key.clone());

        // Try to find the import declaration node to get the module specifier span
        let (start, length) = if !decl_node.is_none() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                // For import equals declarations, try to get the module specifier node
                if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    // For ES6 import declarations, the module specifier should be available
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
                    // For import specifiers, try to find the parent import declaration
                    if let Some(ext) = self.ctx.arena.get_extended(decl_node) {
                        let parent = ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(parent) {
                            if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                                if let Some(import) = self.ctx.arena.get_import_decl(parent_node) {
                                    if let Some(module_node) =
                                        self.ctx.arena.get(import.module_specifier)
                                    {
                                        // Found the module specifier node - use its span
                                        (module_node.pos, module_node.end - module_node.pos)
                                    } else {
                                        // Fall back to the parent declaration node span
                                        (parent_node.pos, parent_node.end - parent_node.pos)
                                    }
                                } else {
                                    (parent_node.pos, parent_node.end - parent_node.pos)
                                }
                            } else {
                                (node.pos, node.end - node.pos)
                            }
                        } else {
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else {
                    // Use the declaration node span for other cases
                    (node.pos, node.end - node.pos)
                }
            } else {
                // No node available - use position 0
                (0, 0)
            }
        } else {
            // No declaration node - use position 0
            (0, 0)
        };

        // Note: We use self.error() which already checks emitted_diagnostics for deduplication
        // The key is (start, code), so we won't emit duplicate errors at the same location
        // Emit the TS2307 error
        use crate::checker::types::diagnostics::{diagnostic_messages, format_message};
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_specifier]);
        self.error(start, length, message, diagnostic_codes::CANNOT_FIND_MODULE);
    }

    /// Emit TS1192 error when a module has no default export.
    ///
    /// This is emitted when trying to use a default import (`import X from 'mod'`)
    /// but the module doesn't export a default binding.
    ///
    /// Note: This error is suppressed when `allowSyntheticDefaultImports` or
    /// `esModuleInterop` is enabled, as those flags allow importing modules
    /// without explicit default exports.
    pub(crate) fn emit_no_default_export_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Only emit if report_unresolved_imports is enabled
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // allowSyntheticDefaultImports allows default imports without explicit default export
        // This is implied by esModuleInterop
        if self.ctx.allow_synthetic_default_imports() {
            return;
        }

        // Get span from declaration node
        let (start, length) = if !decl_node.is_none() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::checker::types::diagnostics::{diagnostic_messages, format_message};
        let message =
            format_message(diagnostic_messages::MODULE_HAS_NO_DEFAULT_EXPORT, &[module_specifier]);
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT,
        );
    }

    /// Emit TS2305 error when a module has no exported member with the given name.
    ///
    /// This is emitted when trying to use a named import (`import { X } from 'mod'`)
    /// but the module doesn't export a member named 'X'.
    pub(crate) fn emit_no_exported_member_error(
        &mut self,
        module_specifier: &str,
        member_name: &str,
        decl_node: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Only emit if report_unresolved_imports is enabled
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get span from declaration node
        let (start, length) = if !decl_node.is_none() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::checker::types::diagnostics::{diagnostic_messages, format_message};
        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
            &[module_specifier, member_name],
        );
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
        );
    }

    /// Check if a module exists for cross-file resolution.
    ///
    /// Returns true if the module can be found via resolved_modules or through
    /// the context's cross-file resolution mechanism.
    pub(crate) fn module_exists_cross_file(&self, module_name: &str) -> bool {
        // Check if it's in resolved_modules (set by the driver for multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules {
            if resolved.contains(module_name) {
                return true;
            }
        }
        // Could add additional cross-file resolution checks here in the future
        false
    }

    pub(crate) fn apply_type_arguments_to_constructor_type(
        &mut self,
        ctor_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use crate::solver::CallableShape;
        use crate::solver::type_queries::get_callable_shape;

        let Some(type_arguments) = type_arguments else {
            return ctor_type;
        };

        if type_arguments.nodes.is_empty() {
            return ctor_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return ctor_type;
        }

        let Some(shape) = get_callable_shape(self.ctx.types, ctor_type) else {
            return ctor_type;
        };
        let mut matching: Vec<&crate::solver::CallSignature> = shape
            .construct_signatures
            .iter()
            .filter(|sig| sig.type_params.len() == type_args.len())
            .collect();

        if matching.is_empty() {
            matching = shape
                .construct_signatures
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .collect();
        }

        if matching.is_empty() {
            return ctor_type;
        }

        let instantiated_constructs: Vec<crate::solver::CallSignature> = matching
            .iter()
            .map(|sig| {
                let mut args = type_args.clone();
                if args.len() < sig.type_params.len() {
                    for param in sig.type_params.iter().skip(args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        args.push(fallback);
                    }
                }
                if args.len() > sig.type_params.len() {
                    args.truncate(sig.type_params.len());
                }
                self.instantiate_constructor_signature(sig, &args)
            })
            .collect();

        let new_shape = CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: instantiated_constructs,
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        };
        self.ctx.types.callable(new_shape)
    }

    /// Apply explicit type arguments to a callable type for function calls.
    ///
    /// When a function is called with explicit type arguments like `fn<T>(x: T)`,
    /// calling it as `fn<number>("hello")` should substitute `T` with `number` and
    /// then check if `"hello"` is assignable to `number`.
    ///
    /// This function creates a new callable type with the type parameters substituted,
    /// so that argument type checking can work correctly.
    pub(crate) fn apply_type_arguments_to_callable_type(
        &mut self,
        callee_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use crate::solver::CallableShape;
        use crate::solver::type_queries::{SignatureTypeKind, classify_for_signatures};

        let Some(type_arguments) = type_arguments else {
            return callee_type;
        };

        if type_arguments.nodes.is_empty() {
            return callee_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return callee_type;
        }

        match classify_for_signatures(self.ctx.types, callee_type) {
            SignatureTypeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);

                // Find call signatures that match the type argument count
                let mut matching: Vec<&crate::solver::CallSignature> = shape
                    .call_signatures
                    .iter()
                    .filter(|sig| sig.type_params.len() == type_args.len())
                    .collect();

                // If no exact match, try signatures with type params
                if matching.is_empty() {
                    matching = shape
                        .call_signatures
                        .iter()
                        .filter(|sig| !sig.type_params.is_empty())
                        .collect();
                }

                if matching.is_empty() {
                    return callee_type;
                }

                // Instantiate each matching signature with the type arguments
                let instantiated_calls: Vec<crate::solver::CallSignature> = matching
                    .iter()
                    .map(|sig| {
                        let mut args = type_args.clone();
                        // Fill in default type arguments if needed
                        if args.len() < sig.type_params.len() {
                            for param in sig.type_params.iter().skip(args.len()) {
                                let fallback = param
                                    .default
                                    .or(param.constraint)
                                    .unwrap_or(TypeId::UNKNOWN);
                                args.push(fallback);
                            }
                        }
                        if args.len() > sig.type_params.len() {
                            args.truncate(sig.type_params.len());
                        }
                        self.instantiate_call_signature(sig, &args)
                    })
                    .collect();

                let new_shape = CallableShape {
                    call_signatures: instantiated_calls,
                    construct_signatures: shape.construct_signatures.clone(),
                    properties: shape.properties.clone(),
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                };
                self.ctx.types.callable(new_shape)
            }
            SignatureTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.type_params.len() != type_args.len() {
                    return callee_type;
                }

                let instantiated_call = self.instantiate_call_signature(
                    &crate::solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: None,
                        return_type: shape.return_type,
                        type_predicate: None,
                        is_method: shape.is_method,
                    },
                    &type_args,
                );

                // Convert single signature to callable
                let new_shape = CallableShape {
                    call_signatures: vec![instantiated_call],
                    construct_signatures: vec![],
                    properties: vec![],
                    string_index: None,
                    number_index: None,
                };
                self.ctx.types.callable(new_shape)
            }
            _ => callee_type,
        }
    }

    pub(crate) fn base_constructor_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if let Some(name) = self.heritage_name_text(expr_idx) {
            // Filter out primitive types and literals that cannot be used in class extends
            if matches!(
                name.as_str(),
                "null"
                    | "undefined"
                    | "true"
                    | "false"
                    | "void"
                    | "0"
                    | "number"
                    | "string"
                    | "boolean"
                    | "never"
                    | "unknown"
                    | "any"
            ) {
                return None;
            }
        }
        let expr_type = self.get_type_of_node(expr_idx);

        // Evaluate application types to get the actual intersection type
        let evaluated_type = self.evaluate_application_type(expr_type);

        let ctor_types = self.constructor_types_from_type(evaluated_type);
        if ctor_types.is_empty() {
            return None;
        }
        let ctor_type = if ctor_types.len() == 1 {
            ctor_types[0]
        } else {
            self.ctx.types.intersection(ctor_types)
        };
        Some(self.apply_type_arguments_to_constructor_type(ctor_type, type_arguments))
    }

    pub(crate) fn constructor_types_from_type(&mut self, type_id: TypeId) -> Vec<TypeId> {
        use rustc_hash::FxHashSet;

        self.ensure_application_symbols_resolved(type_id);
        let mut ctor_types = Vec::new();
        let mut visited = FxHashSet::default();
        self.collect_constructor_types_from_type_inner(type_id, &mut ctor_types, &mut visited);
        ctor_types
    }

    pub(crate) fn collect_constructor_types_from_type_inner(
        &mut self,
        type_id: TypeId,
        ctor_types: &mut Vec<TypeId>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::type_queries::{ConstructorTypeKind, classify_constructor_type};

        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        if !visited.insert(evaluated) {
            return;
        }

        match classify_constructor_type(self.ctx.types, evaluated) {
            ConstructorTypeKind::Callable => {
                ctor_types.push(evaluated);
            }
            ConstructorTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.is_constructor {
                    ctor_types.push(evaluated);
                }
            }
            ConstructorTypeKind::Members(members) => {
                for member in members {
                    self.collect_constructor_types_from_type_inner(member, ctor_types, visited);
                }
            }
            ConstructorTypeKind::Inner(inner) => {
                self.collect_constructor_types_from_type_inner(inner, ctor_types, visited);
            }
            ConstructorTypeKind::Constraint(constraint) => {
                if let Some(constraint) = constraint {
                    self.collect_constructor_types_from_type_inner(constraint, ctor_types, visited);
                }
            }
            ConstructorTypeKind::NeedsTypeEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            ConstructorTypeKind::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            ConstructorTypeKind::TypeQuery(sym_ref) => {
                // typeof X - get the type of the symbol X and collect constructors from it
                use crate::binder::SymbolId;
                let sym_id = SymbolId(sym_ref.0);
                let sym_type = self.get_type_of_symbol(sym_id);
                self.collect_constructor_types_from_type_inner(sym_type, ctor_types, visited);
            }
            ConstructorTypeKind::NotConstructor => {}
        }
    }

    pub(crate) fn static_properties_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo> {
        use rustc_hash::{FxHashMap, FxHashSet};

        self.ensure_application_symbols_resolved(type_id);
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        self.collect_static_properties_from_type_inner(type_id, &mut props, &mut visited);
        props
    }

    pub(crate) fn collect_static_properties_from_type_inner(
        &mut self,
        type_id: TypeId,
        props: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::type_queries::{StaticPropertySource, get_static_property_source};

        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        if !visited.insert(evaluated) {
            return;
        }

        match get_static_property_source(self.ctx.types, evaluated) {
            StaticPropertySource::Properties(properties) => {
                for prop in properties {
                    props.entry(prop.name).or_insert(prop);
                }
            }
            StaticPropertySource::RecurseMembers(members) => {
                for member in members {
                    self.collect_static_properties_from_type_inner(member, props, visited);
                }
            }
            StaticPropertySource::RecurseSingle(inner) => {
                self.collect_static_properties_from_type_inner(inner, props, visited);
            }
            StaticPropertySource::NeedsEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            StaticPropertySource::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            StaticPropertySource::None => {}
        }
    }

    pub(crate) fn base_instance_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        let ctor_type = self.base_constructor_type_from_expression(expr_idx, type_arguments)?;
        self.instance_type_from_constructor_type(ctor_type)
    }

    pub(crate) fn merge_constructor_properties_from_type(
        &mut self,
        ctor_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
    ) {
        let base_props = self.static_properties_from_type(ctor_type);
        for (name, prop) in base_props {
            properties.entry(name).or_insert(prop);
        }
    }

    pub(crate) fn merge_base_instance_properties(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        string_index: &mut Option<crate::solver::IndexSignature>,
        number_index: &mut Option<crate::solver::IndexSignature>,
    ) {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.merge_base_instance_properties_inner(
            base_instance_type,
            properties,
            string_index,
            number_index,
            &mut visited,
        );
    }

    pub(crate) fn merge_base_instance_properties_inner(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        string_index: &mut Option<crate::solver::IndexSignature>,
        number_index: &mut Option<crate::solver::IndexSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::type_queries::{
            BaseInstanceMergeKind, classify_for_base_instance_merge,
        };

        if !visited.insert(base_instance_type) {
            return;
        }

        match classify_for_base_instance_merge(self.ctx.types, base_instance_type) {
            BaseInstanceMergeKind::Object(base_shape_id) => {
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                for base_prop in base_shape.properties.iter() {
                    properties
                        .entry(base_prop.name)
                        .or_insert_with(|| base_prop.clone());
                }
                if let Some(ref idx) = base_shape.string_index {
                    Self::merge_index_signature(string_index, idx.clone());
                }
                if let Some(ref idx) = base_shape.number_index {
                    Self::merge_index_signature(number_index, idx.clone());
                }
            }
            BaseInstanceMergeKind::Intersection(members) => {
                for member in members {
                    self.merge_base_instance_properties_inner(
                        member,
                        properties,
                        string_index,
                        number_index,
                        visited,
                    );
                }
            }
            BaseInstanceMergeKind::Union(members) => {
                use rustc_hash::FxHashMap;
                let mut common_props: Option<FxHashMap<Atom, crate::solver::PropertyInfo>> = None;
                let mut common_string_index: Option<crate::solver::IndexSignature> = None;
                let mut common_number_index: Option<crate::solver::IndexSignature> = None;

                for member in members {
                    let mut member_props: FxHashMap<Atom, crate::solver::PropertyInfo> =
                        FxHashMap::default();
                    let mut member_string_index = None;
                    let mut member_number_index = None;
                    let mut member_visited = rustc_hash::FxHashSet::default();
                    member_visited.insert(base_instance_type);

                    self.merge_base_instance_properties_inner(
                        member,
                        &mut member_props,
                        &mut member_string_index,
                        &mut member_number_index,
                        &mut member_visited,
                    );

                    if common_props.is_none() {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        continue;
                    }

                    let mut props = match common_props.take() {
                        Some(props) => props,
                        None => {
                            // This should never happen due to the check above, but handle gracefully
                            common_props = Some(member_props);
                            common_string_index = member_string_index;
                            common_number_index = member_number_index;
                            continue;
                        }
                    };
                    props.retain(|name, prop| {
                        let Some(member_prop) = member_props.get(name) else {
                            return false;
                        };
                        let merged_type = if prop.type_id == member_prop.type_id {
                            prop.type_id
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.type_id, member_prop.type_id])
                        };
                        let merged_write_type = if prop.write_type == member_prop.write_type {
                            prop.write_type
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.write_type, member_prop.write_type])
                        };
                        prop.type_id = merged_type;
                        prop.write_type = merged_write_type;
                        prop.optional |= member_prop.optional;
                        prop.readonly &= member_prop.readonly;
                        prop.is_method &= member_prop.is_method;
                        true
                    });
                    common_props = Some(props);

                    common_string_index = match (common_string_index.take(), member_string_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };
                    common_number_index = match (common_number_index.take(), member_number_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };

                    if common_props.as_ref().is_none_or(|props| props.is_empty())
                        && common_string_index.is_none()
                        && common_number_index.is_none()
                    {
                        break;
                    }
                }

                if let Some(props) = common_props {
                    for prop in props.into_values() {
                        properties.entry(prop.name).or_insert(prop);
                    }
                }
                if let Some(idx) = common_string_index {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = common_number_index {
                    Self::merge_index_signature(number_index, idx);
                }
            }
            BaseInstanceMergeKind::Other => {}
        }
    }
}
