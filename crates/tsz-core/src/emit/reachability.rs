use std::collections::HashSet;

use crate::program::{DeclarationDisplaySummaries, DefaultExportDeclaration, ProgramFile};
use crate::source::NodeId;
use crate::syntax::{ExpressionKind, Statement, StatementKind};

pub(super) enum DeclarationReachability {
    All,
    Owners(HashSet<NodeId>),
}

impl DeclarationReachability {
    pub(super) fn includes(&self, statement: &Statement) -> bool {
        matches!(self, Self::All)
            || matches!(statement.kind, StatementKind::Export(_))
            || matches!(self, Self::Owners(owners) if owners.contains(&statement.id))
    }
}

/// Prove the declaration owners that survive external-module pruning.
///
/// The checker publishes dependencies for inferred default exports. Emit only
/// consumes those stable declaration identities; it never reconstructs a
/// dependency from rendered type text.
pub(super) fn declaration_reachability(
    file: &ProgramFile,
    summaries: &DeclarationDisplaySummaries,
) -> Option<DeclarationReachability> {
    if !file.is_external_module() {
        return Some(DeclarationReachability::All);
    }
    if file
        .syntax
        .statements
        .iter()
        .any(|statement| matches!(statement.kind, StatementKind::Import(_)))
    {
        return None;
    }
    let declaration_owners = declaration_owners(file);
    let exported_owners = exported_owners(file);
    if declaration_owners.is_subset(&exported_owners) {
        return Some(DeclarationReachability::All);
    }

    let mut default_exports = file.syntax.statements.iter().filter_map(|statement| {
        let StatementKind::Export(export) = &statement.kind else {
            return None;
        };
        export
            .default_export
            .then_some(export.assignment.as_ref())
            .flatten()
            .map(|_| statement)
    });
    let default_export = default_exports.next()?;
    if default_exports.next().is_some() {
        return None;
    }
    let summary = summaries.default_export(file.source.id, default_export.id)?;
    let dependencies = match summary {
        DefaultExportDeclaration::Literal => &[][..],
        DefaultExportDeclaration::Typed { dependencies, .. } => dependencies.as_slice(),
    };
    let mut owners = HashSet::new();
    for dependency in dependencies {
        if dependency.file != file.source.id {
            return None;
        }
        let owner = file.bindings.declaration(*dependency)?.owner;
        if !declaration_owners.contains(&owner) {
            return None;
        }
        owners.insert(owner);
    }
    for owner in exported_owners {
        if !owners.contains(&owner) && !safe_export_root(file, owner) {
            return None;
        }
        owners.insert(owner);
    }
    Some(DeclarationReachability::Owners(owners))
}

fn declaration_owners(file: &ProgramFile) -> HashSet<NodeId> {
    file.syntax
        .statements
        .iter()
        .filter(|statement| {
            matches!(
                statement.kind,
                StatementKind::Variable(_)
                    | StatementKind::Function(_)
                    | StatementKind::Class(_)
                    | StatementKind::TypeAlias(_)
                    | StatementKind::Interface(_)
            )
        })
        .map(|statement| statement.id)
        .collect()
}

fn exported_owners(file: &ProgramFile) -> HashSet<NodeId> {
    let mut owners = HashSet::new();
    for statement in &file.syntax.statements {
        let authored = match &statement.kind {
            StatementKind::Variable(value) => value.exported,
            StatementKind::Function(value) => value.exported || value.default_export,
            StatementKind::Class(value) => value.exported || value.default_export,
            StatementKind::TypeAlias(value) => value.exported,
            StatementKind::Interface(value) => value.exported,
            StatementKind::Export(export) => {
                for specifier in &export.specifiers {
                    if let Some(target) =
                        file.bindings.export_specifier_target(specifier.local_span)
                    {
                        owners.insert(target.owner);
                    }
                }
                if export.default_export
                    && let Some(expression) = export.assignment.as_ref()
                    && let ExpressionKind::Identifier { name_span, .. } = expression.kind
                    && let Some(target) = file.bindings.reference_declaration(name_span)
                    && let Some(target) = file.bindings.declaration(target)
                {
                    owners.insert(target.owner);
                }
                false
            }
            _ => false,
        };
        if authored {
            owners.insert(statement.id);
        }
    }
    owners
}

fn safe_export_root(file: &ProgramFile, owner: NodeId) -> bool {
    file.syntax
        .statements
        .iter()
        .find(|statement| statement.id == owner)
        .is_some_and(|statement| match &statement.kind {
            StatementKind::Variable(value) => value.declarators.iter().all(|declaration| {
                declaration.annotation.is_none()
                    && declaration.initializer.as_ref().is_some_and(|initializer| {
                        matches!(
                            initializer.peel_parentheses().kind,
                            ExpressionKind::Literal(_)
                        )
                    })
            }),
            StatementKind::Class(value) => {
                value.type_parameters.is_empty()
                    && value.extends.is_none()
                    && value.implements.is_empty()
                    && value.members.is_empty()
            }
            _ => false,
        })
}
