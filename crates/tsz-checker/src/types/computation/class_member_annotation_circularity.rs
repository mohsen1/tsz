//! Circular **type-annotation** detection for class properties (TS2502).
//!
//! `tsc` reports TS2502 when a class property's declared type annotation
//! references that same property, directly or indirectly, through a `typeof`
//! query or an indexed access rooted at the enclosing class:
//!
//! ```ts
//! declare const s: unique symbol;
//! class C { static [s]: typeof C[typeof s]; }      // TS2502 '[s]'  (symbol-keyed static)
//! class D { static x: typeof D.x; }                // TS2502 'x'    (string-keyed static)
//! class E { x: typeof this.x; }                    // TS2502 'x'    (instance via `this`)
//! class F { static readonly y: typeof F.y = 0; }   // TS2502 'y'    (static readonly)
//! class H { static a: typeof H.b; static b: typeof H.a; }  // TS2502 a, TS2502 b (indirect)
//! ```
//!
//! The self-referential **variable** form (`const v: typeof v`) and the
//! interface / type-literal indexed forms (`I["x"]`, `{ x: T["x"] }`) already
//! emit TS2502 through their own resolution paths; class *property* members
//! went through neither, so a self-referential class annotation was a silent
//! false negative (issue #14819). The plain instance indexed form
//! (`class G { x: G["x"]; }`, where the object is the *instance* type) is
//! already covered elsewhere and is deliberately left to that path here.
//!
//! Detection is **symbol/receiver gated**, mirroring the TS7023/TS7024 sibling
//! ([`super::class_member_circularity`]): a `Class.X` / `this.X` / `Class[K]`
//! reference is only a self-reference when the receiver resolves to the
//! enclosing class (the class name selects the static side; `this` selects the
//! side of the member it appears in). An unrelated member whose spelling merely
//! collides is never matched, and a reference to a *different* member of the
//! same class is an edge in the dependency graph, circular only if it closes a
//! cycle.

use super::class_member_circularity::cyclic_member_indices;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Identity by which a referenced member is matched back to a declared member.
/// Named members match by their literal property name; symbol-keyed computed
/// members (`[s]`) match by the binder symbol of the key expression, so the
/// declaration `[s]` and the reference `typeof C[typeof s]` resolve to the same
/// key without any rendered-name string comparison.
#[derive(Clone, PartialEq, Eq, Hash)]
enum MemberKey {
    Named(String),
    Symbol(SymbolId),
}

struct AnnotatedMember {
    /// Node the diagnostic is anchored at (the property name).
    name_node: NodeIndex,
    is_static: bool,
    key: MemberKey,
    /// How the member name is rendered in the TS2502 message.
    display: String,
    type_annotation: NodeIndex,
}

