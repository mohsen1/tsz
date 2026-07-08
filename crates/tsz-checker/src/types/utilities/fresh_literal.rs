//! Literal-freshness boundary for widening observation points.
//!
//! Owns the two halves of enum-member/literal widening policy at mutable
//! observation points (mutable bindings, parameter defaults, inferred return
//! and yield contributions):
//!
//! - **When to widen** ([`CheckerState::is_fresh_literal_expression`]): only
//!   *fresh* initializer expressions widen. Fresh means a direct literal, a
//!   direct enum-member access, or a chain of parentheses/conditional
//!   branches/unannotated `const` references ending in one — mirroring tsc's
//!   fresh vs regular literal types and `getWidenedLiteralTypeForInitializer`.
//!   Annotated `const` references, property reads, and call results are
//!   non-fresh and keep their literal/member type.
//! - **What enum members widen to**
//!   ([`CheckerState::enum_member_parent_instance_type`]): the parent enum's
//!   *instance* type `E` (`Enum(def, union-of-member-literals)`), never the
//!   enum's static object/namespace value shape (`typeof E`). The enum
//!   symbol's cached value meaning is resolution-order dependent, so the
//!   result is normalized against the definition store instead of trusting
//!   `get_type_of_symbol` alone.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Bound for freshness chains (`const a = b; const b = ...`) and entity
/// resolution walks. Identifier-following can in principle reach itself
/// through pathological forward references like `const a = a;`, so every
/// recursive walk in this module carries a depth and stops here.
const MAX_FRESH_LITERAL_DEPTH: u8 = 16;

impl<'a> CheckerState<'a> {
    /// Widen a mutable-binding-like observation of `init_type` inferred from
    /// the initializer expression `expr_idx`.
    ///
    /// This is the single freshness-gated entry for `let`/`var` initializers
    /// and parameter defaults: fresh initializers widen (literals to their
    /// primitive base, enum members to the parent enum instance type),
    /// non-fresh initializers keep their type.
    pub(crate) fn widen_mutable_binding_observation(
        &mut self,
        expr_idx: NodeIndex,
        init_type: TypeId,
    ) -> TypeId {
        if self.is_fresh_widening_source(expr_idx, init_type) {
            self.widen_initializer_type_for_mutable_binding(init_type)
        } else {
            init_type
        }
    }

    /// Freshness of an initializer/contribution expression, with the
    /// enum-member-access arm enabled only when the observed type is
    /// enum-member shaped.
    ///
    /// The enum arm resolves symbols (a scope-chain walk), so it must not run
    /// for ordinary property reads like `let x = obj.prop`; the cheap type
    /// gate keeps the non-enum common case on the zero-cost AST fall-through.
    pub(crate) fn is_fresh_widening_source(&self, expr_idx: NodeIndex, init_type: TypeId) -> bool {
        self.is_fresh_literal_expression_inner(
            expr_idx,
            0,
            self.is_enum_member_like_type(init_type),
        )
    }

