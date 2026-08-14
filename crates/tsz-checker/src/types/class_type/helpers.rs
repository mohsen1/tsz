//! Helper routines for class instance type construction.

use crate::query_boundaries::class_type::{self, callable_shape_for_type, object_shape_for_type};
use crate::query_boundaries::common::is_plain_object_type;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, IndexSignature, TypeId, TypeParamInfo, Visibility};

/// Bookkeeping record for a single type parameter pushed into
/// `type_parameter_scope`: the parameter name, its previous binding in that
/// scope (so `pop_type_parameters` can restore it), and a flag indicating
/// whether the push shadowed an enclosing class's type parameter (so the pop
/// can restore the class scope entry too).
pub(crate) type ScopeUpdate = (String, Option<TypeId>, bool);

pub(super) struct MethodAggregate {
    pub(super) overload_signatures: Vec<CallSignature>,
    pub(super) impl_signatures: Vec<CallSignature>,
    pub(super) overload_optional: bool,
    pub(super) impl_optional: bool,
    pub(super) visibility: Visibility,
    pub(super) declaration_order: u32,
    pub(super) is_symbol_named: bool,
}

pub(super) struct AccessorAggregate {
    pub(super) getter: Option<TypeId>,
    pub(super) setter: Option<TypeId>,
    pub(super) visibility: Visibility,
    pub(super) declaration_order: u32,
    pub(super) is_symbol_named: bool,
}

pub(in crate::types_domain) const fn can_skip_base_instantiation(
    base_type_param_count: usize,
    explicit_type_arg_count: usize,
) -> bool {
    base_type_param_count == 0 && explicit_type_arg_count == 0
}

pub(super) const fn exceeds_class_inheritance_depth_limit(depth: usize) -> bool {
    // Keep well above realistic inheritance chains while bounding pathological recursion.
    depth > 256
}

pub(super) fn in_progress_class_instance_result(
    in_resolution_set: bool,
    cached: Option<TypeId>,
) -> Option<TypeId> {
    if in_resolution_set {
        Some(cached.unwrap_or(TypeId::ERROR))
    } else {
        None
    }
}

pub(super) fn declaration_is_module_augmentation(
    arena: &tsz_parser::parser::NodeArena,
    decl_idx: NodeIndex,
) -> bool {
    let mut current = Some(decl_idx);
    while let Some(node_idx) = current {
        let Some(ext) = arena.get_extended(node_idx) else {
            break;
        };
        if ext.parent.is_none() {
            break;
        }
        let parent_idx = ext.parent;
        let Some(parent_node) = arena.get(parent_idx) else {
            break;
        };
        if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module_decl) = arena.get_module(parent_node)
            && let Some(name_node) = arena.get(module_decl.name)
        {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
            if name_node.kind == SyntaxKind::GlobalKeyword as u16 {
                return false;
            }
            if let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == "global"
            {
                return false;
            }
        }
        current = Some(parent_idx);
    }
    false
}

impl<'a> CheckerState<'a> {
    /// Push the effective class type parameters for either TypeScript syntax or
    /// a JavaScript `@template` declaration.
    ///
    /// Keeping this selection in one place ensures class summaries, statement
    /// checking, and instance construction all observe the same binder set.
    pub(crate) fn push_effective_class_type_parameters(
        &mut self,
        class_idx: NodeIndex,
        class: &ClassData,
    ) -> (Vec<TypeParamInfo>, Vec<ScopeUpdate>) {
        let (mut type_params, mut scope_updates) =
            self.push_type_parameters(&class.type_parameters);
        if type_params.is_empty() {
            let (jsdoc_params, jsdoc_updates) =
                self.push_jsdoc_class_template_type_params(class_idx);
            if !jsdoc_params.is_empty() {
                type_params = jsdoc_params;
                scope_updates.extend(jsdoc_updates);
            }
        }
        (type_params, scope_updates)
    }

