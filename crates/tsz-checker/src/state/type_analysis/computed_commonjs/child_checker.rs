use crate::state::CheckerState;
use tsz_common::perf_counters::CheckerCreationReason;

impl<'a> CheckerState<'a> {
    pub(super) fn with_commonjs_child_checker_for_file<R>(
        &mut self,
        target_file_idx: usize,
        f: impl FnOnce(&mut CheckerState<'_>) -> R,
    ) -> Option<R> {
        self.with_commonjs_child_checker_for_file_inner(target_file_idx, true, f, || {})
    }

    pub(super) fn with_commonjs_child_checker_for_file_without_merge<R>(
        &mut self,
        target_file_idx: usize,
        f: impl FnOnce(&mut CheckerState<'_>) -> R,
    ) -> Option<R> {
        self.with_commonjs_child_checker_for_file_inner(target_file_idx, false, f, || {})
    }

    pub(super) fn with_commonjs_child_checker_for_file_before_merge<R>(
        &mut self,
        target_file_idx: usize,
        f: impl FnOnce(&mut CheckerState<'_>) -> R,
        before_merge: impl FnOnce(),
    ) -> Option<R> {
        self.with_commonjs_child_checker_for_file_inner(target_file_idx, true, f, before_merge)
    }

    fn with_commonjs_child_checker_for_file_inner<R>(
        &mut self,
        target_file_idx: usize,
        merge_symbol_file_targets: bool,
        f: impl FnOnce(&mut CheckerState<'_>) -> R,
        before_merge: impl FnOnce(),
    ) -> Option<R> {
        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;
        let arena = all_arenas.get(target_file_idx)?;
        let binder = all_binders.get(target_file_idx)?;
        let source_file = arena.source_files.first()?;

        let mut checker = CheckerState::delegate_for_arena(
            arena.as_ref(),
            binder.as_ref(),
            source_file.file_name.clone(),
            self,
            CheckerCreationReason::CjsExports,
        );
        checker.ctx.current_file_idx = target_file_idx;

        let result = f(&mut checker);
        if merge_symbol_file_targets {
            before_merge();
            self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        }
        Some(result)
    }
}
