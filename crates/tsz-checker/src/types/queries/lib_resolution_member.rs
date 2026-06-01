//! Single-member resolution for simple lib-interface property access.
//!
//! Value-position property access on a lib-interface receiver (e.g.
//! `document.title`) only needs the accessed member's type, not the receiver's
//! entire structural shape. [`resolve_lib_type_by_name`] lowers **every** member
//! and the transitive `extends` closure (~9216 interned types for `Document`);
//! this helper lowers **only** the requested property by reusing the exact same
//! `TypeLowering` configuration the full path uses, so the resulting member type
//! is byte-identical.
//!
//! Scope (intentionally narrow for soundness — anything else returns `None` and
//! falls back to the full-materialization path):
//! - Own **plain property signatures** only (`prop: T`). Methods, accessors,
//!   index signatures, call/construct signatures, and computed/symbol-named
//!   members take the full path.
//! - A single own declaration of the member. Members declared more than once
//!   (overloads / split declarations) take the full path.
//! - Heritage-inherited members are **not** resolved here yet; an own-member
//!   miss returns `None` so the inherited-member lookup stays on the proven
//!   full path.
//!
//! [`resolve_lib_type_by_name`]: super::lib_resolution::CheckerState::resolve_lib_type_by_name

use tsz_lowering::TypeLowering;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

use super::lib_decls::{collect_lib_decls_with_arenas_in_contexts, resolve_lib_fallback_arena};
use super::lib_resolution::{lib_def_id_from_node, resolve_lib_node_in_arenas};
use super::lib_resolution_selected::selected_lib_symbol_for_name;

use crate::state::CheckerState;