    /// Whether a just-computed constructor type for this class symbol may be
    /// cached. Two windows forbid caching:
    /// - the class's own constructor resolution is re-entrant
    ///   (`class_constructor_resolution_set`), or
    /// - the class's INSTANCE type computation is still in flight AND the
    ///   constructor result actually embeds the provisional instance shape
    ///   (see `ctor_result_embeds_inflight_instance`).
    pub(super) fn constructor_cache_admissible(
        &self,
        sym_id: tsz_binder::SymbolId,
        result: TypeId,
    ) -> bool {
        !self.ctx.class_constructor_resolution_set.contains(&sym_id)
            && !self.ctor_result_embeds_inflight_instance(sym_id, result)
    }

    /// Constructor types computed while the class's own INSTANCE type is
    /// still being built (`class_instance_resolution_set`, e.g. a static
    /// self-reference forcing `typeof C` mid-build) can embed the Phase-0
    /// prescan instance shape — missing computed/symbol-keyed members and
    /// heritage — as their construct-signature return. Caching such a result
    /// leaks the partial instance into every later `new C()` (false
    /// TS7053/TS2739/TS2741). The check is narrow on purpose: results whose
    /// construct return does NOT point at the in-flight provisional instance
    /// (e.g. a `Lazy(DefId)` or already-final shape) stay cacheable, so heavy
    /// self-referential classes are not recomputed per reference.
    pub(crate) fn ctor_result_embeds_inflight_instance(
        &self,
        sym_id: tsz_binder::SymbolId,
        result: TypeId,
    ) -> bool {
        if !self.ctx.class_instance_resolution_set.contains(&sym_id) {
            return false;
        }
        let Some(decl_idx) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .and_then(|symbol| symbol.primary_declaration())
        else {
            // No declaration to compare against: be conservative inside the
            // in-flight window and treat the result as provisional.
            return true;
        };
        let Some(provisional_instance) = self
            .ctx
            .class_instance_type_cache
            .borrow()
            .get(&decl_idx)
            .copied()
        else {
            return true;
        };
        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, result)
            .is_some_and(|shape| {
                shape
                    .construct_signatures
                    .iter()
                    .any(|sig| sig.return_type == provisional_instance)
            })
    }

    pub(super) fn class_member_name_is_symbol_named(&mut self, name_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(name_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            && self.is_symbol_property_name(name_idx)
    }

    pub(super) fn class_declaration_symbol(
        &self,
        class_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let arena_ptr = self.ctx.arena as *const _ as usize;
        self.ctx
            .cross_file_node_symbols_for_arena(self.ctx.binder, arena_ptr)
            .and_then(|node_symbols| node_symbols.get(&class_idx.0).copied())
            .or_else(|| self.ctx.binder.get_node_symbol(class_idx))
    }

    /// Resolve the symbol of the class that lexically encloses `start`, if any.
    ///
    /// Walks the parent chain until a class-like node is found, then resolves it
    /// through [`Self::class_declaration_symbol`] so the result matches the
    /// symbol identity used when building the instance type.
    pub(super) fn node_enclosing_class_symbol(
        &self,
        start: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let mut current = start;
        // Bounded walk; class nesting depth is small in practice.
        for _ in 0..64 {
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
            let node = self.ctx.arena.get(current)?;
            if node.is_class_like() {
                return self.class_declaration_symbol(current);
            }
        }
        None
    }

    /// Whether a fresh instance-type build for `class_sym` would re-enter the
    /// in-flight resolution of one of its OWN members whose type is derived from
    /// a body/initializer still present on `node_resolution_stack` in an
    /// enclosing frame. Two shapes qualify:
    ///
    /// - An arrow-/function-valued **property initializer** with a return-type
    ///   annotation that references the enclosing class. Typing the initializer
    ///   resolves the annotation, which (via return-type name validation or
    ///   constructor-type building) requests a fresh instance build while the
    ///   initializer node is still on the stack — the self-reference
    ///   false-negative case (`is_in_flight_class_property_initializer`).
    /// - An **un-annotated method or getter body**. The class-statement checker
    ///   drops `class_instance_type_cache[class]` before checking members, so
    ///   resolving `this.#field` in the body requests a fresh instance build
    ///   while the body is still on the stack — the `#17430` case
    ///   (`node_is_in_flight_class_member_body`).
    ///
    /// In either shape, building now would type that member from its transient
    /// `ERROR` placeholder (the `get_type_of_node` cycle guard), baking an
    /// unsound `ERROR`/`any` type into the cached instance. The caller instead
    /// returns the already-registered instance type (or a lazy self-reference)
    /// rather than rebuilding.
    ///
    /// Both predicates are deliberately narrow (only the two shapes above, not
    /// arbitrary in-class nodes), so ordinary class resolution that legitimately
    /// re-enters a class type while some in-class node is on the stack is not
    /// perturbed. The stack is walked once for both.
    pub(super) fn class_build_reenters_in_flight_member(
        &self,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        self.ctx.node_resolution_stack.iter().any(|&node| {
            self.is_in_flight_class_property_initializer(node, class_sym)
                || self.node_is_in_flight_class_member_body(node, class_sym)
        })
    }

    /// Whether class construction was re-entered from the root expression of
    /// one of the class's own property initializers. Such a nested build can
    /// contain the node-resolution cycle sentinel and must not replace the
    /// enclosing class's stable receiver snapshot.
    pub(super) fn class_build_reenters_in_flight_property_initializer(
        &self,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        self.ctx.node_resolution_stack.iter().any(|&node| {
            let Some(ext) = self.ctx.arena.get_extended(node) else {
                return false;
            };
            let property_idx = ext.parent;
            let Some(property_node) = self.ctx.arena.get(property_idx) else {
                return false;
            };
            if property_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(property) = self.ctx.arena.get_property_decl(property_node) else {
                return false;
            };
            property.initializer == node
                && self.node_enclosing_class_symbol(property_idx) == Some(class_sym)
        })
    }

    /// Whether `node` sits inside the `body` of an un-annotated method or getter
    /// of `class_sym` — the `#17430` shape.
    ///
    /// Only members whose return type is *inferred from a body* can self-cycle
    /// when a fresh instance build re-enters them: an un-annotated method and an
    /// un-annotated getter. An annotated method takes its return type from the
    /// annotation; setters return nothing and constructors have no return type,
    /// so none of those re-infer a body and none is matched — keeping ordinary
    /// class resolution unperturbed.
    ///
    /// Climbs the parent chain from `node`, tracking the child it entered each
    /// parent from, so a member matches only when the entry child is that
    /// member's own `body` (a signature-position node — a computed member name,
    /// a parameter default — is never treated as an in-flight body).
    fn node_is_in_flight_class_member_body(
        &self,
        start: NodeIndex,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let mut current = start;
        let mut child: Option<NodeIndex> = None;
        // Bounded walk; class-member nesting depth is small in practice.
        for _ in 0..64 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            // Only members whose return type is *inferred from a body* can
            // self-cycle on a rebuild: an un-annotated method and an
            // un-annotated getter. Setters return nothing and constructors have
            // no return type, so neither re-infers a body and neither is
            // matched.
            let member_body = match node.kind {
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .filter(|method| method.type_annotation.is_none())
                    .map(|method| method.body),
                syntax_kind_ext::GET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(node)
                    .filter(|accessor| accessor.type_annotation.is_none())
                    .map(|accessor| accessor.body),
                _ => None,
            };
            if let Some(body) = member_body
                && child == Some(body)
                && self.node_enclosing_class_symbol(current) == Some(class_sym)
            {
                // Reached an un-annotated member body of `class_sym` along its
                // `body` child.
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            child = Some(current);
            current = ext.parent;
        }
        false
    }

    /// Whether `node` is an arrow/function-expression initializer of a property
    /// member of `class_sym`.
    fn is_in_flight_class_property_initializer(
        &self,
        node: NodeIndex,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node_data) = self.ctx.arena.get(node) else {
            return false;
        };
        if node_data.kind != syntax_kind_ext::ARROW_FUNCTION
            && node_data.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(ext) = self.ctx.arena.get_extended(node) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let is_property_initializer = self
            .ctx
            .arena
            .get(parent)
            .is_some_and(|p| p.kind == syntax_kind_ext::PROPERTY_DECLARATION);
        if !is_property_initializer {
            return false;
        }
        self.node_enclosing_class_symbol(node) == Some(class_sym)
    }

    /// Check if a method body syntactically returns only `this`.
    /// Returns true if every return statement in the body has `this` as
    /// its expression (or the body is an expression-bodied arrow returning `this`).
    pub(super) fn method_body_returns_only_this(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if body_node.kind != syntax_kind_ext::BLOCK {
            return false;
        }
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };
        let mut found_return = false;
        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT
                && let Some(return_data) = self.ctx.arena.get_return_statement(stmt_node)
            {
                if return_data.expression.is_none() {
                    continue;
                }
                let Some(expr_node) = self.ctx.arena.get(return_data.expression) else {
                    return false;
                };
                if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return false;
                }
                found_return = true;
            }
        }
        found_return
    }

    pub(super) fn merge_class_instance_with_interface(
        &mut self,
        instance_type: TypeId,
        interface_type: TypeId,
    ) -> TypeId {
        let mut properties = FxHashMap::default();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;
        let mut symbol_index = None;
        let mut symbol = None;
        let mut result_is_callable = false;

        let mut merge_shape = |type_id: TypeId, is_derived_class: bool| {
            if let Some(shape) = callable_shape_for_type(self.ctx.types, type_id) {
                result_is_callable = true;
                if is_derived_class {
                    symbol = shape.symbol;
                    string_index = shape.string_index;
                    number_index = shape.number_index;
                } else {
                    if string_index.is_none() {
                        string_index = shape.string_index;
                    }
                    if number_index.is_none() {
                        number_index = shape.number_index;
                    }
                }
                call_signatures.extend(shape.call_signatures.iter().cloned());
                construct_signatures.extend(shape.construct_signatures.iter().cloned());
                for prop in &shape.properties {
                    properties.entry(prop.name).or_insert_with(|| prop.clone());
                }
                return;
            }

            if let Some(shape) = object_shape_for_type(self.ctx.types, type_id) {
                if is_derived_class {
                    symbol = shape.symbol;
                    string_index = shape.string_index_signature().copied();
                    number_index = shape.number_index;
                    symbol_index = shape.symbol_index_signature().copied();
                } else {
                    if string_index.is_none() {
                        string_index = shape.string_index_signature().copied();
                    }
                    if number_index.is_none() {
                        number_index = shape.number_index;
                    }
                    if symbol_index.is_none() {
                        symbol_index = shape.symbol_index_signature().copied();
                    }
                }
                for prop in &shape.properties {
                    properties.entry(prop.name).or_insert_with(|| prop.clone());
                }
            }
        };

        merge_shape(instance_type, true);
        merge_shape(interface_type, false);

        class_type::merged_class_instance_interface_type(
            self.ctx.types,
            class_type::MergedClassInstanceInterfaceSurface {
                result_is_callable,
                call_signatures,
                construct_signatures,
                properties: properties.into_values().collect(),
                string_index,
                number_index,
                symbol_index,
                symbol,
                plain_object_without_indexes: is_plain_object_type(self.ctx.types, instance_type)
                    && string_index.is_none()
                    && number_index.is_none()
                    && symbol_index.is_none(),
            },
        )
    }

    pub(super) fn merge_union_index_signature(
        &self,
        target: &mut Option<IndexSignature>,
        incoming: IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type {
                existing.value_type = class_type::merged_static_late_bound_index_value_type(
                    self.ctx.types,
                    existing.value_type,
                    incoming.value_type,
                );
            }
            existing.readonly &= incoming.readonly;
        } else {
            *target = Some(incoming);
        }
    }

    /// Does this class member's name node key off a plain (non-unique)
    /// `symbol` binding — `class C { [s]() {} }` with `declare const s: symbol`?
    ///
    /// Such a member contributes a `[key: symbol]: V` index signature to the
    /// class instance shape instead of a named member, matching what the
    /// object-literal and interface lowering paths already do. Without this the
    /// member is stashed under a synthetic `__symbol_<file>_<sym>` atom that
    /// only ever matches another declaration whose key resolves to the SAME
    /// binding, so a class and the interface it implements — keyed off two
    /// different `symbol` bindings, as tsc allows — stop being mutually
    /// assignable. `Symbol.NAME` written as property-access syntax is excluded
    /// by `computed_member_key_is_wide_symbol` itself, even when `NAME` is
    /// declared plain `symbol`: tsc derives a NAMED member from the syntactic
    /// well-known shape before it consults the key's type at all (#16307).
    pub(super) fn class_member_computed_key_is_wide_symbol(&mut self, name_idx: NodeIndex) -> bool {
        // Classifying the key evaluates its expression in VALUE position, and a
        // value-position evaluation reports its own diagnostics. Several of those
        // are suppressed only inside a computed-property-name context —
        // `is_in_ambient_computed_property_context` reads
        // `ctx.checking_computed_property_name` and returns early for an
        // interface/type-literal member, an ambient class, and an `abstract` or
        // `declare` member. Publishing that context here is what keeps a
        // type-only-imported key from re-reporting TS1361 on
        // `abstract class F { abstract [onInit](): void }`, which is emit-free and
        // clean under tsc. Restored unconditionally so a nested classification
        // cannot leak this frame's node.
        let prev_checking = self.ctx.checking_computed_property_name;
        self.ctx.checking_computed_property_name = Some(name_idx);
        let is_wide = self.computed_member_key_is_wide_symbol(name_idx);
        self.ctx.checking_computed_property_name = prev_checking;
        is_wide
    }

    /// Fold a wide-`symbol`-keyed class member's value type into the class
    /// instance shape's symbol index signature.
    ///
    /// Several such members union their value types, exactly as tsc widens a
    /// late-bound index signature over every contributing declaration; the
    /// signature stays `readonly` only while every contributor is.
    pub(super) fn merge_class_wide_symbol_member_index(
        &mut self,
        symbol_index: &mut Option<IndexSignature>,
        value_type: TypeId,
        readonly: bool,
    ) {
        let mut index = class_type::static_late_bound_index_signature(TypeId::SYMBOL, value_type);
        index.readonly = readonly;
        self.merge_union_index_signature(symbol_index, index);
    }

    pub(super) fn merge_index_signature_from_unresolved_computed_name(
        &mut self,
        name_idx: NodeIndex,
        value_type: TypeId,
        string_index: &mut Option<IndexSignature>,
        number_index: &mut Option<IndexSignature>,
        symbol_index: &mut Option<IndexSignature>,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        // Only an entity-name key (`[s]`, `[o.p]`) contributes an index
        // signature to the class type. `tsc` gives an arbitrary expression key
        // (`["" + ""]`, `[+s]`, `[f()]`) no index signature at all -- such a
        // member is only ever *checked* against index signatures contributed by
        // others. Without this gate the member manufactures the very signature
        // it is then measured against, producing a spurious `TS2411`.
        if !self.computed_name_uses_entity_expression(computed.expression) {
            return;
        }

        // A key whose type is the wide `symbol` (not `unique symbol`, not a
        // well-known `Symbol.xxx` that resolved to a literal name -- callers
        // only reach this function when the name failed to resolve to one)
        // contributes a `[k: symbol]: V` index signature, mirroring the
        // object-literal routing in
        // `computed_member_key_is_wide_symbol`. `get_index_key_kind`
        // below has no symbol case, so this must be checked first: a
        // symbol-typed key is otherwise silently dropped from every bucket.
        if self.computed_member_key_is_wide_symbol(name_idx) {
            self.merge_union_index_signature(
                symbol_index,
                class_type::static_late_bound_index_signature(TypeId::SYMBOL, value_type),
            );
            return;
        }

        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let key_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev;

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(key_type) {
            if wants_string {
                self.merge_union_index_signature(
                    string_index,
                    class_type::static_late_bound_index_signature(TypeId::STRING, value_type),
                );
            }
            if wants_number {
                self.merge_union_index_signature(
                    number_index,
                    class_type::static_late_bound_index_signature(TypeId::NUMBER, value_type),
                );
            }
        }
    }

    /// For JS classes without syntax-level type parameters, check the leading
    /// JSDoc for `@template` tags and create type parameters from them.
    ///
    /// Returns `(type_params, scope_updates)` with the same shape as `push_type_parameters`.
    /// The caller must pass `scope_updates` to `pop_type_parameters` when done.
    pub(in crate::types_domain) fn push_jsdoc_class_template_type_params(
        &mut self,
        class_idx: NodeIndex,
    ) -> (Vec<TypeParamInfo>, Vec<ScopeUpdate>) {
        if !self.is_js_file() {
            return (Vec::new(), Vec::new());
        }

        let jsdoc = {
            let sf = match self.ctx.arena.source_files.first() {
                Some(sf) => sf,
                None => return (Vec::new(), Vec::new()),
            };
            let source_text: &str = &sf.text;
            let comments = &sf.comments;
            match self.try_leading_jsdoc(
                comments,
                self.ctx.arena.get(class_idx).map_or(0, |n| n.pos),
                source_text,
            ) {
                Some(j) => j,
                None => return (Vec::new(), Vec::new()),
            }
        };

        self.push_jsdoc_template_type_parameters_for_owner(class_idx, &jsdoc)
    }

    pub(super) fn register_final_class_instance_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        instance_type: TypeId,
        class_type_params: &[TypeParamInfo],
    ) {
        let is_class_symbol = self
            .get_symbol_globally(sym_id)
            .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
        if !is_class_symbol {
            return;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        self.ctx
            .definition_store
            .register_type_to_def(instance_type, def_id);
        self.ctx
            .register_class_instance_in_envs(def_id, instance_type);
        self.ctx
            .register_resolved_type(sym_id, instance_type, class_type_params.to_vec());
        if !class_type_params.is_empty() {
            self.get_type_params_for_symbol(sym_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_skip_base_instantiation, exceeds_class_inheritance_depth_limit,
        in_progress_class_instance_result,
    };
    use tsz_solver::TypeId;

    #[test]
    fn skip_base_instantiation_only_without_generics() {
        assert!(can_skip_base_instantiation(0, 0));
        assert!(!can_skip_base_instantiation(1, 0));
        assert!(!can_skip_base_instantiation(0, 1));
        assert!(!can_skip_base_instantiation(2, 3));
    }

    #[test]
    fn class_inheritance_depth_guard_is_conservative() {
        assert!(!exceeds_class_inheritance_depth_limit(1));
        assert!(!exceeds_class_inheritance_depth_limit(100));
        assert!(!exceeds_class_inheritance_depth_limit(256));
        assert!(exceeds_class_inheritance_depth_limit(257));
    }

    #[test]
    fn in_progress_class_instance_uses_cached_or_error() {
        assert_eq!(
            in_progress_class_instance_result(true, Some(TypeId(42))),
            Some(TypeId(42))
        );
        assert_eq!(
            in_progress_class_instance_result(true, None),
            Some(TypeId::ERROR)
        );
        assert_eq!(in_progress_class_instance_result(false, None), None);
    }
}
