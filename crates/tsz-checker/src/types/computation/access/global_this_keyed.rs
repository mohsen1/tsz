use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use crate::types_domain::queries::core::GlobalReceiver;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(super) enum GlobalThisAccessKind {
    Element,
    Other,
}

pub(super) enum GlobalThisKeyStatus {
    NoLiteralStringKey,
    HasLiteralStringKey,
}

pub(super) enum GlobalThisReceiverStatus {
    GlobalThisLike,
    Other,
}

pub(super) enum GlobalThisFlowMode {
    SkipFlowNarrowing,
    ApplyFlowNarrowing,
}

pub(super) struct GlobalThisStringLikeElementAccess {
    pub(super) idx: NodeIndex,
    pub(super) access_kind: GlobalThisAccessKind,
    pub(super) access_expression: NodeIndex,
    pub(super) key_status: GlobalThisKeyStatus,
    pub(super) index_type: TypeId,
    pub(super) receiver_status: GlobalThisReceiverStatus,
    pub(super) flow_mode: GlobalThisFlowMode,
}

pub(super) struct GlobalThisLiteralKeyAccess<'a> {
    pub(super) idx: NodeIndex,
    pub(super) access_kind: GlobalThisAccessKind,
    pub(super) access_expression: NodeIndex,
    pub(super) key_node: NodeIndex,
    pub(super) name: &'a str,
    pub(super) is_global_this_like_receiver: bool,
    pub(super) receiver_status: GlobalThisReceiverStatus,
    pub(super) is_declared_window_global_this: bool,
    pub(super) flow_mode: GlobalThisFlowMode,
}

pub(super) struct GlobalThisWindowKeyUnionAccess {
    pub(super) idx: NodeIndex,
    pub(super) access_kind: GlobalThisAccessKind,
    pub(super) key_status: GlobalThisKeyStatus,
    pub(super) index_type: TypeId,
    pub(super) is_global_this_like_receiver: bool,
    pub(super) receiver_status: GlobalThisReceiverStatus,
    pub(super) is_declared_window_global_this: bool,
    pub(super) flow_mode: GlobalThisFlowMode,
}

