//! Circular implicit-`any` return detection for **class** members (TS7023 /
//! TS7024).
//!
//! `tsc` reports an implicit-`any`-circularity diagnostic when an un-annotated
//! class member's inferred return type is referenced, directly or indirectly,
//! in one of its own return expressions:
//!
//! ```ts
//! class C { m() { return this.m(); } }          // TS7023 'm'
//! class C { f = () => this.f(); }               // TS7024 (anonymous arrow)
//! class C { static f = () => C.f(); }           // TS7024
//! class C { get g() { return this.g; } }        // TS7023 'g'  (read invokes getter)
//! class C { a() { return this.b(); } b() { return this.a(); } } // TS7023 a, TS7023 b
//! ```
//!
//! The variable / object-literal forms are already handled (the resolving-symbol
//! tracker in `function_type_circular.rs` and the object-literal graph in
//! `object_literal_circularity.rs`). Class member symbols carry neither the
//! function/block-scoped variable flags that the former keys on, nor do they go
//! through the object-literal path, so a self-referential class member was a
//! silent false negative (issue #14805).
//!
//! Detection here is **symbol/receiver gated**, never name-keyed against an
//! arbitrary receiver: a `recv.X` self-reference is only a self-invocation when
//! `recv` resolves to the enclosing class (`this`, or the class name for the
//! static side). An unrelated `obj.X` access whose name merely collides with a
//! member is not a self-reference — the same lesson the object-literal `this`
//! gate encodes, and the inverse of the FP tracked by #14730.

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// What kind of self-reference makes a member's inferred return type circular,
/// and how the diagnostic is anchored.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassCircularMemberKind {
    /// `m() {...}` — a self-invocation requires a **call** `this.m()`. A bare
    /// read `this.m` is just the method's function value and is not circular.
    /// Diagnostic: TS7023 at the method name.
    Method,
    /// `get g() {...}` — reading `this.g` *invokes* the getter, so a property
    /// **read** (or call) counts. Diagnostic: TS7023 at the accessor name.
    Getter,
    /// `f = () => ...` — a self-invocation requires a **call** `this.f()`.
    /// Diagnostic: TS7024 (anonymous) at the arrow-function node. A
    /// `function` expression field instead rebinds `this` (TS2683) and is not a
    /// candidate.
    ArrowField,
}

struct ClassCircularMember {
    /// Body whose return expressions are scanned (method/accessor body, or the
    /// arrow body for an arrow field).
    body_idx: NodeIndex,
    kind: ClassCircularMemberKind,
    is_static: bool,
    /// Node the diagnostic is anchored at (member name, or arrow node).
    diagnostic_node: NodeIndex,
    /// Member name, used both as the self-reference lookup key and (for named
    /// members) as the TS7023 message argument. Anonymous arrow fields report
    /// TS7024 and ignore this for display — see [`ClassCircularMemberKind`].
    name: String,
}

