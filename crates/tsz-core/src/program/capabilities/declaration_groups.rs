use std::collections::{BTreeMap, BTreeSet};

use crate::source::{DeclId, FileId};
use crate::syntax::StatementKind;

use super::ProgramFile;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum FunctionGroup<'a> {
    Local(DeclId),
    Global(&'a str),
}

pub(super) fn declaration_overload_files(files: &[ProgramFile]) -> BTreeSet<FileId> {
    let mut incomplete = files
        .iter()
        .filter(|file| class_declaration_groups_need_summary(file))
        .map(|file| file.source.id)
        .collect::<BTreeSet<_>>();
    let mut functions = BTreeMap::<FunctionGroup<'_>, (bool, bool, BTreeSet<FileId>)>::new();
    for file in files {
        for statement in &file.syntax.statements {
            let StatementKind::Function(declaration) = &statement.kind else {
                continue;
            };
            let key = if file.is_external_module() {
                let Some(declaration) = file.bindings.scopes[0]
                    .names
                    .get(&declaration.name)
                    .and_then(|group| group.first())
                else {
                    continue;
                };
                FunctionGroup::Local(*declaration)
            } else {
                FunctionGroup::Global(&declaration.name)
            };
            let group = functions.entry(key).or_default();
            group.0 |= !declaration.has_body;
            group.1 |= declaration.has_body;
            group.2.insert(file.source.id);
        }
    }
    for (_, (has_signature, has_body, sources)) in functions {
        if has_signature && has_body {
            incomplete.extend(sources);
        }
    }
    incomplete
}

fn class_declaration_groups_need_summary(file: &ProgramFile) -> bool {
    file.syntax.statements.iter().any(|statement| {
        let StatementKind::Class(class) = &statement.kind else {
            return false;
        };
        class
            .members
            .iter()
            .filter_map(|member| file.bindings.class_member_group_facts(member.id))
            .any(|facts| {
                facts.callables > 1 && facts.private_callable
                    || facts.implementations > 1
                    || facts.implementations > 0 && facts.implementations < facts.callables
            })
    })
}
