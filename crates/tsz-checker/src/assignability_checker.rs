//! Type assignability, subtyping, identity, and excess property checking.

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, AssignabilityQueryInputs, ExcessPropertiesKind,
    are_types_overlapping_with_env, check_assignable_gate_with_overrides,
    classify_for_assignability_eval, classify_for_excess_properties,
    is_assignable_bivariant_with_resolver, is_assignable_with_overrides,
    is_assignable_with_resolver, is_callable_type, is_redeclaration_identical_with_resolver,
    is_subtype_with_resolver, object_shape_for_type,
};
use crate::state::{CheckerOverrideProvider, CheckerState};
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::NarrowingContext;
use tsz_solver::RelationCacheKey;
use tsz_solver::TypeId;
use tsz_solver::visitor::{collect_lazy_def_ids, collect_type_queries};

// =============================================================================
// Assignability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn get_keyof_type_keys(
        &mut self,
        type_id: TypeId,
        db: &dyn tsz_solver::TypeDatabase,
    ) -> FxHashSet<Atom> {
        if let Some(keyof_type) = tsz_solver::type_queries::get_keyof_type(db, type_id)
            && let Some(key_type) =
                tsz_solver::type_queries::keyof_object_properties(db, keyof_type)
            && let Some(members) = tsz_solver::type_queries::get_union_members(db, key_type)
        {
            return members
                .into_iter()
                .filter_map(|m| {
                    if let Some(str_lit) = tsz_solver::type_queries::get_string_literal_value(db, m)
                    {
                        return Some(str_lit);
                    }
                    None
                })
                .collect();
        }
        FxHashSet::default()
    }

    fn skip_parenthesized_for_assignability(&self, idx: NodeIndex) -> NodeIndex {
        self.skip_parenthesized_expression(idx)
    }

    fn typeof_this_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        this_ref: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_this_target(left, this_ref) {
            return self.string_literal_text(right);
        }
        if self.is_typeof_this_target(right, this_ref) {
            return self.string_literal_text(left);
        }
        None
    }

    fn is_typeof_this_target(&self, expr: NodeIndex, this_ref: NodeIndex) -> bool {
        let expr = self.skip_parenthesized_for_assignability(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }
        let operand = self.skip_parenthesized_for_assignability(unary.operand);
        if operand == this_ref {
            return true;
        }
        self.ctx
            .arena
            .get(operand)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
    }

    fn string_literal_text(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.skip_parenthesized_for_assignability(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self
                .ctx
                .arena
                .get_literal(node)
                .map(|lit| lit.text.as_str());
        }
        None
    }

    fn narrow_this_from_enclosing_typeof_guard(
        &self,
        source_idx: NodeIndex,
        source: TypeId,
    ) -> TypeId {
        let is_this_source = self
            .ctx
            .arena
            .get(source_idx)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
        if !is_this_source {
            return source;
        }

        let mut current = source_idx;
        let mut depth = 0usize;
        while depth < 256 {
            depth += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(current) else {
                break;
            };
            if parent_node.kind != syntax_kind_ext::IF_STATEMENT {
                continue;
            }
            let Some(if_stmt) = self.ctx.arena.get_if_statement(parent_node) else {
                continue;
            };
            if !self.is_node_within(source_idx, if_stmt.then_statement) {
                continue;
            }
            let Some(cond_node) = self.ctx.arena.get(if_stmt.expression) else {
                continue;
            };
            if cond_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(cond_node) else {
                continue;
            };
            let is_equality = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16
                || bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
            if !is_equality {
                continue;
            }
            if let Some(type_name) =
                self.typeof_this_comparison_literal(bin.left, bin.right, source_idx)
            {
                return NarrowingContext::new(self.ctx.types).narrow_by_typeof(source, type_name);
            }
        }

        source
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for one type.
    pub(crate) fn ensure_relation_input_ready(&mut self, type_id: TypeId) {
        self.ensure_refs_resolved(type_id);
        self.ensure_application_symbols_resolved(type_id);
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for multiple types.
    pub(crate) fn ensure_relation_inputs_ready(&mut self, type_ids: &[TypeId]) {
        for &type_id in type_ids {
            self.ensure_relation_input_ready(type_id);
        }
    }

    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) const fn should_suppress_assignability_diagnostic(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        matches!(source, TypeId::ERROR | TypeId::ANY)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
    }

    /// Suppress assignability diagnostics when they are likely parser-recovery artifacts.
    ///
    /// In files with real syntax errors, we often get placeholder nodes and transient
    /// parse states. Checker-level semantics should not emit TS2322 there.
    fn should_suppress_assignability_for_parse_recovery(
        &self,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if !self.has_syntax_parse_errors() {
            return false;
        }

        if self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }

        self.is_parse_recovery_anchor_node(source_idx)
            || self.is_parse_recovery_anchor_node(diag_idx)
    }

    /// Detect nodes that look like parser-recovery artifacts.
    ///
    /// Recovery heuristics:
    /// - Missing-expression placeholders are currently identifiers with empty text.
    /// - Nodes that start very near a syntax parse error are considered unstable.
    /// - Nodes in subtrees that were marked as parse-recovery by the parser are suppressed.
    fn is_parse_recovery_anchor_node(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Missing-expression placeholders used by parser recovery.
        if self
            .ctx
            .arena
            .get_identifier_text(idx)
            .is_some_and(str::is_empty)
        {
            return true;
        }

        // Also suppress diagnostics anchored very near a syntax parse error.
        const DIAG_PARSE_DISTANCE: u32 = 16;
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            let before = err_pos.saturating_sub(DIAG_PARSE_DISTANCE);
            let after = err_pos.saturating_add(DIAG_PARSE_DISTANCE);
            if (node.pos >= before && node.pos <= after)
                || (node.end >= before && node.end <= after)
            {
                return true;
            }
        }

        let mut current = idx;
        let mut walk_guard = 0;
        while current.is_some() {
            walk_guard += 1;
            if walk_guard > 512 {
                break;
            }

            if let Some(current_node) = self.ctx.arena.get(current) {
                let flags = current_node.flags as u32;
                if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                    || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                {
                    return true;
                }
            } else {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Ensure all Ref types in a type are resolved and in the type environment.
    ///
    /// This is critical for intersection/union type assignability. When we have
    /// `type AB = A & B`, the intersection contains Ref(A) and Ref(B). Before we
    /// can check assignability against the intersection, we need to ensure A and B
    /// are resolved and in `type_env` so the subtype checker can resolve them.
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                {
                    worklist.push(result);
                }
            }
        }
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        let mut evaluated = match classify_for_assignability_eval(self.ctx.types, type_id) {
            AssignabilityEvalKind::Application => self.evaluate_type_with_resolution(type_id),
            AssignabilityEvalKind::NeedsEnvEval => self.evaluate_type_with_env(type_id),
            AssignabilityEvalKind::Resolved => type_id,
        };

        // Distribution pass: normalize compound types so mixed representations do not
        // leak into relation checks (for example, `Lazy(Class)` + resolved class object).
        if let Some(distributed) =
            tsz_solver::type_queries::map_compound_members(self.ctx.types, evaluated, |member| {
                self.evaluate_type_for_assignability(member)
            })
        {
            evaluated = distributed;
        }

        evaluated
    }

    // =========================================================================
    // Main Assignability Check
    // =========================================================================

    /// Substitute `ThisType` in a type with the enclosing class instance type.
    ///
    /// When inside a class body, `ThisType` represents the polymorphic `this` type
    /// (a type parameter bounded by the class). Since the `this` expression evaluates
    /// to the concrete class instance type, we must substitute `ThisType` → class
    /// instance type before assignability checks. This matches tsc's behavior where
    /// `return this`, `f(this)`, etc. succeed when the target type is `this`.
    fn substitute_this_type_if_needed(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types can't contain ThisType
        if type_id.is_intrinsic() {
            return type_id;
        }

        let needs_substitution = tsz_solver::is_this_type(self.ctx.types, type_id);

        if !needs_substitution {
            return type_id;
        }

        let Some(class_info) = &self.ctx.enclosing_class else {
            return type_id;
        };
        let class_idx = class_info.class_idx;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return type_id;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return type_id;
        };

        let instance_type = self.get_class_instance_type(class_idx, class_data);

        if tsz_solver::is_this_type(self.ctx.types, type_id) {
            instance_type
        } else {
            tsz_solver::substitute_this_type(self.ctx.types, type_id, instance_type)
        }
    }

    /// Check if source type is assignable to target type.
    ///
    /// This is the main entry point for assignability checking, used throughout
    /// the type system to validate assignments, function calls, returns, etc.
    /// Assignability is more permissive than subtyping.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_relation_input_ready(target);

        // Substitute `ThisType` in the target with the class instance type.
        // In tsc, `this` acts as a type parameter constrained to the class type.
        // The `this` expression evaluates to the concrete class instance type, so when
        // the target (return type, parameter type, etc.) contains `ThisType`, we need to
        // resolve it to the class instance type before the assignability check.
        let target = self.substitute_this_type_if_needed(target);

        // Pre-check: Function interface accepts any callable type.
        // Must check before evaluate_type_for_assignability resolves Lazy(DefId)
        // to ObjectShape, losing the DefId identity needed to recognize it as Function.
        {
            use tsz_solver::visitor::lazy_def_id;
            let is_function_target = lazy_def_id(self.ctx.types, target).is_some_and(|t_def| {
                self.ctx.type_env.try_borrow().ok().is_some_and(|env| {
                    env.is_boxed_def_id(t_def, tsz_solver::IntrinsicKind::Function)
                })
            });
            if is_function_target {
                let source_eval = self.evaluate_type_for_assignability(source);
                if is_callable_type(self.ctx.types, source_eval) {
                    return true;
                }
            }
        }

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable = !tsz_solver::visitor::contains_infer_types(self.ctx.types, source)
            && !tsz_solver::visitor::contains_infer_types(self.ctx.types, target);

        let flags = self.ctx.pack_relation_flags();

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        // Use CheckerContext as the resolver instead of TypeEnvironment
        // This enables access to symbol information for enum type detection
        let overrides = CheckerOverrideProvider::new(self, None);
        let result = is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        );

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to"
        );

        // Add keyof type checking logic
        if let Some(keyof_type) = tsz_solver::type_queries::get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) =
                tsz_solver::type_queries::get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys =
                tsz_solver::type_queries::get_allowed_keys(self.ctx.types, keyof_type);
            if !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    ///
    /// This keeps the same checker gateway (resolver + overrides + caches) as
    /// `is_assignable_to`, but forces the strict-function-types relation flag.
    pub fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        self.ensure_relation_input_ready(target);

        let target = self.substitute_this_type_if_needed(target);
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let is_cacheable = !tsz_solver::visitor::contains_infer_types(self.ctx.types, source)
            && !tsz_solver::visitor::contains_infer_types(self.ctx.types, target);
        let flags = self.ctx.pack_relation_flags() | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);
            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let overrides = CheckerOverrideProvider::new(self, None);
        let result = is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        );

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);
            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to_strict"
        );
        result
    }

    /// Check if `source` type is assignable to `target` type, resolving Ref types.
    ///
    /// Uses the provided `TypeEnvironment` to resolve type references.
    pub fn is_assignable_to_with_env(
        &self,
        source: TypeId,
        target: TypeId,
        env: &tsz_solver::TypeEnvironment,
    ) -> bool {
        let flags = self.ctx.pack_relation_flags();
        let overrides = CheckerOverrideProvider::new(self, Some(env));
        is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: env,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        )
    }

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables `strict_function_types` for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_relation_input_ready(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable = !tsz_solver::visitor::contains_infer_types(self.ctx.types, source)
            && !tsz_solver::visitor::contains_infer_types(self.ctx.types, target);

        // For bivariant checks, we strip the strict_function_types flag
        // so the cache key is distinct from regular assignability checks.
        let flags = self.ctx.pack_relation_flags() & !RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;

        if is_cacheable {
            // Note: For assignability checks, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        // Preserve existing behavior: bivariant path does not use checker overrides.
        let result = is_assignable_bivariant_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );

        // Cache the result for non-inference types
        // Use ORIGINAL types for cache key (not evaluated types)
        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to_bivariant"
        );
        result
    }

    /// Check if two types have any overlap (can ever be equal).
    ///
    /// Used for TS2367: "This condition will always return 'false'/'true' since
    /// the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if the types can potentially be equal, false if they can never
    /// have any common value.
    pub fn are_types_overlapping(&mut self, left: TypeId, right: TypeId) -> bool {
        // Ensure centralized relation preconditions before overlap check.
        self.ensure_relation_input_ready(left);
        self.ensure_relation_input_ready(right);

        let env = self.ctx.type_env.borrow();
        are_types_overlapping_with_env(
            self.ctx.types,
            &env,
            left,
            right,
            self.ctx.strict_null_checks(),
        )
    }

    // =========================================================================
    // Weak Union and Excess Property Checking
    // =========================================================================

    /// Check if we should skip the general assignability error for an object literal.
    /// Returns true if:
    /// 1. It's a weak union violation (TypeScript shows excess property error instead)
    /// 2. OR if the object literal has excess properties (TypeScript prioritizes TS2353 over TS2345/TS2322)
    pub(crate) fn should_skip_weak_union_error(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation first (using scoped borrow)
        if self.is_weak_union_violation(source, target) {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target, source_idx) {
            return false;
        }

        // There are excess properties. Check if all matching properties have compatible types.
        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return true;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(target_shape) = object_shape_for_type(self.ctx.types, resolved_target) else {
            return true;
        };

        let source_props = source_shape.properties.as_slice();
        let target_props = target_shape.properties.as_slice();

        // Check if any source property that exists in target has a wrong type
        for source_prop in source_props {
            if let Some(target_prop) = target_props.iter().find(|p| p.name == source_prop.name) {
                let source_prop_type = source_prop.type_id;
                let target_prop_type = target_prop.type_id;

                let effective_target_type = if target_prop.optional {
                    self.ctx
                        .types
                        .union(vec![target_prop_type, TypeId::UNDEFINED])
                } else {
                    target_prop_type
                };

                let is_assignable =
                    { self.is_assignable_to(source_prop_type, effective_target_type) };

                if !is_assignable {
                    return false;
                }
            }
        }

        true
    }

    /// Check assignability and emit the standard TS2322/TS2345-style diagnostic when needed.
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an assignability diagnostic was emitted.
    pub(crate) fn check_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at(source, target, source_idx, source_idx)
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics with independent
    /// source and diagnostic anchors.
    ///
    /// `source_idx` is used for weak-union/excess-property prioritization.
    /// `diag_idx` is where the assignability diagnostic is anchored.
    pub(crate) fn check_assignable_or_report_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }

        if tsz_solver::type_queries::is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) =
                tsz_solver::type_queries::get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type =
                tsz_solver::type_queries::get_keyof_type(self.ctx.types, target).unwrap();
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            if !allowed_keys.contains(&str_lit) {
                self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
                return false;
            }
        }

        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit a generic TS2322 diagnostic at `diag_idx`.
    ///
    /// This is used for call sites that intentionally avoid detailed reason rendering
    /// but still share centralized mismatch/suppression behavior.
    pub(crate) fn check_assignable_or_report_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(arg_idx, arg_idx) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, arg_idx)
        {
            return true;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }

    /// Returns true when an assignability mismatch should produce a diagnostic.
    ///
    /// This centralizes the standard "not assignable + not weak-union/excess-property
    /// suppression" decision so call sites emitting different diagnostics can share it.
    pub(crate) fn should_report_assignability_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Mirrors `should_report_assignability_mismatch` but uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to_bivariant(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable_to(left, right) && self.is_assignable_to(right, left)
    }

    /// Check if two types are comparable (overlap).
    ///
    /// Corresponds to TypeScript's `isTypeComparableTo`: returns true if the types
    /// have any overlap. TSC's comparableRelation differs from assignability:
    /// - For union sources: uses `someTypeRelatedToType` (ANY member suffices)
    /// - For union targets: also checks per-member overlap
    ///
    /// Used for switch/case comparability (TS2678), equality narrowing, etc.
    pub(crate) fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        // Fast path: direct bidirectional assignability
        if self.is_assignable_to(source, target) || self.is_assignable_to(target, source) {
            return true;
        }

        // TSC's comparable relation decomposes unions and checks if ANY member
        // is related to the other type. This handles cases like:
        // - `User.A | User.B` comparable to `User.A` (User.A member matches)
        // - `string & Brand` comparable to `"a"` (string member of intersection)
        use crate::query_boundaries::dispatch as query;

        // Decompose source union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, source) {
            for member in &members {
                if self.is_assignable_to(*member, target) || self.is_assignable_to(target, *member)
                {
                    return true;
                }
            }
        }

        // Decompose target union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, target) {
            for member in &members {
                if self.is_assignable_to(source, *member) || self.is_assignable_to(*member, source)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if source object literal has properties that don't exist in target.
    ///
    /// Uses TypeId-based freshness tracking (fresh object literals only).
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        _source_idx: NodeIndex,
    ) -> bool {
        use tsz_solver::freshness;
        // Only fresh object literals trigger excess property checking.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        if source_props.is_empty() {
            return false;
        }

        let resolved_target = self.resolve_type_for_property_access(target);

        match classify_for_excess_properties(self.ctx.types, resolved_target) {
            ExcessPropertiesKind::Object(shape_id) => {
                let target_shape = self.ctx.types.object_shape(shape_id);
                let target_props = target_shape.properties.as_slice();

                if target_props.is_empty() {
                    return false;
                }

                if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                    return false;
                }

                source_props
                    .iter()
                    .any(|source_prop| !target_props.iter().any(|p| p.name == source_prop.name))
            }
            ExcessPropertiesKind::ObjectWithIndex(_shape_id) => false,
            ExcessPropertiesKind::Union(members) => {
                let mut target_shapes = Vec::new();
                let mut matched_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        // If a union member has no object shape and is a type parameter
                        // or the `object` intrinsic, it accepts any properties, so EPC
                        // should not apply.
                        if tsz_solver::type_queries::is_type_parameter(
                            self.ctx.types,
                            resolved_member,
                        ) || resolved_member == TypeId::OBJECT
                        {
                            return false;
                        }
                        continue;
                    };

                    if shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                    {
                        return false;
                    }

                    target_shapes.push(shape.clone());

                    if self.ctx.types.is_subtype_of(source, member) {
                        matched_shapes.push(shape);
                    }
                }

                if target_shapes.is_empty() {
                    return false;
                }

                let effective_shapes = if matched_shapes.is_empty() {
                    target_shapes
                } else {
                    matched_shapes
                };

                source_props.iter().any(|source_prop| {
                    !effective_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::Intersection(members) => {
                let mut target_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        continue;
                    };

                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        return false;
                    }

                    target_shapes.push(shape);
                }

                if target_shapes.is_empty() {
                    return false;
                }

                source_props.iter().any(|source_prop| {
                    !target_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::NotObject => false,
        }
    }

    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        // Keep failure analysis on the same relation boundary as `is_assignable_to`
        // (CheckerContext resolver + checker overrides) so mismatch suppression and
        // diagnostic rendering observe identical compatibility semantics.
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source,
            target,
            flags: self.ctx.pack_relation_flags(),
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
        };
        let gate = check_assignable_gate_with_overrides(&inputs, &overrides, Some(&self.ctx), true);
        if gate.related {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            };
        }
        gate.analysis.unwrap_or(
            crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            },
        )
    }

    pub(crate) fn is_weak_union_violation(&mut self, source: TypeId, target: TypeId) -> bool {
        self.analyze_assignability_failure(source, target)
            .weak_union_violation
    }

    // =========================================================================
    // Subtype Checking
    // =========================================================================

    /// Check if `source` type is a subtype of `target` type.
    ///
    /// This is the main entry point for subtype checking, used for type compatibility
    /// throughout the type system. Subtyping is stricter than assignability.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // Fast path: identity check
        if source == target {
            return true;
        }

        // Keep subtype preconditions aligned with assignability to avoid
        // caching relation answers before lazy/application refs are prepared.
        self.ensure_relation_input_ready(source);
        self.ensure_relation_input_ready(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        let is_cacheable = !tsz_solver::visitor::contains_infer_types(self.ctx.types, source)
            && !tsz_solver::visitor::contains_infer_types(self.ctx.types, target);
        let flags = self.ctx.pack_relation_flags();

        if is_cacheable {
            // Note: For subtype checks in the checker, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_subtype_cache(cache_key) {
                return cached;
            }
        }

        let binder = self.ctx.binder;

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };
        let relation_result = {
            let env = self.ctx.type_env.borrow();
            is_subtype_with_resolver(
                self.ctx.types,
                &*env,
                source,
                target,
                flags,
                &self.ctx.inheritance_graph,
                Some(&is_class_fn),
            )
        };

        if relation_result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        let result = relation_result.is_related();

        // Cache the result for non-inference types
        if is_cacheable {
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            self.ctx.types.insert_subtype_cache(cache_key, result);
        }

        result
    }

    /// Check if source type is a subtype of target type with explicit environment.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &tsz_solver::TypeEnvironment,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // CRITICAL: Before checking subtypes, ensure all Ref types are resolved
        self.ensure_relation_input_ready(source);
        self.ensure_relation_input_ready(target);

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };

        let result = is_subtype_with_resolver(
            self.ctx.types,
            env,
            source,
            target,
            self.ctx.pack_relation_flags(),
            &self.ctx.inheritance_graph,
            Some(&is_class_fn),
        );

        if result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        result.is_related()
    }

    // =========================================================================
    // Type Identity and Compatibility
    // =========================================================================

    /// Check if two types are identical (same `TypeId`).
    pub fn are_types_identical(&self, type1: TypeId, type2: TypeId) -> bool {
        type1 == type2
    }

    /// Check if variable declaration types are compatible (used for multiple declarations).
    ///
    /// Delegates to the Solver's `CompatChecker` to determine if two types are
    /// compatible for redeclaration (TS2403). This moves enum comparison logic
    /// from Checker to Solver per Phase 5 Anti-Pattern 8.1 removal.
    pub(crate) fn are_var_decl_types_compatible(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> bool {
        // Ensure Ref/Lazy types are resolved before checking compatibility
        self.ensure_relation_input_ready(prev_type);
        self.ensure_relation_input_ready(current_type);

        let flags = self.ctx.pack_relation_flags();
        // Delegate to the Solver's Lawyer layer for redeclaration identity checking
        {
            let env = self.ctx.type_env.borrow();
            if is_redeclaration_identical_with_resolver(
                self.ctx.types,
                &*env,
                prev_type,
                current_type,
                flags,
                &self.ctx.inheritance_graph,
                self.ctx.sound_mode(),
            ) {
                return true;
            }
        }

        // TS2403 enum-object fallback: When one type is an enum type and the other is
        // a structural object type, TypeScript considers them compatible if the object
        // type matches the enum's "typeof" shape (its property object form).
        //
        // Example: `var e = E1` (inferred: Enum(E1_def, ...)) vs
        //          `var e: { readonly A: E1.A; readonly B: E1.B; readonly C: E1.C; [n: number]: string; }`
        //
        // In TSC, `var e = E1` gives `e` the type `typeof E1` which is structurally
        // equivalent to the explicit object literal annotation. Our solver stores the
        // inferred type as TypeData::Enum(def_id, member_union), so we need to
        // substitute the enum's object shape and retry the check.
        if let Some(result) = self.try_enum_object_redeclaration_check(prev_type, current_type) {
            return result;
        }

        false
    }

    /// Try checking redeclaration compatibility using enum object shape substitution.
    ///
    /// When one type is a nominal enum type (`TypeData::Enum`) and the other is a
    /// structural non-enum type, attempts to replace the enum type with its
    /// "typeof enum" object shape and retries the compatibility check.
    ///
    /// This handles: `var e = E1; var e: { readonly A: E1.A; ... }`
    /// where TSC considers both to be `typeof E1`.
    ///
    /// Returns Some(bool) if enum substitution was applicable, None otherwise.
    fn try_enum_object_redeclaration_check(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> Option<bool> {
        use tsz_binder::symbol_flags;
        use tsz_solver::visitor::enum_components;

        // Extract the SymbolId for a type if it's an enum TYPE (not a member).
        // Separated from the closure to allow reborrowing.
        fn get_enum_type_sym(
            type_id: TypeId,
            types: &dyn tsz_solver::TypeDatabase,
            ctx: &crate::context::CheckerContext<'_>,
        ) -> Option<tsz_binder::SymbolId> {
            let (def_id, _) = enum_components(types, type_id)?;
            let sym_id = ctx.def_to_symbol_id(def_id)?;
            let symbol = ctx.binder.get_symbol(sym_id)?;
            // Must be an enum but NOT an enum member (we want the enum type itself)
            if (symbol.flags & symbol_flags::ENUM) != 0
                && (symbol.flags & symbol_flags::ENUM_MEMBER) == 0
            {
                Some(sym_id)
            } else {
                None
            }
        }

        let prev_enum_sym = get_enum_type_sym(prev_type, self.ctx.types, &self.ctx);
        let current_enum_sym = get_enum_type_sym(current_type, self.ctx.types, &self.ctx);

        // Only proceed if exactly one side is an enum type and the other is NOT.
        // If both are enums, they were already handled (correctly rejected) by the
        // solver's enum_redeclaration_check.
        let (enum_sym, non_enum_type) = match (prev_enum_sym, current_enum_sym) {
            (Some(sym), None) => (sym, current_type),
            (None, Some(sym)) => (sym, prev_type),
            _ => return None, // Both enum or both non-enum: delegate to solver result
        };

        // Build the "typeof Enum" object shape using checker's enum_object_type helper.
        // This creates: { readonly A: E.A; readonly B: E.B; ... [n: number]: string }
        let enum_obj_type = self.enum_object_type(enum_sym)?;

        // Retry the check with the enum's object shape substituted in.
        // We check bidirectionally: enum_obj matches non_enum, AND non_enum matches enum_obj.
        let flags = self.ctx.pack_relation_flags();
        let env = self.ctx.type_env.borrow();
        let compatible = is_redeclaration_identical_with_resolver(
            self.ctx.types,
            &*env,
            enum_obj_type,
            non_enum_type,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );
        Some(compatible)
    }

    /// Check if source type is assignable to ANY member of a target union.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        let flags = self.ctx.pack_relation_flags();
        let env = self.ctx.type_env.borrow();

        for &target in targets {
            if is_assignable_with_resolver(
                self.ctx.types,
                &*env,
                source,
                target,
                flags,
                &self.ctx.inheritance_graph,
                self.ctx.sound_mode(),
            ) {
                return true;
            }
        }
        false
    }
}
