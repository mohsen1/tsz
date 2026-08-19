//! Interface type resolution (heritage merging, structural merge).
//! - `merge_properties` - Merge derived and base interface properties
//!
//! # Responsibilities
//!
//! - Interface type construction (call signatures, construct signatures, properties)
//! - Heritage clause processing (extends)
//! - Base interface/class/alias type merging
//! - Index signature handling
//! - Type parameter instantiation for generic bases

use crate::state::CheckerState;
use crate::types_domain::type_node_helpers::type_node_includes_explicit_undefined;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::IndexSignature;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

// =============================================================================
// Interface Type Resolution
// =============================================================================

/// Maximum nesting for `merge_interface_heritage_types_inner`'s
/// `heritage_merge_depth` counter (#16308). Mirrors `LIB_HERITAGE_MERGE_MAX_DEPTH`
/// in `lib_resolution_heritage.rs`: real chains stay far under this; it is
/// pure backstop against a pathologically deep distinct-name chain, since
/// genuine cycles are already caught by `check_interface_inheritance_cycle`
/// (TS2310) and OS-stack risk is bounded separately by `with_stack_guard`.
const INTERFACE_HERITAGE_MERGE_MAX_DEPTH: u32 = 50;

/// Debug kill-switch for #14101 part-4 (heritage base-member incorporation).
///
/// When a heritage base classifies as `Other` but still has an extractable
/// object shape (a constrained type-parameter seen through its constraint, or a
/// function-with-properties), its members were dropped at the `_ => derived`
/// fallthrough of `merge_interface_types_impl`, causing false member-drop
/// `TS2339`/`TS2344`. Incorporating them changes a diagnostic, so it is gated:
/// set `TSZ_DISABLE_HERITAGE_BASE_MEMBER_INCORP=1` to restore the legacy drop
/// for byte-parity bisection / conformance A/B; defaults to enabled.
fn heritage_base_member_incorp_disabled() -> bool {
    use std::sync::OnceLock;
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| {
        std::env::var("TSZ_DISABLE_HERITAGE_BASE_MEMBER_INCORP")
            .is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// `TSZ_XARENA_HERITAGE_TYPEARG=1` recovers an empty heritage base parameter
/// list arena-directly when a generic base is re-resolved through a secondary
/// (importing) arena (issue #14345). Default-OFF, so flag-OFF is byte-parity
/// with the historical behavior (empty `base_type_params` truncates the
/// supplied heritage arguments away and the substitution is a no-op, leaving
/// inherited members bound to the base's free type parameter -- the `T`-leak).
///
/// When ON, and only when the broken shape is present (heritage arguments
/// supplied but `get_type_params_for_symbol` returned no parameters), the base
/// declaration's parameter names are read directly from its home arena so the
/// arguments bind structurally (`T -> A`, `tsc` parity). Operates on the
/// resolved base symbol's declarations + arenas; no name/file string checks.
fn xarena_heritage_typearg_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_XARENA_HERITAGE_TYPEARG").is_ok_and(|v| v == "1"))
}

/// Deduplicate call signatures keeping the LAST occurrence of each unique
/// signature. Two signatures are considered duplicates when they have identical
/// parameter type lists and return types. This handles diamond inheritance:
/// when `C extends C1, C2` and both C1/C2 inherit from B, shared signatures
/// from B appear twice. By keeping the last occurrence, shared base signatures
/// (like a catch-all `(x: string): void`) sort after all derived-specific
/// overloads, ensuring correct overload resolution order.
fn dedup_call_signatures_keep_last(sigs: &mut Vec<tsz_solver::CallSignature>) {
    if sigs.len() <= 1 {
        return;
    }
    // Build a signature key from param types + return type for identity.
    // Walk from the end and record the last index for each unique key.
    // Then retain only those positions.
    type SignatureKey = (SmallVec<[TypeId; 4]>, TypeId);

    let key_of = |sig: &tsz_solver::CallSignature| -> SignatureKey {
        let param_types = sig.params.iter().map(|p| p.type_id).collect();
        (param_types, sig.return_type)
    };

    let mut seen: FxHashMap<SignatureKey, usize> = FxHashMap::default();
    // Record the LAST index for each key
    for (i, sig) in sigs.iter().enumerate() {
        seen.insert(key_of(sig), i);
    }
    // Retain only signatures whose index matches their last occurrence
    let mut i = 0;
    sigs.retain(|sig| {
        let idx = i;
        i += 1;
        seen.get(&key_of(sig)).copied() == Some(idx)
    });
}

/// How `merge_interface_types` combines a named member that exists on both
/// the "derived" and "base" side of a structural merge.
///
/// TypeScript distinguishes two structural-merge situations that share the
/// same code path here:
///
/// - **Heritage** (`interface Derived extends Base`): a derived member with the
///   same name as a base member *overrides* (entirely replaces) it. The base
///   signature does not survive into an overload set. This is true even when
///   the derived member is itself an overload set — only the derived
///   signatures remain (`d.m(...)` resolves against the derived member alone).
///   Anonymous call/construct signatures still accumulate because they have no
///   name to override by.
/// - **Declaration** (two `interface Foo {}` declarations merged into one
///   symbol, module augmentation, lib declaration merging): same-named method
///   signatures *accumulate* into a single overload set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InterfaceMergeMode {
    /// `extends` heritage — derived members override base members by name.
    Heritage,
    /// Declaration/augmentation merge — same-named methods accumulate overloads.
    Declaration,
    /// Declaration merge across program files (global script-interface
    /// re-opens). Same accumulation semantics as [`Self::Declaration`], with
    /// one addition: the base side is a *later* program file's declaration
    /// group, so its accumulated signatures get `declaration_group` stamps
    /// offset above the derived side's. Storage keeps forward declaration
    /// order (earlier file first, tsc display order) while
    /// `reorder_overload_candidates` tries the later group first at call
    /// resolution (tsc `reorderCandidates`). Callers must pass the
    /// earlier-file type as `derived` and the later-file type as `base`.
    CrossFileDeclaration,
}

/// Merges an additional string-keyed index signature into an existing one by
/// unioning their key patterns, enabling excess-property checking to accept any
/// key that matches ANY of the declared template-literal patterns.
pub(crate) fn merge_string_index_by_union(
    existing: &mut IndexSignature,
    extra: IndexSignature,
    factory: tsz_solver::construction::TypeFactory<'_>,
) {
    if existing.key_type != extra.key_type {
        existing.key_type = factory.union2(existing.key_type, extra.key_type);
    }
    if existing.value_type != extra.value_type {
        existing.value_type = factory.union2(existing.value_type, extra.value_type);
    }
    existing.readonly &= extra.readonly;
}

