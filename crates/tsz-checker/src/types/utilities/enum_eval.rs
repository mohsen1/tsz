//! Non-const enum numeric initializer evaluation helpers.
//!
//! These methods remain inherent methods on `CheckerState`; this module only
//! owns the memo/depth state and expression walker used by regular enum member
//! values. `const_enum_eval` stays separate because declaration checking calls
//! it without a `CheckerState`.

use crate::state::{CheckerState, EnumKind};
use rustc_hash::FxHashMap;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::numeric::{to_int32, to_uint32};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::cycle_guard::{self, CycleSetId};

// Thread-local memoization cache for evaluated enum member values.
//
// Caches successful and failed evaluation results to avoid redundant work when
// multiple enum members reference the same target in complex dependency graphs.
// Cleared at the start of each top-level evaluation via `EVAL_DEPTH`.
thread_local! {
    static EVAL_MEMO: std::cell::RefCell<rustc_hash::FxHashMap<NodeIndex, Option<f64>>>
        = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

// Thread-local recursion depth counter for `evaluate_constant_expression`.
//
// Prevents stack overflow from deeply nested (but non-cyclic) expression trees.
// The depth limit matches `evaluate_const_enum_initializer` (100).
thread_local! {
    static EVAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

const MAX_EVAL_DEPTH: u32 = 100;

/// Clear the enum evaluation memo cache and reset depth.
/// Called between compilation sessions to prevent stale `NodeIndex`-keyed results.
pub(crate) fn clear_enum_eval_memo() {
    EVAL_MEMO.with(|m| m.borrow_mut().clear());
    EVAL_DEPTH.with(|d| d.set(0));
}

// RAII guard that decrements `EVAL_DEPTH` on drop, and clears the memo cache
// when depth returns to 0 (end of top-level evaluation).
struct DepthGuard;
impl Drop for DepthGuard {
    fn drop(&mut self) {
        EVAL_DEPTH.with(|d| {
            let new_depth = d.get().saturating_sub(1);
            d.set(new_depth);
            if new_depth == 0 {
                // Clear memoization cache at the end of the top-level evaluation
                // to avoid stale results across unrelated evaluation chains.
                EVAL_MEMO.with(|m| m.borrow_mut().clear());
            }
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EnumMemberConstValue {
    Number(f64),
    String(String),
}

impl<'a> CheckerState<'a> {
    pub(crate) fn enum_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let source_sym = self.enum_symbol_from_full_enum_type(source)?;
        let target_sym = self.enum_symbol_from_full_enum_type(target)?;

        if source_sym == target_sym {
            return None;
        }

        let source_name = self.ctx.binder.get_symbol(source_sym)?.escaped_name.clone();
        let target_name = self.ctx.binder.get_symbol(target_sym)?.escaped_name.clone();
        if source_name != target_name {
            return Some(false);
        }

        if self.is_const_enum_symbol(source_sym) || self.is_const_enum_symbol(target_sym) {
            return Some(false);
        }

        if self.enum_kind(source_sym) != Some(EnumKind::Numeric)
            || self.enum_kind(target_sym) != Some(EnumKind::Numeric)
        {
            return None;
        }

        let source_members = self.enum_member_compat_map(source_sym)?;
        let target_members = self.enum_member_compat_map(target_sym)?;
        let is_subset = source_members
            .iter()
            .all(|(name, value)| target_members.get(name) == Some(value));
        Some(is_subset)
    }

    fn enum_member_compat_map(
        &self,
        sym_id: SymbolId,
    ) -> Option<FxHashMap<String, EnumMemberConstValue>> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut result = FxHashMap::default();
        let mut next_numeric_value = 0.0;
        let mut saw_enum_decl = false;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx) else {
                continue;
            };
            saw_enum_decl = true;

            for &member_idx in &enum_decl.members.nodes {
                let member_node = self.ctx.arena.get(member_idx)?;
                let member = self.ctx.arena.get_enum_member(member_node)?;
                let member_name = self.get_property_name(member.name)?;

                let value = if let Some(initializer) = member.initializer.into_option() {
                    match self.enum_member_initializer_const_value(initializer)? {
                        EnumMemberConstValue::Number(value) => {
                            next_numeric_value = value + 1.0;
                            EnumMemberConstValue::Number(value)
                        }
                        EnumMemberConstValue::String(value) => EnumMemberConstValue::String(value),
                    }
                } else {
                    let value = EnumMemberConstValue::Number(next_numeric_value);
                    next_numeric_value += 1.0;
                    value
                };

                result.insert(member_name, value);
            }
        }

        saw_enum_decl.then_some(result)
    }

    /// Get the literal type of an enum member from its initializer.
    ///
    /// Returns the literal type (e.g., Literal(0), Literal("a")) of the enum member.
    /// This is used to create `TypeData::Enum(member_def_id`, `literal_type`) for nominal typing.
    pub(crate) fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        // Get the member node
        let Some(member_node) = self.ctx.arena.get(member_decl) else {
            return TypeId::ERROR;
        };
        let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
            return TypeId::ERROR;
        };

        // Check if member has an explicit initializer.
        if let Some(initializer) = member.initializer.into_option() {
            match self.enum_member_initializer_const_value(initializer) {
                Some(EnumMemberConstValue::String(value)) => return factory.literal_string(&value),
                Some(EnumMemberConstValue::Number(value)) => return factory.literal_number(value),
                None => {}
            }
        }

        // No explicit initializer or computed value.
        // For numeric enums, recover the auto-incremented literal value so
        // callers can preserve member identity (`E.B` -> `0`).
        if let Some(member_sym) = self
            .ctx
            .binder
            .get_node_symbol(member_decl)
            .or_else(|| self.ctx.binder.get_node_symbol(member.name))
            && let Some(symbol) = self.ctx.binder.get_symbol(member_sym)
            && symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
            && symbol.parent.is_some()
            && let Some(auto_value) = self.compute_auto_increment_value(symbol.parent, member_decl)
        {
            return factory.literal_number(auto_value);
        }

        // Fall back to NUMBER type when we cannot compute a specific literal.
        TypeId::NUMBER
    }

    /// Evaluate a constant numeric expression (for enum member initializers).
    ///
    /// Handles: numeric literals, unary +/-/~, binary +/-/*/ // /%/|/&/^/<</>>/>>>,
    /// parenthesized expressions, and references to other enum members via
    /// property access (E.V1) or element access (E["V1"], E[`V1`]).
    /// Returns None if the expression cannot be evaluated at compile time.
    pub(crate) fn evaluate_constant_expression(&self, expr_idx: NodeIndex) -> Option<f64> {
        // Depth guard: prevent stack overflow from deeply nested expressions.
        let current_depth = EVAL_DEPTH.with(|d| {
            let depth = d.get() + 1;
            d.set(depth);
            depth
        });
        if current_depth > MAX_EVAL_DEPTH {
            EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return None;
        }
        let _depth_guard = DepthGuard;

        let node = self.ctx.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                match unary.operator {
                    op if op == SyntaxKind::MinusToken as u16 => Some(-operand),
                    op if op == SyntaxKind::PlusToken as u16 => Some(operand),
                    op if op == SyntaxKind::TildeToken as u16 => {
                        Some(f64::from(!to_int32(operand)))
                    }
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.ctx.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                match bin.operator_token {
                    op if op == SyntaxKind::PlusToken as u16 => Some(left + right),
                    op if op == SyntaxKind::MinusToken as u16 => Some(left - right),
                    op if op == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                    op if op == SyntaxKind::SlashToken as u16 => Some(left / right),
                    op if op == SyntaxKind::PercentToken as u16 => Some(left % right),
                    op if op == SyntaxKind::BarToken as u16 => {
                        Some(f64::from(to_int32(left) | to_int32(right)))
                    }
                    op if op == SyntaxKind::AmpersandToken as u16 => {
                        Some(f64::from(to_int32(left) & to_int32(right)))
                    }
                    op if op == SyntaxKind::CaretToken as u16 => {
                        Some(f64::from(to_int32(left) ^ to_int32(right)))
                    }
                    op if op == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(f64::from(to_int32(left) << (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(f64::from(to_int32(left) >> (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some(f64::from(to_uint32(left) >> (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::AsteriskAsteriskToken as u16 => Some(left.powf(right)),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Bare identifier: resolve as enum member reference (e.g., `B = A` in same enum),
                // or recognize global numeric constants `NaN` and `Infinity`.
                let name = self.ctx.arena.get_identifier_text(expr_idx)?;
                match name {
                    "NaN" => return Some(f64::NAN),
                    "Infinity" => return Some(f64::INFINITY),
                    _ => {}
                }
                // Use the binder's scope-based resolution to find the symbol, since enum members
                // are bound in the enum's block scope, not in file_locals.
                let lib_binders = self.get_lib_binders();
                let sym_id = self.ctx.binder.resolve_name_with_filter(
                    name,
                    self.ctx.arena,
                    expr_idx,
                    &lib_binders,
                    |_| true,
                )?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                    let member_decl = symbol.value_declaration;

                    // Check memoization cache first.
                    if let Some(cached) = EVAL_MEMO.with(|m| m.borrow().get(&member_decl).copied())
                    {
                        return cached;
                    }

                    let member_node = self.ctx.arena.get(member_decl)?;
                    let member_data = self.ctx.arena.get_enum_member(member_node)?;

                    // Cycle detection via shared CycleGuard
                    let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;

                    let result = if member_data.initializer.is_some() {
                        self.evaluate_constant_expression(member_data.initializer)
                    } else {
                        // Auto-incremented: find parent enum symbol and compute.
                        let parent_sym = symbol.parent;
                        if parent_sym.is_none() {
                            return None;
                        }
                        self.compute_auto_increment_value(parent_sym, member_decl)
                    };

                    // Cache the result.
                    EVAL_MEMO.with(|m| m.borrow_mut().insert(member_decl, result));
                    result
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Resolve a property access or element access expression that references
    /// an enum member, and evaluate its numeric value.
    ///
    /// Handles patterns like:
    /// - `E.V1` (property access on enum)
    /// - `A.B.C.E.V1` (qualified namespace chain)
    /// - `E["V1"]` (element access with string literal)
    /// - `E[`V1`]` (element access with template literal)
    fn evaluate_enum_member_access(&self, expr_idx: NodeIndex) -> Option<f64> {
        // Collect the chain of identifiers and the final member name.
        // For `A.B.C.E.V1`: segments = ["A", "B", "C", "E"], member_name = "V1"
        let (segments, member_name) = self.collect_access_chain(expr_idx)?;
        if segments.is_empty() {
            return None;
        }

        // Walk the binder's symbol table to find the enum symbol.
        let root_name = &segments[0];
        let mut current_sym_id = self.ctx.binder.file_locals.get(root_name)?;

        for segment in &segments[1..] {
            let symbol = self.ctx.binder.get_symbol(current_sym_id)?;
            // Try exports first (for namespaces), then members (for enums/classes)
            current_sym_id = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        // `current_sym_id` should now point to the enum symbol.
        // Look up the member in its exports (enum members are stored as exports).
        let enum_symbol = self.ctx.binder.get_symbol(current_sym_id)?;
        let member_sym_id = enum_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(&member_name))
            .or_else(|| {
                enum_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(&member_name))
            })?;

        let member_symbol = self.ctx.binder.get_symbol(member_sym_id)?;
        if !member_symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            return None;
        }

        // Get the member's value declaration and evaluate its initializer.
        let member_decl = member_symbol.value_declaration;

        // Check memoization cache first: if we've already evaluated this member
        // (successfully or not), return the cached result immediately.
        if let Some(cached) = EVAL_MEMO.with(|m| m.borrow().get(&member_decl).copied()) {
            return cached;
        }

        let member_node = self.ctx.arena.get(member_decl)?;
        let member_data = self.ctx.arena.get_enum_member(member_node)?;

        let result = if member_data.initializer.is_none() {
            // Auto-incremented member: we need to compute its position value.
            // Walk through all declarations of the parent enum to find this member's
            // auto-incremented value.
            //
            // Guard against cycles through auto-increment:
            // e.g., `enum E { A = F.C }; enum F { B = E.A, C }`
            // E.A -> F.C (auto-inc) -> compute_auto_inc walks F.B -> E.A -> F.C -> ...
            let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;
            self.compute_auto_increment_value(current_sym_id, member_decl)
        } else {
            // Guard against self-referencing and mutually-recursive enum initializers
            // (e.g., `B = E.B` or `enum E { A = F.B }; enum F { B = E.A }`).
            let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;
            self.evaluate_constant_expression(member_data.initializer)
        };

        // Cache the result for future lookups of the same member.
        EVAL_MEMO.with(|m| m.borrow_mut().insert(member_decl, result));
        result
    }

    /// Collect the identifier chain from a property/element access expression.
    /// Returns `(object_segments, member_name)`.
    /// For `A.B.C.E.V1`: `(["A", "B", "C", "E"], "V1")`
    /// For `E["V1"]`: (["E"], "V1")
    fn collect_access_chain(&self, expr_idx: NodeIndex) -> Option<(Vec<String>, String)> {
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = self
                    .ctx
                    .arena
                    .get_identifier(prop_node)?
                    .escaped_text
                    .clone();

                // Recursively collect the object chain.
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                // Get the string key from the element access argument.
                let arg_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = if arg_node.kind == SyntaxKind::StringLiteral as u16
                    || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    self.ctx.arena.get_literal(arg_node)?.text.clone()
                } else {
                    return None;
                };

                // Collect the object chain.
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn enum_member_declared_const_value(
        &self,
        enum_sym_id: SymbolId,
        target_member_decl: NodeIndex,
    ) -> Option<EnumMemberConstValue> {
        let enum_symbol = self.ctx.binder.get_symbol(enum_sym_id)?;

        for &decl_idx in &enum_symbol.declarations {
            let enum_decl = self.ctx.arena.get_enum_at(decl_idx)?;
            let mut auto_value = 0.0;
            for &member_idx in &enum_decl.members.nodes {
                let member_node = self.ctx.arena.get(member_idx)?;
                let member_data = self.ctx.arena.get_enum_member(member_node)?;

                if member_idx == target_member_decl {
                    return if let Some(initializer) = member_data.initializer.into_option() {
                        self.enum_member_initializer_const_value(initializer)
                    } else {
                        Some(EnumMemberConstValue::Number(auto_value))
                    };
                }

                if let Some(initializer) = member_data.initializer.into_option() {
                    auto_value = self.next_numeric_enum_auto_value(initializer)?;
                } else {
                    auto_value += 1.0;
                }
            }
        }
        None
    }

    pub(crate) fn enum_member_initializer_const_value(
        &self,
        initializer: NodeIndex,
    ) -> Option<EnumMemberConstValue> {
        let init_node = self.ctx.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(init_node)?;
                Some(EnumMemberConstValue::String(lit.text.clone()))
            }
            _ => self
                .evaluate_constant_expression(initializer)
                .map(EnumMemberConstValue::Number),
        }
    }

    fn next_numeric_enum_auto_value(&self, initializer: NodeIndex) -> Option<f64> {
        match self.enum_member_initializer_const_value(initializer)? {
            EnumMemberConstValue::Number(value) => Some(value + 1.0),
            EnumMemberConstValue::String(_) => None,
        }
    }

    /// Compute the auto-incremented value for an enum member without an initializer.
    /// Walks through all declarations of the parent enum up to the target member,
    /// tracking the auto-increment counter.
    fn compute_auto_increment_value(
        &self,
        enum_sym_id: tsz_binder::SymbolId,
        target_member_decl: NodeIndex,
    ) -> Option<f64> {
        match self.enum_member_declared_const_value(enum_sym_id, target_member_decl)? {
            EnumMemberConstValue::Number(value) => Some(value),
            EnumMemberConstValue::String(_) => None,
        }
    }
}