impl CheckerState<'_> {
    /// Emit TS7023 / TS7024 for un-annotated class members whose inferred return
    /// type is circular through a `this.`/`Class.` self-invocation. Runs once
    /// per class, after the members have been checked.
    pub(crate) fn check_class_member_circular_returns(
        &mut self,
        class_idx: NodeIndex,
        members: &[NodeIndex],
    ) {
        if !self.ctx.no_implicit_any() || self.has_syntax_parse_errors() || self.is_js_file() {
            return;
        }

        let candidates = self.collect_class_circular_candidates(members);
        if candidates.is_empty() {
            return;
        }

        // (is_static, name) -> candidate index, used to resolve `this.X` /
        // `Class.X` back to a member. Private names already carry a `#`, so they
        // never collide with public members of the same spelling.
        let mut by_key: FxHashMap<(bool, String), usize> = FxHashMap::default();
        for (idx, member) in candidates.iter().enumerate() {
            by_key
                .entry((member.is_static, member.name.clone()))
                .or_insert(idx);
        }

        let class_sym = self.ctx.binder.get_node_symbol(class_idx);

        // Edge i -> j when member i's return expressions self-invoke member j.
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); candidates.len()];
        for (i, member) in candidates.iter().enumerate() {
            let mut return_exprs = Vec::new();
            self.collect_initializer_return_expressions_in_function_body(
                member.body_idx,
                &mut return_exprs,
            );
            let mut targets: FxHashSet<usize> = FxHashSet::default();
            for &expr_idx in &return_exprs {
                self.collect_class_self_invocations(
                    expr_idx,
                    member.is_static,
                    class_sym,
                    &by_key,
                    &candidates,
                    &mut targets,
                );
            }
            adjacency[i] = targets.into_iter().collect();
        }

        let cyclic = cyclic_member_indices(&adjacency);
        for idx in cyclic {
            self.emit_class_circular_member_diagnostic(&candidates[idx]);
        }
    }

    fn emit_class_circular_member_diagnostic(&mut self, member: &ClassCircularMember) {
        use crate::diagnostics::diagnostic_codes;
        // Arrow-function fields are anonymous (TS7024); methods and getters are
        // named (TS7023 with the member name).
        if member.kind == ClassCircularMemberKind::ArrowField {
            self.error_at_node_msg(
                member.diagnostic_node,
                diagnostic_codes::FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A,
                &[],
            );
        } else {
            self.error_at_node_msg(
                member.diagnostic_node,
                diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                &[&member.name],
            );
        }
    }

    fn collect_class_circular_candidates(&self, members: &[NodeIndex]) -> Vec<ClassCircularMember> {
        let mut candidates = Vec::new();
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Method: un-annotated return type, with a body. Generator methods
            // are included — `*m() { return this.m(); }` is circular through its
            // `Generator` return value, exactly as `tsc` reports (a `yield`
            // self-reference is not detected here, matching the object-literal
            // mechanism's return-expression scope).
            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(node)
                    && method.type_annotation.is_none()
                    && method.body.is_some()
                    && let Some(name) = self.get_property_name(method.name)
                {
                    candidates.push(ClassCircularMember {
                        body_idx: method.body,
                        kind: ClassCircularMemberKind::Method,
                        is_static: self.has_static_modifier(&method.modifiers),
                        diagnostic_node: method.name,
                        name,
                    });
                }
                continue;
            }

            // Getter: un-annotated return type, with a body. (Setters have no
            // return value and cannot be return-type circular.)
            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && accessor.type_annotation.is_none()
                    && accessor.body.is_some()
                    && let Some(name) = self.get_property_name(accessor.name)
                {
                    candidates.push(ClassCircularMember {
                        body_idx: accessor.body,
                        kind: ClassCircularMemberKind::Getter,
                        is_static: self.has_static_modifier(&accessor.modifiers),
                        diagnostic_node: accessor.name,
                        name,
                    });
                }
                continue;
            }

            // Arrow-function field: `f = () => ...` with no return-type
            // annotation. A `function` expression field rebinds `this` and is
            // reported as TS2683 instead, so it is not a candidate.
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.ctx.arena.get_property_decl(node)
                && prop.type_annotation.is_none()
                && prop.initializer.is_some()
            {
                let init_idx = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(prop.initializer);
                if let Some(init_node) = self.ctx.arena.get(init_idx)
                    && init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    && let Some(func) = self.ctx.arena.get_function(init_node)
                    && func.type_annotation.is_none()
                    && func.body.is_some()
                    && let Some(name) = self.get_property_name(prop.name)
                {
                    candidates.push(ClassCircularMember {
                        body_idx: func.body,
                        kind: ClassCircularMemberKind::ArrowField,
                        is_static: self.has_static_modifier(&prop.modifiers),
                        diagnostic_node: init_idx,
                        name,
                    });
                }
            }
        }
        candidates
    }

    /// Walk a single return expression subtree, recording which candidate
    /// members it self-invokes. Stops at nested function/class boundaries that
    /// rebind `this` or introduce a fresh lexical scope; arrow functions are
    /// deliberately *not* descended into here — a self-call hidden inside a
    /// nested closure makes the closure circular, not the enclosing member
    /// (matching the object-literal `this`-member graph).
    fn collect_class_self_invocations(
        &self,
        expr_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
        by_key: &FxHashMap<(bool, String), usize>,
        candidates: &[ClassCircularMember],
        targets: &mut FxHashSet<usize>,
    ) {
        if self.expression_is_void_prefix_unary(expr_idx) {
            return;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return;
        }

        // A call whose callee is `this.X` / `Class.X` invokes member X — valid
        // for any member kind (method, getter, or arrow field).
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
        {
            let callee = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(call.expression);
            if let Some((target_static, name)) =
                self.class_self_member_access(callee, current_is_static, class_sym)
                && let Some(&j) = by_key.get(&(target_static, name))
            {
                targets.insert(j);
            }
        }

        // A bare read `this.X` / `Class.X` invokes member X only when X is a
        // getter (property access triggers the accessor); a method/field read is
        // just the member's value and is not circular.
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some((target_static, name)) =
                self.class_self_member_access(expr_idx, current_is_static, class_sym)
            && let Some(&j) = by_key.get(&(target_static, name))
            && candidates[j].kind == ClassCircularMemberKind::Getter
        {
            targets.insert(j);
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_class_self_invocations(
                child_idx,
                current_is_static,
                class_sym,
                by_key,
                candidates,
                targets,
            );
        }
    }

    /// If `access_idx` is a property access `recv.NAME` whose receiver resolves
    /// to the enclosing class (`this`, or the class name on the static side),
    /// return `(target_is_static, NAME)`. `this` selects the instance side from
    /// an instance member and the static side from a static member; the bare
    /// class name always selects the static side.
    fn class_self_member_access(
        &self,
        access_idx: NodeIndex,
        current_is_static: bool,
        class_sym: Option<SymbolId>,
    ) -> Option<(bool, String)> {
        let node = self.ctx.arena.get(access_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let receiver = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let receiver_node = self.ctx.arena.get(receiver)?;

        let target_static = if receiver_node.kind == SyntaxKind::ThisKeyword as u16 {
            current_is_static
        } else if receiver_node.kind == SyntaxKind::Identifier as u16
            && class_sym.is_some()
            && self.resolve_identifier_symbol_without_tracking(receiver) == class_sym
        {
            true
        } else {
            return None;
        };

        let name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;
        Some((target_static, name))
    }
}

/// Indices of candidates that lie on a self-invocation cycle (including direct
/// self-loops). DFS path-stack collection, mirroring the object-literal
/// `collect_circular_return_graph_sites` cycle walk. Shared with the class
/// member type-annotation circularity detector (`class_member_type_circularity`).
pub(crate) fn cyclic_member_indices(adjacency: &[Vec<usize>]) -> FxHashSet<usize> {
    let mut cyclic = FxHashSet::default();
    let mut visited = vec![false; adjacency.len()];
    let mut stack = Vec::new();
    for start in 0..adjacency.len() {
        collect_cycle(start, adjacency, &mut visited, &mut stack, &mut cyclic);
    }
    cyclic
}

fn collect_cycle(
    node: usize,
    adjacency: &[Vec<usize>],
    visited: &mut [bool],
    stack: &mut Vec<usize>,
    cyclic: &mut FxHashSet<usize>,
) {
    if let Some(pos) = stack.iter().position(|&n| n == node) {
        cyclic.extend(stack[pos..].iter().copied());
        return;
    }
    if visited[node] {
        return;
    }
    stack.push(node);
    for &target in &adjacency[node] {
        collect_cycle(target, adjacency, visited, stack, cyclic);
    }
    stack.pop();
    visited[node] = true;
}