impl<'a> CheckerState<'a> {
    pub(super) fn try_global_this_literal_key_access(
        &mut self,
        request: GlobalThisLiteralKeyAccess<'_>,
    ) -> Option<TypeId> {
        let is_this_global = matches!(
            request.receiver_status,
            GlobalThisReceiverStatus::GlobalThisLike
        );
        if !request.is_global_this_like_receiver
            && !is_this_global
            && !request.is_declared_window_global_this
        {
            return None;
        }

        let targets_global_this =
            self.is_global_this_expression(request.access_expression) || is_this_global;
        if request.is_declared_window_global_this
            && matches!(request.access_kind, GlobalThisAccessKind::Element)
        {
            return Some(self.resolve_declared_window_literal_key(request));
        }

        // For element access (`globalThis["y"]`), tsc reports TS2339 at the
        // full expression span. For property access (`globalThis.y`), at the
        // property name.
        let error_node = if matches!(request.access_kind, GlobalThisAccessKind::Element) {
            request.idx
        } else {
            request.key_node
        };
        let property_type = self.resolve_global_this_property_type(
            request.name,
            error_node,
            targets_global_this && !request.is_declared_window_global_this,
            GlobalReceiver::from_targets_global_this(targets_global_this),
        );
        if property_type == TypeId::ERROR {
            return Some(TypeId::ERROR);
        }

        // TS7053: When noImplicitAny is enabled and the access target is
        // `typeof globalThis` (via `this` resolving to global, or a direct
        // `globalThis["x"]`), and the property is not found, emit the
        // can't-index diagnostic. A JS file's bare `this["y"] = value` (an
        // `=` assignment target, not a compound assignment or `++`/`--`,
        // which read the missing property first) is tsc's "declare a new
        // global property" leniency and stays silent; every other shape —
        // reads, compound writes, and a read used as the base of a further
        // write like `this["y"]["z"] = 1` — still reports.
        if targets_global_this
            && property_type == TypeId::ANY
            && self.ctx.no_implicit_any()
            && matches!(request.access_kind, GlobalThisAccessKind::Element)
            && !(self.is_js_file() && self.is_bare_equals_assignment_target(request.idx))
        {
            let index_str = format!("\"{}\"", request.name);
            self.error_at_node(
                request.idx,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                    &[&index_str, "typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
            );
        }

        Some(self.apply_global_this_flow_mode(request.idx, property_type, request.flow_mode))
    }

    pub(super) fn try_global_this_window_key_union_access(
        &mut self,
        request: GlobalThisWindowKeyUnionAccess,
    ) -> Option<TypeId> {
        // Handle `window[k]` where `k` is a typed identifier whose type is a
        // single string literal or a union of string literals, e.g.
        // `const k: 'resizeTo' | 'resizeBy'`. The literal-string branch only
        // fires when the AST argument is a string literal node, so variable
        // indices fall through to the general union-keys path. Resolving each
        // key directly against the `Window` lib type preserves callable shapes
        // for assignment targets and callback contextual typing.
        if !matches!(request.key_status, GlobalThisKeyStatus::NoLiteralStringKey)
            || !matches!(request.access_kind, GlobalThisAccessKind::Element)
            || !(request.is_global_this_like_receiver
                || matches!(
                    request.receiver_status,
                    GlobalThisReceiverStatus::GlobalThisLike
                )
                || request.is_declared_window_global_this)
        {
            return None;
        }

        let (string_keys, number_keys) =
            self.get_literal_key_union_from_type(request.index_type)?;
        if string_keys.is_empty() || !number_keys.is_empty() {
            return None;
        }
        let window_type = self.resolve_lib_type_by_name("Window")?;
        let mut resolved_types: Vec<TypeId> = Vec::with_capacity(string_keys.len());
        for key_atom in &string_keys {
            let prop_result = crate::query_boundaries::property_access::resolve_property_access(
                self.ctx.types,
                window_type,
                *key_atom,
            );
            resolved_types.push(prop_result.success_type()?);
        }
        if resolved_types.is_empty() {
            return None;
        }

        // Write context: value must satisfy every possible key -> intersection.
        // Read context: result is one of the keyed properties -> union.
        let combined = if matches!(request.flow_mode, GlobalThisFlowMode::SkipFlowNarrowing) {
            tsz_solver::utils::intersection_or_single(self.ctx.types, resolved_types)
        } else {
            tsz_solver::utils::union_or_single(self.ctx.types, resolved_types)
        };
        Some(self.apply_global_this_flow_mode(request.idx, combined, request.flow_mode))
    }

    pub(super) fn try_global_this_string_like_element_access(
        &mut self,
        request: GlobalThisStringLikeElementAccess,
    ) -> Option<TypeId> {
        if !matches!(request.key_status, GlobalThisKeyStatus::NoLiteralStringKey)
            || !matches!(request.access_kind, GlobalThisAccessKind::Element)
            || !(self.is_global_this_expression(request.access_expression)
                || matches!(
                    request.receiver_status,
                    GlobalThisReceiverStatus::GlobalThisLike
                ))
            || !self.ctx.no_implicit_any()
            || self.is_js_file()
        {
            return None;
        }

        let string_literal_keys = self
            .get_literal_key_union_from_type(request.index_type)
            .filter(|(string_keys, number_keys)| number_keys.is_empty() && !string_keys.is_empty())
            .map(|(string_keys, _)| string_keys);
        let index_is_string_like = string_literal_keys.is_some()
            || crate::query_boundaries::common::is_string_type(self.ctx.types, request.index_type);
        if !index_is_string_like {
            return None;
        }

        if let Some(keys) = string_literal_keys {
            let mut resolved_types: Vec<TypeId> = Vec::with_capacity(keys.len());
            let mut all_resolved = true;
            for key_atom in &keys {
                let name = self.ctx.types.resolve_atom(*key_atom);
                let resolved = self.resolve_global_this_property_type(
                    &name,
                    request.idx,
                    true,
                    GlobalReceiver::GlobalThis,
                );
                if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    resolved_types.push(resolved);
                } else {
                    all_resolved = false;
                    break;
                }
            }
            if all_resolved && !resolved_types.is_empty() {
                return Some(
                    if matches!(request.flow_mode, GlobalThisFlowMode::SkipFlowNarrowing) {
                        tsz_solver::utils::intersection_or_single(self.ctx.types, resolved_types)
                    } else {
                        let combined =
                            tsz_solver::utils::union_or_single(self.ctx.types, resolved_types);
                        self.apply_flow_narrowing(request.idx, combined)
                    },
                );
            }
        }

        let index_str = self.format_type_diagnostic(request.index_type);
        self.error_at_node(
            request.idx,
            &format_message(
                diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                &[&index_str, "typeof globalThis"],
            ),
            diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        );
        Some(TypeId::ANY)
    }

    /// Whether `idx` is the left-hand side of a bare `=` assignment. Distinct
    /// from the broader `property_access_is_direct_write_target` helper used
    /// elsewhere: a compound assignment (`+=`) or `++`/`--` reads the current
    /// value before writing it, so it does not qualify for the "declare a new
    /// global property" leniency and must still resolve the (missing)
    /// property like any other read.
    fn is_bare_equals_assignment_target(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };
        binary.left == idx && binary.operator_token == SyntaxKind::EqualsToken as u16
    }

    fn resolve_declared_window_literal_key(
        &mut self,
        request: GlobalThisLiteralKeyAccess<'_>,
    ) -> TypeId {
        if let Some(window_type) = self.resolve_lib_type_by_name("Window") {
            let prop_result = crate::query_boundaries::property_access::resolve_property_access(
                self.ctx.types,
                window_type,
                self.ctx.types.intern_string(request.name),
            );
            if let Some(type_id) = prop_result.success_type() {
                return self.apply_global_this_flow_mode(request.idx, type_id, request.flow_mode);
            }
        }
        if self.ctx.no_implicit_any() && !self.is_js_file() {
            self.error_at_node(
                request.key_node,
                "Element implicitly has an 'any' type because index expression is not of type 'number'.",
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE,
            );
        }
        TypeId::ANY
    }

    fn apply_global_this_flow_mode(
        &mut self,
        idx: NodeIndex,
        type_id: TypeId,
        flow_mode: GlobalThisFlowMode,
    ) -> TypeId {
        if matches!(flow_mode, GlobalThisFlowMode::SkipFlowNarrowing) {
            type_id
        } else {
            self.apply_flow_narrowing(idx, type_id)
        }
    }
}
