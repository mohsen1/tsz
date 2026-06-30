//! Cross-arena resolution of the value type contributed by a module
//! augmentation's NEW exports (#14853).
//!
//! When `declare module "x" { export const c: T }` (or `function`/`class`/
//! `enum`) augments an ambient module declared in another file and *adds a new
//! export* (rather than merging into an existing one), the augmentation
//! declaration node lives in a foreign arena relative to the file currently
//! being checked. The merge previously typed such a new export `any`, dropping
//! every assignability error against it. This routes the declaration through a
//! delegate child checker over the owning arena/binder so the real declared
//! type is recovered, mirroring
//! `delegate_cross_arena_interface_member_simple_types`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Resolve the declared value type of an augmentation export declaration
    /// `node`, honoring the `arena` that owns it.
    ///
    /// Returns `None` when no concrete type can be recovered (the caller is
    /// responsible for the `any` fallback). For a same-arena declaration this
    /// resolves directly; for a foreign-arena declaration it constructs a
    /// transient delegate child checker over the owning arena/binder so type
    /// references in the declaration resolve against the correct symbol table.
    pub(crate) fn augmentation_export_declaration_type(
        &mut self,
        node: NodeIndex,
        arena: &tsz_parser::parser::NodeArena,
    ) -> Option<TypeId> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.augmentation_node_value_type_local(node);
        }

        // O(1) via global_arena_index; falls back to None when the arena is not
        // part of the program overlay (e.g. lib arenas), in which case the
        // current binder is the best available interpreter.
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let delegate_binder_arc = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let delegate_binder = delegate_binder_arc.as_deref()?;

        if !Self::enter_cross_arena_delegation() {
            return None;
        }
        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        let delegate_file_name = arena
            .source_files
            .first()
            .map_or_else(|| self.ctx.file_name.clone(), |sf| sf.file_name.clone());

        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        // Transient delegation child: diagnostics are discarded at teardown.
        checker.ctx.diagnostics_discarded = true;
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);

        let result = checker.augmentation_node_value_type_local(node);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        result
    }

    /// Resolve the value type of an augmentation export declaration `node`
    /// interpreted in the *current* checker's arena/binder.
    ///
    /// Prefers a variable declaration's explicit type annotation (matching the
    /// established same-file path), then falls back to the declared symbol's
    /// type, which uniformly covers `function`/`class`/`enum` declarations as
    /// the value they introduce.
    fn augmentation_node_value_type_local(&mut self, node: NodeIndex) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        // A type only "resolved" enough to use as the export's type when it is
        // neither the error nor the unevaluated sentinel.
        let concrete = |type_id: TypeId| {
            (type_id != TypeId::ERROR && type_id != TypeId::UNKNOWN).then_some(type_id)
        };

        let arena = self.ctx.arena;
        // For a variable declaration prefer the explicit type annotation, matching
        // the established same-file path (`get_type_of_node` on the declaration node
        // would resolve through its initializer / declared type indirectly).
        let annotation = arena
            .get(node)
            .filter(|decl_node| decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION)
            .and_then(|decl_node| arena.get_variable_declaration(decl_node))
            .map(|decl| decl.type_annotation)
            .filter(NodeIndex::is_some);

        if let Some(type_annotation) = annotation
            && let Some(type_id) = concrete(self.get_type_of_node(type_annotation))
        {
            return Some(type_id);
        }

        // Resolve through the declared symbol: this yields the value the
        // declaration introduces — a class's constructor type (`typeof C`), an
        // enum's enum object type, an annotation-less variable's declared type —
        // which `get_type_of_node` on the raw declaration node does not (it would
        // produce the class instance type or `void` for an enum).
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && let Some(type_id) = concrete(self.get_type_of_symbol(sym_id))
        {
            return Some(type_id);
        }

        // Function declarations are keyed on their name node rather than the
        // declaration node, so `get_node_symbol` misses them; the declaration
        // node's own type is the function type in that case.
        concrete(self.get_type_of_node(node))
    }
}
