//! Accessor declaration validation (abstract consistency, setter parameters).

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
            getter: Option<(NodeIndex, bool)>, // (name_node_idx, is_abstract)
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
                let name_node_idx = accessor.name;

                // Get accessor name
                if let Some(name) = self.get_property_name(accessor.name) {
                    let pair = accessors.entry(name).or_default();
                    if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        pair.getter = Some((name_node_idx, is_abstract));
                    } else {
                        pair.setter = Some((name_node_idx, is_abstract));
                    }
                }
            }
        }

        // Check for abstract mismatch
        for (_, pair) in accessors {
            if let (
                Some((getter_name_idx, getter_abstract)),
                Some((setter_name_idx, setter_abstract)),
            ) = (pair.getter, pair.setter)
                && getter_abstract != setter_abstract
            {
                // Report error on accessor names (tsc points to the property name)
                self.error_at_node(
                    getter_name_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT,
                );
                self.error_at_node(
                    setter_name_idx,
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
        accessor_name: Option<NodeIndex>,
    ) {
        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for initializer (error 1052)
            // tsc points at the accessor name (e.g., `X` in `set X(v = 0)`)
            if param.initializer.is_some() {
                let error_node = accessor_name.unwrap_or(param.name);
                self.error_at_node(
                    error_node,
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
            self.maybe_report_implicit_any_parameter(param, has_jsdoc, 0);

            // Also report TS7032 on the setter name if the parameter implicitly has type any.
            if param.type_annotation.is_none()
                && !has_jsdoc
                && self.ctx.no_implicit_any()
                && let Some(name_idx) = accessor_name
            {
                let prop_name = self.parameter_name_for_error(name_idx);
                let message = format!(
                    "Property '{prop_name}' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                );
                self.error_at_node(
                        name_idx,
                        &message,
                        diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                    );
            }
        }
    }

    // =========================================================================
    // Getter/Setter Type Compatibility (TS2322)
    // =========================================================================

    /// Check that getter return types are assignable to setter parameter types.
    ///
    /// tsc 6.0 removed TS2380 (getter/setter must have same type), but still
    /// checks that the getter's return type is assignable to the setter's
    /// parameter type.  When it isn't, TS2322 is emitted at the getter name.
    ///
    /// Example:
    /// ```typescript
    /// class C {
    ///     get bar() { return 0; }      // TS2322: number not assignable to string
    ///     set bar(n: string) {}
    /// }
    /// ```
    pub(crate) fn check_accessor_type_compatibility(&mut self, members: &[NodeIndex]) {
        // (name_idx, body_idx, type_annotation) for getter;
        // (param_type_annotation, param_idx) for setter.
        type GetterInfo = Option<(NodeIndex, NodeIndex, NodeIndex)>;
        type SetterInfo = Option<(NodeIndex, NodeIndex)>;

        let mut pairs: FxHashMap<String, (GetterInfo, SetterInfo)> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                continue;
            };

            let Some(name) = self.get_property_name(accessor.name) else {
                continue;
            };

            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                // Store getter: (name_idx, body_idx, type_annotation)
                pairs.entry(name).or_default().0 =
                    Some((accessor.name, accessor.body, accessor.type_annotation));
            } else if node.kind == syntax_kind_ext::SET_ACCESSOR {
                // Store setter: (first param's type_annotation, first param idx)
                if let Some(&first_param) = accessor.parameters.nodes.first()
                    && let Some(param_node) = self.ctx.arena.get(first_param)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    pairs.entry(name).or_default().1 = Some((param.type_annotation, first_param));
                }
            }
        }

        // Check compatibility for each pair
        for (_name, (getter, setter)) in pairs {
            let Some((getter_name, getter_body, getter_type_ann)) = getter else {
                continue;
            };
            let Some((setter_type_ann, _setter_param)) = setter else {
                continue;
            };
            // Only check when the setter has an explicit type annotation.
            // When the setter has no annotation, its type is inferred from
            // the getter, so they're always compatible.
            if setter_type_ann == NodeIndex::NONE {
                continue;
            }
            // Skip abstract accessors — they have no body, so there's no
            // return statement to anchor the diagnostic to.  tsc doesn't
            // emit TS2322 for abstract getter/setter type mismatches.
            if getter_body == NodeIndex::NONE {
                continue;
            }

            // Get getter return type: prefer explicit annotation, else infer from body
            let getter_return_type = if getter_type_ann != NodeIndex::NONE {
                self.get_type_from_type_node(getter_type_ann)
            } else {
                self.infer_getter_return_type(getter_body)
            };

            // Get setter parameter type from its annotation
            let setter_param_type = self.get_type_from_type_node(setter_type_ann);

            // Check assignability: getter return type must be assignable to setter param type.
            // tsc emits TS2322 at the first return statement in the getter body.
            if getter_return_type != setter_param_type
                && getter_return_type != tsz_solver::TypeId::ANY
                && setter_param_type != tsz_solver::TypeId::ANY
            {
                // Find the first return statement in the getter body for diagnostic location
                let diag_idx = self
                    .find_first_return_in_block(getter_body)
                    .unwrap_or(getter_name);
                self.check_assignable_or_report_at(
                    getter_return_type,
                    setter_param_type,
                    diag_idx,
                    diag_idx,
                );
            }
        }
    }

    /// Find the first return statement inside a block body.
    fn find_first_return_in_block(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.ctx.arena.get(body_idx)?;
        let block = self.ctx.arena.get_block(body_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return Some(stmt_idx);
            }
        }
        None
    }
}
