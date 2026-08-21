use crate::program::ProgramFile;
use crate::syntax::StatementKind;

/// Declaration pruning in an external module depends on checked reference
/// reachability. Until emit receives that summary, omit the declaration
/// product instead of publishing every private declaration or dropping a
/// private declaration used by an exported API.
pub(super) fn requires_checked_declaration_reachability(file: &ProgramFile) -> bool {
    file.is_external_module()
        && file
            .syntax
            .statements
            .iter()
            .any(|statement| match &statement.kind {
                StatementKind::Import(_) => true,
                StatementKind::Variable(declaration) => !declaration.exported,
                StatementKind::Function(declaration) => !declaration.exported,
                StatementKind::Class(declaration) => {
                    !declaration.exported && !declaration.default_export
                }
                StatementKind::TypeAlias(declaration) => !declaration.exported,
                StatementKind::Interface(declaration) => !declaration.exported,
                StatementKind::Export(_)
                | StatementKind::If(_)
                | StatementKind::Switch(_)
                | StatementKind::Break(_)
                | StatementKind::Continue(_)
                | StatementKind::Return(_)
                | StatementKind::Block(_)
                | StatementKind::Expression(_)
                | StatementKind::Empty
                | StatementKind::Unknown => false,
            })
}
