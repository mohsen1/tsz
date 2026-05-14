//! Element access helper methods: index type validation, generic index detection,
//! numeric index extraction, and union/tuple diagnostic support.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::{CheckerState, EnumKind};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
    pub(crate) fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
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
            index_type
        };

        self.ctx
            .types
            .resolve_element_access_type(object_type, solver_index_type, literal_index)
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

        for &member in &members {
            let Some((has_string, has_number)) = self.numeric_index_surfaces(member) else {
                return false;
            };
            saw_indexed_member = true;
            all_have_string_surface &= has_string;
            all_have_number_surface &= has_number;
        }

        saw_indexed_member && !all_have_string_surface && !all_have_number_surface
    }

    fn numeric_index_surfaces(&self, object_type: TypeId) -> Option<(bool, bool)> {
        match query::classify_element_indexable(self.ctx.types, object_type) {
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => Some((has_string, has_number)),
            query::ElementIndexableKind::Intersection(members) => {
                let mut has_string = false;
                let mut has_number = false;
                for member in members {
                    if let Some((member_string, member_number)) =
                        self.numeric_index_surfaces(member)
                    {
                        has_string |= member_string;
                        has_number |= member_number;
                    }
                }
                (has_string || has_number).then_some((has_string, has_number))
            }
            query::ElementIndexableKind::Union(members) => {
                let mut all_have_string_surface = true;
                let mut all_have_number_surface = true;
                let mut saw_indexed_member = false;

                for member in members {
                    let (has_string, has_number) = self.numeric_index_surfaces(member)?;
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
        }
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
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
        else {
            return false;
        };
        let Some(string_index) = shape.string_index.as_ref() else {
            return false;
        };
        if matches!(string_index.key_type, TypeId::STRING | TypeId::SYMBOL) {
            return false;
        }

        !self.is_assignable_to(index_type, string_index.key_type)
    }

    /// Check if an index type is "generic" — i.e., it cannot be resolved to a
    /// concrete property key and must remain deferred in an `IndexAccess` type.
    ///
    /// Generic index types include: keyof T, type parameters, indexed access types,
    /// conditional types, and intersections containing any of the above
    /// (e.g., `keyof Boxified<T> & string` from for-in variable typing).
    pub(crate) fn is_generic_index_type(&self, index_type: TypeId) -> bool {
        crate::query_boundaries::common::is_type_parameter(self.ctx.types, index_type)
            || crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
                .is_some()
            || crate::query_boundaries::common::is_index_access_type(self.ctx.types, index_type)
            || crate::query_boundaries::common::is_conditional_type(self.ctx.types, index_type)
            || crate::query_boundaries::common::is_generic_application(self.ctx.types, index_type)
            || self.intersection_has_generic_index(index_type)
    }

    /// Check if an intersection type contains a generic index member.
    ///
    /// For-in variables over generic types get type `keyof ExprType & string`,
    /// which is an intersection. This helper recursively checks whether any
    /// member of the intersection is a generic index type.
    pub(crate) fn intersection_has_generic_index(&self, type_id: TypeId) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            members.iter().any(|&m| self.is_generic_index_type(m))
        } else {
            false
        }
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

    fn index_resolves_to_keyof_of_receiver(
        &mut self,
        index_type: TypeId,
        evaluated_receiver: TypeId,
    ) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members.iter().copied().any(|member| {
                self.index_resolves_to_keyof_of_receiver(member, evaluated_receiver)
            });
        }
        if let Some(inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
        {
            return self.evaluate_type_with_env(inner) == evaluated_receiver;
        }
        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint)
        {
            return self.evaluate_type_with_env(inner) == evaluated_receiver;
        }
        false
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
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members
                .iter()
                .copied()
                .any(|member| self.is_valid_index_for_type_param(member, type_param));
        }
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, index_type) {
            let evaluated = self.evaluate_type_with_env(index_type);
            if evaluated != index_type && evaluated != TypeId::ERROR {
                return self.is_valid_index_for_type_param(evaluated, type_param);
            }
        }
        // Direct keyof T
        if let Some(keyof_inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
        {
            return self.same_type_param_identity(keyof_inner, type_param)
                || crate::query_boundaries::common::type_param_info(self.ctx.types, type_param)
                    .and_then(|param| param.constraint)
                    .is_some_and(|constraint| {
                        self.same_type_param_identity(constraint, keyof_inner)
                    });
        }
        // K extends keyof T (type param whose constraint is keyof T)
        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint)
        {
            return self.same_type_param_identity(keyof_inner, type_param)
                || crate::query_boundaries::common::type_param_info(self.ctx.types, type_param)
                    .and_then(|param| param.constraint)
                    .is_some_and(|constraint| {
                        self.same_type_param_identity(constraint, keyof_inner)
                    });
        }
        false
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
        left == right
            || crate::query_boundaries::common::type_param_info(self.ctx.types, left)
                .zip(crate::query_boundaries::common::type_param_info(
                    self.ctx.types,
                    right,
                ))
                .is_some_and(|(l, r)| l.name == r.name)
    }

    fn type_contains_same_type_param_identity(&mut self, ty: TypeId, type_param: TypeId) -> bool {
        if self.same_type_param_identity(ty, type_param) {
            return true;
        }

        if let Some(inner) = crate::query_boundaries::common::keyof_inner_type(self.ctx.types, ty)
            && self.type_contains_same_type_param_identity(inner, type_param)
        {
            return true;
        }

        if let Some((object_type, index_type)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, ty)
            && (self.type_contains_same_type_param_identity(object_type, type_param)
                || self.type_contains_same_type_param_identity(index_type, type_param))
        {
            return true;
        }

        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty)
            && members
                .iter()
                .any(|&member| self.type_contains_same_type_param_identity(member, type_param))
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, ty)
            && members
                .iter()
                .any(|&member| self.type_contains_same_type_param_identity(member, type_param))
        {
            return true;
        }

        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
            && let Some(constraint) = param_info.constraint
            && self.type_contains_same_type_param_identity(constraint, type_param)
        {
            return true;
        }

        if crate::query_boundaries::common::is_generic_application(self.ctx.types, ty) {
            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated != ty
                && evaluated != TypeId::ERROR
                && self.type_contains_same_type_param_identity(evaluated, type_param)
            {
                return true;
            }
        }

        false
    }

    pub(crate) fn generic_index_mentions_transformed_current_type_param(
        &mut self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> bool {
        if let Some(keyof_inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
        {
            return !self.same_type_param_identity(keyof_inner, type_param)
                && self.type_contains_same_type_param_identity(keyof_inner, type_param);
        }

        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
        {
            return self
                .generic_index_mentions_transformed_current_type_param(constraint, type_param);
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, index_type)
        {
            return members.iter().any(|&member| {
                self.generic_index_mentions_transformed_current_type_param(member, type_param)
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members.iter().any(|&member| {
                self.generic_index_mentions_transformed_current_type_param(member, type_param)
            });
        }

        if crate::query_boundaries::common::is_generic_application(self.ctx.types, index_type) {
            let evaluated = self.evaluate_type_with_env(index_type);
            if evaluated != index_type && evaluated != TypeId::ERROR {
                return self
                    .generic_index_mentions_transformed_current_type_param(evaluated, type_param);
            }
        }

        false
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
        if let Some(keyof_inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
            && crate::query_boundaries::common::is_type_parameter(self.ctx.types, keyof_inner)
            && keyof_inner != type_param
        {
            return Some(keyof_inner);
        }

        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint)
            && crate::query_boundaries::common::is_type_parameter(self.ctx.types, keyof_inner)
            && keyof_inner != type_param
        {
            return Some(keyof_inner);
        }

        None
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
        self.is_assignable_to(source_key_space, object_key_space)
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

        members.iter().any(|&member| {
            let member_keyof = self.ctx.types.evaluate_keyof(member);
            !self.is_assignable_to(index_type, member_keyof)
        })
    }

    pub(crate) fn is_generic_key_space(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, type_id).is_some()
            || crate::query_boundaries::common::is_type_parameter(self.ctx.types, type_id)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_generic_key_space(member));
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_generic_key_space(member));
        }

        false
    }
}
