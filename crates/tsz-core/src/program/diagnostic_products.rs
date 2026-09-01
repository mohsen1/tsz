use crate::diagnostics::Diagnostic;
/// Raw phases stay independently queryable; the compiler publishes the first.
pub(super) struct DiagnosticPhaseProducts<'a>(pub(super) [&'a [Diagnostic]; 3]);
impl<'a> DiagnosticPhaseProducts<'a> {
    pub(super) fn compiler_product(self) -> &'a [Diagnostic] {
        self.0
            .into_iter()
            .find(|diagnostics| !diagnostics.is_empty())
            .unwrap_or_default()
    }
}