impl CheckerState<'_> {
    /// Maximum number of `extends`-closure levels the lazy member walk will
    /// descend before giving up and returning `None` (caller falls back to full
    /// materialization). The DOM heritage graph (`Document -> ParentNode`,
    /// `Document -> EventTarget` via `Node`) is shallow; this bound keeps the
    /// walk from looping on a malformed/cyclic lib graph.
    const LAZY_MEMBER_HERITAGE_MAX_DEPTH: u32 = 8;

    /// Resolve a single own-or-inherited **plain property** or **method overload
    /// set** `prop_name` of the simple lib interface named `name`, returning its
    /// lowered member type without materializing the rest of the interface or its
    /// `extends` closure.
    ///
    /// Returns `None` (caller falls back to full materialization) when:
    /// - the interface symbol cannot be selected,
    /// - the member is neither a plain property signature nor a method-overload
    ///   set (accessors, index signatures, mixed property+method),
    /// - a plain property is declared more than once (split declaration),
    /// - the member has a computed/symbol name,
    /// - the member is not declared on this interface and cannot be found on
    ///   exactly one eligible base in the `extends` closure, or
    /// - lowering the member fails.
    ///
    /// The member's type is produced by the same `TypeLowering` calls the full
    /// lib path (`lower_merged_interface_declarations` /
    /// `finish_interface_parts`) uses and is lowered in its **declaring**
    /// interface's context, so the result is byte-identical to full
    /// materialization for the eligible shape.
    pub(crate) fn resolve_simple_lib_interface_own_property(
        &mut self,
        name: &str,
        prop_name: &str,
    ) -> Option<TypeId> {
        self.resolve_simple_lib_interface_member_at_depth(name, prop_name, 0)
    }

    /// Heritage-aware member resolution: tries the interface's own declarations
    /// first, then walks the bare-`extends` closure (depth-bounded) for an
    /// inherited declaration. `depth` guards against deep/cyclic lib heritage.
    fn resolve_simple_lib_interface_member_at_depth(
        &mut self,
        name: &str,
        prop_name: &str,
        depth: u32,
    ) -> Option<TypeId> {
        if self.ctx.skip_lib_type_resolution {
            return None;
        }
        if depth > Self::LAZY_MEMBER_HERITAGE_MAX_DEPTH {
            return None;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();
        let lib_binders = self.get_lib_binders();

        let sym_id = if self.ctx.file_local_type_shadow_for_lib_name(name) {
            None
        } else {
            self.ctx.binder.file_locals.get(name)
        }
        .or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        });

        let (sym_id, selected_binder_arc) =
            selected_lib_symbol_for_name(&self.ctx, name, sym_id, &lib_binders)?;
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        let symbol = selected_binder.get_symbol_with_libs(sym_id, &lib_binders)?;

        let fallback_arena =
            resolve_lib_fallback_arena(selected_binder, sym_id, &lib_contexts, self.ctx.arena);
        let decls_with_arenas = collect_lib_decls_with_arenas_in_contexts(
            selected_binder,
            sym_id,
            &symbol.declarations,
            fallback_arena,
            &lib_contexts,
            Some(self.ctx.arena),
        );

        // Find the own declaration(s) of `prop_name` across the interface's
        // declarations. We support two shapes, both byte-identical to the full
        // path:
        // - exactly one plain `PropertySignature` (`prop: T`), or
        // - one or more `MethodSignature` overloads (`prop(...): R`), all sharing
        //   the same name (the DOM `querySelector` shape).
        // Any other combination (a property mixed with methods, an accessor, an
        // index signature, a string/computed/symbol member name, or a missing
        // member) is ambiguous or out of scope and returns `None` so the proven
        // full-materialization path stays authoritative.
        let mut property_member: Option<(NodeIndex, &NodeArena)> = None;
        // Method overload declarations of `prop_name`, in source order. A single
        // shared arena is required: the merged-callable lowering runs in one
        // `TypeLowering` bound to that arena, mirroring how the full path lowers
        // an interface body in its own arena.
        let mut method_members: Vec<NodeIndex> = Vec::new();
        let mut method_arena: Option<&NodeArena> = None;
        // Bare-identifier `extends` base names (no type arguments) collected from
        // this interface's heritage clauses, in source order, for the inherited-
        // member walk performed only when no own member is found.
        let mut heritage_base_names: Vec<String> = Vec::new();
        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                // A merged declaration that is not an interface body (e.g. the
                // companion `declare var Document: { ... }` value declaration).
                // Skip it; the interface body decl is elsewhere in the list.
                continue;
            };
            collect_bare_extends_base_names(arena, interface, &mut heritage_base_names);
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                let kind = member_node.kind;
                if kind != PROPERTY_SIGNATURE && kind != METHOD_SIGNATURE {
                    continue;
                }
                let Some(sig) = arena.get_signature(member_node) else {
                    continue;
                };
                // Plain identifier member name only — string-literal, computed,
                // and symbol names take the full path so their exact naming
                // semantics (quoting, symbol keys) stay authoritative.
                let Some(member_name) = arena.get_identifier_text(sig.name) else {
                    continue;
                };
                if member_name != prop_name {
                    continue;
                }
                if kind == PROPERTY_SIGNATURE {
                    if property_member.is_some() {
                        // Declared more than once — ambiguous, fall back.
                        return None;
                    }
                    property_member = Some((member_idx, arena));
                } else {
                    // Method overloads must all come from one arena so a single
                    // `TypeLowering` can lower the whole set; a cross-arena split
                    // is unexpected for an own method group, so fall back.
                    match method_arena {
                        Some(existing) if !std::ptr::eq(existing, arena) => return None,
                        _ => method_arena = Some(arena),
                    }
                    method_members.push(member_idx);
                }
            }
        }

        // A name declared as both a property and a method is a conflict the full
        // path resolves with its own merge semantics; do not approximate it here.
        if property_member.is_some() && !method_members.is_empty() {
            return None;
        }

        // No own declaration of `prop_name`: walk the bare-`extends` closure for
        // an inherited member. We resolve each base **by name**, so the inherited
        // member is lowered in its declaring interface's own context (correct for
        // base-relative `this`/type references). The base names were captured in
        // source order; we require exactly one base to declare the member so an
        // ambiguous diamond falls back to the full path.
        if property_member.is_none() && method_members.is_empty() {
            return self.resolve_inherited_lib_member(&heritage_base_names, prop_name, depth);
        }

        // Build the same hybrid-resolver TypeLowering the full lib path uses, so
        // the member lowers to a byte-identical type. The arena bound below is
        // the member's own arena (property annotation or method-overload set).
        let binder = selected_binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
                .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            lib_def_id_from_node(
                &self.ctx,
                binder,
                node_idx,
                &decls_with_arenas,
                fallback_arena,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let build_lowering = || {
            let lowering = TypeLowering::with_hybrid_resolver(
                fallback_arena,
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &resolver,
            )
            .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
            .with_lazy_type_params_resolver(&lazy_type_params_resolver)
            .with_name_def_id_resolver(&name_resolver);
            if self.ctx.all_binders.is_some() || self.ctx.global_file_locals_index.is_some() {
                lowering.prefer_name_def_id_resolution()
            } else {
                lowering
            }
        };

        // Method overload set: lower every overload signature as a method call
        // signature and assemble the merged callable, exactly like the full
        // interface path's `MethodSignature` arm + `finish_interface_parts`.
        // Gated by its own `TSZ_DISABLE_LAZY_METHOD` kill-switch so the
        // higher-risk method/overload path can be A/B compared independently of
        // the single-property path.
        if !method_members.is_empty() {
            if crate::state_checking::lazy_lib_member::lazy_lib_method_disabled() {
                return None;
            }
            let member_arena = method_arena?;
            let member_type = build_lowering()
                .with_arena(member_arena)
                .lower_method_overload_set_type(&method_members)?;
            if member_type == TypeId::ERROR {
                return None;
            }
            return Some(member_type);
        }

        let (member_idx, member_arena) = property_member?;
        let member_node = member_arena.get(member_idx)?;
        let sig = member_arena.get_signature(member_node)?;
        if sig.type_annotation == NodeIndex::NONE {
            // `prop;` with no annotation lowers to `any` in the full path; that
            // is cheap, but keep the full path authoritative for the implicit
            // shape rather than reimplement the default here.
            return None;
        }
        // Optional and readonly properties carry extra read/write semantics
        // (`?` interacts with `exactOptionalPropertyTypes`; `readonly` affects
        // the write type). Leave those on the full path so their exact behavior
        // is authoritative — this fast path only handles plain `prop: T`.
        if sig.question_token || self.has_readonly_modifier(&sig.modifiers) {
            return None;
        }

        let member_type = build_lowering()
            .with_arena(member_arena)
            .lower_type(sig.type_annotation);
        if member_type == TypeId::ERROR {
            return None;
        }
        Some(member_type)
    }

    /// Resolve `prop_name` inherited from a simple lib interface's bare-`extends`
    /// closure: try each base **by name** and return the lowered member type when
    /// exactly one base declares it. More than one declaring base is ambiguous
    /// (the full path owns merge/override semantics), so return `None`.
    ///
    /// Each base is resolved through
    /// [`Self::resolve_simple_lib_interface_member_at_depth`], which enforces the
    /// same simple-non-generic-unaugmented-unshadowed eligibility on the base and
    /// lowers the member in the base's own context. A base that fails eligibility
    /// simply yields `None` for that branch; if every branch is `None` the whole
    /// walk falls back to full materialization.
    ///
    /// Gated by the `TSZ_DISABLE_LAZY_METHOD` kill-switch alongside the
    /// method-overload fast path: the heritage walk is the other half of the
    /// vite-pattern closer (`document.querySelector` is inherited from
    /// `ParentNode`) and is A/B compared together with it.
    fn resolve_inherited_lib_member(
        &mut self,
        base_names: &[String],
        prop_name: &str,
        depth: u32,
    ) -> Option<TypeId> {
        if crate::state_checking::lazy_lib_member::lazy_lib_method_disabled() {
            return None;
        }
        if base_names.is_empty() {
            return None;
        }
        let mut found: Option<TypeId> = None;
        for base in base_names {
            // A base shadowed by a file-local type, or otherwise ineligible, must
            // not be resolved through the lazy path; the predicate inside the
            // recursive call enforces that and yields `None` for such a base.
            if self.ctx.file_local_type_shadow_for_lib_name(base) {
                return None;
            }
            if let Some(member_type) =
                self.resolve_simple_lib_interface_member_at_depth(base, prop_name, depth + 1)
            {
                if found.is_some() {
                    // Two bases both declare the member — ambiguous override;
                    // defer to the full path's merge semantics.
                    return None;
                }
                found = Some(member_type);
            }
        }
        found
    }
}

