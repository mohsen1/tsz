use std::collections::BTreeMap;

use crate::bind::{JavaScriptPropertyAssignmentTarget, Meaning};
use crate::source::{DeclId, FileId, NodeId};

use super::ProgramFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JavaScriptAssignmentDisposition {
    Complete(DeclId),
    Incomplete,
}

#[derive(Debug, Default)]
pub(crate) struct JavaScriptAssignments {
    assignments: BTreeMap<(FileId, NodeId), JavaScriptAssignmentDisposition>,
    rhs_declarations: BTreeMap<(FileId, NodeId), DeclId>,
    roots: BTreeMap<DeclId, bool>,
    declaration_groups: BTreeMap<DeclId, Vec<DeclId>>,
    children: BTreeMap<DeclId, Vec<DeclId>>,
}

impl JavaScriptAssignments {
    pub(super) fn build(
        files: &[ProgramFile],
        global_values: &BTreeMap<String, Vec<DeclId>>,
    ) -> Self {
        let mut result = Self::default();
        let mut groups = BTreeMap::<(DeclId, Vec<String>), Vec<(NodeId, NodeId, DeclId)>>::new();
        let mut root_groups = BTreeMap::<DeclId, Vec<DeclId>>::new();

        for file in files {
            for assignment in &file.bindings.javascript_property_assignments {
                if assignment.target == JavaScriptPropertyAssignmentTarget::OrdinaryIndex {
                    continue;
                }
                let key = (file.source.id, assignment.left);
                let Some(roots) = assignment.root.as_deref().and_then(|root| {
                    resolve_root_group(file, assignment.scope, root, global_values)
                }) else {
                    if assignment.root.is_none() {
                        result.defer(key);
                    }
                    continue;
                };
                let Some(declaration) = assignment.declaration else {
                    result.defer_roots(&roots);
                    result.defer(key);
                    continue;
                };
                let root = roots[0];
                root_groups.entry(root).or_insert(roots);
                groups
                    .entry((root, assignment.properties.clone()))
                    .or_default()
                    .push((assignment.left, assignment.right, declaration));
            }
        }

        for candidates in groups.values_mut() {
            candidates.sort_by_key(|candidate| candidate.2);
        }
        let is_expando = |declaration: DeclId| {
            files[declaration.file.0 as usize]
                .bindings
                .javascript_expando_initializers
                .contains(&declaration)
        };

        for ((root, properties), candidates) in &groups {
            let roots = &root_groups[root];
            let parents_are_expando = roots.iter().all(|root| is_expando(*root))
                && (1..properties.len()).all(|length| {
                    groups
                        .get(&(*root, properties[..length].to_vec()))
                        .is_some_and(|parents| {
                            parents.iter().all(|candidate| is_expando(candidate.2))
                        })
                });
            if !parents_are_expando {
                result.defer_roots(roots);
                for candidate in candidates {
                    result.defer((candidate.2.file, candidate.0));
                }
                continue;
            }

            let canonical = candidates[0].2;
            for root in roots {
                result.roots.entry(*root).or_insert(false);
            }
            result
                .assignments
                .extend(candidates.iter().map(|candidate| {
                    (
                        (candidate.2.file, candidate.0),
                        JavaScriptAssignmentDisposition::Complete(candidate.2),
                    )
                }));
            result.rhs_declarations.extend(
                candidates
                    .iter()
                    .map(|candidate| ((candidate.2.file, candidate.1), candidate.2)),
            );
            result.declaration_groups.insert(
                canonical,
                candidates.iter().map(|candidate| candidate.2).collect(),
            );

            if properties.len() == 1 {
                for parent in roots {
                    result.children.entry(*parent).or_default().push(canonical);
                }
            } else {
                let parent = groups[&(*root, properties[..properties.len() - 1].to_vec())][0].2;
                result.children.entry(parent).or_default().push(canonical);
            }
        }

        result
    }

    fn defer(&mut self, key: (FileId, NodeId)) {
        self.assignments
            .insert(key, JavaScriptAssignmentDisposition::Incomplete);
    }

    fn defer_roots(&mut self, roots: &[DeclId]) {
        self.roots.extend(roots.iter().map(|root| (*root, true)));
    }

    pub(crate) fn assignment(
        &self,
        file: FileId,
        assignment: NodeId,
    ) -> Option<JavaScriptAssignmentDisposition> {
        self.assignments.get(&(file, assignment)).copied()
    }

    pub(crate) fn root(&self, declaration: DeclId) -> Option<bool> {
        self.roots.get(&declaration).copied()
    }

    pub(crate) fn rhs_declaration(&self, file: FileId, rhs: NodeId) -> Option<DeclId> {
        self.rhs_declarations.get(&(file, rhs)).copied()
    }

    pub(crate) fn declarations(&self, canonical: DeclId) -> &[DeclId] {
        self.declaration_groups
            .get(&canonical)
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn children(&self, declaration: DeclId) -> &[DeclId] {
        self.children.get(&declaration).map_or(&[], Vec::as_slice)
    }
}

fn resolve_root_group(
    file: &ProgramFile,
    scope: crate::bind::ScopeId,
    name: &str,
    global_values: &BTreeMap<String, Vec<DeclId>>,
) -> Option<Vec<DeclId>> {
    let local = file.bindings.resolve(scope, name, Meaning::Value);
    let mut declarations = match local.and_then(|id| file.bindings.declaration(id)) {
        Some(declaration) if declaration.scope.0 != 0 || file.is_external_module() => file
            .bindings
            .scopes
            .get(declaration.scope.0 as usize)?
            .names
            .get(name)?
            .iter()
            .copied()
            .filter(|id| {
                file.bindings
                    .declaration(*id)
                    .is_some_and(|declaration| declaration.meaning == Meaning::Value)
            })
            .collect(),
        Some(_) | None => global_values.get(name)?.clone(),
    };
    declarations.sort_unstable();
    declarations.dedup();
    (!declarations.is_empty()).then_some(declarations)
}
