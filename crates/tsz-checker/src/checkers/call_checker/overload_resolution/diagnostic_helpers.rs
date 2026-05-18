use crate::query_boundaries::common::PendingDiagnosticBuilder;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn argument_not_assignable_for_overload_arg(
        &self,
        args: &[NodeIndex],
        index: usize,
        actual: TypeId,
        expected: TypeId,
    ) -> tsz_solver::PendingDiagnostic {
        let diagnostic = PendingDiagnosticBuilder::argument_not_assignable(actual, expected);
        let Some(&arg_idx) = args.get(index) else {
            return diagnostic;
        };
        let Some(loc) = self.get_source_location(arg_idx) else {
            return diagnostic;
        };
        let length = loc.length();

        diagnostic.with_span(tsz_solver::SourceSpan::new(loc.file, loc.start, length))
    }
}