impl<'a> CheckerState<'a> {
    /// Emit TS2502 for class properties whose declared type annotation is
    /// circular through a `typeof Class.m` / `typeof this.m` / `typeof Class[k]`
    /// self-reference. Runs once per class, after its members are checked.
    pub(crate) fn check_class_member_circular_annotations(
        &mut self,
        class_idx: NodeIndex,
        members: &[NodeIndex],
    ) {
        if self.has_syntax_parse_errors() || self.is_js_file() {
            return;
        }

        let candidates = self.collect_annotated_members(members);
        if candidates.is_empty() {
            return;
        }

        // (is_static_side, key) -> candidate index. `this`/class-name receivers
        // and computed-symbol keys are normalized into this space so a reference
        // resolves to at most one member.
        let mut by_key: FxHashMap<(bool, MemberKey), usize> = FxHashMap::default();
        for (idx, member) in candidates.iter().enumerate() {
            by_key
                .entry((member.is_static, member.key.clone()))
                .or_insert(idx);
        }

        let class_sym = self.ctx.binder.get_node_symbol(class_idx);

        // Edge i -> j when member i's annotation references member j.
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); candidates.len()];
        for (i, member) in candidates.iter().enumerate() {
            let mut targets: FxHashSet<usize> = FxHashSet::default();
            self.collect_annotation_self_references(
                member.type_annotation,
                member.is_static,
                class_sym,
                &by_key,
                &mut targets,
            );
            adjacency[i] = targets.into_iter().collect();
        }

        for idx in cyclic_member_indices(&adjacency) {
            let message = format!(
                "'{}' is referenced directly or indirectly in its own type annotation.",
                candidates[idx].display
            );
            self.error_at_node(candidates[idx].name_node, &message, 2502);
        }
    }

    fn collect_annotated_members(&self, members: &[NodeIndex]) -> Vec<AnnotatedMember> {
        let mut candidates = Vec::new();
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
            let Some((key, display)) = self.member_key_and_display(prop.name) else {
                continue;
            };
            candidates.push(AnnotatedMember {
                name_node: prop.name,
                is_static: self.has_static_modifier(&prop.modifiers),
                key,
                display,
                type_annotation: prop.type_annotation,
            });
        }
        candidates
    }

    /// The matching key and rendered name for a property-name node. A computed
    /// name `[s]` resolves to its key symbol and renders as `[s]`; every other
    /// literal name keys and renders by its text (string-literal names quoted,
    /// matching how `tsc` renders TS2502 for quoted members).
    fn member_key_and_display(&self, name_idx: NodeIndex) -> Option<(MemberKey, String)> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let expr = computed.expression;
            let expr_node = self.ctx.arena.get(expr)?;
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let sym = self.resolve_identifier_symbol_without_tracking(expr)?;
            let text = self.ctx.arena.get_identifier_at(expr)?.escaped_text.clone();
            return Some((MemberKey::Symbol(sym), format!("[{text}]")));
        }

        let raw = crate::types_domain::queries::core::get_literal_property_name(
            self.ctx.arena,
            name_idx,
        )?;
        let display = if name_node.kind == SyntaxKind::StringLiteral as u16 {
            format!("\"{raw}\"")
        } else {
            raw.clone()
        };
        Some((MemberKey::Named(raw), display))
    }

    /// Walk a property's type annotation, recording which candidate members it
    /// self-references. Each `typeof Class.m` / `typeof this.m` / `typeof Class[k]`
    /// node contributes at most one target; the generic descent below also covers
    /// composite annotations (`typeof C.x | number`, `Array<typeof C.x>`, ...).
    fn collect_annotation_self_references(
        &self,
        node_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
        by_key: &FxHashMap<(bool, MemberKey), usize>,
        targets: &mut FxHashSet<usize>,
    ) {
        if node_idx.is_none() {
            return;
        }
        if let Some(reference) =
            self.annotation_node_member_ref(node_idx, current_is_static, class_sym)
            && let Some(&j) = by_key.get(&reference)
        {
            targets.insert(j);
        }
        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_annotation_self_references(
                child_idx,
                current_is_static,
                class_sym,
                by_key,
                targets,
            );
        }
    }

    /// If `node_idx` is a `typeof Class.m` / `typeof this.m` query or a
    /// `(typeof Class)[k]` / `(typeof this)[k]` indexed access whose receiver is
    /// the enclosing class, return the `(target_is_static, key)` it references.
    fn annotation_node_member_ref(
        &self,
        node_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<(bool, MemberKey)> {
        let node = self.ctx.arena.get(node_idx)?;

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            let query = self.ctx.arena.get_type_query(node)?;
            let entity = self.ctx.arena.get(query.expr_name)?;
            if entity.kind != syntax_kind_ext::QUALIFIED_NAME {
                return None;
            }
            let qn = self.ctx.arena.get_qualified_name(entity)?;
            let side = self.receiver_static_side(qn.left, current_is_static, class_sym)?;
            let name = self
                .ctx
                .arena
                .get_identifier_at(qn.right)?
                .escaped_text
                .clone();
            return Some((side, MemberKey::Named(name)));
        }

        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            let indexed = self.ctx.arena.get_indexed_access_type(node)?;
            let object = self.ctx.arena.get(indexed.object_type)?;
            // Only `typeof Class[..]` / `typeof this[..]` is handled here; the
            // bare `Class[..]` (instance-type) form is owned by the existing
            // indexed-access self-reference path.
            if object.kind != syntax_kind_ext::TYPE_QUERY {
                return None;
            }
            let object_query = self.ctx.arena.get_type_query(object)?;
            let side =
                self.receiver_static_side(object_query.expr_name, current_is_static, class_sym)?;
            let key = self.index_member_key(indexed.index_type)?;
            return Some((side, key));
        }

        None
    }

    /// Resolve a `typeof` receiver entity (`Class` or `this`) to the static side
    /// it selects, or `None` when it is not the enclosing class. The bare class
    /// name selects the static side (`typeof Class`); `this` selects the side of
    /// the member the annotation appears in.
    fn receiver_static_side(
        &self,
        receiver_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<bool> {
        let receiver = self.ctx.arena.get(receiver_idx)?;
        if receiver.kind == SyntaxKind::ThisKeyword as u16 {
            return Some(current_is_static);
        }
        if receiver.kind == SyntaxKind::Identifier as u16
            && class_sym.is_some()
            && self.resolve_identifier_symbol_without_tracking(receiver_idx) == class_sym
        {
            return Some(true);
        }
        None
    }

    /// Resolve the key of an indexed-access index node: a string/numeric literal
    /// type keys by name; a `typeof s` query keys by the binder symbol of `s`
    /// (matching a computed-symbol member declaration `[s]`).
    fn index_member_key(&self, index_idx: NodeIndex) -> Option<MemberKey> {
        let index = self.ctx.arena.get(index_idx)?;
        if index.kind == syntax_kind_ext::LITERAL_TYPE {
            let lit_type = self.ctx.arena.get_literal_type(index)?;
            let inner = self.ctx.arena.get(lit_type.literal)?;
            let lit = self.ctx.arena.get_literal(inner)?;
            return Some(MemberKey::Named(lit.text.clone()));
        }
        if index.kind == syntax_kind_ext::TYPE_QUERY {
            let query = self.ctx.arena.get_type_query(index)?;
            let entity = self.ctx.arena.get(query.expr_name)?;
            if entity.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let sym = self.resolve_identifier_symbol_without_tracking(query.expr_name)?;
            return Some(MemberKey::Symbol(sym));
        }
        None
    }
}
