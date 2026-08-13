//! Widening-provenance gate for the mutable-binding non-strict nullish widen
//! (#16384 leg B).
//!
//! tsc's non-strict nullish widening (`nullWideningType`/`undefinedWideningType`
//! reaching `getWidenedType`) is a property of the *expression*, not the type:
//! only the bare `null`/`undefined` keyword, or an identifier resolving to the
//! global `undefined`, carries the widening flavour. A value that merely has
//! type `undefined`/`null` through a declared source (`declare var q: undefined`)
//! does not. tsz carries no per-type widening flag, so
//! [`CheckerState::widen_initializer_type_for_mutable_binding`] must recover the
//! provenance from the initializer's own fresh array/object-literal syntax
//! before calling the solver's purely type-shape-driven `widen_nullish_to_any_deep`.
//!
//! This mirrors the reusable gate the return-contribution seam needs for the
//! same rule (`return_contribution_nullish_leaves_are_widening`, tracked
//! separately in #16383/#16384 leg A); the two seams walk different AST shapes
//! (a return expression vs. a mutable-binding initializer) so are not merged
//! into one function, but the underlying rule — and the "fail closed on
//! anything the walk cannot account for" policy — is identical.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Whether every `null`/`undefined` leaf that
    /// [`crate::query_boundaries::widening::widen_nullish_to_any_deep`] would
    /// touch inside `expr`'s fresh array/object-literal structure is a genuine
    /// widening source, so the deep nullish-to-`any` widen is safe to apply.
    ///
    /// Recurses into nested fresh array/object literals (`[[undefined]]`,
    /// `{ p: [undefined] }`). Anything the walk cannot account for — a spread
    /// element, a shorthand/method/accessor object member, a computed key, or
    /// a leaf whose checked type was not cached — fails closed (`false`), so a
    /// case this gate cannot prove stays with tsz's prior (non-widening)
    /// behaviour rather than risk over-widening a declared value.
    pub(crate) fn initializer_nullish_leaves_are_widening(&self, expr: NodeIndex) -> bool {
        self.initializer_nullish_leaves_are_widening_inner(expr, 0)
    }

    /// Whether a *call argument* expression is itself a fresh array/object
    /// literal whose nullish leaves are all widening sources (#16384 leg A).
    ///
    /// tsc's `getInferredType` ends in `getWidenedType`, so a widening-flavoured
    /// argument propagates the flavour into the inferred type argument:
    /// `declare function id[ T ](x: T): T; var v = id([undefined]);` infers
    /// `any[]`, not `undefined[]`. The candidate seam is the only place this can
    /// be recovered — the variable-declaration seam cannot, because a call
    /// expression is never a fresh literal.
    ///
    /// Unlike [`CheckerState::is_fresh_literal_expression`], this deliberately
    /// does **not** follow identifiers to a fresh-by-reference initializer, and
    /// does not accept a bare literal token. The widening-provenance walk is
    /// only meaningful over literal syntax written at the argument position:
    /// `declare var qa: undefined[]; id(qa)` must keep `undefined[]`, and an
    /// identifier's own checked type carries no flavour to recover. The bare
    /// `undefined`/`null` argument (`id(undefined)` → `any`) already resolves
    /// correctly through the scalar path and is left alone here.
    pub(crate) fn fresh_literal_argument_nullish_leaves_are_widening(
        &self,
        expr: NodeIndex,
    ) -> bool {
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            return false;
        }
        self.initializer_nullish_leaves_are_widening(expr)
    }

    fn initializer_nullish_leaves_are_widening_inner(&self, expr: NodeIndex, depth: u8) -> bool {
        // Cycle / runaway-recursion guard, mirroring `is_fresh_literal_expression`.
        const MAX_DEPTH: u8 = 16;
        if depth > MAX_DEPTH {
            return false;
        }

        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };

        if node.kind == SyntaxKind::NullKeyword as u16
            || node.kind == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        if node.kind == SyntaxKind::Identifier as u16
            && crate::control_flow::narrowing_helpers::is_global_undefined_identifier(
                self.ctx.arena,
                self.ctx.binder,
                expr,
            )
        {
            return true;
        }
        // A spread element's source is opaque to this walk (its elements are
        // not individually visible here) — fail closed rather than assume.
        if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
            return false;
        }

        // A plain assignment expression (`x = <value>`) evaluates to its RHS
        // value — tsc types (and widening-flavours) `x = y` as `y`'s own type,
        // not `x`'s declared type (`check_assignment_expression` already
        // returns `right_type` for exactly this reason). The provenance walk
        // must follow the same unwrap, or a widening literal reached only
        // through an assignment (`var b = a = [undefined, null]`) fails closed
        // and `b` keeps its unwidened tuple instead of `[any, any]`. Compound
        // assignments (`+=` and friends) are excluded: their value type is not
        // simply the RHS's.
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.initializer_nullish_leaves_are_widening_inner(binary.right, depth + 1);
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(array) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            return array.elements.nodes.iter().all(|&elem| {
                // An ELIDED element (`[,,]`, parsed as `NodeIndex::NONE`) is a
                // widening source: the user wrote no value at all, so tsc gives
                // the hole `undefinedWideningType` exactly as it does the bare
                // `undefined` keyword. Without this the hole falls into the
                // node-lookup guard below and fails closed, which left
                // `var a = [,,]` at `undefined[]` where tsc says `any[]` and
                // regressed `widenedTypes/arrayLiteralWidened.ts` (#16393).
                //
                // The enclosing `all` is what keeps this honest: the same
                // fixture requires `var x: undefined = undefined; var d = [, x]`
                // to STAY `undefined[]`, because one non-widening element makes
                // the whole literal non-widening. A hole is permissive on its
                // own and decisive nowhere.
                if elem == NodeIndex::NONE {
                    return true;
                }
                self.initializer_nullish_leaves_are_widening_inner(elem, depth + 1)
            });
        }

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            return obj.elements.nodes.iter().all(|&elem| {
                let Some(elem_node) = self.ctx.arena.get(elem) else {
                    return false;
                };
                // Shorthand/method/accessor/spread members are opaque here —
                // only a plain `name: value` assignment is walked.
                let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                    return false;
                };
                self.initializer_nullish_leaves_are_widening_inner(prop.initializer, depth + 1)
            });
        }

        // Any other leaf expression (identifier, call, property access, ...):
        // it only needs to be a widening source when the widener would actually
        // touch it — a leaf the widener leaves alone never contributes a nullish
        // leaf to the composite, so it cannot make the widen unsafe.
        //
        // The question has to be asked of the widener itself, not of the two
        // scalar ids: `t != UNDEFINED && t != NULL` is not closed under nesting,
        // so a leaf typed `undefined[]` passed the gate and the deep widener
        // then rewrote its *interior* — `declare function supply(): undefined[];
        // var v = [supply()]` inferred `any[][]` where tsc keeps `undefined[][]`
        // (#16396). `widen_nullish_to_any_deep(leaf) == leaf` is the same
        // predicate in nesting-closed form: it agrees on the scalar cases
        // (`undefined`/`null` widen to `any`, so both still fail here) and
        // additionally rejects every composite the widener would rewrite.
        // Reaching this point already means the leaf is not a widening source —
        // the bare `null`/`undefined` keyword and the global `undefined`
        // identifier returned `true` above. This is the form #16383's
        // return-contribution seam uses, which is correct on both repros today.
        //
        // An *uncached* leaf type is not evidence of a leaf the widener would
        // skip, so it fails closed, per this walk's stated policy. The
        // mutable-binding seam never observes the difference (it runs after the
        // initializer has been typed), but the generic-call candidate seam does:
        // the argument's element types are not necessarily resident when
        // candidates are normalized, and reading `None` as "safe to widen" there
        // turned `declare var q: undefined; id([q])` into `any[]` when tsc keeps
        // `undefined[]` (#16384 leg A).
        let Some(&leaf_type) = self.ctx.node_types.get(&expr.0) else {
            return false;
        };
        crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, leaf_type)
            == leaf_type
    }

    /// Whether `expr` (an array literal) has at least one immediate element
    /// that is a genuine nullish-widening source — the bare `null`/`undefined`
    /// keyword, an elided hole, or the global `undefined` identifier.
    ///
    /// Unlike [`Self::initializer_nullish_leaves_are_widening`] (an `all`
    /// question: "is it *safe* to widen whatever nullish leaves exist"), this
    /// is an `any` question: "does a genuine widening leaf exist at all". The
    /// distinction matters for a resulting-type-only gate (`array_element_type
    /// == ANY`): `declare var y: any; var b = [y];` also ends with element
    /// type `any` (from `y`'s own declared type, not widening), and
    /// `initializer_nullish_leaves_are_widening` vacantly returns `true` for
    /// it too (the widener never touches an already-`any` leaf, so it can't
    /// make widening "unsafe") — but tsc reports no diagnostic there. This
    /// walk distinguishes the two by requiring an actual nullish leaf, not
    /// just the absence of a leaf the widener would mishandle.
    pub(crate) fn array_literal_has_direct_nullish_leaf(&self, expr: NodeIndex) -> bool {
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return false;
        }
        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        array
            .elements
            .nodes
            .iter()
            .any(|&elem| self.expr_is_direct_nullish_widening_leaf(elem))
    }

    /// Whether `expr` is itself a genuine nullish-widening source: an elided
    /// array-literal hole, the bare `null`/`undefined` keyword, or an
    /// identifier resolving to the global `undefined`. Shared leaf-level
    /// predicate behind both [`Self::array_literal_has_direct_nullish_leaf`]
    /// (an `any`-of-leaves question over a whole literal) and per-slot tuple
    /// widening in a destructuring initializer's tuple-context typing, where
    /// each element widens independently rather than all-or-nothing.
    pub(crate) fn expr_is_direct_nullish_widening_leaf(&self, expr: NodeIndex) -> bool {
        if expr == NodeIndex::NONE {
            return true;
        }
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind == SyntaxKind::NullKeyword as u16
            || node.kind == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        node.kind == SyntaxKind::Identifier as u16
            && crate::control_flow::narrowing_helpers::is_global_undefined_identifier(
                self.ctx.arena,
                self.ctx.binder,
                expr,
            )
    }
}