impl<'a> CheckerState<'a> {
    fn resolve_interface_heritage_symbol_by_name(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .file_locals
            .get(normalized)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(normalized, &lib_binders)
            })
            .or_else(|| {
                normalized
                    .rsplit('.')
                    .next()
                    .filter(|tail| *tail != normalized)
                    .and_then(|tail| {
                        self.ctx.binder.file_locals.get(tail).or_else(|| {
                            self.ctx
                                .binder
                                .get_global_type_with_libs(tail, &lib_binders)
                        })
                    })
            })
            // Cross-file lookup binders do not fold lib globals into
            // `file_locals`; without this explicit fallback, a lib heritage
            // base (`extends Request`) silently fails to resolve when the
            // declaring file is checked through cross-arena delegation,
            // making results depend on root-file order.
            .or_else(|| self.ctx.binder.program_global_type(normalized))
    }

    /// Get the type of an interface declaration.
    ///
    /// This function collects the interface members (call/construct signatures,
    /// properties, index signatures), processes `extends` heritage, and merges bases.
    ///
    /// # Arguments
    /// * `idx` - The `NodeIndex` of the interface declaration
    ///
    /// # Returns
    /// The `TypeId` representing the interface type
    ///
    /// # Example
    /// ```typescript
    /// interface Window {
    ///     title: string;
    /// }
    ///
    /// interface Window {
    ///     alert(message: string): void;
    /// }
    ///
    /// // Window type has both title and alert
    /// ```
    pub(crate) fn get_type_of_interface(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature as SolverCallSignature, CallableShape, IndexSignature, ObjectShape,
            PropertyInfo,
        };
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(interface) = self.ctx.arena.get_interface(node) else {
            return TypeId::ERROR; // Missing interface data - propagate error
        };
        let interface_symbol = self.ctx.binder.get_node_symbol(idx);

        let own_type_param_names: FxHashSet<String> = interface
            .type_parameters
            .as_ref()
            .map(|params| {
                params
                    .nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        self.ctx
                            .arena
                            .get(param_idx)
                            .and_then(|param_node| self.ctx.arena.get_type_parameter(param_node))
                            .and_then(|param| {
                                self.ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| ident.escaped_text.to_string())
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut hidden_merged_type_params = Vec::new();
        if let Some(sym_id) = interface_symbol {
            for param in self.get_type_params_for_symbol(sym_id) {
                let name = self.ctx.types.resolve_atom_ref(param.name).to_string();
                if !own_type_param_names.contains(&name)
                    && let Some(previous) = self.ctx.type_parameter_scope.remove(&name)
                {
                    hidden_merged_type_params.push((name, previous));
                }
            }
        }

        let (_interface_type_params, interface_type_param_updates) =
            self.push_type_parameters(&interface.type_parameters);

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
            declaration_order: u32,
            is_symbol_named: bool,
            is_string_named: bool,
            single_quoted_name: bool,
        }

        let mut call_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut construct_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut symbol_index: Option<IndexSignature> = None;
        let mut member_order: u32 = 0;

        // Track method overloads: group call signatures by method name.
        // When an interface has multiple method signatures with the same name
        // (overloads), we need to combine them into a single Callable type
        // so that overload resolution works correctly.
        struct MethodOverloadEntry {
            signatures: Vec<SolverCallSignature>,
            optional: bool,
            readonly: bool,
            declaration_order: u32,
            is_symbol_named: bool,
            is_string_named: bool,
            single_quoted_name: bool,
        }
        let mut method_overloads: Vec<(Atom, MethodOverloadEntry)> = Vec::new();

        // Iterate over this interface's own members
        for &member_idx in &interface.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == CALL_SIGNATURE {
                // Extract call signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    if let Some(ref _params) = sig.parameters {}
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) =
                        self.extract_params_from_signature_in_type_literal(sig);
                    self.push_typeof_param_scope(&params);
                    let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE);
                        if is_predicate {
                            self.return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                                crate::signature_builder::signature_param_nodes(&sig.parameters),
                            )
                        } else {
                            (
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation),
                                None,
                            )
                        }
                    } else {
                        // Return ANY to match TypeScript's implicit 'any' return type
                        (TypeId::ANY, None)
                    };
                    self.pop_typeof_param_scope(&params);

                    call_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                        is_method: false,
                        declaration_group: 0,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == CONSTRUCT_SIGNATURE {
                // Extract construct signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    if let Some(ref _params) = sig.parameters {}
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) =
                        self.extract_params_from_signature_in_type_literal(sig);
                    self.push_typeof_param_scope(&params);
                    let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE);
                        if is_predicate {
                            self.return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                                crate::signature_builder::signature_param_nodes(&sig.parameters),
                            )
                        } else {
                            (
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation),
                                None,
                            )
                        }
                    } else {
                        // Return ANY to match TypeScript's implicit 'any' return type
                        (TypeId::ANY, None)
                    };
                    self.pop_typeof_param_scope(&params);

                    construct_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                        is_method: false,
                        declaration_group: 0,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == PROPERTY_SIGNATURE {
                // Extract property signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let name_atom = self.get_member_name_atom(sig.name);
                    if let Some(name_atom) = name_atom {
                        let is_symbol_named = self.is_symbol_property_name(sig.name);
                        let (is_string_named, single_quoted_name) =
                            self.ctx.arena.string_property_name_flags(sig.name);
                        let type_id = if sig.type_annotation.is_some() {
                            self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        let write_type = if self.ctx.compiler_options.exact_optional_property_types
                            && sig.question_token
                            && sig.type_annotation.is_some()
                            && !type_node_includes_explicit_undefined(
                                self.ctx.arena,
                                sig.type_annotation,
                            ) {
                            crate::query_boundaries::common::remove_undefined(
                                self.ctx.types.as_type_database(),
                                type_id,
                            )
                        } else {
                            type_id
                        };

                        member_order += 1;
                        properties.push(PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type,
                            optional: sig.question_token,
                            readonly: self.has_readonly_modifier(&sig.modifiers),
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: member_order,
                            is_string_named,
                            is_symbol_named,
                            single_quoted_name,
                            non_widening: false,
                        });
                    }
                }
            } else if member_node.kind == METHOD_SIGNATURE {
                // Extract method signature as a full call signature.
                // Method overloads (multiple signatures with the same name) are
                // collected and later combined into a single Callable type so
                // that overload resolution works correctly (e.g., Object.freeze).
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let name_atom = self.get_member_name_atom(sig.name);
                    if let Some(name_atom) = name_atom {
                        let is_symbol_named = self.is_symbol_property_name(sig.name);
                        let (is_string_named, single_quoted_name) =
                            self.ctx.arena.string_property_name_flags(sig.name);
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        self.push_typeof_param_scope(&params);
                        let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                            let is_predicate =
                                self.ctx.arena.get(sig.type_annotation).is_some_and(|node| {
                                    node.kind == syntax_kind_ext::TYPE_PREDICATE
                                });
                            if is_predicate {
                                self.return_type_and_predicate_in_type_literal(
                                    sig.type_annotation,
                                    &params,
                                    crate::signature_builder::signature_param_nodes(
                                        &sig.parameters,
                                    ),
                                )
                            } else {
                                (
                                    self.get_type_from_type_node_in_type_literal(
                                        sig.type_annotation,
                                    ),
                                    None,
                                )
                            }
                        } else {
                            (TypeId::ANY, None)
                        };
                        self.pop_typeof_param_scope(&params);
                        self.pop_type_parameters(type_param_updates);

                        let call_sig = SolverCallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: true,
                            declaration_group: 0,
                        };

                        member_order += 1;
                        let optional = sig.question_token;
                        let readonly = self.has_readonly_modifier(&sig.modifiers);

                        // Add to overload group or create new group
                        if let Some(entry) = method_overloads
                            .iter_mut()
                            .find(|(name, _)| *name == name_atom)
                        {
                            entry.1.signatures.push(call_sig);
                        } else {
                            method_overloads.push((
                                name_atom,
                                MethodOverloadEntry {
                                    signatures: vec![call_sig],
                                    optional,
                                    readonly,
                                    declaration_order: member_order,
                                    is_symbol_named,
                                    is_string_named,
                                    single_quoted_name,
                                },
                            ));
                        }
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                    let name_atom = self.get_member_name_atom(accessor.name);
                    if let Some(name_atom) = name_atom {
                        let is_symbol_named = self.is_symbol_property_name(accessor.name);
                        let (is_string_named, single_quoted_name) =
                            self.ctx.arena.string_property_name_flags(accessor.name);
                        member_order += 1;
                        let current_order = member_order;
                        let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            declaration_order: current_order,
                            is_symbol_named,
                            is_string_named,
                            single_quoted_name,
                        });

                        if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                            let getter_type = if accessor.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(
                                    accessor.type_annotation,
                                )
                            } else {
                                TypeId::ANY
                            };
                            entry.getter = Some(getter_type);
                        } else {
                            let setter_type = accessor
                                .parameters
                                .nodes
                                .first()
                                .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                                .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                                .and_then(|param| {
                                    (param.type_annotation.is_some()).then(|| {
                                        self.get_type_from_type_node_in_type_literal(
                                            param.type_annotation,
                                        )
                                    })
                                })
                                .unwrap_or(TypeId::UNKNOWN);
                            entry.setter = Some(setter_type);
                        }
                    }
                }
            } else if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
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
                    TypeId::ANY
                };

                // TS1268/TS1337: Check index signature parameter type validity.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let (is_generic_or_literal, is_valid_index_type) =
                    self.classify_index_sig_param_type(key_type, param_data.type_annotation);
                if !is_valid_index_type && !has_param_grammar_error {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    if is_generic_or_literal {
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                        );
                    } else {
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
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
                if is_valid_index_type {
                    if key_type == TypeId::NUMBER {
                        Self::merge_index_signature(&mut number_index, info);
                    } else if key_type == TypeId::SYMBOL {
                        Self::merge_index_signature(&mut symbol_index, info);
                    } else {
                        match string_index.as_mut() {
                            None => string_index = Some(info),
                            Some(existing) => {
                                merge_string_index_by_union(existing, info, factory);
                            }
                        }
                    }
                }
            }
        }

        // Convert method overloads to properly-typed properties.
        // Single-method entries become Function types; multiple-signature entries
        // become Callable types with explicit overloads so the solver can perform
        // overload resolution (e.g., Object.freeze's specific literal-preserving
        // overload is tried before the generic fallback).
        for (name, entry) in method_overloads {
            let type_id = if entry.signatures.len() == 1 {
                // Single method: create a Function type
                let sig = entry
                    .signatures
                    .into_iter()
                    .next()
                    .expect("single signature confirmed by len check");
                factory.function(tsz_solver::FunctionShape {
                    type_params: sig.type_params,
                    params: sig.params,
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_predicate: sig.type_predicate,
                    is_constructor: false,
                    is_method: true,
                })
            } else {
                // Multiple overloads: create a Callable type with all signatures
                let shape = CallableShape {
                    call_signatures: entry.signatures,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                };
                factory.callable(shape)
            };
            properties.push(PropertyInfo {
                name,
                type_id,
                write_type: type_id,
                optional: entry.optional,
                readonly: entry.readonly,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: entry.declaration_order,
                is_string_named: entry.is_string_named,
                is_symbol_named: entry.is_symbol_named,
                single_quoted_name: entry.single_quoted_name,
                non_widening: false,
            });
        }

        // Convert accessors to properties
        for (name, accessor) in accessors {
            let read_type = accessor
                .getter
                .or(accessor.setter)
                .unwrap_or(TypeId::UNKNOWN);
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter out so we fall back to getter type, matching tsc.
            let write_type = accessor
                .setter
                .filter(|&t| t != TypeId::UNKNOWN)
                .or(accessor.getter)
                .unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
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
                is_string_named: accessor.is_string_named,
                is_symbol_named: accessor.is_symbol_named,
                single_quoted_name: accessor.single_quoted_name,
                non_widening: false,
            });
        }

        let result = if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            // `CallableShape` keeps the single-slot index convention: a `symbol`
            // index rides in `string_index` (its `key_type` discriminates it).
            let shape = CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index: string_index.or(symbol_index),
                number_index,
                symbol: interface_symbol,
                is_abstract: false,
            };
            factory.callable(shape)
        } else if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
            factory.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
                symbol_index,
                symbol: interface_symbol,
                ..ObjectShape::default()
            })
        } else if !properties.is_empty() {
            factory.object_with_symbol(properties, interface_symbol)
        } else {
            TypeId::ANY
        };

        self.pop_type_parameters(interface_type_param_updates);
        for (name, type_id) in hidden_merged_type_params {
            self.ctx.type_parameter_scope.insert(name, type_id);
        }
        self.merge_interface_heritage_types(std::slice::from_ref(&idx), result)
    }

    /// Merge interface heritage types (extends clauses).
    ///
    /// This function processes the heritage clauses of interface declarations
    /// and merges the base interface/class/alias types into the derived type.
    ///
    /// # Arguments
    /// * `declarations` - The interface declarations to process
    /// * `derived_type` - The initial derived type
    ///
    /// # Returns
    /// The merged `TypeId` including all base interface members
    pub(crate) fn merge_interface_heritage_types(
        &mut self,
        declarations: &[NodeIndex],
        derived_type: TypeId,
    ) -> TypeId {
        // Cross-context OS-stack breaker (#14111). The heritage-merge cycle
        // `merge → get_type_of_symbol(base) → compute_type_of_symbol → merge`
        // hops fresh / cross-arena child `CheckerContext`s that reset the
        // per-context `heritage_merge_depth` `Cell` (and `enter_recursion`
        // counter) to zero, so neither logical guard can bound the real OS
        // call stack across those boundaries — a declaration-merged /
        // augmented-module interface graph (NestJS-style backends such as
        // directus/api) recurses until the thread stack aborts. The
        // thread-local breaker survives context boundaries and is the only
        // mechanism that does. Bail to the partially-merged `derived_type`,
        // matching the logical `heritage_merge_depth` bail below.
        crate::checkers_domain::with_stack_guard(derived_type, || {
            self.merge_interface_heritage_types_inner(declarations, derived_type)
        })
    }

    fn merge_interface_heritage_types_inner(
        &mut self,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use tracing::trace;

        trace!(decls = declarations.len(), derived_type_id = %derived_type.0, "merge_interface_heritage_types called");

        // Depth guard: heritage merging can trigger get_type_of_symbol on base
        // interfaces, which in turn calls compute_type_of_symbol →
        // merge_interface_heritage_types again for cross-referencing interfaces.
        //
        // #16308: a legitimate, non-cyclic same-file six-level chain (real
        // shape: mobx's `IObservableArray<T> extends Array<T>`, reached
        // through several layers of the project's own interfaces) used to
        // hit the old limit of 5 on its sixth nested call and silently drop
        // the `Array<T>` base with no "incomplete" signal — so the truncated
        // type then got cached as final by every caller. See
        // `INTERFACE_HERITAGE_MERGE_MAX_DEPTH`'s doc comment for why raising
        // this bound is safe (cycles and OS-stack risk are caught elsewhere).
        let heritage_depth = self.ctx.heritage_merge_depth.get();
        if heritage_depth >= INTERFACE_HERITAGE_MERGE_MAX_DEPTH {
            return derived_type;
        }
        // Bail out early if type resolution fuel is exhausted.
        if !self.ctx.consume_fuel() {
            return derived_type;
        }
        self.ctx.heritage_merge_depth.set(heritage_depth + 1);

        let mut pushed_derived = false;
        let mut derived_param_updates = Vec::new();
        let current_sym = declarations
            .first()
            .and_then(|&decl_idx| self.ctx.binder.get_node_symbol(decl_idx));

        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };

            if !pushed_derived {
                let (_params, updates) = self.push_type_parameters(&interface.type_parameters);
                derived_param_updates = updates;
                pushed_derived = true;
            }

            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };

                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };

                    let base_name = self
                        .entity_name_text(expr_idx)
                        .or_else(|| self.expression_text(expr_idx));
                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx).or_else(|| {
                        base_name
                            .as_deref()
                            .and_then(|name| self.resolve_interface_heritage_symbol_by_name(name))
                    }) else {
                        continue;
                    };
                    // When the base is named through a chain of named re-exports
                    // (`export type { X } from './x'` / `export { X } from`), the
                    // local heritage symbol is an import alias whose own
                    // declaration is the import specifier — it carries no
                    // type-parameter list. Both the base body
                    // (`get_type_of_symbol`) and the base params
                    // (`get_type_params_for_symbol`) must read off the *same*
                    // original declaration symbol; otherwise the param list is
                    // empty and the `instantiate_type` substitution below is a
                    // no-op, leaving the inherited member bound to the base's
                    // free type parameter instead of the supplied argument
                    // (false TS2322/TS2416 on `const y: Y`). Re-point to the
                    // chased generic declaration, mirroring the arity / "is
                    // generic" diagnostic gate (#13797). Non-generic and
                    // namespace / `import =` / `export =` aliases keep the
                    // original surface (the helper returns `None`).
                    let base_sym_id = self
                        .resolve_heritage_alias_to_declaration_symbol(base_sym_id, expr_idx)
                        .unwrap_or(base_sym_id);
                    let Some((
                        base_symbol_declarations,
                        base_symbol_value_declaration,
                        base_symbol_name,
                        base_symbol_flags,
                    )) = self
                        .get_cross_file_symbol(base_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
                        .map(|symbol| {
                            (
                                symbol.declarations.clone(),
                                symbol.value_declaration,
                                symbol.escaped_name.clone(),
                                symbol.flags,
                            )
                        })
                    else {
                        continue;
                    };

                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    // Resolve the base type and its type params.  We use
                    // get_type_of_symbol (which caches) for the type, and
                    // get_type_params_for_symbol (which also caches) for params.
                    // This ensures the TypeParam TypeIds in base_type_params match
                    // the TypeIds embedded in the base type's member signatures,
                    // which is critical for substitution to work correctly.
                    let mut base_type = None;

                    // Try class instance type first (needs special handling)
                    for &base_decl_idx in &base_symbol_declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                            base_type =
                                Some(self.get_class_instance_type(base_decl_idx, base_class));
                            break;
                        }
                    }
                    if base_type.is_none() && base_symbol_value_declaration.is_some() {
                        let base_decl_idx = base_symbol_value_declaration;
                        if let Some(base_node) = self.ctx.arena.get(base_decl_idx)
                            && let Some(base_class) = self.ctx.arena.get_class(base_node)
                        {
                            base_type =
                                Some(self.get_class_instance_type(base_decl_idx, base_class));
                        }
                    }

                    // Cross-module class base: the base class declaration lives in
                    // another file's arena, so the local-arena lookups above cannot
                    // see it (NodeIndex is per-arena). For a class base — including one
                    // reached through an import alias or cross-file re-export — resolve
                    // the class *instance* type from the class symbol (the same resolver
                    // class heritage uses), rather than falling through to
                    // `get_type_of_symbol`, which for a class symbol yields its
                    // *constructor* (static) type. Merging a constructor type would
                    // contribute only static/construct members, dropping every inherited
                    // instance member (TS2339). This makes the interface-extends-class
                    // direction symmetric with the class-extends-class direction in
                    // `instance_merge.rs`.
                    //
                    // The returned instance type embeds the base class's own type
                    // parameter `TypeId`s, so its params (not the alias symbol's) drive
                    // the generic-base instantiation below.
                    let mut base_class_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
                    if base_type.is_none()
                        && let Some(class_sym) = self.heritage_base_class_symbol(base_sym_id)
                        && let Some((instance_type, instance_params)) =
                            self.class_instance_type_with_params_from_symbol(class_sym)
                        && instance_type != TypeId::ERROR
                        && instance_type != TypeId::UNKNOWN
                    {
                        base_type = Some(instance_type);
                        base_class_params = Some(instance_params);
                    }

                    // For interfaces/type aliases, resolve through symbol type
                    if base_type.is_none() {
                        let resolved = self.get_type_of_symbol(base_sym_id);
                        if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                            base_type = Some(resolved);
                        } else if !self.ctx.lib_contexts.is_empty() {
                            // Fallback: if get_type_of_symbol returned UNKNOWN/ERROR
                            // (e.g., due to circular heritage chains like
                            // IteratorObject <-> Iterator in esnext.iterator.d.ts),
                            // try resolving via lib type resolution which has
                            // dedicated cycle-breaking logic.
                            if let Some(lib_type) = self.resolve_lib_type_by_name(&base_symbol_name)
                                && lib_type != TypeId::ERROR
                                && lib_type != TypeId::UNKNOWN
                            {
                                base_type = Some(lib_type);
                            }
                        }
                    }

                    let Some(mut base_type) = base_type else {
                        continue;
                    };

                    // Use get_type_params_for_symbol to get the ORIGINAL TypeParam
                    // TypeIds that match the ones in base_type's member signatures.
                    // Previously we used push_type_parameters which creates NEW
                    // TypeIds that don't match, causing substitution to be a no-op.
                    //
                    // A class base resolves through `class_instance_type_with_params_from_symbol`,
                    // which returns the base class's own type parameters matching the
                    // `TypeId`s embedded in the (uninstantiated) instance type; use those
                    // so the substitution below applies the heritage arguments correctly.
                    let base_is_class_base = base_class_params.is_some();
                    let mut base_type_params = base_class_params
                        .unwrap_or_else(|| self.get_type_params_for_symbol(base_sym_id));

                    // A generic base reached through a cross-file import / re-export
                    // alias keeps `base_sym_id` on the *alias* symbol, whose own
                    // declaration is the import specifier and so carries no
                    // type-parameter list. `get_type_of_symbol` above still follows the
                    // alias to the base's expanded body (whose members embed the base's
                    // free type parameter), but `get_type_params_for_symbol` on the
                    // alias returns nothing — so with heritage arguments supplied there
                    // is no parameter to bind them to, the `instantiate_type`
                    // substitution below is a silent no-op, and every inherited member
                    // stays bound to the base's free `T` (false TS2322 on `c.value` for
                    // `interface Crate extends Container<number>` imported from another
                    // module — #13767 / #13212 cross-file heritage residual).
                    //
                    // Recover the base declaration's type-parameter list by reading it
                    // directly from the declaring module's arena, resolved through the
                    // full re-export chain (`reference_import_alias_export_target` ->
                    // `resolve_reexport_chain_to_declaration`, which handles multi-hop
                    // barrels and renames). Re-exported `SymbolId`s collide numerically
                    // across binders (#14344), so identity-based re-resolution and arity
                    // comparisons are unreliable here; the by-name substitution only
                    // needs the parameter *names*, and those are interned to the same
                    // `Atom`s already embedded as free type parameters in `base_type`.
                    // Gated on the broken signature only — heritage arguments supplied,
                    // no params resolved, and not the class path — so same-file and lib
                    // generic bases (already param-matched) and genuinely non-generic
                    // bases (recovered params stay empty) are left unchanged.
                    if !base_is_class_base && base_type_params.is_empty() && !type_args.is_empty() {
                        let chain_target = self
                            .ctx
                            .binder
                            .get_symbol(base_sym_id)
                            .filter(|alias_symbol| {
                                self.reference_symbol_is_import_alias(alias_symbol)
                            })
                            .and_then(|alias_symbol| {
                                self.reference_import_alias_export_target(
                                    alias_symbol,
                                    &base_symbol_name,
                                )
                            });
                        if let Some((decl_sym, Some(owner_file_idx))) = chain_target
                            && let Some((decl_flags, decl_decls, decl_name)) = self
                                .ctx
                                .get_binder_for_file(owner_file_idx)
                                .and_then(|binder| binder.get_symbol(decl_sym))
                                .map(|decl_symbol| {
                                    (
                                        decl_symbol.flags,
                                        decl_symbol.declarations.clone(),
                                        decl_symbol.escaped_name.clone(),
                                    )
                                })
                        {
                            let owner_arena = self.ctx.get_arena_for_file(owner_file_idx as u32);
                            for decl_idx in decl_decls {
                                if let Some(params) = self
                                    .extract_simple_type_params_from_decl_in_arena(
                                        owner_arena,
                                        decl_flags,
                                        decl_idx,
                                        &decl_name,
                                    )
                                    && !params.is_empty()
                                {
                                    base_type_params = params;
                                    break;
                                }
                            }
                        }
                    }

                    // A generic base re-resolved through a secondary (importing)
                    // arena can return an empty `base_type_params` even though the
                    // heritage clause supplies arguments: the re-entrant heritage
                    // resolution trips the recursion guard / def-param cache in
                    // `get_type_params_for_symbol`. Without parameters to bind, the
                    // arity reconcile below truncates the supplied arguments away
                    // and the substitution is a no-op, so every inherited member
                    // stays bound to the base's *free* type parameter (the `T`-leak:
                    // `interface NonEmptyArray<A> extends Array<A>` keeps the lib
                    // `Array`'s `T` as its `number` index type -> false TS2411).
                    //
                    // Recover the base declaration's parameter names arena-directly
                    // (the same recovery the lib-priming path performs), so the
                    // arguments bind structurally (`T -> A`). Gated on the broken
                    // shape only: heritage arguments supplied and no params resolved.
                    if xarena_heritage_typearg_enabled()
                        && base_type_params.is_empty()
                        && !type_args.is_empty()
                        && let Some(recovered) =
                            self.recover_user_heritage_base_type_params(base_sym_id)
                    {
                        base_type_params = recovered;
                    }

                    if type_args.len() < base_type_params.len() {
                        for (param_index, param) in
                            base_type_params.iter().enumerate().skip(type_args.len())
                        {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            let substitution = TypeSubstitution::from_args(
                                self.ctx.types,
                                &base_type_params[..param_index],
                                &type_args,
                            );
                            type_args.push(
                                crate::query_boundaries::common::instantiate_type_preserving_meta(
                                    self.ctx.types,
                                    fallback,
                                    &substitution,
                                ),
                            );
                        }
                    }
                    if type_args.len() > base_type_params.len() {
                        type_args.truncate(base_type_params.len());
                    }

                    let has_structural_self_arg = current_sym.is_some_and(|current_sym| {
                        type_args.iter().copied().any(|arg| {
                            self.type_requires_structure_of_symbol_for_base_type(arg, current_sym)
                        })
                    });

                    // A generic type alias is referenced nominally as `Lazy(DefId)`,
                    // which carries no inline type-parameter occurrences, so
                    // `instantiate_type` is a no-op that silently drops the heritage
                    // type arguments. Cross-file (multi-arena) resolution of a generic
                    // alias commonly yields exactly that bare `Lazy` here — e.g.
                    // `interface I extends Omit<Base, K>` / `Pick<…>` / `Partial<…>` /
                    // `Record<…>` — and without the arguments the base collapses to an
                    // argument-less `Lazy` that resolves to `unknown` and is then dropped
                    // by the merge, losing every inherited member.
                    //
                    // Form the generic application `Alias<args>` (mirroring the canonical
                    // `Name<args>` type-reference lowering) and then resolve it env-aware
                    // to its concrete apparent shape. Resolving here — rather than leaving
                    // a deferred `Application` for the merge to wrap in an intersection —
                    // is what lets the inherited members compose through a *second* level
                    // of generic-interface inheritance (`interface R<…> extends I<…>`):
                    // an `Object & Application` intersection base does not merge cleanly
                    // through `merge_with_intersection`, but a plain `Object` does. When
                    // the application stays generic (its arguments still depend on the
                    // deriving interface's own type parameters) evaluation makes no
                    // progress and the deferred application is kept, preserving the
                    // existing single-level behaviour.
                    //
                    // Scoped to type-alias bases: interface/class heritage resolves to an
                    // already-expanded object/callable (or class instance) body whose
                    // member signatures embed the base's own type-parameter `TypeId`s, so
                    // those keep going through `instantiate_type`, which substitutes them
                    // directly (and aliases whose body the resolver does expand inline
                    // are not `Lazy`, so they are unaffected too).
                    let base_is_lazy_alias_ref = base_symbol_flags
                        & tsz_binder::symbol_flags::TYPE_ALIAS
                        != 0
                        && crate::query_boundaries::common::lazy_def_id(self.ctx.types, base_type)
                            .is_some();
                    base_type = if base_is_lazy_alias_ref && !type_args.is_empty() {
                        let application = self.ctx.types.application(base_type, type_args.clone());
                        let resolved = self.evaluate_type_with_env(application);
                        if resolved != application
                            && crate::query_boundaries::common::classify_for_interface_merge(
                                self.ctx.types,
                                resolved,
                            )
                            .is_structurally_mergeable()
                        {
                            resolved
                        } else {
                            application
                        }
                    } else {
                        let substitution = TypeSubstitution::from_args(
                            self.ctx.types,
                            &base_type_params,
                            &type_args,
                        );
                        instantiate_type(self.ctx.types, base_type, &substitution)
                    };

                    // #14351 lazy-reference relation capture (inert data — read
                    // only by the flag-gated variance branch, so an unread map
                    // keeps flag-OFF byte-identical to main). Record the
                    // instantiated heritage edge `(derived, parent) -> base_type`
                    // so the relation layer can relate `Apply1<A>` to
                    // `Functor1<B>` by per-argument variance on the instantiated
                    // base (`Functor1<F>` here, with the derived interface's args
                    // already substituted above) without materializing members.
                    // Only meaningful for generic heritage edges (the eager
                    // member walk is the cost there); skip when no arguments flow.
                    if !type_args.is_empty()
                        && let Some(current_sym) = current_sym
                    {
                        let derived_def = self.ctx.get_or_create_def_id(current_sym);
                        let parent_def = self.ctx.get_or_create_def_id(base_sym_id);
                        if derived_def != parent_def {
                            self.ctx.definition_store.add_heritage_instantiation(
                                derived_def,
                                parent_def,
                                base_type,
                            );
                        }
                    }

                    let is_builtin_array_heritage =
                        matches!(base_symbol_name.as_str(), "Array" | "ReadonlyArray");
                    let requires_self = !is_builtin_array_heritage
                        && current_sym.is_some_and(|current_sym| {
                            has_structural_self_arg
                                || self.type_requires_structure_of_symbol_for_base_type(
                                    base_type,
                                    current_sym,
                                )
                        });

                    if let Some(current_sym) = current_sym
                        && requires_self
                    {
                        self.report_recursive_base_type_for_symbol(current_sym);
                        self.report_instantiated_type_alias_mapped_constraint_cycles(
                            base_sym_id,
                            &base_type_params,
                            &type_args,
                            current_sym,
                        );
                        derived_type = self.merge_interface_types_heritage(derived_type, base_type);
                        continue;
                    }

                    derived_type = self.merge_interface_types_heritage(derived_type, base_type);
                }
            }
        }

        if pushed_derived {
            self.pop_type_parameters(derived_param_updates);
        }

        self.ctx.heritage_merge_depth.set(heritage_depth);
        derived_type
    }

    /// Merge two interface types structurally.
    ///
    /// This function merges a derived interface type with a base interface type,
    /// combining their call signatures, construct signatures, properties, and index signatures.
    /// Derived members take precedence over base members.
    ///
    /// # Arguments
    /// * `derived` - The derived interface type
    /// * `base` - The base interface type
    ///
    /// # Returns
    /// The merged `TypeId`
    pub(crate) fn merge_interface_types(&mut self, derived: TypeId, base: TypeId) -> TypeId {
        self.merge_interface_types_with_mode(derived, base, InterfaceMergeMode::Declaration)
    }

    /// Cross-program-file variant of [`Self::merge_interface_types`]: `earlier`
    /// is the merged type of the declaration groups from earlier program files,
    /// `later` a subsequent file's declaration group. Accumulated overload sets
    /// keep forward storage order but stamp the later group's
    /// `declaration_group` above the earlier side's so call resolution tries
    /// the later group first. See [`InterfaceMergeMode::CrossFileDeclaration`].
    pub(crate) fn merge_interface_types_cross_file_declaration(
        &mut self,
        earlier: TypeId,
        later: TypeId,
    ) -> TypeId {
        self.merge_interface_types_with_mode(
            earlier,
            later,
            InterfaceMergeMode::CrossFileDeclaration,
        )
    }

    /// Heritage (`extends`) variant of [`Self::merge_interface_types`]: a
    /// derived member that shares a name with a base member overrides
    /// (replaces) it rather than accumulating an overload set. See
    /// [`InterfaceMergeMode`] for the structural rule and why anonymous call
    /// signatures still accumulate.
    pub(crate) fn merge_interface_types_heritage(
        &mut self,
        derived: TypeId,
        base: TypeId,
    ) -> TypeId {
        self.merge_interface_types_with_mode(derived, base, InterfaceMergeMode::Heritage)
    }

    pub(crate) fn merge_interface_types_with_mode(
        &mut self,
        derived: TypeId,
        base: TypeId,
        mode: InterfaceMergeMode,
    ) -> TypeId {
        if derived == base {
            return derived;
        }
        // Depth guard: merge_interface_types can recurse through merge_properties
        // and resolve_type_for_interface_merge, creating an unbounded cycle.
        if !self.ctx.enter_recursion() {
            return derived;
        }
        // Cross-context OS-stack breaker (#14111): `enter_recursion` is a
        // per-context `Cell` that resets across the fresh / cross-arena child
        // contexts this structural merge can hop while resolving members, so
        // it cannot bound the real call stack on its own. Bail to `derived`.
        let result = crate::checkers_domain::with_stack_guard(derived, || {
            self.merge_interface_types_impl(derived, base, mode)
        });
        self.ctx.leave_recursion();
        result
    }

    fn merge_interface_types_impl(
        &mut self,
        derived: TypeId,
        base: TypeId,
        mode: InterfaceMergeMode,
    ) -> TypeId {
        use crate::query_boundaries::common::{InterfaceMergeKind, classify_for_interface_merge};
        use tracing::trace;
        use tsz_solver::{CallableShape, ObjectShape};

        // Bail out if type resolution fuel is exhausted to prevent
        // expensive merges from hanging on augmented module interfaces
        // (e.g., react + create-emotion-styled cross-referencing).
        if !self.ctx.consume_fuel() {
            return derived;
        }

        trace!(derived_id = %derived.0, base_id = %base.0, "merge_interface_types called");
        let factory = self.ctx.types.factory();

        // Resolve Application/Lazy types before classification.
        // When an interface extends a type alias (e.g., `interface TaggedPair<T> extends Pair<T>`
        // where `type Pair<T> = AB<T, T>`), the instantiated base type may be an Application
        // (e.g., `AB<number, number>`) which classify_for_interface_merge cannot structurally
        // merge. Evaluating it first resolves it to an Object type with the actual properties.
        let derived_resolved = self.resolve_type_for_interface_merge(derived);
        let base_resolved = self.resolve_type_for_interface_merge(base);

        let derived_kind = classify_for_interface_merge(self.ctx.types, derived_resolved);
        let base_kind = classify_for_interface_merge(self.ctx.types, base_resolved);
        trace!(derived_kind = ?derived_kind, base_kind = ?base_kind, "Classified types for merge");

        match (derived_kind, base_kind) {
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                trace!(
                    derived_call_sigs = derived_shape.call_signatures.len(),
                    derived_construct_sigs = derived_shape.construct_signatures.len(),
                    base_call_sigs = base_shape.call_signatures.len(),
                    base_construct_sigs = base_shape.construct_signatures.len(),
                    "Callable+Callable merge signature counts"
                );
                // In a cross-file declaration merge the base side is a later
                // program file's declaration group: offset its group stamps
                // above the derived side's so `reorder_overload_candidates`
                // tries the later group first while storage keeps forward
                // declaration order (tsc reorderCandidates vs display order).
                let base_group_offset = if mode == InterfaceMergeMode::CrossFileDeclaration {
                    derived_shape
                        .call_signatures
                        .iter()
                        .chain(derived_shape.construct_signatures.iter())
                        .map(|sig| sig.declaration_group)
                        .max()
                        .unwrap_or(0)
                        + 1
                } else {
                    0
                };
                let offset_group = |mut sig: tsz_solver::CallSignature| {
                    sig.declaration_group += base_group_offset;
                    sig
                };
                let mut call_signatures = derived_shape.call_signatures.clone();
                call_signatures
                    .extend(base_shape.call_signatures.iter().cloned().map(offset_group));
                // Deduplicate inherited call signatures from diamond inheritance.
                // When C extends C1 and C2, and both inherit from B (which has a
                // catch-all like `(x: string): void`), that catch-all appears in
                // both C1's and C2's chains. Without deduplication, it appears
                // before C2's specific overloads, causing wrong overload resolution.
                // Keep the LAST occurrence so shared base signatures sort after
                // all derived-specific overloads.
                dedup_call_signatures_keep_last(&mut call_signatures);
                let mut construct_signatures = derived_shape.construct_signatures.clone();
                construct_signatures.extend(
                    base_shape
                        .construct_signatures
                        .iter()
                        .cloned()
                        .map(offset_group),
                );
                dedup_call_signatures_keep_last(&mut construct_signatures);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index: derived_shape
                        .string_index
                        .or_else(|| base_shape.string_index),
                    number_index: derived_shape
                        .number_index
                        .or_else(|| base_shape.number_index),
                    symbol: derived_shape.symbol,
                    is_abstract: derived_shape.is_abstract || base_shape.is_abstract,
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape.string_index,
                    number_index: derived_shape.number_index,
                    symbol: derived_shape.symbol,
                    is_abstract: derived_shape.is_abstract,
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .or_else(|| base_shape.string_index),
                    number_index: derived_shape
                        .number_index
                        .or_else(|| base_shape.number_index),
                    symbol: derived_shape.symbol,
                    is_abstract: derived_shape.is_abstract,
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: base_shape.string_index,
                    number_index: base_shape.number_index,
                    symbol: derived_shape.symbol,
                    is_abstract: base_shape.is_abstract,
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .or_else(|| base_shape.string_index),
                    number_index: derived_shape
                        .number_index
                        .or_else(|| base_shape.number_index),
                    symbol: derived_shape.symbol,
                    is_abstract: base_shape.is_abstract,
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.object_with_symbol(properties, derived_shape.symbol)
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                tracing::trace!(
                    ?derived_shape_id,
                    ?base_shape_id,
                    has_base_string_index = base_shape.string_index.is_some(),
                    has_base_number_index = base_shape.number_index.is_some(),
                    "merge_interface_types: Object + ObjectWithIndex"
                );
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                // Read index slots through the signature accessors: a `symbol`
                // index may ride in the `string_index` slot (single-slot
                // convention), and the raw fields would migrate it into the
                // merged string slot, dropping the symbol key space (#15508).
                let result = factory.object_with_index(ObjectShape {
                    properties,
                    string_index: base_shape.string_index_signature().copied(),
                    number_index: base_shape.number_index,
                    symbol_index: base_shape.symbol_index_signature().copied(),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                });
                tracing::trace!(result_type = %result.0, "merge_interface_types: created merged type");
                result
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape.string_index_signature().copied(),
                    number_index: derived_shape.number_index,
                    symbol_index: derived_shape.symbol_index_signature().copied(),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties, mode);
                factory.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape
                        .string_index_signature()
                        .copied()
                        .or_else(|| base_shape.string_index_signature().copied()),
                    number_index: derived_shape
                        .number_index
                        .or_else(|| base_shape.number_index),
                    symbol_index: derived_shape
                        .symbol_index_signature()
                        .copied()
                        .or_else(|| base_shape.symbol_index_signature().copied()),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                })
            }
            // When one side is an intersection (e.g., from global augmentation merging
            // an interface with additional properties), decompose it and merge the
            // callable/object parts properly so that construct signatures are preserved.
            // Use resolved types so that Lazy wrappers (e.g., type aliases) are
            // expanded to their structural intersection form before decomposition.
            (_, InterfaceMergeKind::Intersection) | (InterfaceMergeKind::Intersection, _) => self
                .merge_with_intersection(
                    derived_resolved,
                    derived_kind,
                    base_resolved,
                    base_kind,
                    mode,
                ),
            // When the derived interface has no own members (TypeId::ANY), just use the base.
            (InterfaceMergeKind::Other, _) if derived == TypeId::ANY => base,
            // When the base is an Array or Tuple type (e.g., `interface MyTuple extends [] { ... }`),
            // create an intersection of derived & base. This preserves the array/tuple nature
            // of the base in the resulting type, which is critical for:
            // - Weak type detection (TS2559): the intersection prevents false weak-type violations
            //   because the target is not a standalone object.
            // - Assignability: array/tuple sources can be checked against the tuple base.
            // Track the result so the checker can also suppress false NoCommonProperties failures.
            (_, InterfaceMergeKind::Other)
                if crate::query_boundaries::common::is_array_or_tuple_type(
                    self.ctx.types,
                    base_resolved,
                ) && derived != TypeId::ANY =>
            {
                let result = factory.intersection(vec![derived, base]);
                self.ctx.types_extending_array.insert(result);
                result
            }
            (_, InterfaceMergeKind::Other)
                if crate::query_boundaries::common::mapped_type_id(
                    self.ctx.types,
                    base_resolved,
                )
                .is_some()
                    && derived != TypeId::ANY =>
            {
                factory.intersection2(derived, base_resolved)
            }
            (_, InterfaceMergeKind::Other)
                if crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    base_resolved,
                ) && derived != TypeId::ANY =>
            {
                factory.intersection2(derived, base_resolved)
            }
            // #14101 part-4: a base classified `Other` that nonetheless has an
            // extractable object shape (constrained type-parameter through its
            // constraint, or a function-with-properties) drops the base's members
            // at the `_ => derived` fallthrough below. Incorporate them (derived
            // overrides base by name, via Heritage-mode `merge_properties`) so
            // inherited members are not lost. Conservative: ONLY when both sides
            // have object shapes; non-object bases (Union/Enum/Function) keep the
            // legacy `derived` (no broad intersection). Gated by
            // `heritage_base_member_incorp_disabled`.
            (_, InterfaceMergeKind::Other)
                if mode == InterfaceMergeMode::Heritage
                    && derived != TypeId::ANY
                    && !heritage_base_member_incorp_disabled() =>
            {
                match (
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        derived_resolved,
                    ),
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        base_resolved,
                    ),
                ) {
                    (Some(derived_shape), Some(base_shape)) => {
                        let properties = self.merge_properties(
                            &derived_shape.properties,
                            &base_shape.properties,
                            mode,
                        );
                        factory.object_with_index(ObjectShape {
                            properties,
                            string_index: derived_shape
                                .string_index_signature()
                                .copied()
                                .or_else(|| base_shape.string_index_signature().copied()),
                            number_index: derived_shape
                                .number_index
                                .or_else(|| base_shape.number_index),
                            symbol_index: derived_shape
                                .symbol_index_signature()
                                .copied()
                                .or_else(|| base_shape.symbol_index_signature().copied()),
                            symbol: derived_shape.symbol,
                            ..ObjectShape::default()
                        })
                    }
                    _ => derived,
                }
            }
            _ => derived,
        }
    }

    fn resolve_type_for_interface_merge(&mut self, type_id: TypeId) -> TypeId {
        if crate::query_boundaries::common::needs_evaluation_for_merge(self.ctx.types, type_id) {
            // Use the solver evaluator without ensure_relation_input_ready.
            // evaluate_type_with_env triggers lazy ref resolution which can cause
            // explosive type creation on augmented module interfaces (react + emotion).
            //
            // Suppress `this` binding so that ThisType references inside resolved
            // Lazy types are preserved. During heritage merging, `this` must remain
            // unbound until the final derived interface is constructed; binding it
            // here would incorrectly lock it to the base interface identity (e.g.,
            // `A` instead of the derived `D`).
            use crate::query_boundaries::state::type_environment::evaluate_type_suppressing_this;
            let env = self.ctx.type_env.borrow();
            let evaluated = evaluate_type_suppressing_this(self.ctx.types, &*env, type_id);
            if evaluated != type_id {
                return evaluated;
            }
        }
        type_id
    }

    /// Merge an interface type with an intersection base/derived.
    ///
    /// When a lib interface is augmented (e.g., `ErrorConstructor` gets `captureStackTrace`
    /// from user code), the resolved type is an intersection like
    /// `Callable(call_sigs, construct_sigs, props) & Object(captureStackTrace)`.
    ///
    /// When a derived interface (e.g., `RangeErrorConstructor extends ErrorConstructor`)
    /// needs to merge with this intersection base, we must decompose the intersection,
    /// find the callable member, merge it properly with the derived callable (preserving
    /// construct signatures), and then re-wrap with the remaining intersection members.
    fn merge_with_intersection(
        &mut self,
        derived: TypeId,
        _derived_kind: crate::query_boundaries::common::InterfaceMergeKind,
        base: TypeId,
        base_kind: crate::query_boundaries::common::InterfaceMergeKind,
        mode: InterfaceMergeMode,
    ) -> TypeId {
        use crate::query_boundaries::common::intersection_members;
        use crate::query_boundaries::common::{InterfaceMergeKind, classify_for_interface_merge};

        let factory = self.ctx.types.factory();

        // Determine which side is the intersection and which is the "other" type
        let (intersection_id, other_id, other_is_derived) =
            if matches!(base_kind, InterfaceMergeKind::Intersection) {
                (base, derived, true)
            } else {
                (derived, base, false)
            };

        // Get the intersection members
        let Some(members) = intersection_members(self.ctx.types, intersection_id) else {
            return factory.intersection2(derived, base);
        };

        // Find the best structurally mergeable member in the intersection.
        // Prefer callable members over plain object members so interfaces like
        // `RangeErrorConstructor extends ErrorConstructor` merge against the
        // callable core first, then re-apply object augmentations such as
        // `captureStackTrace`.
        let rank_member = |kind: InterfaceMergeKind| match kind {
            InterfaceMergeKind::Callable(_) => Some(0_u8),
            InterfaceMergeKind::ObjectWithIndex(_) => Some(1_u8),
            InterfaceMergeKind::Object(_) => Some(2_u8),
            _ => None,
        };

        let mut best_mergeable: Option<(usize, TypeId, u8)> = None;
        let mut resolved_members = Vec::with_capacity(members.len());
        for (idx, &member) in members.iter().enumerate() {
            let resolved_member = self.resolve_type_for_interface_merge(member);
            let kind = classify_for_interface_merge(self.ctx.types, resolved_member);
            if let Some(rank) = rank_member(kind)
                && best_mergeable
                    .as_ref()
                    .is_none_or(|(_, _, best_rank)| rank < *best_rank)
            {
                best_mergeable = Some((idx, resolved_member, rank));
            }
            resolved_members.push((member, resolved_member));
        }

        let mergeable_member = best_mergeable.map(|(_, resolved_member, _)| resolved_member);
        let other_members: Vec<_> = resolved_members
            .into_iter()
            .enumerate()
            .filter_map(|(idx, (member, _))| {
                (best_mergeable
                    .as_ref()
                    .is_none_or(|(best_idx, _, _)| idx != *best_idx))
                .then_some(member)
            })
            .collect();

        // If we found a mergeable member, structurally merge it with the other side
        if let Some(mergeable_id) = mergeable_member {
            let (merge_derived, merge_base) = if other_is_derived {
                (other_id, mergeable_id)
            } else {
                (mergeable_id, other_id)
            };

            // Recursively merge the parts (hits Callable+Callable, Object+Object,
            // Callable+Object, etc. paths instead of the Intersection path)
            let merged = self.merge_interface_types_with_mode(merge_derived, merge_base, mode);

            // Re-wrap with the remaining intersection members (e.g., string[])
            if other_members.is_empty() {
                merged
            } else {
                let mut all = vec![merged];
                all.extend(other_members);
                factory.intersection(all)
            }
        } else {
            // No mergeable member found - fall back to plain intersection
            factory.intersection2(derived, base)
        }
    }

    /// Get the interned Atom for a member name node, resolving computed symbol
    /// names before falling back to syntactic literal names.
    fn get_member_name_atom(&mut self, name_idx: NodeIndex) -> Option<Atom> {
        let name = self.get_property_name_resolved(name_idx)?;
        Some(self.ctx.types.intern_string(&name))
    }

    fn recover_user_heritage_base_type_params(
        &mut self,
        base_sym_id: tsz_binder::SymbolId,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        use tsz_binder::symbol_flags;
        let (flags, declarations, escaped_name) = self
            .ctx
            .binder
            .get_symbol(base_sym_id)
            .map(|s| (s.flags, s.declarations.clone(), s.escaped_name.clone()))?;
        if flags & (symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS) == 0 {
            return None;
        }
        // Resolve the base declaration's home arena(s) the same way lib priming
        // does, then read the parameter list arena-directly. This bypasses both
        // the recursion guard and the def-param cache in
        // `get_type_params_for_symbol`, which return an empty list during the
        // re-entrant heritage resolution that drops the heritage arguments.
        let fallback_arena = crate::types_domain::queries::lib_decls::resolve_lib_fallback_arena(
            self.ctx.binder,
            base_sym_id,
            &self.ctx.lib_contexts,
            self.ctx.arena,
        );
        let decls_with_arenas =
            crate::types_domain::queries::lib_decls::collect_lib_decls_with_arenas_in_contexts(
                self.ctx.binder,
                base_sym_id,
                &declarations,
                fallback_arena,
                &self.ctx.lib_contexts,
                Some(self.ctx.arena),
            );
        for (decl_idx, decl_arena) in &decls_with_arenas {
            if decl_arena.get(*decl_idx).is_some()
                && let Some(params) = self.extract_simple_type_params_from_decl_in_arena(
                    decl_arena,
                    flags,
                    *decl_idx,
                    &escaped_name,
                )
                && !params.is_empty()
            {
                return Some(params);
            }
        }
        None
    }
}
