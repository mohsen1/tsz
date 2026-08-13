//! TS7031 for a destructuring `var`/`let`/`const` declaration whose
//! initializer is a fresh array literal with a direct `null`/`undefined`
//! widening leaf at a leaf binding's position — split out of
//! `implicit_any_checks.rs` to stay under the checker file-size guard.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Emit TS7031 for array-binding leaves of a destructuring `var`/`let`/
    /// `const` declaration whose initializer is a fresh array literal, at
    /// each position where the literal's own element is a genuine
    /// `null`/`undefined` widening leaf (bare keyword, elided hole, or the
    /// global `undefined` identifier — see
    /// [`CheckerState::expr_is_direct_nullish_widening_leaf`]).
    ///
    /// Mirrors the value-side per-slot widen in `array_literal.rs`'s
    /// tuple-context element loop: under `noImplicitAny` with
    /// `strictNullChecks` off, that slot's inferred type is `any`, so an
    /// unannotated leaf binding with no own default at that position
    /// implicitly has that `any` type. A leaf with its own default
    /// initializer (`b = 5`) takes its type from the default instead, so it
    /// is skipped; a rest element's type is a tuple of the remaining literal
    /// elements, a distinct shape this function does not attempt to name.
    /// Nested binding patterns recurse into the corresponding nested
    /// array-literal source element, when there is one.
    pub(crate) fn emit_implicit_any_for_var_destructuring_nullish_array_initializer(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        if self.ctx.strict_null_checks() {
            return;
        }
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(init_literal) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };

        for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
            let Some(&source_elem_idx) = init_literal.elements.nodes.get(index) else {
                // Index beyond the literal's own length: a different
                // (pre-existing) path owns out-of-bounds destructuring.
                break;
            };
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if binding_elem.dot_dot_dot_token || binding_elem.initializer.is_some() {
                continue;
            }
            let name_is_pattern = self
                .ctx
                .arena
                .get(binding_elem.name)
                .map(|n| {
                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                })
                .unwrap_or(false);
            if name_is_pattern {
                if source_elem_idx.is_some() {
                    self.emit_implicit_any_for_var_destructuring_nullish_array_initializer(
                        binding_elem.name,
                        source_elem_idx,
                    );
                }
                continue;
            }
            if !self.expr_is_direct_nullish_widening_leaf(source_elem_idx) {
                continue;
            }
            let binding_name = self.parameter_name_for_error(binding_elem.name);
            self.error_at_node_msg(
                binding_elem.name,
                diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                &[&binding_name, "any"],
            );
        }
    }
}
