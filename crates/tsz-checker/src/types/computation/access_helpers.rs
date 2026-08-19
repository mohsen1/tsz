//! Element access helper methods: index type validation, generic index detection,
//! numeric index extraction, and union/tuple diagnostic support.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::{CheckerState, EnumKind};
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::property_access_chain_text_in_arena;
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{SymbolRef, TypeId};

const MAX_NUMERIC_INDEX_SURFACE_DEPTH: u32 = 1000;
const HASH_MAP_ENTRY_OVERHEAD_ESTIMATE: usize = 8;

type NumericIndexSurface = Option<(bool, bool)>;

struct NumericIndexSurfaceWalk<'state, 'ctx> {
    checker: &'state CheckerState<'ctx>,
    visiting: FxHashSet<TypeId>,
    memo: FxHashMap<TypeId, NumericIndexSurface>,
    cut_seen: bool,
}

impl<'state, 'ctx> NumericIndexSurfaceWalk<'state, 'ctx> {
    fn new(checker: &'state CheckerState<'ctx>) -> Self {
        Self {
            checker,
            visiting: FxHashSet::default(),
            memo: FxHashMap::default(),
            cut_seen: false,
        }
    }

    fn surface(&mut self, object_type: TypeId) -> NumericIndexSurface {
        self.surface_inner(object_type, 0)
    }

    fn surface_inner(&mut self, object_type: TypeId, depth: u32) -> NumericIndexSurface {
        if let Some(&cached) = self.memo.get(&object_type) {
            return cached;
        }
        if depth >= MAX_NUMERIC_INDEX_SURFACE_DEPTH || !self.visiting.insert(object_type) {
            self.cut_seen = true;
            return None;
        }

        // Use the resolver-aware classifier so wrapped `Record`/application
        // surfaces participate without name or source-text shortcuts.
        let result = match query::classify_element_indexable_with_resolver(
            self.checker.ctx.types,
            &self.checker.ctx,
            object_type,
        ) {
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => Some((has_string, has_number)),
            query::ElementIndexableKind::Intersection(members) => {
                let mut has_string = false;
                let mut has_number = false;
                for member in members {
                    if let Some((member_string, member_number)) =
                        self.surface_inner(member, depth + 1)
                    {
                        has_string |= member_string;
                        has_number |= member_number;
                    }
                    if self.cut_seen {
                        break;
                    }
                }
                (!self.cut_seen && (has_string || has_number)).then_some((has_string, has_number))
            }
            query::ElementIndexableKind::Union(members) => {
                let mut all_have_string_surface = true;
                let mut all_have_number_surface = true;
                let mut saw_indexed_member = false;

                for member in members {
                    let Some((has_string, has_number)) = self.surface_inner(member, depth + 1)
                    else {
                        self.visiting.remove(&object_type);
                        if !self.cut_seen {
                            self.memo.insert(object_type, None);
                        }
                        return None;
                    };
                    saw_indexed_member = true;
                    all_have_string_surface &= has_string;
                    all_have_number_surface &= has_number;
                }

                saw_indexed_member.then_some((all_have_string_surface, all_have_number_surface))
            }
            query::ElementIndexableKind::Array
            | query::ElementIndexableKind::Tuple
            | query::ElementIndexableKind::StringLike
            | query::ElementIndexableKind::Other => None,
        };

        self.visiting.remove(&object_type);
        if !self.cut_seen {
            self.memo.insert(object_type, result);
        }
        result
    }

    fn entry_count(&self) -> usize {
        self.memo.len()
    }

    fn estimated_size_bytes(&self) -> usize {
        self.memo.capacity().saturating_mul(
            mem::size_of::<TypeId>()
                .saturating_add(mem::size_of::<NumericIndexSurface>())
                .saturating_add(HASH_MAP_ENTRY_OVERHEAD_ESTIMATE),
        )
    }

    fn trace_union_result(&self, object_type: TypeId, result: Option<bool>) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!(
                object_type = object_type.0,
                result = ?result,
                cut_seen = self.cut_seen,
                memo_entries = self.entry_count(),
                memo_estimated_size_bytes = self.estimated_size_bytes(),
                "numeric_index_surface_walk"
            );
        }
    }
}

impl<'a> CheckerState<'a> {
    /// Whether the element-access receiver resolves (through aliases /
    /// qualified names / the object type) to a `const enum` symbol. Shared by
    /// the const-enum access guard and the TS2339 const-enum missing-member
    /// path in `get_type_of_element_access_with_request`.
    pub(crate) fn element_access_receiver_is_const_enum(
        &mut self,
        expression: NodeIndex,
        object_type_for_access: TypeId,
    ) -> bool {
        self.resolve_identifier_symbol(expression)
            .map(|sym_id| {
                self.resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
                    .unwrap_or(sym_id)
            })
            .or_else(|| {
                self.resolve_qualified_symbol(expression).map(|sym_id| {
                    self.resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
                        .unwrap_or(sym_id)
                })
            })
            .filter(|&sym_id| self.is_const_enum_symbol(sym_id))
            .or_else(|| {
                self.enum_symbol_from_type(object_type_for_access)
                    .filter(|&sym_id| self.is_const_enum_symbol(sym_id))
            })
            .is_some()
    }

    /// TS2476 (const-enum member accessed without a string literal) and the
    /// negative-tuple-index guard for element access. Extracted verbatim from
    /// `get_type_of_element_access_with_request` as pure code motion to keep
    /// `access.rs` under the 2000-LOC arch cap. Returns `Some(TypeId::ERROR)`
    /// when either guard fires (the caller returns it), `None` to continue.
    pub(crate) fn element_access_const_enum_and_negative_index_guard(
        &mut self,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        object_type_for_access: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        // TS2476: A const enum member can only be accessed using a string literal.
        if self.element_access_receiver_is_const_enum(expression, object_type_for_access) {
            let arg_is_string_literal =
                self.ctx
                    .arena
                    .get(name_or_argument)
                    .is_some_and(|arg_node| {
                        arg_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                            || arg_node.kind
                                == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    });
            if !arg_is_string_literal {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    name_or_argument,
                    diagnostic_messages::A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL,
                    diagnostic_codes::A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL,
                );
                return Some(TypeId::ERROR);
            }
        }

        if let Some(index_value) = self
            .get_number_value_from_element_index(name_or_argument)
            .or_else(|| {
                crate::query_boundaries::common::number_literal_value(self.ctx.types, index_type)
            })
            && index_value.is_finite()
            && index_value.fract() == 0.0
            && index_value < 0.0
        {
            let object_for_tuple_check = {
                let unwrapped = crate::query_boundaries::common::unwrap_readonly(
                    self.ctx.types,
                    object_type_for_access,
                );
                self.resolve_lazy_type(unwrapped)
            };
            let object_for_tuple_check = crate::query_boundaries::common::unwrap_readonly(
                self.ctx.types,
                object_for_tuple_check,
            );
            if crate::query_boundaries::common::is_tuple_type(
                self.ctx.types,
                object_for_tuple_check,
            ) {
                self.error_at_node(
                    name_or_argument,
                    crate::diagnostics::diagnostic_messages::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                    crate::diagnostics::diagnostic_codes::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                );
                return Some(TypeId::ERROR);
            }
        }

