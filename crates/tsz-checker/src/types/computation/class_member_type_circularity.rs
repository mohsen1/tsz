//! Circular **type-annotation** detection for class property members (TS2502).
//!
//! `tsc` reports TS2502 when a class property's declared type annotation
//! resolves, directly or indirectly, back to that property's own type. The
//! self-reference travels through a `typeof Class.member` / `typeof this.member`
//! query, or a `(typeof Class)[K]` / `Class[K]` / `this[K]` indexed access whose
//! key resolves to a member:
//!
//! ```ts
//! declare const s: unique symbol;
//! class C { static x: typeof C.x; }                       // TS2502 'x'   (direct, static)
//! class C { x: typeof this.x; }                           // TS2502 'x'   (direct, instance)
//! class C { static [s]: typeof C[typeof s]; }             // TS2502 '[s]' (symbol-keyed)
//! class C { static a: typeof C.b; static b: typeof C.a; } // TS2502 a, b  (indirect)
//! ```
//!
//! The variable, interface, and type-literal forms already emit TS2502 through
//! their own paths (`variable_checking::circularity`,
//! `state_checking_members::interface_checks`,
//! `types::type_literal_checker`). Class member declarations had no equivalent
//! guard, so a self-referential member type was a silent false negative
//! (issue #14819).
//!
//! Detection is **symbol/receiver gated**, never name-keyed against an arbitrary
//! receiver: a `recv.X` / `recv[K]` reference counts as a self-reference only
//! when `recv` resolves to the enclosing class — the class name / `typeof Class`
//! for the static side, the class instance type / `this` for the instance side.
//! An unrelated `obj.X` whose name merely collides with a member is not a
//! self-reference. This mirrors the receiver gate used by the TS7023/TS7024
//! class-member return-circularity detector in `class_member_circularity.rs`,
//! whose cycle walk (`cyclic_member_indices`) is reused here.

use super::class_member_circularity::cyclic_member_indices;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Identity a member declaration is matched against a reference by.
///
/// A computed name whose expression resolves to a symbol (`[s]`) keys by that
/// symbol so a `typeof C[typeof s]` access lands on it; every other member keys
/// by its canonical property-name string so `typeof C.x` / `C["x"]` land on it.
#[derive(Clone, PartialEq, Eq, Hash)]
enum MemberKey {
    Named(String),
    Sym(SymbolId),
}

struct AnnotatedMember {
    /// Diagnostic anchor, and the source of the `{0}` message argument (resolved
    /// lazily via `property_name_for_error` only for members that prove cyclic).
    name_idx: NodeIndex,
    is_static: bool,
    /// `(is_static, key)` identity references are resolved against.
    key: MemberKey,
    /// Declared type annotation walked for self-references.
    type_annotation: NodeIndex,
}

