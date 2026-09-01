use crate::diagnostics::Diagnostic;
/// Raw phases stay independently queryable; the compiler publishes all of them.
pub(super) struct DiagnosticPhaseProducts<'a>(pub(super) [&'a [Diagnostic]; 3]);
impl<'a> DiagnosticPhaseProducts<'a> {
    pub(super) fn append_to(self, diagnostics: &mut Vec<Diagnostic>) {
        diagnostics.extend(self.0.into_iter().flatten().cloned());
    }
}
