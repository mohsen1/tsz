//! Type literal checking (type resolution, references, and signatures within type literals).
//!
//! Type literals represent inline object types like `{ x: string; y: number }` or
//! callable types with call/construct signatures.

use crate::state::{CheckerState, ParamTypeResolutionMode};
use crate::symbol_resolver::TypeSymbolResolution;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::visitor::is_template_literal_type;

// =============================================================================
// Type Literal Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Node Resolution in Type Literals
    // =========================================================================

    /// Get type from a type node within a type literal context.
    ///
    /// This handles special resolution needed for types declared within
    /// type literals, such as recursive type references.
    pub(crate) fn get_type_from_type_node_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let factory = self.ctx.types.factory();

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self.get_type_from_type_reference_in_type_literal(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            // Check node_types cache first — resolve_type_queries_with_flow may have
            // pre-resolved this typeof with flow narrowing.
            if let Some(&cached) = self.ctx.node_types.get(&idx.0)
                && cached != TypeId::ERROR
            {
                return cached;
            }
            return self.get_type_from_type_query(idx);
        }
        if node.kind == syntax_kind_ext::UNION_TYPE {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                let members = composite
                    .types
                    .nodes
                    .iter()
                    .map(|&member_idx| self.get_type_from_type_node_in_type_literal(member_idx))
                    .collect::<Vec<_>>();
                return factory.union(members);
            }
            return TypeId::ERROR;
        }
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                let elem_type =
                    self.get_type_from_type_node_in_type_literal(array_type.element_type);
                return factory.array(elem_type);
            }
            return TypeId::ERROR; // Missing array type data - propagate error
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR {
            // Handle readonly and other type operators in type literals
            return self.get_type_from_type_operator(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.get_type_from_type_literal(idx);
        }

        self.get_type_from_type_node(idx)
    }

    fn get_type_from_type_reference_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let factory = self.ctx.types.factory();

        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type reference data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && name_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(sym_id) => sym_id,
                TypeSymbolResolution::ValueOnly(sym_id) => {
                    let name = self
                        .entity_name_text(type_name_idx)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    self.report_wrong_meaning(
                        &name,
                        type_name_idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    );
                    return TypeId::ERROR;
                }
                TypeSymbolResolution::NotFound => {
                    let _ = self.resolve_qualified_name(type_name_idx);
                    return TypeId::ERROR;
                }
            };
            // Stable-identity helper: resolve symbol body + create Lazy(DefId)
            let base_type = self.resolve_symbol_as_lazy_type(sym_id);
            if has_type_args {
                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return factory.application(base_type, type_args);
            }
            return base_type;
        }

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.as_str();

            // Type literal members inside namespaces should prefer same-namespace
            // type declarations before falling back to file/global symbols.
            if self.lookup_type_parameter(name).is_none()
                && let Some(sym_id) =
                    self.resolve_unqualified_name_in_enclosing_namespace(type_name_idx, name)
            {
                // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                let base_type = self.resolve_symbol_as_lazy_type(sym_id);
                if has_type_args {
                    let type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| {
                                    self.get_type_from_type_node_in_type_literal(arg_idx)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    return factory.application(base_type, type_args);
                }
                return base_type;
            }

            if has_type_args {
                // Handle compiler-intrinsic types that need special TypeData
                // variants instead of generic Application types.
                // NoInfer, Uppercase, etc. are intrinsic — their DefId has no body,
                // so Application(Lazy(DefId), args) can never be evaluated.
                if self.lookup_type_parameter(name).is_none() {
                    match name {
                        "NoInfer" => {
                            if let Some(args) = &type_ref.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let inner = self.get_type_from_type_node_in_type_literal(first_arg);
                                return self.ctx.types.no_infer(inner);
                            }
                            return TypeId::ERROR;
                        }
                        "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize" => {
                            if let Some(args) = &type_ref.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg =
                                    self.get_type_from_type_node_in_type_literal(first_arg);
                                return self.ctx.types.string_intrinsic_by_name(name, type_arg);
                            }
                            return TypeId::ERROR;
                        }
                        _ => {}
                    }
                }
                let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                let type_param = self.lookup_type_parameter(name);
                let type_resolution =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx);
                let sym_id = match type_resolution {
                    TypeSymbolResolution::Type(sym_id) => Some(sym_id),
                    TypeSymbolResolution::ValueOnly(sym_id) => {
                        self.report_wrong_meaning(
                            name,
                            type_name_idx,
                            sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => None,
                };

                if is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    // Array/ReadonlyArray not found - check if lib files are loaded
                    // When --noLib is used, emit TS2318 instead of silently creating Array type
                    if !self.ctx.has_lib_loaded() {
                        // No lib files loaded - emit TS2318 for missing global type
                        self.error_cannot_find_global_type(name, type_name_idx);
                        // Still process type arguments to avoid cascading errors
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        return TypeId::ERROR;
                    }
                    // Lib files are loaded but Array not found - fall back to creating Array type
                    let elem_type = type_ref
                        .type_arguments
                        .as_ref()
                        .and_then(|args| args.nodes.first().copied())
                        .map_or(TypeId::UNKNOWN, |idx| {
                            self.get_type_from_type_node_in_type_literal(idx)
                        });
                    let array_type = factory.array(elem_type);
                    if name == "ReadonlyArray" {
                        return factory.readonly_type(array_type);
                    }
                    return array_type;
                }

                if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    if self.is_known_global_type_name(name) {
                        // TS2318/TS2583: Emit error for missing global type
                        // Process type arguments for validation first
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        // Emit the appropriate error
                        self.error_cannot_find_global_type(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if name == "await" {
                        self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_name_idx) {
                        return TypeId::ANY;
                    }
                    // Route through boundary for TS2304/TS2552 with spelling suggestions
                    let _ = self.resolve_type_name_or_report(name, type_name_idx);
                    return TypeId::ERROR;
                }
                // For Array<T> / ReadonlyArray<T> with type arguments, convert to
                // proper array types (Array(T) / Readonly(Array(T))) instead of
                // Application(Lazy(DefId), [T]). This matches what TypeLowering does
                // and ensures assignability with `T[]` / `readonly T[]`.
                if is_builtin_array
                    && type_param.is_none()
                    && let Some(args) = &type_ref.type_arguments
                    && let Some(&first_arg) = args.nodes.first()
                {
                    let elem_type = self.get_type_from_type_node_in_type_literal(first_arg);
                    let array_type = factory.array(elem_type);
                    if name == "ReadonlyArray" {
                        return factory.readonly_type(array_type);
                    }
                    return array_type;
                }

                let base_type = if let Some(type_param) = type_param {
                    type_param
                } else if let Some(sym_id) = sym_id {
                    // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                    self.resolve_symbol_as_lazy_type(sym_id)
                } else {
                    TypeId::ERROR
                };

                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return factory.application(base_type, type_args);
            }

            if name == "Array" || name == "ReadonlyArray" {
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
                {
                    // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                    return self.resolve_symbol_as_lazy_type(sym_id);
                }
                if let Some(type_param) = self.lookup_type_parameter(name) {
                    return type_param;
                }
                // Array/ReadonlyArray not found - check if lib files are loaded
                // When --noLib is used, emit TS2318 instead of silently creating Array type
                if !self.ctx.has_lib_loaded() {
                    // No lib files loaded - emit TS2318 for missing global type
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                // Lib files are loaded but Array not found - fall back to creating Array type
                let elem_type = type_ref
                    .type_arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first().copied())
                    .map_or(TypeId::UNKNOWN, |idx| {
                        self.get_type_from_type_node_in_type_literal(idx)
                    });
                let array_type = factory.array(elem_type);
                if name == "ReadonlyArray" {
                    return factory.readonly_type(array_type);
                }
                return array_type;
            }

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

            if name != "Array"
                && let TypeSymbolResolution::ValueOnly(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
            {
                self.report_wrong_meaning(
                    name,
                    type_name_idx,
                    sym_id,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }

            if let Some(type_param) = self.lookup_type_parameter(name) {
                return type_param;
            }
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(type_name_idx)
            {
                // Prime lib generic metadata before resolving the symbol body so
                // bare lib references inside type literals keep their default
                // type arguments instead of caching an uninstantiated Lazy type.
                if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(sym_id) {
                    self.prime_lib_type_params(name);
                }
                // Resolve the symbol's structural body first
                let _ = self.type_reference_symbol_type(sym_id);
                // For generic types with all-default type parameters (e.g., Uint8Array<T = ArrayBufferLike>),
                // wrap in Application(Lazy(DefId), defaults) to match resolve_simple_type_reference behavior.
                // Without this, bare Lazy(DefId) misses the default instantiation and causes false
                // TS2322 when compared against an explicit Application (e.g., Uint8Array<ArrayBuffer>).
                let type_params = self.get_type_params_for_symbol(sym_id);
                if !type_params.is_empty() && type_params.iter().all(|p| p.default.is_some()) {
                    let default_args: Vec<TypeId> =
                        tsz_solver::resolve_default_type_args(self.ctx.types, &type_params);
                    let def_id = self
                        .ctx
                        .get_or_create_def_id_with_params(sym_id, type_params);
                    let base_type_id = factory.lazy(def_id);
                    return factory.application(base_type_id, default_args);
                }
                // Stable-identity: create Lazy(DefId) (body already resolved above)
                return self.ctx.create_lazy_type_ref(sym_id);
            }

            if name == "await" {
                self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                return TypeId::ERROR;
            }
            if self.is_known_global_type_name(name) {
                // TS2318/TS2583: Emit error for missing global type
                self.error_cannot_find_global_type(name, type_name_idx);
                return TypeId::ERROR;
            }
            // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
            if self.is_unresolved_import_symbol(type_name_idx) {
                return TypeId::ANY;
            }
            // Route through boundary for TS2304/TS2552 with spelling suggestions
            let _ = self.resolve_type_name_or_report(name, type_name_idx);
            return TypeId::ERROR;
        }

        TypeId::ANY
    }

    // =========================================================================
    // Parameter Extraction
    // =========================================================================

    pub(crate) fn extract_params_from_signature_in_type_literal(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::InTypeLiteral,
        )
    }

    fn enclosing_type_literal_owner_name(&self, idx: NodeIndex) -> Option<String> {
        let mut current = idx;
        let mut depth = 0usize;
        while depth < 64 {
            depth += 1;
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    let alias = self.ctx.arena.get_type_alias(node)?;
                    let ident = self.ctx.arena.get_identifier_at(alias.name)?;
                    return Some(ident.escaped_text.clone());
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    let ident = self.ctx.arena.get_identifier_at(decl.name)?;
                    return Some(ident.escaped_text.clone());
                }
                _ => {}
            }
        }
        None
    }

    fn type_literal_accessor_circular_reference(
        &self,
        type_node_idx: NodeIndex,
        accessor_name_idx: NodeIndex,
        owner_name: &str,
    ) -> bool {
        let Some(accessor_name) = self.get_property_name(accessor_name_idx) else {
            return false;
        };
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };

        if type_node.kind == syntax_kind_ext::TYPE_QUERY {
            let Some(query) = self.ctx.arena.get_type_query(type_node) else {
                return false;
            };
            let Some(expr_node) = self.ctx.arena.get(query.expr_name) else {
                return false;
            };

            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let object_name = self
                    .ctx
                    .arena
                    .get_identifier_at(access.expression)
                    .map(|ident| ident.escaped_text.as_str());
                let property_name = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.as_str());
                return object_name == Some(owner_name)
                    && property_name == Some(accessor_name.as_str());
            }

            if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) else {
                    return false;
                };
                let object_name = self
                    .ctx
                    .arena
                    .get_identifier_at(qn.left)
                    .map(|ident| ident.escaped_text.as_str());
                let property_name = self
                    .ctx
                    .arena
                    .get_identifier_at(qn.right)
                    .map(|ident| ident.escaped_text.as_str());
                return object_name == Some(owner_name)
                    && property_name == Some(accessor_name.as_str());
            }
        }

        if type_node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            let Some(indexed) = self.ctx.arena.get_indexed_access_type(type_node) else {
                return false;
            };
            let Some(object_type_node) = self.ctx.arena.get(indexed.object_type) else {
                return false;
            };
            if object_type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
            let Some(type_ref) = self.ctx.arena.get_type_ref(object_type_node) else {
                return false;
            };
            let object_name = self
                .ctx
                .arena
                .get_identifier_at(type_ref.type_name)
                .map(|ident| ident.escaped_text.as_str());
            if object_name != Some(owner_name) {
                return false;
            }

            let Some(index_node) = self.ctx.arena.get(indexed.index_type) else {
                return false;
            };
            if let Some(lit) = self.ctx.arena.get_literal(index_node) {
                return lit.text == accessor_name;
            }
            if let Some(lit_type) = self.ctx.arena.get_literal_type(index_node)
                && let Some(inner) = self.ctx.arena.get(lit_type.literal)
                && let Some(lit) = self.ctx.arena.get_literal(inner)
            {
                return lit.text == accessor_name;
            }
        }

        false
    }

    pub(crate) fn type_literal_has_circular_accessor_reference(
        &self,
        type_node_idx: NodeIndex,
    ) -> bool {
        struct AccessorMemberInfo {
            circular_self_reference: bool,
        }

        #[derive(Default)]
        struct AccessorAggregate {
            getter: Option<AccessorMemberInfo>,
            setter: Option<AccessorMemberInfo>,
        }

        let Some(owner_name) = self.enclosing_type_literal_owner_name(type_node_idx) else {
            return false;
        };
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
            return false;
        };

        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();

        for &member_idx in &type_lit.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if (member.kind != syntax_kind_ext::GET_ACCESSOR
                && member.kind != syntax_kind_ext::SET_ACCESSOR)
                || self.ctx.arena.get_accessor(member).is_none()
            {
                continue;
            }
            let Some(accessor) = self.ctx.arena.get_accessor(member) else {
                continue;
            };
            let Some(name) = self.get_property_name(accessor.name) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name);
            let entry = accessors.entry(name_atom).or_default();

            if member.kind == syntax_kind_ext::GET_ACCESSOR {
                let circular_self_reference = accessor.type_annotation.is_some()
                    && self.type_literal_accessor_circular_reference(
                        accessor.type_annotation,
                        accessor.name,
                        &owner_name,
                    );
                entry.getter = Some(AccessorMemberInfo {
                    circular_self_reference,
                });
            } else {
                let mut circular_self_reference = false;
                if let Some(&param_idx) = accessor.parameters.nodes.first()
                    && let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    circular_self_reference = param.type_annotation.is_some()
                        && self.type_literal_accessor_circular_reference(
                            param.type_annotation,
                            accessor.name,
                            &owner_name,
                        );
                }
                entry.setter = Some(AccessorMemberInfo {
                    circular_self_reference,
                });
            }
        }

        accessors.values().any(|accessor| {
            accessor
                .getter
                .as_ref()
                .is_some_and(|getter| getter.circular_self_reference)
                || accessor
                    .setter
                    .as_ref()
                    .is_some_and(|setter| setter.circular_self_reference)
        })
    }

    // =========================================================================
    // Type Literal Resolution
    // =========================================================================

    /// Get type from a type literal node (anonymous object types).
    ///
    /// Type literals represent inline object types like `{ x: string; y: number }` or
    /// callable types with call/construct signatures. This function parses the type
    /// literal and creates the appropriate type representation.
    ///
    /// ## Type Literal Members:
    /// - **Property Signatures**: Named properties with types (`{ x: string }`)
    /// - **Method Signatures**: Function-typed methods (`{ method(): void }`)
    /// - **Call Signatures**: Callable objects (`{ (): string }`)
    /// - **Construct Signatures**: Constructor functions (`{ new(): T }`)
    /// - **Index Signatures**: Dynamic property access (`{ [key: string]: T }`)
    ///
    /// ## Modifiers:
    /// - `?`: Optional property (can be undefined)
    /// - `readonly`: Read-only property (cannot be assigned to)
    ///
    /// ## Type Resolution:
    /// - Property types are resolved via `get_type_from_type_node_in_type_literal`
    /// - Type parameters are pushed/popped for each member
    /// - Index signatures are tracked by key type (string or number)
    ///
    /// ## Result Type:
    /// - **Callable**: If has call/construct signatures
    /// - **`ObjectWithIndex`**: If has index signatures
    /// - **Object**: Plain object type otherwise
    pub(crate) fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, PropertyInfo,
        };
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR; // Missing type literal data - propagate error
        };
        let owner_name = self.enclosing_type_literal_owner_name(idx);

        struct AccessorMemberInfo {
            name_idx: NodeIndex,
            type_annotation: NodeIndex,
            resolved_type: TypeId,
            circular_self_reference: bool,
        }

        struct AccessorAggregate {
            getter: Option<AccessorMemberInfo>,
            setter: Option<AccessorMemberInfo>,
            declaration_order: u32,
        }

        let mut properties = Vec::new();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;
        let mut has_abstract_construct_sig = false;
        // Global member counter for preserving source declaration order across
        // both properties and methods. Using properties.len() would give methods
        // higher declaration_order than all properties since methods are merged
        // after the loop, breaking tsc's interleaved display order.
        let mut member_order: u32 = 0;
        // Collect method signatures grouped by name to support overloaded methods.
        // Each entry maps method name -> list of (CallSignature, optional, readonly).
        let mut method_overloads: FxHashMap<Atom, Vec<(CallSignature, bool, bool)>> =
            FxHashMap::default();
        // Track insertion order and declaration_order for method overloads.
        let mut method_overload_order: Vec<(Atom, u32)> = Vec::new();

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(sig) = self.ctx.arena.get_signature(member) {
                match member.kind {
                    CALL_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            );
                        call_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        if self.has_abstract_modifier(&sig.modifiers) {
                            has_abstract_construct_sig = true;
                        }
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            );
                        construct_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name_resolved(sig.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);

                        if member.kind == METHOD_SIGNATURE {
                            if let Some(ref _params) = sig.parameters {}
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, this_type) =
                                self.extract_params_from_signature_in_type_literal(sig);
                            let (return_type, type_predicate) = self
                                .return_type_and_predicate_in_type_literal(
                                    sig.type_annotation,
                                    &params,
                                );
                            let call_sig = CallSignature {
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate,
                                is_method: true,
                            };
                            self.pop_type_parameters(type_param_updates);
                            let optional = sig.question_token;
                            let readonly = self.has_readonly_modifier(&sig.modifiers);
                            let entry = method_overloads.entry(name_atom).or_default();
                            if entry.is_empty() {
                                member_order += 1;
                                method_overload_order.push((name_atom, member_order));
                            }
                            entry.push((call_sig, optional, readonly));
                        } else {
                            let type_id = if sig.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            member_order += 1;
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: member_order,
                                is_string_named: false,
                            });
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if param_data.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };

                // TS1337 / TS1268: Validate index signature parameter type.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                if !has_param_grammar_error && param_data.type_annotation.is_some() {
                    // Check the AST node kind first: type parameters and literal types
                    // trigger TS1337 regardless of what their constraint resolves to
                    // (e.g. `T extends string` resolves to STRING but is still invalid).
                    let type_ann_kind = self
                        .ctx
                        .arena
                        .get(param_data.type_annotation)
                        .map_or(0, |n| n.kind);
                    let is_generic_or_literal = self.is_type_param_or_literal_in_index_sig(
                        type_ann_kind,
                        param_data.type_annotation,
                    );
                    if is_generic_or_literal {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                        );
                    } else {
                        let is_valid_index_type = key_type == TypeId::STRING
                            || key_type == TypeId::NUMBER
                            || key_type == TypeId::SYMBOL
                            || is_template_literal_type(self.ctx.types, key_type);
                        if !is_valid_index_type {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.error_at_node(
                                param_idx,
                                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            );
                        }
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let param_name = self
                    .ctx
                    .arena
                    .get(param_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                    param_name,
                };
                if key_type == TypeId::NUMBER {
                    number_index = Some(info);
                } else {
                    string_index = Some(info);
                }
                continue;
            }

            // Handle accessor declarations (get/set) in type literals
            if (member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || member.kind == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && let Some(name) = self.get_property_name(accessor.name)
            {
                let name_atom = self.ctx.types.intern_string(&name);
                let is_new_accessor = !accessors.contains_key(&name_atom);
                if is_new_accessor {
                    member_order += 1;
                }
                let current_order = member_order;
                let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                    getter: None,
                    setter: None,
                    declaration_order: current_order,
                });

                if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
                    let circular_self_reference = accessor.type_annotation.is_some()
                        && owner_name.as_deref().is_some_and(|owner_name| {
                            self.type_literal_accessor_circular_reference(
                                accessor.type_annotation,
                                accessor.name,
                                owner_name,
                            )
                        });
                    let resolved_type =
                        if accessor.type_annotation.is_some() && !circular_self_reference {
                            self.get_type_from_type_node_in_type_literal(accessor.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                    entry.getter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation: accessor.type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                } else {
                    let mut type_annotation = NodeIndex::NONE;
                    let mut circular_self_reference = false;
                    let mut resolved_type = TypeId::UNKNOWN;
                    if let Some(&param_idx) = accessor.parameters.nodes.first()
                        && let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        type_annotation = param.type_annotation;
                        circular_self_reference = param.type_annotation.is_some()
                            && owner_name.as_deref().is_some_and(|owner_name| {
                                self.type_literal_accessor_circular_reference(
                                    param.type_annotation,
                                    accessor.name,
                                    owner_name,
                                )
                            });
                        if param.type_annotation.is_some() && !circular_self_reference {
                            resolved_type =
                                self.get_type_from_type_node_in_type_literal(param.type_annotation);
                        }
                    }
                    entry.setter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                }
            }
        }

        // Convert accessors to properties (getter-only implies readonly)
        for (name, accessor) in accessors {
            let getter_requires_ts2502 = accessor.getter.as_ref().is_some_and(|getter| {
                getter.circular_self_reference
                    && accessor.setter.as_ref().is_none_or(|setter| {
                        setter.type_annotation.is_none() || setter.circular_self_reference
                    })
            });
            let setter_requires_ts2502 = accessor.setter.as_ref().is_some_and(|setter| {
                setter.circular_self_reference
                    && accessor.getter.as_ref().is_none_or(|getter| {
                        getter.type_annotation.is_none() || getter.circular_self_reference
                    })
            });

            let getter_type = accessor.getter.as_ref().map(|getter| {
                if getter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(getter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if getter.circular_self_reference {
                    accessor
                        .setter
                        .as_ref()
                        .map_or(TypeId::ANY, |setter| setter.resolved_type)
                } else {
                    getter.resolved_type
                }
            });
            let setter_type = accessor.setter.as_ref().map(|setter| {
                if setter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(setter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if setter.circular_self_reference {
                    accessor
                        .getter
                        .as_ref()
                        .map_or(TypeId::UNKNOWN, |getter| getter.resolved_type)
                } else {
                    setter.resolved_type
                }
            });

            let read_type = getter_type.or(setter_type).unwrap_or(TypeId::UNKNOWN);
            let write_type = setter_type.or(getter_type).unwrap_or(read_type);
            let readonly = getter_type.is_some() && setter_type.is_none();
            properties.push(PropertyInfo {
                name,
                type_id: read_type,
                write_type,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: accessor.declaration_order,
                is_string_named: false,
            });
        }

        // Merge overloaded method signatures into properties.
        // Single-signature methods become Function types; multi-signature become Callable types.
        for (name_atom, decl_order) in method_overload_order {
            if let Some(sigs) = method_overloads.remove(&name_atom) {
                let optional = sigs.iter().all(|(_, opt, _)| *opt);
                let readonly = sigs.iter().any(|(_, _, ro)| *ro);
                let method_type = if sigs.len() == 1 {
                    let (sig, _, _) = sigs
                        .into_iter()
                        .next()
                        .expect("sigs.len() == 1 guard ensures at least one element");
                    factory.function(FunctionShape {
                        type_params: sig.type_params,
                        params: sig.params,
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: true,
                    })
                } else {
                    let merged_sigs: Vec<CallSignature> =
                        sigs.into_iter().map(|(sig, _, _)| sig).collect();
                    factory.callable(CallableShape {
                        call_signatures: merged_sigs,
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                        symbol: None,
                        is_abstract: false,
                    })
                };
                properties.push(PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional,
                    readonly,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: decl_order,
                    is_string_named: false,
                });
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            return factory.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: None,
                is_abstract: has_abstract_construct_sig,
            });
        }

        if string_index.is_some() || number_index.is_some() {
            return factory.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
                ..ObjectShape::default()
            });
        }

        factory.object(properties)
    }
}
