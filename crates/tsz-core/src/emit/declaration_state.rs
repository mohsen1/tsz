//! Typed declaration-printer state: checked inputs and product completion share one owner.
use super::Printer;
use crate::program::DeclarationDisplaySummaries;

#[derive(Debug, Clone, Copy)]
pub(super) enum DeclarationState<'a> {
    NotEmitting,
    Emitting(Option<&'a DeclarationDisplaySummaries>),
    Incomplete,
}

impl<'a> Printer<'a> {
    pub(super) fn finish_declaration(self) -> Option<String> {
        matches!(self.declaration_state, DeclarationState::Emitting(_)).then_some(self.output)
    }

    pub(super) const fn begin_declaration(
        &mut self,
        summaries: Option<&'a DeclarationDisplaySummaries>,
    ) {
        self.declaration_state = DeclarationState::Emitting(summaries);
    }

    pub(super) const fn ensure_declaration_started(&mut self) {
        if matches!(self.declaration_state, DeclarationState::NotEmitting) {
            self.begin_declaration(None);
        }
    }

    pub(super) const fn reject_declaration(&mut self) {
        self.declaration_state = DeclarationState::Incomplete;
    }

    pub(super) const fn declaration_is_complete(&self) -> bool {
        matches!(self.declaration_state, DeclarationState::Emitting(_))
    }

    pub(super) const fn declaration_summaries(&self) -> Option<&'a DeclarationDisplaySummaries> {
        match self.declaration_state {
            DeclarationState::Emitting(summaries) => summaries,
            DeclarationState::NotEmitting | DeclarationState::Incomplete => None,
        }
    }

    pub(super) const fn emitting_declaration(&self) -> bool {
        !matches!(self.declaration_state, DeclarationState::NotEmitting)
    }
}
