//! Accessor Checking Module
//!
//! This module contains methods for validating accessor declarations.
//! It handles:
//! - Accessor abstract consistency (TS1044)
//! - Setter parameter validation (TS1052, TS1053, TS7006)
//!
//! This module extends `CheckerState` with accessor-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

// =============================================================================
// Accessor Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Accessor Abstract Consistency
    // =========================================================================

    /// Check that accessor pairs (get/set) have consistent abstract modifiers.
    ///
    /// Validates that if a getter and setter for the same property both exist,
    /// they must both be abstract or both be non-abstract.
    /// Emits TS1044 on mismatched accessor abstract modifiers.
    ///
    /// ## Parameters:
    /// - `members`: Slice of class member node indices to check
    ///
    /// ## Validation:
    /// - Collects all getters and setters by property name
    /// - Checks for abstract/non-abstract mismatches
    /// - Reports TS1044 on both accessors if mismatch found
    pub(crate) fn check_accessor_abstract_consistency(&mut self, members: &[NodeIndex]) {
        // Collect getters and setters by name
        #[derive(Default)]
        struct AccessorPair {
            getter: Option<(NodeIndex, bool)>, // (node_idx, is_abstract)
            setter: Option<(NodeIndex, bool)>,
        }

        let mut accessors: FxHashMap<String, AccessorPair> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if (node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
            {
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);

                // Get accessor name
                if let Some(name) = self.get_property_name(accessor.name) {
                    let pair = accessors.entry(name).or_default();
                    if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        pair.getter = Some((member_idx, is_abstract));
                    } else {
                        pair.setter = Some((member_idx, is_abstract));
                    }
                }
            }
        }

        // Check for abstract mismatch
        for (_, pair) in accessors {
            if let (Some((getter_idx, getter_abstract)), Some((setter_idx, setter_abstract))) =
                (pair.getter, pair.setter)
                && getter_abstract != setter_abstract
            {
                // Report error on both accessors
                self.error_at_node(
                    getter_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT,
                );
                self.error_at_node(
                    setter_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT,
                );
            }
        }
    }

    // =========================================================================
    // Setter Parameter Validation
    // =========================================================================

    /// Check setter parameter constraints (TS1052, TS1053, TS7006).
    ///
    /// This function validates that setter parameters comply with TypeScript rules:
    /// - TS1052: Setter parameters cannot have initializers
    /// - TS1053: Setter cannot have rest parameters
    /// - TS7006: Parameters without type annotations are implicitly 'any'
    ///
    /// When a setter has a paired getter, the setter parameter type is inferred
    /// from the getter return type, so TS7006 is suppressed.
    ///
    /// ## Error Messages:
    /// - TS1052: "A 'set' accessor parameter cannot have an initializer."
    /// - TS1053: "A 'set' accessor cannot have rest parameter."
    pub(crate) fn check_setter_parameter(
        &mut self,
        parameters: &[NodeIndex],
        has_paired_getter: bool,
        accessor_jsdoc: Option<&str>,
    ) {
        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for initializer (error 1052)
            if !param.initializer.is_none() {
                self.error_at_node(
                    param.name,
                    "A 'set' accessor parameter cannot have an initializer.",
                    diagnostic_codes::A_SET_ACCESSOR_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            }

            // Check for rest parameter (error 1053)
            if param.dot_dot_dot_token {
                self.error_at_node(
                    param_idx,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER,
                );
            }

            // Check for implicit any (error 7006)
            // When a setter has a paired getter, the parameter type is inferred from
            // the getter return type, so it's contextually typed (suppress TS7006).
            // Also check for inline JSDoc @param/@type annotations and accessor-level
            // JSDoc @param annotations (e.g., `/** @param {string} value */ set p(value)`).
            let has_jsdoc = has_paired_getter
                || self.param_has_inline_jsdoc_type(param_idx)
                || accessor_jsdoc.is_some_and(|jsdoc| {
                    let pname = self.parameter_name_for_error(param.name);
                    Self::jsdoc_has_param_type(jsdoc, &pname) || Self::jsdoc_has_type_tag(jsdoc)
                });
            self.maybe_report_implicit_any_parameter(param, has_jsdoc);
        }
    }

    // =========================================================================
    // Accessor Type Compatibility
    // =========================================================================

    /// Check compatibility between getter and setter types.
    ///
    /// TypeScript 5.1+ allows unrelated types for get/set accessors ONLY if both
    /// have explicit type annotations.
    ///
    /// If either lacks an annotation, the types must be consistent:
    /// - The return type of the getter must be assignable to the parameter type of the setter.
    ///
    /// ## Parameters:
    /// - `members`: Slice of class member node indices to check
    pub(crate) fn check_accessor_type_compatibility(&mut self, members: &[NodeIndex]) {
        use tsz_solver::TypeId;

        // Collect getters and setters by name
        #[derive(Default)]
        struct AccessorPair {
            getter: Option<NodeIndex>,
            setter: Option<NodeIndex>,
        }

        let mut accessors: FxHashMap<String, AccessorPair> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if (node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
            {
                // Get accessor name
                if let Some(name) = self.get_property_name(accessor.name) {
                    let pair = accessors.entry(name).or_default();
                    if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        pair.getter = Some(member_idx);
                    } else {
                        pair.setter = Some(member_idx);
                    }
                }
            }
        }

        // Check for type incompatibility
        for (_, pair) in accessors {
            if let (Some(getter_idx), Some(setter_idx)) = (pair.getter, pair.setter) {
                let Some(getter_node) = self.ctx.arena.get(getter_idx) else {
                    continue;
                };
                let Some(setter_node) = self.ctx.arena.get(setter_idx) else {
                    continue;
                };

                let Some(getter_accessor) = self.ctx.arena.get_accessor(getter_node) else {
                    continue;
                };
                let Some(setter_accessor) = self.ctx.arena.get_accessor(setter_node) else {
                    continue;
                };

                // Check for explicit annotations
                let getter_has_annotation = !getter_accessor.type_annotation.is_none();

                let setter_param_has_annotation =
                    if let Some(&p_idx) = setter_accessor.parameters.nodes.first() {
                        let Some(p_node) = self.ctx.arena.get(p_idx) else {
                            continue;
                        };
                        let Some(p) = self.ctx.arena.get_parameter(p_node) else {
                            continue;
                        };
                        !p.type_annotation.is_none()
                    } else {
                        false
                    };

                // If both have explicit annotations, they are allowed to differ (TS 5.1+)
                if getter_has_annotation && setter_param_has_annotation {
                    continue;
                }

                // Get types
                let getter_type_id = self.get_type_of_function(getter_idx);
                let setter_type_id = self.get_type_of_function(setter_idx);

                // Resolve shapes via Solver query boundaries (Phase 5 - Anti-Pattern removal)
                let getter_return_type = tsz_solver::type_queries::get_function_return_type(
                    self.ctx.types,
                    getter_type_id,
                );

                let setter_param_type = tsz_solver::type_queries::get_function_parameter_types(
                    self.ctx.types,
                    setter_type_id,
                )
                .first()
                .copied()
                .unwrap_or(TypeId::ERROR);

                if getter_return_type == TypeId::ERROR || setter_param_type == TypeId::ERROR {
                    continue;
                }

                // If not assignable, report error on the getter name
                let error_node = if !getter_accessor.name.is_none() {
                    getter_accessor.name
                } else {
                    getter_idx
                };

                self.check_assignable_or_report(getter_return_type, setter_param_type, error_node);
            }
        }
    }
}