/// Append the bare-identifier `extends` base names (those with **no** type
/// arguments) of `interface` to `out`, in source order. Bases written with type
/// arguments (`extends Foo<T>`) or as qualified names are skipped: the lazy
/// member walk only follows non-generic bare bases, so a generic base falls back
/// to the full path.
fn collect_bare_extends_base_names(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
    out: &mut Vec<String>,
) {
    let Some(clauses) = interface.heritage_clauses.as_ref() else {
        return;
    };
    for &clause_idx in &clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            // A bare single-`Identifier` heritage base (`extends Foo`) carries no
            // type arguments — the common simple-lib-interface shape and the only
            // shape the lazy walk follows. `Document extends Node, ParentNode, …`
            // is stored this way in the lib arenas.
            if let Some(name) = arena.get_identifier_text(type_idx) {
                out.push(name.to_string());
                continue;
            }
            // A `TypeReference` base (`extends Foo<T>` or a qualified name) needs
            // argument substitution / namespace resolution the lazy walk cannot
            // supply; only follow it when it is a bare un-parameterized name.
            if let Some(type_ref) = arena.get_type_ref(type_node) {
                let has_type_arguments = type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                if !has_type_arguments
                    && let Some(name) = arena.get_identifier_text(type_ref.type_name)
                {
                    out.push(name.to_string());
                }
                continue;
            }
            // Class-style heritage written as an expression-with-type-arguments
            // (defensive; interface heritage in lib arenas uses the forms above).
            if let Some(expr_args) = arena.get_expr_type_args(type_node) {
                let has_type_arguments = expr_args
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                if !has_type_arguments
                    && let Some(name) = arena.get_identifier_text(expr_args.expression)
                {
                    out.push(name.to_string());
                }
            }
        }
    }
}
