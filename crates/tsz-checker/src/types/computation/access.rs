include!("access_large_methods/get_type_of_element_access_with_request_13_2.rs");

use crate::context::TypingRequest;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::property_access_chain_text_in_arena;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[path = "access/symbol_constructor_index.rs"]
mod symbol_constructor_index;

pub(crate) fn is_optional_chain(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                // Parentheses break the chain (fall through to `_ => false`),
                // so `(o?.a).b` is not a continuation.
                access.question_dot_token || is_optional_chain(arena, access.expression)
            } else {
                false
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            if node.is_optional_chain() {
                return true;
            }
            if let Some(call) = arena.get_call_expr(node) {
                is_optional_chain(arena, call.expression)
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(crate) fn optional_chain_root(arena: &NodeArena, idx: NodeIndex) -> NodeIndex {
    let Some(node) = arena.get(idx) else {
        return idx;
    };
    match node.kind {
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                return optional_chain_root(arena, call.expression);
            }
            idx
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                return optional_chain_root(arena, access.expression);
            }
            idx
        }
        _ => idx,
    }
}

impl<'a> CheckerState<'a> {
    /// Get the type of an element access expression (e.g., arr[0], obj["prop"]).
    ///
    /// Handles element access with optional chaining, index signatures,
    /// and nullish coalescing.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_element_access_with_request(idx, &TypingRequest::NONE)
    }

    /// Resolve the member-access result type when the (resolved) object type is
    /// `unknown`, under `strictNullChecks`.
    ///
    /// `tsc` forbids accessing a member of a value of type `unknown` — by name
    /// (`x.p`), by index (`x[k]`), or through an optional chain (`x?.p` / `x?.[k]`)
    /// — so under `strictNullChecks` we emit the diagnostic and return `Some`:
    /// `TS18046` (`'x' is of type 'unknown'.`) when the base expression has a
    /// printable name, otherwise the object form `TS2571` (`Object is of type
    /// 'unknown'.`), returning `ERROR` to stop cascading diagnostics. When
    /// `strictNullChecks` is off, `unknown` behaves like `any`; we return `None` so
    /// each caller can apply its own non-strict fallback (index-signature handling
    /// for element access, `error_property_not_exist_at` for property access).
    ///
    /// This is the single decision gate for the unknown-object access result,
    /// shared by the element-access `literal_string`/`literal_index` arms and the
    /// property-access path, so the `TS2571`/`TS18046` choice is not re-derived
    /// independently in each place.
    pub(crate) fn unknown_object_access_result(&mut self, base_expr: NodeIndex) -> Option<TypeId> {
        if !self.ctx.compiler_options.strict_null_checks {
            return None;
        }
        if self.error_is_of_type_unknown(base_expr) {
            Some(TypeId::ERROR)
        } else {
            Some(TypeId::ANY)
        }
    }

    __tsz_split_access_get_type_of_element_access_with_request_13_2!();
}

#[cfg(test)]
#[path = "tests/access.rs"]
mod tests;