        None
    }

    /// Resolve the member-access result type when the (resolved) object type is
    /// `unknown`, under `strictNullChecks`.
    ///
    /// `tsc` forbids accessing a member of a value of type `unknown` — by name
    /// (`x.p`), by index (`x[k]`), or through an optional chain (`x?.p` / `x?.[k]`)
    /// — so under `strictNullChecks` we emit the diagnostic and return `Some`:
    /// `TS18046` (`'x' is of type 'unknown'.`) when the base expression has a
    /// printable name, otherwise the object form `TS2571` (`Object is of type
    /// 'unknown'.`), returning `ERROR` to stop cascading diagnostics.
    ///
    /// When `strictNullChecks` is off, `unknown` is not simply `any`: for a
    /// statically known property name (`prop_name`), tsc still resolves it
    /// against `unknown`'s apparent type, which is the `Object.prototype`
    /// surface (`toString`, `valueOf`, `hasOwnProperty`, ...). A genuine
    /// member there returns `Some` with its real type; anything else —
    /// including a general (non-literal) index with no fixed name to check —
    /// returns `None` so each caller applies its own non-strict fallback for
    /// a truly missing member (index-signature handling for element access,
    /// `error_property_not_exist_at` for dot-access property access).
    ///
    /// This is the single decision gate for the unknown-object access result,
    /// shared by the element-access `literal_string`/`literal_index` arms and the
    /// property-access path, so the `TS2571`/`TS18046`/`Object.prototype` choice
    /// is not re-derived independently in each place.
    pub(crate) fn unknown_object_access_result(
        &mut self,
        base_expr: NodeIndex,
        prop_name: Option<&str>,
    ) -> Option<TypeId> {
        if !self.ctx.compiler_options.strict_null_checks {
            let prop_name = prop_name?;
            let prop_atom = self.ctx.types.intern_string(prop_name);
            return match crate::query_boundaries::property_access::resolve_unknown_non_strict_property_access(
                self.ctx.types,
                prop_atom,
            ) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    ..
                } => Some(type_id),
                _ => None,
            };
        }
        if self.error_is_of_type_unknown(base_expr) {
            Some(TypeId::ERROR)
        } else {
            Some(TypeId::ANY)
        }
    }

    pub(crate) fn expando_element_key_name(&mut self, key_expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(key_expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.ctx.arena.get_identifier(node)?;
                let name = &ident.escaped_text;

                // Resolve through the binder the same way detect_expando_assignment
                // does, so the key matches what was stored at bind time.
                let binder_sym = self
                    .ctx
                    .binder
                    .get_node_symbol(key_expr_idx)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, key_expr_idx)
                    })
                    .or_else(|| self.ctx.binder.file_locals.get(name));
                if let Some(sym_id) = binder_sym
                    && let Some(key) = self.resolved_const_expando_key_from_binder(sym_id, 0)
                {
                    return Some(key);
                }

                // Fallback: resolve through the type system for non-binder cases.
                let prev = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                let key_type = self.get_type_of_node(key_expr_idx);
                self.ctx.preserve_literal_types = prev;

                if let Some(lit) =
                    crate::query_boundaries::common::literal_value(self.ctx.types, key_type)
                {
                    return Some(match lit {
                        tsz_solver::LiteralValue::String(s) => self.ctx.types.resolve_atom(s),
                        tsz_solver::LiteralValue::Number(n) => {
                            tsz_solver::utils::js_number_to_string(n.0).into_owned()
                        }
                        tsz_solver::LiteralValue::Boolean(b) => b.to_string(),
                        tsz_solver::LiteralValue::BigInt(b) => self.ctx.types.resolve_atom(b),
                    });
                }

                if let Some(sym_ref) =
                    crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, key_type)
                {
                    return Some(format!("__unique_{}", sym_ref.0));
                }

                Some(name.to_string())
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    pub(crate) fn is_direct_expando_element_write_base(&self, object_expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            // `this` (and a `var self = this` alias) inside a class is a nominally
            // typed instance, not an untyped JS namespace/object-literal value —
            // tsc never grants it expando-write immunity. A non-literal-like key
            // there stays a checked miss (`TS7053`), and a literal-like key is
            // already a real synthesized member (`js_class_properties.rs`), so
            // this path is only reachable for the non-literal case either way.
            && !(self.ctx.enclosing_class.is_some()
                && self.expression_resolves_to_this(object_expr_idx))
    }

    /// Whether `idx` is `this` itself or a local `const`/`let`/`var` bound
    /// directly to `this` (the common `var self = this;` alias pattern).
    /// Single-hop only, matching `collect_this_aliases`'s scope.
    fn expression_resolves_to_this(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl = symbol.value_declaration;
        if decl.is_none() {
            return false;
        }
        let Some(decl_node) = self.ctx.arena.get(decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        self.ctx
            .arena
            .get(var_decl.initializer)
            .is_some_and(|init| init.kind == SyntaxKind::ThisKeyword as u16)
    }

    pub(crate) fn is_expando_element_access_read(
        &mut self,
        object_expr_idx: NodeIndex,
        key_expr_idx: NodeIndex,
    ) -> bool {
        let Some(obj_key) = property_access_chain_text_in_arena(self.ctx.arena, object_expr_idx)
        else {
            return false;
        };
        let Some(prop_key) = self.expando_element_key_name(key_expr_idx) else {
            return false;
        };

        if self
            .ctx
            .binder
            .expando_properties
            .get(&obj_key)
            .is_some_and(|props| {
                props
                    .iter()
                    .any(|prop| self.canonical_expando_property_name(prop) == prop_key)
            })
        {
            return true;
        }

        // Use global expando index for O(1) lookup instead of O(N) binder scan.
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            if expando_idx.get(&obj_key).is_some_and(|props| {
                props
                    .iter()
                    .any(|prop| self.canonical_expando_property_name(prop) == prop_key)
            }) {
                return true;
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .expando_properties
                    .get(&obj_key)
                    .is_some_and(|props| {
                        props
                            .iter()
                            .any(|prop| self.canonical_expando_property_name(prop) == prop_key)
                    })
                {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn get_number_value_from_element_index(&self, idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::NumericLiteral as u16 {
            return self
                .ctx
                .arena
                .get_literal(node)
                .and_then(|literal| literal.value);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_number_value_from_element_index(paren.expression);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            let data = self.ctx.arena.get_unary_expr(node)?;
            let operand = self.get_number_value_from_element_index(data.operand)?;
            return match data.operator {
                k if k == SyntaxKind::MinusToken as u16 => Some(-operand),
                k if k == SyntaxKind::PlusToken as u16 => Some(operand),
                _ => None,
            };
        }

        if node.kind == syntax_kind_ext::LITERAL_TYPE
            && let Some(literal_type) = self.ctx.arena.get_literal_type(node)
        {
            return self.get_number_value_from_element_index(literal_type.literal);
        }

        None
    }

    /// Get the element access type for array/tuple/object with index signatures.
    ///
    /// Computes the type when accessing an element using an index.
    /// Uses `ElementAccessEvaluator` from solver for structured error handling.
    /// Resolve a receiver object's non-numeric index-signature key aliases
    /// (e.g. the lib global `PropertyKey` => `string | number | symbol`) so the
    /// resolver-less element-access evaluator can classify the full key space.
    /// Returns the input unchanged when there is nothing to resolve (the common
    /// case: no index signature, or an already-structural key). See #14315.
    pub(crate) fn resolve_receiver_index_signature_keys(&mut self, object_type: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
        else {
            return object_type;
        };
        let Some(idx) = shape.string_index.as_ref() else {
            return object_type;
        };
        let resolved_key = self.resolve_lazy_type(idx.key_type);
        let resolved_key = self.resolve_lazy_members_in_union(resolved_key);
        if resolved_key == idx.key_type {
            return object_type;
        }
        let mut new_shape = (*shape).clone();
        if let Some(slot) = new_shape.string_index.as_mut() {
            slot.key_type = resolved_key;
        }
        self.ctx.types.object_with_index(new_shape)
    }

    pub(crate) fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        // Flatten any tuple spread element whose type is an alias / pending
        // application before the resolver-less solver query (see
        // `flatten_tuple_spread_rests`).
        let object_type = self.flatten_tuple_spread_rests(object_type);
        // Resolve the receiver's index-signature key aliases (e.g. the lib
        // global `PropertyKey`, only resolvable at use time) so the resolver-less
        // element-access evaluator below classifies the full key space — notably
        // the `symbol` arm. Without this, a symbol access into
        // `{ [k: PropertyKey]: V }` falls through to `undefined`, surfacing as a
        // false TS7053 (see #14315). Mirrors the index-type resolution below.
        let object_type = self.resolve_receiver_index_signature_keys(object_type);
        // Normalize index type for enum values
        let solver_index_type = if let Some(index) = literal_index {
            self.ctx.types.literal_number(index as f64)
        } else if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            // Numeric enum values are number-like at runtime.
            TypeId::NUMBER
        } else {
            // Resolve `Lazy(DefId)` alias references on the index type before the
            // solver query. The solver's element-access evaluator is
            // resolver-less, so an index whose type is (or contains) a type-alias
            // reference never matches the receiver's concrete keys and silently
            // falls through to `undefined`. This surfaces as false TS2532/TS2722/
            // TS18048 on `obj[expr]` whenever `expr`'s type is an alias — most
            // commonly a property access of an alias-typed property
            // (`obj[node.kind]` where `kind: SomeUnionAlias`), since a property
            // read keeps the alias form while a plain variable read arrives
            // already resolved. Resolve the top-level alias and any aliased union
            // members here, mirroring `resolve_lazy_members_in_union` used for
            // relation queries.
            let resolved = self.resolve_lazy_type(index_type);
            let resolved = self.evaluate_application_type(resolved);
            self.resolve_lazy_members_in_union(resolved)
        };

        self.ctx
            .types
            .resolve_element_access_type(object_type, solver_index_type, literal_index)
    }

    pub(crate) fn wide_symbol_binding_access_type(
        &mut self,
        index_node: NodeIndex,
        pre_resolution_object_type: TypeId,
        object_type_for_access: TypeId,
        index_type: TypeId,
        index_type_for_access: TypeId,
        skip_flow_narrowing: bool,
        write_presence_only: bool,
    ) -> Option<TypeId> {
        if index_type != TypeId::SYMBOL {
            return None;
        }
        if !crate::query_boundaries::common::is_type_parameter(
            self.ctx.types,
            pre_resolution_object_type,
        ) && let Some(property_name) =
            self.symbol_valued_binding_property_name(index_node, index_type)
        {
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            if let Some(prop) = crate::query_boundaries::common::find_property_by_str(
                self.ctx.types,
                resolved_type,
                &property_name,
            ) {
                return Some(if skip_flow_narrowing {
                    if write_presence_only {
                        TypeId::ANY
                    } else {
                        prop.write_type
                    }
                } else {
                    prop.type_id
                });
            }
        }
        if index_type_for_access == index_type {
            return None;
        }
        let result = self.get_element_access_type(object_type_for_access, TypeId::SYMBOL, None);
        (result != TypeId::UNDEFINED && result != TypeId::ERROR).then_some(result)
    }

    /// Resolve a tuple's spread (`rest`) element types and rebuild it so the
    /// solver's resolver-less element-access evaluator sees a flat tuple.
    ///
    /// `[A, ...Alias]` / `[A, ...Util<T>]` stores the spread operand as a bare
    /// `Lazy(DefId)` alias reference or a pending `Application`. The solver's
    /// numeric element-access path (`tuple_fixed_slot`) only descends into a
    /// rest element that is already a concrete `Tuple`, so an unresolved spread
    /// made an in-bounds read fall back to the whole-tuple element union
    /// (`head | tail`) — surfacing as false `TS2339`/`TS2493`. Type-position
    /// indexed access (`T[1]`) already resolves these because it runs the
    /// resolver-backed evaluator; this brings the value position to parity.
    ///
    /// Only tuples carrying a spread element do any work — purely fixed tuples
    /// (the common case) and non-tuples return unchanged. Each rest element is
    /// resolved through the same alias/application path used for the index, then
    /// the tuple is rebuilt via `factory.tuple()`, which inlines a now-concrete
    /// fixed-tuple spread (`createNormalizedTupleType`). `readonly` is preserved.
    fn flatten_tuple_spread_rests(&mut self, object_type: TypeId) -> TypeId {
        // A recursive list utility `[H, ...Util<R>]` resolves one nesting level
        // per pass: resolving the spread exposes the next `[H, ...Util<R')]`,
        // which `factory.tuple()` inlines, leaving a fresh spread to resolve.
        // Iterate to a fixpoint under a bound so deep recursion fully flattens
        // while a non-terminating alias cannot spin.
        const MAX_FLATTEN_PASSES: usize = 64;
        let mut current = object_type;
        for _ in 0..MAX_FLATTEN_PASSES {
            let next = self.flatten_tuple_spread_rests_once(current);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    fn flatten_tuple_spread_rests_once(&mut self, object_type: TypeId) -> TypeId {
        let inner = crate::query_boundaries::common::unwrap_readonly(self.ctx.types, object_type);
        let is_readonly = inner != object_type;
        let Some(elements) = crate::query_boundaries::common::tuple_elements(self.ctx.types, inner)
        else {
            return object_type;
        };
        if !elements.iter().any(|elem| elem.rest) {
            return object_type;
        }
        let mut changed = false;
        let mut rebuilt = Vec::with_capacity(elements.len());
        for elem in elements {
            if elem.rest {
                // Resolve a bare `Lazy(DefId)` alias and reduce a pending
                // application through the resolver-backed environment — the
                // solver's own element-access evaluator is resolver-less and
                // would leave the spread opaque.
                let resolved = self.resolve_lazy_type(elem.type_id);
                let mut resolved = self.evaluate_application_type(resolved);
                // A homomorphic-map-over-rest spread
                // (`...{ [K in keyof R]: F<R[K]> }`, produced by the solver when
                // a recursive utility's rest could not be flattened eagerly in
                // the resolver-less / depth-limited evaluation frame) is neither
                // a `Lazy` nor an `Application`, so the resolution above leaves it
                // opaque. Evaluate it through the full env-backed evaluator so the
                // spread inlines its now-concrete tuple instead of surfacing as a
                // single nested-tuple element.
                if crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved)
                    .is_some()
                {
                    resolved = self.evaluate_type_with_env(resolved);
                }
                if resolved != elem.type_id {
                    changed = true;
                }
                rebuilt.push(tsz_solver::TupleElement {
                    type_id: resolved,
                    ..elem
                });
            } else {
                rebuilt.push(elem);
            }
        }
        if !changed {
            return object_type;
        }
        // Rebuild via `factory.tuple()`, which inlines a now-concrete tuple
        // spread (`createNormalizedTupleType`). `readonly` is re-applied so the
        // receiver shape is unchanged apart from the splice.
        let factory = self.ctx.types.factory();
        let tuple = factory.tuple(rebuilt);
        if is_readonly {
            factory.readonly_type(tuple)
        } else {
            tuple
        }
    }

    pub(crate) fn recover_assignment_target_type_for_errored_element_index(
        &mut self,
        object_type: TypeId,
        index_expr: NodeIndex,
    ) -> Option<TypeId> {
        if matches!(
            object_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return None;
        }

        if let Some(index) = self
            .get_number_value_from_element_index(index_expr)
            .filter(|value| value.is_finite() && value.fract() == 0.0 && *value >= 0.0)
            .and_then(|value| self.get_numeric_index_from_number(value))
        {
            let recovered = self.get_element_access_type(object_type, TypeId::NUMBER, Some(index));
            if recovered != TypeId::ERROR {
                return Some(recovered);
            }
        }

        let candidate_indices: &[TypeId] = if self.is_array_like_type(object_type) {
            &[TypeId::NUMBER, TypeId::STRING]
        } else {
            &[TypeId::STRING, TypeId::NUMBER]
        };

        for &candidate_index in candidate_indices {
            if self.should_report_no_index_signature(object_type, candidate_index, None) {
                continue;
            }
            let recovered = self.get_element_access_type(object_type, candidate_index, None);
            if recovered != TypeId::ERROR {
                return Some(recovered);
            }
        }

        None
    }

    /// Resolve index signature value type when the index expression is error-typed.
    ///
    /// tsc resolves element access through index signatures even when the index
    /// expression evaluates to an error type (e.g., `ENUM1[undeclaredIdentifier]`).
    /// The error type is assignable to both `number` and `string`, so it can match
    /// any index signature. Returns the first matching index signature's value type.
    pub(crate) fn resolve_index_signature_for_error_index(
        &mut self,
        object_type: TypeId,
    ) -> Option<TypeId> {
        if matches!(
            object_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return None;
        }

        // Try number index first (for arrays, tuples, enums), then string
        for &candidate in &[TypeId::NUMBER, TypeId::STRING] {
            let result = self.get_element_access_type(object_type, candidate, None);
            if result != TypeId::ERROR {
                return Some(result);
            }
        }
        None
    }

    pub(crate) fn union_has_missing_concrete_element_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        let is_unique_symbol =
            crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, index_type)
                .is_some();
        let is_concrete_numeric = literal_index.is_some();
        if !is_unique_symbol && !is_concrete_numeric {
            return false;
        }

        // Tuple/array unions have their own out-of-bounds diagnostics and should
        // not be collapsed into TS7053 here.
        if members
            .iter()
            .any(|&member| self.is_array_like_type(member))
        {
            return false;
        }

        let solver_index_type = if let Some(index) = literal_index {
            self.ctx.types.literal_number(index as f64)
        } else if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            TypeId::NUMBER
        } else {
            index_type
        };

        members.iter().any(|&member| {
            let member_result = self.ctx.types.resolve_element_access_type(
                member,
                solver_index_type,
                literal_index,
            );
            member_result == TypeId::ERROR || member_result == TypeId::UNDEFINED
        })
    }

    pub(crate) fn union_has_no_common_numeric_index_surface(
        &self,
        object_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        if literal_index.is_none() {
            return false;
        }

        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        // Array and tuple unions have positional semantics and their own diagnostics.
        // Keep this helper focused on object index signatures.
        if members
            .iter()
            .any(|&member| self.is_array_like_type(member))
        {
            return false;
        }

        let mut all_have_string_surface = true;
        let mut all_have_number_surface = true;
        let mut saw_indexed_member = false;
        let mut surfaces = NumericIndexSurfaceWalk::new(self);

        for &member in &members {
            if !self.is_index_signature_only_member(member) {
                return false;
            }
            let Some((has_string, has_number)) = surfaces.surface(member) else {
                surfaces.trace_union_result(object_type, None);
                return false;
            };
            saw_indexed_member = true;
            all_have_string_surface &= has_string;
            all_have_number_surface &= has_number;
        }

        let result = saw_indexed_member && !all_have_string_surface && !all_have_number_surface;
        surfaces.trace_union_result(object_type, Some(result));
        result
    }

    fn is_index_signature_only_member(&self, object_type: TypeId) -> bool {
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
            .is_some_and(|shape| {
                shape.properties.is_empty()
                    && (shape.string_index.is_some() || shape.number_index.is_some())
            })
    }

    /// Check if a type is a union of tuples where ALL members are out of bounds
    /// for the given literal index. Used to emit TS2339 instead of TS2493.
    pub(crate) fn is_union_of_tuples_all_out_of_bounds(
        &self,
        object_type: TypeId,
        index: usize,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };
        let mut has_any_tuple = false;
        for member in &members {
            if let Some(elems) = crate::query_boundaries::type_computation::access::tuple_elements(
                self.ctx.types,
                *member,
            ) {
                has_any_tuple = true;
                let has_rest = elems.iter().any(|e| e.rest);
                if has_rest || index < elems.len() {
                    return false;
                }
            } else {
                return false;
            }
        }
        has_any_tuple
    }

    pub(crate) fn narrow_string_index_signature_rejects_index(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        // For Mapped types (e.g. `{ [K in \`on${string}\`]?: V }`), `object_shape_for_type`
        // returns None because the shape is not directly accessible. Evaluate the type first
        // to get the underlying ObjectWithIndex, then extract the string index.
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
                .or_else(|| {
                    let evaluated =
                        crate::query_boundaries::common::evaluate_type(self.ctx.types, object_type);
                    if evaluated != object_type {
                        crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            evaluated,
                        )
                    } else {
                        None
                    }
                });
        let Some(shape) = shape else {
            return false;
        };
        let Some(string_index) = shape.string_index.as_ref() else {
            return false;
        };
        if matches!(string_index.key_type, TypeId::STRING | TypeId::SYMBOL) {
            return false;
        }

        !self
            .indexed_access_key_space_relation_outcome(index_type, string_index.key_type)
            .related
    }

    /// Check if an index type is "generic" — i.e., it cannot be resolved to a
    /// concrete property key and must remain deferred in an `IndexAccess` type.
    ///
    /// Generic index types include: keyof T, type parameters, indexed access types,
    /// conditional types, and intersections containing any of the above
    /// (e.g., `keyof Boxified<T> & string` from for-in variable typing).
    pub(crate) fn is_generic_index_type(&self, index_type: TypeId) -> bool {
        crate::query_boundaries::key_constraints::is_generic_index_type(self.ctx.types, index_type)
    }

    /// Check if an intersection type contains a generic index member.
    ///
    /// For-in variables over generic types get type `keyof ExprType & string`,
    /// which is an intersection. This helper recursively checks whether any
    /// member of the intersection is a generic index type.
    pub(crate) fn intersection_has_generic_index(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::key_constraints::intersection_has_generic_index(
            self.ctx.types,
            type_id,
        )
    }

    /// Preserve deferred indexed-access identity for generic write targets whose
    /// semantic shape still depends on type parameters. Eagerly resolving these
    /// targets through property/index lookup destroys the canonical `Obj[K]`
    /// form and yields structural artifacts like `({ all: ... }[keyof T] & string) | undefined`
    /// in TS2322 messages.
    pub(crate) fn should_preserve_generic_indexed_write_target(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        let index_mentions_keyof =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type).is_some()
                || crate::query_boundaries::common::intersection_members(
                    self.ctx.types,
                    index_type,
                )
                .is_some_and(|members| {
                    members.iter().copied().any(|member| {
                        crate::query_boundaries::common::keyof_inner_type(self.ctx.types, member)
                            .is_some()
                    })
                });

        if !index_mentions_keyof
            || !crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                object_type,
            )
        {
            return false;
        }

        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, object_type)
            || crate::query_boundaries::common::is_generic_application(self.ctx.types, object_type)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, object_type)
        {
            return members.iter().copied().any(|member| {
                crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                    || crate::query_boundaries::common::is_generic_application(
                        self.ctx.types,
                        member,
                    )
                    || crate::query_boundaries::common::mapped_type_id(self.ctx.types, member)
                        .is_some()
            });
        }

        let resolved = self.resolve_lazy_type(object_type);
        crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved).is_some()
    }

    /// Choose the best receiver `TypeId` for a write-position indexed-access
    /// diagnostic, matching tsc's displayed form.
    ///
    /// - `raw_object_type` is the pre-evaluation form (may be an application
    ///   like `Errors<T>`); used as-is when available so the alias name is
    ///   preserved in the message.
    /// - `object_type` is the evaluated form; used as the fallback to strip
    ///   transparent empty-object members from intersections like `A & {}`.
    pub(crate) fn display_receiver_for_generic_indexed_write(
        &self,
        object_type: TypeId,
        raw_object_type: TypeId,
    ) -> TypeId {
        // When the caller already has the Application form (e.g. `Errors<T>`),
        // use it directly — the evaluated form would be the expanded structural
        // intersection, which produces a noisier diagnostic.
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, raw_object_type)
        {
            return raw_object_type;
        }

        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, object_type)
        else {
            return object_type;
        };

        // Strip transparent `{}` members; return the sole meaningful member or
        // the full intersection when two or more meaningful members are present.
        let mut it = members
            .into_iter()
            .filter(|&m| !crate::query_boundaries::common::is_empty_object_type(self.ctx.types, m));
        match (it.next(), it.next()) {
            (Some(single), None) => single,
            _ => object_type,
        }
    }

    /// Decide whether a write-context element access on a *concrete* receiver
    /// should keep the deferred `IndexAccess(receiver, index)` form instead
    /// of resolving through the receiver's index signature.
    ///
    /// This fires when the index expression is a generic key — `keyof T`
    /// (directly), an intersection containing `keyof T`, or a type parameter
    /// whose constraint reduces to `keyof T` — and `T` evaluates to the same
    /// type as the receiver. Preserving the deferred form lets the
    /// assignability gate report TS2322 with a `Receiver[K]` target display
    /// (matching tsc) and prevents the read-side `noUncheckedIndexedAccess`
    /// widening from making `undefined` writes silently typecheck.
    ///
    /// Companion to `should_preserve_generic_indexed_write_target`, which
    /// covers the dual case (generic receiver, keyof-mentioning index).
    pub(crate) fn concrete_receiver_write_target_should_preserve_indexed_access(
        &mut self,
        receiver: TypeId,
        index_type: TypeId,
    ) -> bool {
        let evaluated_receiver = self.evaluate_type_with_env(receiver);
        if evaluated_receiver == TypeId::ERROR {
            return false;
        }
        self.index_resolves_to_keyof_of_receiver(index_type, evaluated_receiver)
    }

    /// READ-side companion to `concrete_receiver_write_target_should_preserve_indexed_access`:
    /// `bag[k]` where `bag: Bag` (concrete, no type parameters) and `k: KSel
    /// extends keyof Bag` keeps the deferred `Bag[KSel]` identity as the
    /// expression's own type instead of eagerly resolving to the union of
    /// member value types (#17718 witness 2, oracle-verified via
    /// `scripts/conformance/oracle.sh` vs pinned typescript@7.0.2).
    ///
    /// Gated on two conditions beyond the write-side sibling:
    ///
    /// 1. The receiver carries no type parameter at all: `evaluate_type_with_env`
    ///    is a no-op on a generic application whose type argument stays
    ///    unresolved (e.g. `Mapped5<K>` indexed by `keyof Mapped5<K>` inside a
    ///    function generic over `K`), so the write-side predicate alone would
    ///    also fire there and pre-empt the mapped-type-aware
    ///    `remapped_mapped_index_access_result` resolution — which already
    ///    answers that case correctly — with an unevaluated shell the relation
    ///    can't recognize as satisfying its own filtered-key constraint (false
    ///    TS2322 on the `f5` witness in
    ///    `mapped_indexed_access_diagnostic_tests::remapped_mapped_type_constraint_indexed_access_diagnostics_match_tsc_surface`).
    /// 2. The receiver exposes no string/number index signature: a solver-level
    ///    `evaluate_type_with_env(IndexAccess(receiver, index))` has no access
    ///    to the checker's `noUncheckedIndexedAccess` compiler-option state, so
    ///    it can't append the `| undefined` marker tsc's own index-signature
    ///    read path adds under NUIA. Deferring for such a receiver silently
    ///    drops that marker and swallows a real TS2322 (caught by
    ///    `nuia_any_index_emits_ts2322_tests::nuia_generic_key_read_still_emits_ts2322_against_strict_slot`
    ///    — `{ a: string; b: string; [key: string]: string }` read through a
    ///    generic key). NUIA only ever affects index-signature-derived reads,
    ///    never a plain named-property `keyof` read, so excluding receivers
    ///    with an applicable index signature keeps witness 2's own target
    ///    (`interface Bag { one: number; two: number }`, no index signature)
    ///    unaffected.
    pub(crate) fn concrete_receiver_read_target_should_preserve_indexed_access(
        &mut self,
        receiver: TypeId,
        index_type: TypeId,
    ) -> bool {
        if crate::query_boundaries::containment_queries::contains_type_parameters(
            self.ctx.types,
            receiver,
        ) {
            return false;
        }
        let evaluated_receiver = self.evaluate_type_with_env(receiver);
        if evaluated_receiver == TypeId::ERROR {
            return false;
        }
        if crate::query_boundaries::index_signature::has_string_or_number_index_signature(
            self.ctx.types,
            evaluated_receiver,
        ) {
            return false;
        }
        self.index_resolves_to_keyof_of_receiver(index_type, evaluated_receiver)
    }

    fn index_resolves_to_keyof_of_receiver(
        &mut self,
        index_type: TypeId,
        evaluated_receiver: TypeId,
    ) -> bool {
        let types = self.ctx.types;
        crate::query_boundaries::key_constraints::index_resolves_to_keyof_of_receiver(
            types,
            index_type,
            evaluated_receiver,
            &mut |ty| self.evaluate_type_with_env(ty),
        )
    }

    /// Check if an index type is known to be a valid key for a given type parameter.
    ///
    /// Returns true for:
    /// - `keyof T` where T is the target type param (direct keyof)
    /// - `K extends keyof T` where T is the target type param (constrained key)
    pub(crate) fn is_valid_index_for_type_param(
        &mut self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> bool {
        let types = self.ctx.types;
        crate::query_boundaries::key_constraints::is_valid_index_for_type_param(
            types,
            index_type,
            type_param,
            &mut |ty| self.evaluate_type_with_env(ty),
        )
    }

    /// Reports TS7053 when a type parameter is indexed by *another* type
    /// parameter whose constraint is symbol-only and which is not a valid key of
    /// the receiver (e.g. `T[K]` where `K extends typeof sym` but `K` is unrelated
    /// to `keyof T`). Returns true when a diagnostic was emitted, so the caller
    /// can short-circuit the access to `error`.
    pub(crate) fn report_symbol_only_type_param_index_mismatch(
        &mut self,
        access_expression: NodeIndex,
        pre_resolution_object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let symbol_only_constraint =
            crate::query_boundaries::common::is_type_parameter(
                self.ctx.types,
                pre_resolution_object_type,
            ) && crate::query_boundaries::common::is_type_parameter(self.ctx.types, index_type)
                && crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .is_some_and(|constraint| {
                    crate::query_boundaries::key_constraints::is_symbol_only_key_constraint(
                        self.ctx.types,
                        constraint,
                    )
                })
                && !self.is_valid_index_for_type_param(index_type, pre_resolution_object_type);
        if !symbol_only_constraint {
            return false;
        }
        let index_type_str = self.format_type(index_type);
        let object_type_str = self.format_type(pre_resolution_object_type);
        let message = format_message(
            diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            &[&index_type_str, &object_type_str],
        );
        self.error_at_node(
            access_expression,
            &message,
            diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
        );
        true
    }

    /// The deferred indexed access `T[typeof sym]` when `pre_resolution_object`
    /// is a type parameter and `index_type` is a unique symbol that names a
    /// member of that parameter's constraint; `None` otherwise.
    ///
    /// `tsc` resolves a generic receiver to its constraint's apparent type before
    /// indexing, so `x[fooProp]` on `T extends Foo<number>` (where `Foo` declares
    /// `[fooProp]: T`) is the deferred indexed access `T[typeof fooProp]`, never a
    /// TS7053. The string-keyed property path already resolves the constraint for
    /// member lookup; the symbol-keyed path otherwise indexed the bare,
    /// unresolved type parameter — whose element access fails — and produced a
    /// false implicit-any element access. `resolve_type_for_property_access`
    /// intentionally leaves a type parameter unresolved, so this consults the
    /// constraint directly, following it transitively (`T extends U extends Foo`).
    pub(crate) fn type_param_unique_symbol_index_access(
        &mut self,
        pre_resolution_object: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        const MAX_CONSTRAINT_CHAIN_DEPTH: usize = 8;

        if !crate::query_boundaries::common::is_type_parameter(
            self.ctx.types,
            pre_resolution_object,
        ) || crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, index_type)
            .is_none()
        {
            return None;
        }

        // Advance `current` up the constraint chain until it reaches a concrete
        // (non-type-parameter) constraint, then decide indexability against that
        // constraint's apparent type. The depth bound also terminates a
        // pathological self-referential constraint cycle.
        let mut current = pre_resolution_object;
        let mut depth = 0;
        while depth < MAX_CONSTRAINT_CHAIN_DEPTH {
            depth += 1;
            let constraint = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                current,
            )?;
            if crate::query_boundaries::common::is_type_parameter(self.ctx.types, constraint) {
                current = constraint;
                continue;
            }
            let apparent = self.resolve_type_for_property_access(constraint);
            let member = self
                .ctx
                .types
                .resolve_element_access_type(apparent, index_type, None);
            if member == TypeId::ERROR || member == TypeId::UNDEFINED {
                return None;
            }
            return Some(
                self.ctx
                    .types
                    .factory()
                    .index_access(pre_resolution_object, index_type),
            );
        }
        None
    }

    pub(crate) fn constraint_keyof_write_target_for_type_param(
        &mut self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> Option<TypeId> {
        let constraint =
            crate::query_boundaries::common::type_param_info(self.ctx.types, type_param)?
                .constraint?;
        let keyof_inner =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)?;
        let evaluated_constraint = self.evaluate_type_with_env(constraint);
        if self.evaluate_type_with_env(keyof_inner) != evaluated_constraint {
            return None;
        }

        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, constraint)
                .or_else(|| {
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        evaluated_constraint,
                    )
                })?;
        let evaluated_index = self.evaluate_type_with_env(index_type);
        let members =
            crate::query_boundaries::common::union_members(self.ctx.types, evaluated_index)
                .map(|members| members.to_vec())
                .unwrap_or_else(|| vec![evaluated_index]);

        let mut write_targets = Vec::new();
        for member in members {
            let name =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, member)?;
            let prop = shape.properties.iter().find(|prop| prop.name == name)?;
            write_targets.push(prop.write_type);
        }

        match write_targets.as_slice() {
            [] => None,
            [only] => Some(*only),
            _ => {
                let intersection = self.ctx.types.factory().intersection(write_targets);
                Some(self.evaluate_type_with_env(intersection))
            }
        }
    }

    fn same_type_param_identity(&self, left: TypeId, right: TypeId) -> bool {
        crate::query_boundaries::key_constraints::same_type_param_identity(
            self.ctx.types,
            left,
            right,
        )
    }

    fn type_contains_same_type_param_identity(&mut self, ty: TypeId, type_param: TypeId) -> bool {
        let types = self.ctx.types;
        crate::query_boundaries::key_constraints::type_contains_same_type_param_identity(
            types,
            ty,
            type_param,
            &mut |candidate| self.evaluate_type_with_env(candidate),
        )
    }

    pub(crate) fn generic_index_mentions_transformed_current_type_param(
        &mut self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> bool {
        let types = self.ctx.types;
        crate::query_boundaries::key_constraints::generic_index_mentions_transformed_current_type_param(
            types,
            index_type,
            type_param,
            &mut |ty| self.evaluate_type_with_env(ty),
        )
    }

    /// `T[K]` where `K extends keyof S` and `S` is a generic key-remapping
    /// mapped type is a valid index: tsc keeps `keyof S` a deferred mapped index
    /// assignable to `keyof T` for any object type parameter `T`. See
    /// `key_constraints::indexed_access_is_deferred_generic_mapped_index`.
    pub(crate) fn indexed_access_is_deferred_generic_mapped_index(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        index_constraint: Option<TypeId>,
    ) -> bool {
        crate::query_boundaries::key_constraints::indexed_access_is_deferred_generic_mapped_index(
            self.ctx.types,
            &self.ctx,
            object_type,
            index_type,
            index_constraint,
        )
    }

    /// `T[K]` where `K`'s constraint is `keyof F<T>` (a transform of the object
    /// type parameter `T`) is still a valid index when the transformed key space
    /// is assignable to `keyof T`. Key-preserving transforms keep
    /// `keyof F<T> = keyof T` — a transparent alias `type Alias<T> = T`,
    /// `NonNullable<T>` (`T & {}`), `Readonly<T>`, `Partial<T>`. Only a
    /// key-*changing* transform (a key remap, `Pick`/`Omit`, or a *foreign* type
    /// parameter's keys such as `keyof U` with `U extends T`) produces keys
    /// outside `keyof T` and must keep emitting `TS2536`.
    ///
    /// The structural `generic_index_mentions_transformed_current_type_param`
    /// heuristic cannot by itself tell a key-preserving transform from a
    /// key-changing one (both merely *mention* `T`), so callers gate its
    /// `TS2536` on this relation-backed query. Returns true when the index key
    /// space genuinely indexes `object_param` and the diagnostic must be
    /// suppressed.
    ///
    /// Suppression is restricted to a **type-parameter** index `K extends keyof
    /// F<T>`. tsc allows an unconstrained generic `T` to be indexed by a *key
    /// parameter* whose constraint reduces to `keyof T`, but a **direct**
    /// transformed-keyof value such as `k: keyof (T & {})` still draws TS2536
    /// even though `keyof (T & {})` reduces to `keyof T` (see
    /// `conformance/types/unknown/unknownControlFlow.ts` `ff3`). Only the
    /// type-parameter form is suppressed here; a direct keyof expression keeps
    /// the heuristic's diagnostic.
    pub(crate) fn transformed_index_key_space_indexes_object(
        &mut self,
        index_type: TypeId,
        index_constraint: Option<TypeId>,
        object_param: TypeId,
    ) -> bool {
        if !crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type) {
            return false;
        }
        let constraint = index_constraint
            .or_else(|| {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
            })
            .unwrap_or(index_type);
        let index_key_space = self.evaluate_type_with_env(constraint);
        let object_key_space = self.ctx.types.evaluate_keyof(object_param);
        self.indexed_access_key_space_relation_outcome(index_key_space, object_key_space)
            .related
    }

    /// Return the type parameter source when `index_type` is `keyof S` or `K extends keyof S`
    /// for a type parameter `S` different from `type_param`.
    ///
    /// The caller can then decide whether indexing should be legal based on
    /// type-parameter relation direction (e.g. `U[keyof T]` is legal when `U extends T`,
    /// but `T[keyof U]` is not).
    pub(crate) fn keyof_source_type_param(
        &self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> Option<TypeId> {
        crate::query_boundaries::key_constraints::keyof_source_type_param(
            self.ctx.types,
            index_type,
            type_param,
        )
    }

    /// Check whether `object_param[keyof key_source]` is valid because the
    /// object's constraint is known to cover the other type parameter's keys.
    ///
    /// This accepts mutually-constrained generic pairs like:
    /// `InternalSpec extends Record<keyof PublicSpec, any> | undefined`
    /// used as `InternalSpec[keyof PublicSpec]`.
    pub(crate) fn object_constraint_covers_keyof_source(
        &mut self,
        object_param: TypeId,
        key_source: TypeId,
    ) -> bool {
        let Some(object_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, object_param)
        else {
            return false;
        };
        let Some(object_constraint) = object_info.constraint else {
            return false;
        };

        let object_constraint = self.evaluate_type_with_env(object_constraint);
        let object_constraint = self
            .split_nullish_type(object_constraint)
            .0
            .unwrap_or(object_constraint);

        let object_key_space = self.ctx.types.evaluate_keyof(object_constraint);
        let source_key_space = self.ctx.types.evaluate_keyof(key_source);
        self.indexed_access_key_space_relation_outcome(source_key_space, object_key_space)
            .related
    }

    pub(crate) fn should_report_union_generic_key_mismatch_ts2536(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };
        if members.len() < 2 || !self.is_generic_key_space(index_type) {
            return false;
        }

        // A key whose constraint is `keyof X` where `X` is STILL GENERIC after
        // evaluation (e.g. `P extends keyof Map[T]` with `T` a live type
        // parameter) cannot produce a member-wise TS2536 in tsc against a
        // CONCRETE receiver union: tsc keeps the matching receiver deferred as
        // `Map[T]`, and `keyof Map[T] <= keyof Map[T]` holds by identity. tsz
        // distributes a `Map[T]` receiver into its value-type union when the
        // distribution is lossless (see `keyof_constraint_distribution_is_lossy`
        // in the index-access evaluator), which would otherwise manufacture a
        // member-keyof mismatch tsc never checks (conformance
        // `intersectionsOfLargeUnions2` after predicate narrowing to
        // `U extends ElementTagNameMap[T]`).
        //
        // The suppression deliberately requires BOTH:
        // - generic `keyof` inner (a concrete `P extends keyof A` against an
        //   unrelated `A | B` receiver keeps reporting TS2536 exactly like
        //   tsc), and
        // - a fully concrete receiver union (a generic receiver like `T | U`
        //   indexed by `keyof (T & U)` is tsc's own deferred-relation failure,
        //   `keyofAndIndexedAccessErrors` f20, and must keep erroring).
        let types = self.ctx.types;
        let constraint_inner = crate::query_boundaries::common::keyof_inner_type(types, index_type)
            .or_else(|| {
                crate::query_boundaries::common::type_param_info(types, index_type)
                    .and_then(|info| info.constraint)
                    .and_then(|c| crate::query_boundaries::common::keyof_inner_type(types, c))
            });
        if let Some(inner) = constraint_inner {
            let eval_inner = self.evaluate_type_with_env(inner);
            if eval_inner == object_type
                || (crate::query_boundaries::common::contains_type_parameters(types, eval_inner)
                    && !crate::query_boundaries::common::contains_type_parameters(
                        types,
                        object_type,
                    ))
            {
                return false;
            }
        }

        members.iter().any(|&member| {
            let member_keyof = self.ctx.types.evaluate_keyof(member);
            !self
                .indexed_access_key_space_relation_outcome(index_type, member_keyof)
                .related
        })
    }

    pub(crate) fn is_generic_key_space(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::key_constraints::is_generic_key_space(self.ctx.types, type_id)
    }

    /// When `index_node` is a `symbol`-typed identifier, follow its import
    /// chain to the canonical binding and return
    /// `UniqueSymbol(SymbolRef(canonical_id))`.
    ///
    /// Returns `None` when the node is not a plain identifier.  The caller
    /// uses this to override the `index_type_for_access` so the solver's
    /// property-lookup loop can match `__unique_<id>` entries produced by
    /// `get_property_name_resolved` for non-unique symbol computed properties.
    pub(crate) fn nonunique_symbol_index_type(&self, index_node: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(index_node)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol_without_tracking(index_node)?;
        // Follow import aliases to the canonical declaration symbol.
        let mut current = sym_id;
        let mut hops = 0usize;
        while hops < 32 {
            hops += 1;
            let Some(next) = self.ctx.binder.resolve_import_symbol(current) else {
                break;
            };
            if next == current {
                break;
            }
            current = next;
        }
        Some(self.ctx.types.unique_symbol(SymbolRef(current.0)))
    }

    /// Whether at least one union member lacks the exact symbol member and also
    /// lacks a symbol-bearing index signature. Receiver index-signature keys are
    /// normalized before probing so `Record<PropertyKey, V>` still satisfies the
    /// symbol surface.
    pub(crate) fn union_member_missing_symbol_key(
        &mut self,
        object_type: TypeId,
        index_type_for_access: TypeId,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        for &member in &members {
            let member = self.resolve_receiver_index_signature_keys(member);
            if self.symbol_keyed_access_is_missing(member, index_type_for_access) {
                return true;
            }
        }
        false
    }

    /// Decide whether a wide-`symbol` element access made through a
    /// `symbol`-typed identifier lacks a matching member on `object_type`, i.e.
    /// whether tsc would report an implicit-any element access (TS7053/TS7015).
    ///
    /// `index_type_for_access` is the binding-identity `UniqueSymbol(ref)` the
    /// binder produced for the identifier. The key is satisfied when the type
    /// provides a `symbol` index signature (`{ [k: symbol]: V }`) or declares a
    /// member under that exact binding; otherwise it is missing.
    ///
    /// Array/tuple/string-like receivers can never declare a member keyed by such
    /// a binding — their element resolver leniently returns the element type for
    /// any symbol key — so they are always missing unless an explicit `symbol`
    /// index signature is present.
    pub(crate) fn symbol_keyed_access_is_missing(
        &mut self,
        object_type: TypeId,
        index_type_for_access: TypeId,
    ) -> bool {
        // A `symbol` index signature accepts any symbol key. Probe it with the wide
        // `symbol` type, which only resolves through such a signature.
        let via_symbol_index =
            self.ctx
                .types
                .resolve_element_access_type(object_type, TypeId::SYMBOL, None);
        if via_symbol_index != TypeId::UNDEFINED && via_symbol_index != TypeId::ERROR {
            return false;
        }

        if self.is_array_like_type(object_type) {
            return true;
        }

        !self.symbol_keyed_property_exists(object_type, index_type_for_access)
    }

    fn symbol_keyed_property_exists(
        &mut self,
        object_type: TypeId,
        index_type_for_access: TypeId,
    ) -> bool {
        let Some(sym_ref) = crate::query_boundaries::common::unique_symbol_ref(
            self.ctx.types,
            index_type_for_access,
        ) else {
            return false;
        };

        let unique_name = format!("__unique_{}", sym_ref.0);
        if crate::query_boundaries::common::find_property_by_str(
            self.ctx.types,
            object_type,
            &unique_name,
        )
        .is_some()
        {
            return true;
        }

        let Some(well_known_name) =
            self.well_known_symbol_name_by_type_identity(index_type_for_access)
        else {
            return false;
        };
        crate::query_boundaries::common::find_property_by_str(
            self.ctx.types,
            object_type,
            &well_known_name,
        )
        .is_some()
    }

    /// Resolve the well-known-symbol member name (`hasInstance`, `iterator`,
    /// etc.) that a `unique symbol`-typed element-access key denotes, by TYPE
    /// IDENTITY against the lib's own `SymbolConstructor` interface — never by
    /// decoding the key's `SymbolRef` as a binder `SymbolId`.
    ///
    /// A well-known symbol's `SymbolRef` is minted by the lowering layer from
    /// the `unique symbol` type-operator's NODE INDEX
    /// (`lower_type_operator`'s `SymbolRef(node_idx.0)`, used for a readonly
    /// property signature like `SymbolConstructor.iterator`) — not a
    /// `SymbolId` at all, and a per-binder-local raw id silently collides with
    /// an unrelated symbol when looked up directly (#16961). Even the DIRECT
    /// `Symbol.iterator` spelling only ever avoided this: it resolves through
    /// a separate, purely-syntactic shortcut
    /// (`computed_names::well_known_symbol_access_shape`) that requires the
    /// base identifier's literal text to be `"Symbol"` — an alias
    /// (`const S = Symbol`) bypasses that shortcut and falls through to here.
    ///
    /// Every read of `Symbol.<name>` — direct or aliased — resolves through
    /// ordinary property-type computation against the SAME merged
    /// `SymbolConstructor` type, so `<name>`'s member type is exactly
    /// `index_type` whenever `index_type` denotes that well-known symbol;
    /// comparing by `TypeId` sidesteps the identity-space mismatch entirely.
    pub(crate) fn well_known_symbol_name_by_type_identity(
        &mut self,
        index_type: TypeId,
    ) -> Option<String> {
        let ctor_sym_id = crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
            "SymbolConstructor",
            self.ctx.binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        )?;
        let ctor_type_raw = self.get_type_of_symbol(ctor_sym_id);
        let ctor_type = self.resolve_type_for_property_access(ctor_type_raw);
        // `SymbolConstructor` is declaration-merged across several lib files
        // (base symbol type, iterable, well-known-symbol augmentations); its
        // resolved type is a naked intersection of each augmentation's own
        // object shape rather than one flattened shape, so a direct
        // `get_object_shape`/`find_property_by_str` on `ctor_type` itself
        // finds nothing — the properties only appear one level down, on the
        // intersection's members.
        let mut props = Vec::new();
        collect_object_properties_for_identity_match(self.ctx.types, ctor_type, 0, 1, &mut props);
        props
            .iter()
            .find(|prop| prop.type_id == index_type)
            .map(|prop| format!("[Symbol.{}]", self.ctx.types.resolve_atom(prop.name)))
    }

    /// Whether an `ESSymbolLike` element-access key resolves *only* through a
    /// `string` index signature.
    ///
    /// tsc's `checkIndexedAccessIndexType` reports TS2538 ("Type 'X' cannot be
    /// used as an index type") — not the implicit-any TS7053 — when a `symbol`
    /// or `unique symbol` key indexes an object that (a) has a `string` index
    /// signature (so the access still yields that signature's value type,
    /// exactly as tsc does — no cascade), (b) has no `symbol` index signature
    /// that would accept any symbol key, and (c) is not matched by a declared
    /// symbol-keyed property. Arrays/tuples are excluded — a symbol key there
    /// takes the numeric-index diagnostic paths (TS7015/TS7053).
    ///
    /// The resolved value type is left to the caller: tsc keeps the `string`
    /// signature's value type, so this only governs *which diagnostic* is
    /// emitted, never the access result.
    pub(crate) fn symbol_key_resolves_via_string_index_only(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        index_type_for_access: TypeId,
    ) -> bool {
        // Cheap key discriminator first: the predicate can only ever be `true`
        // for an ESSymbolLike key, so bail before the object-shape traversals on
        // the common (string/number-keyed) element-access path. A bare `symbol`
        // is a binder-converted identifier (`index_type_for_access != index_type`
        // guards out un-converted widened well-known-symbol reads); a `unique
        // symbol` carries its binding identity.
        let is_wide_symbol = index_type == TypeId::SYMBOL && index_type_for_access != index_type;
        let is_unique_symbol =
            crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, index_type)
                .is_some();
        if !is_wide_symbol && !is_unique_symbol {
            return false;
        }

        // Arrays/tuples take the numeric-index diagnostic paths (TS7015/TS7053).
        if self.is_array_like_type(object_type) {
            return false;
        }
        let object_type = self.resolve_receiver_index_signature_keys(object_type);
        // Without a `string` index the object reports the implicit-any TS7053
        // family instead; TS2538 is reserved for the string-fallthrough case.
        // (This also excludes namespace / expando receivers, which have none.)
        if crate::query_boundaries::index_signature::resolve_string_index(
            self.ctx.types,
            object_type,
        )
        .is_none()
        {
            return false;
        }
        // A genuine `symbol` index signature accepts any symbol key — no error.
        if crate::query_boundaries::index_signature::has_symbol_index_signature(
            self.ctx.types,
            object_type,
        ) {
            return false;
        }

        // A bare `symbol` can never name a specific declared property, so a
        // string-index fallthrough is the only way such an access resolves.
        if is_wide_symbol {
            return true;
        }

        // A `unique symbol` key is legal when the object declares that exact
        // symbol-keyed property; only an unmatched key falls through to the
        // string index. Probe declared symbol properties directly instead of
        // asking generic element access, which may intentionally return the
        // string-index value type for diagnostic parity.
        !self.symbol_keyed_property_exists(object_type, index_type_for_access)
    }
}

/// Collect properties reachable from `type_id` for a by-`TypeId` identity
/// match, descending into intersection/union members up to `max_depth` —
/// mirrors `tsz_solver::type_queries::collect_property_name_atoms_for_diagnostics`'s
/// traversal shape but keeps each property's `TypeId`, not just its name.
fn collect_object_properties_for_identity_match(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<tsz_solver::PropertyInfo>,
) {
    if depth > max_depth {
        return;
    }
    use crate::query_boundaries::property_access::{
        PropertyTraversalKind, classify_property_traversal,
    };
    match classify_property_traversal(db, type_id) {
        PropertyTraversalKind::Object(shape) => {
            out.extend(shape.properties.iter().cloned());
        }
        PropertyTraversalKind::Callable(shape) => {
            out.extend(shape.properties.iter().cloned());
        }
        PropertyTraversalKind::Members(members) => {
            for member in members {
                collect_object_properties_for_identity_match(db, member, depth + 1, max_depth, out);
            }
        }
        PropertyTraversalKind::Other => {}
    }
}