impl CheckerState<'_> {
    /// Emit TS2502 for class property members whose declared type annotation is
    /// circular through a `this.`/`Class.` self-reference. Runs once per class,
    /// after the members have been checked.
    pub(crate) fn check_class_member_circular_type_annotations(
        &mut self,
        class_idx: NodeIndex,
        members: &[NodeIndex],
    ) {
        if self.has_syntax_parse_errors() || self.is_js_file() {
            return;
        }

        let candidates = self.collect_annotated_class_members(members);
        if candidates.is_empty() {
            return;
        }

        let class_sym = self.ctx.binder.get_node_symbol(class_idx);

        // (is_static, key) -> candidate index. Private names already carry a `#`
        // so they never collide with public members of the same spelling.
        let mut by_key: FxHashMap<(bool, MemberKey), usize> = FxHashMap::default();
        for (idx, member) in candidates.iter().enumerate() {
            by_key
                .entry((member.is_static, member.key.clone()))
                .or_insert(idx);
        }

        // Edge i -> j when member i's annotation references member j.
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); candidates.len()];
        for (i, member) in candidates.iter().enumerate() {
            let mut refs = Vec::new();
            self.collect_member_type_self_references(
                member.type_annotation,
                member.is_static,
                class_sym,
                &mut refs,
            );
            let mut targets: FxHashSet<usize> = FxHashSet::default();
            for key in refs {
                if let Some(&j) = by_key.get(&key) {
                    targets.insert(j);
                }
            }
            adjacency[i] = targets.into_iter().collect();
        }

        // Reuse the adjacency cycle walk from the sibling TS7023/TS7024
        // return-circularity detector; the member name (the `{0}` argument) is
        // rendered only here, for the members that actually lie on a cycle.
        for idx in cyclic_member_indices(&adjacency) {
            let Some(display_name) = self.property_name_for_error(candidates[idx].name_idx) else {
                continue;
            };
            self.error_at_node_msg(
                candidates[idx].name_idx,
                crate::diagnostics::diagnostic_codes::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_TYPE_ANNOTATION,
                &[&display_name],
            );
        }
    }

    fn collect_annotated_class_members(&self, members: &[NodeIndex]) -> Vec<AnnotatedMember> {
        let mut out = Vec::new();
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(node) else {
                continue;
            };
            if prop.type_annotation.is_none() {
                continue;
            }
            let Some(key) = self.class_member_key(prop.name) else {
                continue;
            };
            out.push(AnnotatedMember {
                name_idx: prop.name,
                is_static: self.has_static_modifier(&prop.modifiers),
                key,
                type_annotation: prop.type_annotation,
            });
        }
        out
    }

    /// The identity a member is matched by: a symbol for a `[s]`-style computed
    /// name whose expression resolves to a symbol, otherwise the canonical
    /// property name. Members with no stable identity (e.g. a dynamic computed
    /// name) are skipped — they can neither be referenced nor reference back.
    fn class_member_key(&self, name_idx: NodeIndex) -> Option<MemberKey> {
        if let Some(sym) = self.computed_name_symbol(name_idx) {
            return Some(MemberKey::Sym(sym));
        }
        self.get_property_name(name_idx).map(MemberKey::Named)
    }

    /// Symbol a `[expr]` computed property name binds to, when `expr` is a bare
    /// identifier referencing an in-scope (symbol) value. `None` for non-computed
    /// names and dynamic computed expressions.
    fn computed_name_symbol(&self, name_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(name_idx)?;
        if node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(node)?;
        let expr = self.ctx.arena.get(computed.expression)?;
        if expr.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.resolve_identifier_symbol_without_tracking(computed.expression)
    }

    /// Collect every `(is_static, key)` self-reference reachable in a member's
    /// type annotation. Function/constructor types are safe recursion boundaries
    /// and type literals / mapped types defer member resolution, so neither is
    /// descended — matching the variable-path boundary semantics in
    /// `variable_checking::circularity::find_circular_reference_impl`.
    fn collect_member_type_self_references(
        &self,
        type_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
        refs: &mut Vec<(bool, MemberKey)>,
    ) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::MAPPED_TYPE
        ) {
            return;
        }

        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(query) = self.ctx.arena.get_type_query(node)
            && let Some(key) =
                self.type_query_member_reference(query.expr_name, current_is_static, class_sym)
        {
            refs.push(key);
        }

        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && let Some(indexed) = self.ctx.arena.get_indexed_access_type(node)
            && let Some(key) = self.indexed_access_member_reference(
                indexed.object_type,
                indexed.index_type,
                current_is_static,
                class_sym,
            )
        {
            refs.push(key);
        }

        for child in self.ctx.arena.get_children(type_idx) {
            self.collect_member_type_self_references(child, current_is_static, class_sym, refs);
        }
    }

    /// `typeof Recv.NAME` -> the member `NAME` on the static or instance side,
    /// when `Recv` is the enclosing class name or `this`.
    fn type_query_member_reference(
        &self,
        expr_name: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<(bool, MemberKey)> {
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }
        let qn = self.ctx.arena.get_qualified_name(node)?;
        let target_static = self.receiver_static_side(qn.left, current_is_static, class_sym)?;
        let name = self
            .ctx
            .arena
            .get_identifier_at(qn.right)?
            .escaped_text
            .clone();
        Some((target_static, MemberKey::Named(name)))
    }

    /// `(typeof Class)[K]` / `Class[K]` / `this[K]` -> the member identified by
    /// `K` on the side selected by the object type.
    fn indexed_access_member_reference(
        &self,
        object_idx: NodeIndex,
        index_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<(bool, MemberKey)> {
        let target_static =
            self.indexed_object_static_side(object_idx, current_is_static, class_sym)?;
        let key = self.index_type_member_key(index_idx)?;
        Some((target_static, key))
    }

    /// Which side of the enclosing class an indexed-access object type denotes:
    /// `typeof Class` -> static, the class instance type reference -> instance,
    /// `this` -> the referencing member's own side. `None` for any other object.
    fn indexed_object_static_side(
        &self,
        object_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<bool> {
        let object_idx = crate::types_domain::unique_symbol_arena::unwrap_parenthesized_type(
            self.ctx.arena,
            object_idx,
        );
        let node = self.ctx.arena.get(object_idx)?;

        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(query) = self.ctx.arena.get_type_query(node)
            && self.identifier_resolves_to_class(query.expr_name, class_sym)
        {
            return Some(true);
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && self.identifier_resolves_to_class(type_ref.type_name, class_sym)
        {
            return Some(false);
        }

        if node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(current_is_static);
        }

        None
    }

    /// The member key an indexed-access index type selects: a `typeof s` query
    /// keys by the symbol `s`; a string/number literal type keys by its
    /// canonical name. `None` for anything else (`keyof`, unions, …).
    fn index_type_member_key(&self, index_idx: NodeIndex) -> Option<MemberKey> {
        let index_idx = crate::types_domain::unique_symbol_arena::unwrap_parenthesized_type(
            self.ctx.arena,
            index_idx,
        );
        let node = self.ctx.arena.get(index_idx)?;

        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(query) = self.ctx.arena.get_type_query(node)
            && let Some(expr) = self.ctx.arena.get(query.expr_name)
            && expr.kind == SyntaxKind::Identifier as u16
        {
            return self
                .resolve_identifier_symbol_without_tracking(query.expr_name)
                .map(MemberKey::Sym);
        }

        if node.kind == syntax_kind_ext::LITERAL_TYPE
            && let Some(lit_type) = self.ctx.arena.get_literal_type(node)
            && let Some(inner) = self.ctx.arena.get(lit_type.literal)
            && let Some(lit) = self.ctx.arena.get_literal(inner)
        {
            if inner.kind == SyntaxKind::StringLiteral as u16 {
                return Some(MemberKey::Named(lit.text.clone()));
            }
            if inner.kind == SyntaxKind::NumericLiteral as u16 {
                let canonical = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                    .unwrap_or_else(|| lit.text.clone());
                return Some(MemberKey::Named(canonical));
            }
        }

        None
    }

    /// Static side from a `Class.`/`this.` receiver in a `typeof` query: `this`
    /// follows the referencing member's own side; the bare class name selects
    /// the static side. `None` for any other receiver.
    fn receiver_static_side(
        &self,
        receiver_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<bool> {
        let node = self.ctx.arena.get(receiver_idx)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some(current_is_static);
        }
        if self.identifier_resolves_to_class(receiver_idx, class_sym) {
            return Some(true);
        }
        None
    }

    fn identifier_resolves_to_class(
        &self,
        identifier_idx: NodeIndex,
        class_sym: Option<SymbolId>,
    ) -> bool {
        class_sym.is_some()
            && self
                .ctx
                .arena
                .get(identifier_idx)
                .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && self.resolve_identifier_symbol_without_tracking(identifier_idx) == class_sym
    }
}
