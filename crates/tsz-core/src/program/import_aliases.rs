//! Program-owned import aliases used by semantic and service queries.
//!
//! The binder gives an import one file-local declaration identity. This module
//! connects the bounded direct-relative/named-export form to the target's
//! program identity without admitting external-module declarations to the
//! script global scope.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::bind::{BoundFile, DeclarationKind, Meaning, ScopeId};
use crate::source::{DeclId, FileId};
use crate::syntax::{Statement, StatementKind};

use super::{Program, ProgramFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeQueryRoot {
    Declaration(DeclId),
    ImportAlias {
        declaration: DeclId,
        target: Option<DeclId>,
    },
}

impl TypeQueryRoot {
    #[must_use]
    pub(crate) const fn semantic_declaration(self) -> DeclId {
        match self {
            Self::Declaration(declaration) => declaration,
            Self::ImportAlias {
                declaration,
                target,
            } => match target {
                Some(target) => target,
                None => declaration,
            },
        }
    }

    #[must_use]
    pub(crate) const fn navigation_declaration(self) -> DeclId {
        match self {
            Self::Declaration(declaration) | Self::ImportAlias { declaration, .. } => declaration,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ImportAliases {
    value_targets: BTreeMap<DeclId, Option<DeclId>>,
}

impl ImportAliases {
    pub(super) fn build(files: &[ProgramFile], allow_js: bool) -> Self {
        let source_paths = files
            .iter()
            .map(|file| (normalize_path(&file.source.host_path), file.source.id))
            .collect::<BTreeMap<_, _>>();
        let mut value_targets = BTreeMap::new();

        for file in files {
            for statement in &file.syntax.statements {
                let StatementKind::Import(import) = &statement.kind else {
                    continue;
                };
                let target_file = resolve_relative_source(
                    &source_paths,
                    file,
                    &import.module_specifier,
                    allow_js,
                );
                for binding in &import.bindings {
                    let target = target_file.zip(binding.imported.as_deref()).and_then(
                        |(target_file, imported)| {
                            (!binding.namespace)
                                .then(|| {
                                    direct_exported_value(&files[target_file.0 as usize], imported)
                                })
                                .flatten()
                        },
                    );
                    for declaration in file.bindings.declarations.iter().filter(|declaration| {
                        declaration.owner == statement.id
                            && declaration.kind == DeclarationKind::Import
                            && declaration.name == binding.local
                            && declaration.name_span == binding.local_span
                    }) {
                        value_targets.insert(declaration.id, target);
                    }
                }
            }
        }

        Self { value_targets }
    }

    fn target(&self, declaration: DeclId) -> Option<Option<DeclId>> {
        self.value_targets.get(&declaration).copied()
    }
}

impl Program {
    /// Resolve the root of a `typeof` type query without confusing a
    /// type-only import with a runtime use. Lexical values win, then a local
    /// import alias, then the script global value scope.
    #[must_use]
    pub(crate) fn resolve_type_query_root(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
    ) -> Option<TypeQueryRoot> {
        let bound = &self.files.get(file.0 as usize)?.bindings;
        if let Some(declaration) = bound.resolve(scope, name, Meaning::Value) {
            return Some(self.type_query_root_for(declaration));
        }
        if let Some(declaration) = self.resolve_import_alias(bound, scope, name) {
            return Some(self.type_query_root_for(declaration));
        }
        self.resolve_global(name, Meaning::Value)
            .map(TypeQueryRoot::Declaration)
    }

    fn type_query_root_for(&self, declaration: DeclId) -> TypeQueryRoot {
        match self.import_aliases.target(declaration) {
            Some(target) => TypeQueryRoot::ImportAlias {
                declaration,
                target,
            },
            None => TypeQueryRoot::Declaration(declaration),
        }
    }

    fn resolve_import_alias(
        &self,
        bound: &BoundFile,
        mut scope: ScopeId,
        name: &str,
    ) -> Option<DeclId> {
        loop {
            let current = bound.scopes.get(scope.0 as usize)?;
            if let Some(ids) = current.names.get(name)
                && let Some(declaration) = ids
                    .iter()
                    .rev()
                    .copied()
                    .find(|declaration| self.import_aliases.target(*declaration).is_some())
            {
                return Some(declaration);
            }
            scope = current.parent?;
        }
    }
}

fn resolve_relative_source(
    source_paths: &BTreeMap<PathBuf, FileId>,
    importer: &ProgramFile,
    specifier: &str,
    allow_js: bool,
) -> Option<FileId> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }
    let parent = importer.source.host_path.parent().unwrap_or(Path::new(""));
    let base = normalize_path(&parent.join(specifier));
    if base.extension().is_some() {
        if !exact_source_kind_supported(&base, allow_js) {
            return None;
        }
        return source_paths.get(&base).copied();
    }

    let mut extensions = vec!["ts", "tsx", "d.ts", "mts", "d.mts", "cts", "d.cts"];
    if allow_js {
        extensions.extend(["js", "jsx", "mjs", "cjs"]);
    }
    for extension in extensions {
        let mut candidate = base.clone();
        candidate.set_extension(extension);
        if let Some(file) = source_paths.get(&candidate).copied() {
            return Some(file);
        }
    }

    // Keep every unowned directory/package/export-map form typed as an
    // unresolved alias. Module-resolution diagnostics own those cases later.
    None
}

fn exact_source_kind_supported(path: &Path, allow_js: bool) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    matches!(extension, "ts" | "tsx" | "mts" | "cts")
        || allow_js && matches!(extension, "js" | "jsx" | "mjs" | "cjs")
}

fn direct_exported_value(file: &ProgramFile, name: &str) -> Option<DeclId> {
    let ids = file.bindings.scopes.first()?.names.get(name)?;
    let mut values = ids.iter().copied().filter(|id| {
        file.bindings.declaration(*id).is_some_and(|declaration| {
            declaration.meaning == Meaning::Value
                && statement_directly_exports_value(
                    &file.syntax.statements,
                    declaration.owner,
                    &declaration.name,
                )
        })
    });
    let declaration = values.next()?;
    values.next().is_none().then_some(declaration)
}

fn statement_directly_exports_value(
    statements: &[Statement],
    owner: crate::source::NodeId,
    name: &str,
) -> bool {
    statements.iter().any(|statement| {
        statement.id == owner
            && match &statement.kind {
                StatementKind::Variable(declaration) => {
                    declaration.exported && declaration.name == name
                }
                StatementKind::Function(declaration) => {
                    declaration.exported && !declaration.default_export && declaration.name == name
                }
                StatementKind::Class(declaration) => {
                    declaration.exported && !declaration.default_export && declaration.name == name
                }
                StatementKind::Import(_)
                | StatementKind::Export(_)
                | StatementKind::TypeAlias(_)
                | StatementKind::Interface(_)
                | StatementKind::If(_)
                | StatementKind::Switch(_)
                | StatementKind::Break(_)
                | StatementKind::Continue(_)
                | StatementKind::Return(_)
                | StatementKind::Block(_)
                | StatementKind::Expression(_)
                | StatementKind::Empty
                | StatementKind::Unknown => false,
            }
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir) => {}
                Some(Component::Prefix(_) | Component::ParentDir) | None => {
                    normalized.push(component.as_os_str());
                }
                Some(Component::CurDir) => unreachable!("current directories are removed eagerly"),
            },
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}