    /// Cheap shape test with the same detection breadth as
    /// [`Self::enum_member_parent_instance_type`]: a def-backed
    /// `Enum(member_def, _)` or a symbol-backed identity carrying the
    /// `ENUM_MEMBER` flag (the shape some resolution orders observe before the
    /// member's def-backed type is interned). Tag reads and map lookups only —
    /// no symbol scope walk, no type computation.
    fn is_enum_member_like_type(&self, type_id: TypeId) -> bool {
        if self.is_enum_member_type_for_widening(type_id) {
            return true;
        }
        self.ctx
            .resolve_type_to_symbol_id(type_id)
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::ENUM_MEMBER))
    }

    /// The parent enum's instance type for an enum-*member* type, or `None`
    /// when `type_id` is not an enum member.
    ///
    /// The instance type is `Enum(parent_def, union-of-member-literals)` — the
    /// type a `: E` annotation denotes — not the enum's static object value
    /// shape (`typeof E`). `get_type_of_symbol` on the enum symbol can return
    /// either depending on which position resolved the enum first, so the
    /// result is normalized against the definition store's registered
    /// structural body.
    pub(crate) fn enum_member_parent_instance_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        // Def-backed member identity: `Enum(member_def, literal)`.
        if let Some(member_def) =
            crate::query_boundaries::common::enum_def_id(self.ctx.types, type_id)
        {
            // The parent edge only exists for member defs; the parent enum
            // type itself resolves to `None` here and is returned unchanged
            // by the widening callers.
            let parent_def = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_enum_parent(member_def))?;
            let parent_sym = self.ctx.def_to_symbol_id_with_fallback(parent_def)?;
            return Some(self.enum_instance_type_for_enum_symbol(parent_sym, parent_def));
        }

        // Legacy identity: the member is only reachable through the symbol
        // table (no `DefId` on the type).
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }
        let parent_sym = symbol.parent;
        let parent_def = self.ctx.definition_store.find_def_by_symbol(parent_sym.0);
        Some(match parent_def {
            Some(parent_def) => self.enum_instance_type_for_enum_symbol(parent_sym, parent_def),
            // No definition mapping to normalize against; the symbol's type
            // is the only available meaning.
            None => self.get_type_of_symbol(parent_sym),
        })
    }

    /// The enum instance type (`Enum(enum_def, member-union)`) for an enum
    /// symbol, independent of whether the symbol's cached value meaning is the
    /// instance type or the static namespace object.
    fn enum_instance_type_for_enum_symbol(
        &mut self,
        enum_sym: SymbolId,
        enum_def: tsz_solver::def::DefId,
    ) -> TypeId {
        // Forces the enum declaration's type computation on first touch,
        // which also registers the structural member union as the def body.
        let symbol_type = self.get_type_of_symbol(enum_sym);
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, symbol_type)
            == Some(enum_def)
        {
            return symbol_type;
        }
        // The cached value meaning is the namespace object (`typeof E`);
        // rebuild the instance type from the registered structural body.
        if let Some(body) = self.ctx.definition_store.get_body(enum_def) {
            if crate::query_boundaries::common::enum_def_id(self.ctx.types, body) == Some(enum_def)
            {
                return body;
            }
            return self.ctx.types.factory().enum_type(enum_def, body);
        }
        symbol_type
    }

    /// Check if an expression produces a "fresh" literal type that should be widened.
    ///
    /// In TypeScript, literal types created from literal expressions are "fresh" and get
    /// widened when assigned to mutable bindings (let/var). Literal types from other
    /// sources (variable references, type annotations, narrowing) are "non-fresh" and
    /// should NOT be widened.
    ///
    /// A direct enum-member access (`E.A`, `Ns.E.A`, `E["A"]`) mints a fresh
    /// enum literal exactly like a primitive literal token; a property read of
    /// an enum-member-typed property on an ordinary object (`o.p`) does not.
    ///
    /// An identifier referring to an unannotated `const` declaration whose
    /// initializer is itself a fresh literal expression is also treated as fresh: tsc
    /// tracks such bindings as widening literal types and widens them when copied into a
    /// mutable binding.
    ///
    /// ## Examples:
    /// ```typescript
    /// let x = "foo";          // "foo" is fresh → widened to string
    /// let a: "foo" = "foo";
    /// let y = a;              // a's type is non-fresh → y: "foo" (not widened)
    /// let z = a || "bar";     // result from || is non-fresh → z: "foo" (not widened)
    ///
    /// const tag = "start";    // unannotated const literal → widening literal type
    /// let m = tag;            // tag is fresh-by-reference → widened to string
    ///
    /// enum E { A, B }
    /// let e = E.A;            // E.A is fresh → widened to E
    /// const c: E.A = E.A;
    /// let n = c;              // c's type is non-fresh → n: E.A (not widened)
    /// ```
    pub(crate) fn is_fresh_literal_expression(&self, idx: NodeIndex) -> bool {
        self.is_fresh_literal_expression_inner(idx, 0, false)
    }

    /// `enum_member_arm` enables the enum-member-access case; callers turn it
    /// on only when the observed type is enum-member shaped (see
    /// [`Self::is_fresh_widening_source`]), keeping symbol resolution off the
    /// common property-read path.
    fn is_fresh_literal_expression_inner(
        &self,
        idx: NodeIndex,
        depth: u8,
        enum_member_arm: bool,
    ) -> bool {
        if depth > MAX_FRESH_LITERAL_DEPTH {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        let kind = node.kind;

        // Direct literal tokens are always fresh
        if kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return true;
        }

        // Parenthesized expressions inherit freshness from inner expression
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.is_fresh_literal_expression_inner(
                paren.expression,
                depth + 1,
                enum_member_arm,
            );
        }

        // Prefix unary (+/-) on numeric/bigint literals are fresh
        if kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(prefix) = self.ctx.arena.get_unary_expr(node)
        {
            let op = prefix.operator;
            if op == SyntaxKind::PlusToken as u16 || op == SyntaxKind::MinusToken as u16 {
                return self.is_fresh_literal_expression_inner(
                    prefix.operand,
                    depth + 1,
                    enum_member_arm,
                );
            }
        }

        // Conditional expressions: fresh if either branch produces a fresh type.
        // E.g., `cond ? true : undefined` has a fresh `true` branch, so the
        // result type `true | undefined` should be widened to `boolean | undefined`.
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            return self.is_fresh_literal_expression_inner(
                cond.when_true,
                depth + 1,
                enum_member_arm,
            ) || self.is_fresh_literal_expression_inner(
                cond.when_false,
                depth + 1,
                enum_member_arm,
            );
        }

        // Object and array literals need widening (property types get widened)
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        {
            return true;
        }

        // Template expressions (with substitutions) produce string, which doesn't need widening
        // but we mark them fresh for consistency
        if kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            return true;
        }

        // A direct enum-member access mints a fresh enum literal, exactly as
        // a primitive literal token mints a fresh primitive literal. Property
        // reads whose base is not the enum object (`o.p` where `p: E.A`)
        // resolve to `None` here and stay non-fresh.
        if enum_member_arm
            && (kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && self.enum_member_access_symbol(idx, depth).is_some()
        {
            return true;
        }

        // Identifier referencing an unannotated `const` declaration whose
        // initializer is itself a fresh literal expression. tsc tracks these
        // bindings as widening literal types, so copying them into a `let`/`var`
        // binding must still widen.
        if kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(decl_idx) = symbol.primary_declaration()
            && self.ctx.arena.is_const_variable_declaration(decl_idx)
            && let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_none()
            && var_decl.initializer.is_some()
        {
            return self.is_fresh_literal_expression_inner(
                var_decl.initializer,
                depth + 1,
                enum_member_arm,
            );
        }

        // Everything else (identifiers, call expressions, binary expressions, etc.)
        // produces non-fresh types that should NOT be widened
        false
    }

    /// The enum-*member* symbol denoted by a direct member access expression
    /// (`E.A`, `Ns.E.A`, `E["A"]`), or `None` when the access does not read a
    /// member off the enum object itself.
    fn enum_member_access_symbol(&self, idx: NodeIndex, depth: u8) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        if access.question_dot_token {
            return None;
        }
        let member_name: &str = {
            let name_node = self.ctx.arena.get(access.name_or_argument)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .as_str()
            } else if name_node.kind == SyntaxKind::StringLiteral as u16
                || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                self.ctx.arena.get_literal(name_node)?.text.as_str()
            } else {
                return None;
            }
        };
        let enum_sym_id = self.value_entity_symbol(access.expression, depth + 1)?;
        let enum_symbol = self.checker_symbol(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }
        let member_sym_id = enum_symbol.exports.as_ref()?.get(member_name)?;
        let member_symbol = self.checker_symbol(member_sym_id)?;
        member_symbol
            .has_any_flags(symbol_flags::ENUM_MEMBER)
            .then_some(member_sym_id)
    }

    /// Resolve an entity-shaped value expression (identifier or dotted chain,
    /// possibly parenthesized) to the symbol it denotes, following import
    /// aliases and unannotated `const` value bindings — the same freshness
    /// transparency the identifier arm of
    /// [`Self::is_fresh_literal_expression`] gives literal initializers.
    fn value_entity_symbol(&self, idx: NodeIndex, depth: u8) -> Option<SymbolId> {
        if depth > MAX_FRESH_LITERAL_DEPTH {
            return None;
        }
        let node = self.ctx.arena.get(idx)?;
        let kind = node.kind;
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.value_entity_symbol(paren.expression, depth + 1);
        }
        if kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol_without_tracking(idx)?;
            return self.value_entity_target(sym_id, depth);
        }
        if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let container_sym = self.value_entity_symbol(access.expression, depth + 1)?;
            let name_node = self.ctx.arena.get(access.name_or_argument)?;
            let name = self
                .ctx
                .arena
                .get_identifier(name_node)?
                .escaped_text
                .as_str();
            let member_sym = self
                .checker_symbol(container_sym)?
                .exports
                .as_ref()?
                .get(name)?;
            return self.value_entity_target(member_sym, depth);
        }
        None
    }

    /// Follow one resolution step for [`Self::value_entity_symbol`]: import
    /// aliases resolve to their target, and an unannotated `const` bound to
    /// another entity (`const e = E`) is transparent.
    fn value_entity_target(&self, sym_id: SymbolId, depth: u8) -> Option<SymbolId> {
        if depth > MAX_FRESH_LITERAL_DEPTH {
            return None;
        }
        let symbol = self.checker_symbol(sym_id)?;
        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let resolved = self.resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())?;
            if resolved != sym_id {
                return self.value_entity_target(resolved, depth + 1);
            }
        }
        if let Some(decl_idx) = symbol.primary_declaration()
            && self.ctx.arena.is_const_variable_declaration(decl_idx)
            && let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_none()
            && var_decl.initializer.is_some()
            && let Some(target) = self.value_entity_symbol(var_decl.initializer, depth + 1)
        {
            return Some(target);
        }
        Some(sym_id)
    }

    /// A symbol view that works for both same-file and cross-file symbols.
    pub(crate) fn checker_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
    }
}
