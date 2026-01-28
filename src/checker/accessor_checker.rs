//! Accessor Checking Module
//!
//! This module contains methods for validating accessor declarations.
//! It handles:
//! - Accessor abstract consistency (TS1044)
//! - Setter parameter validation (TS1052, TS1053, TS7006)
//!
//! This module extends CheckerState with accessor-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::diagnostic_codes;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use std::collections::HashMap;

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

        let mut accessors: HashMap<String, AccessorPair> = HashMap::new();

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
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
                );
                self.error_at_node(
                    setter_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
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
    /// ## Error Messages:
    /// - TS1052: "A 'set' accessor parameter cannot have an initializer."
    /// - TS1053: "A 'set' accessor cannot have rest parameter."
    pub(crate) fn check_setter_parameter(&mut self, parameters: &[NodeIndex]) {
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
                    diagnostic_codes::SETTER_PARAMETER_CANNOT_HAVE_INITIALIZER,
                );
            }

            // Check for rest parameter (error 1053)
            if param.dot_dot_dot_token {
                self.error_at_node(
                    param_idx,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::SETTER_CANNOT_HAVE_REST_PARAMETER,
                );
            }

            // Check for implicit any (error 7006)
            // Setter parameters without type annotation implicitly have 'any' type
            self.maybe_report_implicit_any_parameter(param, false);
        }
    }
}
