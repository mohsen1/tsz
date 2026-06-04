fn literal_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    arena
        .get(idx)
        .and_then(|node| arena.get_literal(node))
        .map(|lit| lit.text.clone())
}

fn build_ambient_global_type_only_names(
    program: &MergedProgram,
    preserve_const_enums: bool,
) -> FxHashSet<String> {
    let mut type_only_names = FxHashSet::default();
    let mut value_names = FxHashSet::default();

    for file in &program.files {
        let input_path = PathBuf::from(&file.file_name);
        if !is_declaration_file(&input_path) {
            continue;
        }

        let Some(source) = file
            .arena
            .get(file.source_file)
            .and_then(|node| file.arena.get_source_file(node))
        else {
            continue;
        };

        if source_file_has_top_level_module_syntax(&file.arena, &source.statements.nodes) {
            continue;
        }

        type_only_names.extend(
            tsz_emitter::transforms::module_commonjs::build_type_only_declaration_names(
                &file.arena,
                &source.statements.nodes,
                preserve_const_enums,
            ),
        );
        value_names.extend(
            tsz_emitter::transforms::module_commonjs::build_value_declaration_names(
                &file.arena,
                &source.statements.nodes,
                preserve_const_enums,
            ),
        );
    }

    type_only_names.retain(|name| !value_names.contains(name));
    type_only_names
}

fn build_type_only_export_equals_modules(
    program: &MergedProgram,
    preserve_const_enums: bool,
) -> FxHashSet<String> {
    let mut modules = FxHashSet::default();

    for file in &program.files {
        let input_path = PathBuf::from(&file.file_name);
        if !is_declaration_file(&input_path) {
            continue;
        }

        let Some(source) = file
            .arena
            .get(file.source_file)
            .and_then(|node| file.arena.get_source_file(node))
        else {
            continue;
        };

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = file.arena.get_module(stmt_node) else {
                continue;
            };
            if !file
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let Some(module_name) = file
                .arena
                .get(module.name)
                .and_then(|node| file.arena.get_literal(node))
                .map(|lit| lit.text.clone())
            else {
                continue;
            };
            let Some(statements) = module_block_statements(&file.arena, module.body) else {
                continue;
            };
            let Some(export_name) = export_equals_identifier_name(&file.arena, statements) else {
                continue;
            };
            let value_names =
                tsz_emitter::transforms::module_commonjs::build_value_declaration_names(
                    &file.arena,
                    statements,
                    preserve_const_enums,
                );

            if !value_names.contains(&export_name) {
                modules.insert(module_name);
            }
        }
    }

    modules
}

fn module_block_statements(arena: &NodeArena, body_idx: NodeIndex) -> Option<&[NodeIndex]> {
    let body_node = arena.get(body_idx)?;
    let block = arena.get_module_block(body_node)?;
    block
        .statements
        .as_ref()
        .map(|statements| statements.nodes.as_slice())
}

fn export_equals_identifier_name(arena: &NodeArena, statements: &[NodeIndex]) -> Option<String> {
    statements.iter().find_map(|&stmt_idx| {
        let node = arena.get(stmt_idx)?;
        if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
            return None;
        }
        let export_assignment = arena.get_export_assignment(node)?;
        if !export_assignment.is_export_equals {
            return None;
        }
        arena.identifier_text_owned(export_assignment.expression)
    })
}

fn source_file_has_top_level_module_syntax(arena: &NodeArena, statements: &[NodeIndex]) -> bool {
    statements.iter().any(|&stmt_idx| {
        let Some(node) = arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::EXPORT_DECLARATION
                || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let Some(import_data) = arena.get_import_decl(node) else {
                    return false;
                };
                let Some(spec_node) = arena.get(import_data.module_specifier) else {
                    return false;
                };
                spec_node.kind == SyntaxKind::StringLiteral as u16
                    || spec_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
            }
            _ => false,
        }
    })
}

fn mark_ambient_global_type_only_export_specifiers(
    arena: &NodeArena,
    source_file_idx: NodeIndex,
    ambient_global_type_only_names: &FxHashSet<String>,
    type_only_nodes: &mut FxHashSet<NodeIndex>,
) {
    if ambient_global_type_only_names.is_empty() {
        return;
    }

    let Some(source) = arena
        .get(source_file_idx)
        .and_then(|node| arena.get_source_file(node))
    else {
        return;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            continue;
        }

        let Some(export_decl) = arena.get_export_decl(node) else {
            continue;
        };
        if export_decl.is_type_only || export_decl.module_specifier.is_some() {
            continue;
        }

        let Some(clause_node) = arena.get(export_decl.export_clause) else {
            continue;
        };
        let Some(named_exports) = arena.get_named_imports(clause_node) else {
            continue;
        };

        for &spec_idx in &named_exports.elements.nodes {
            let Some(spec) = arena.get_specifier_at(spec_idx) else {
                continue;
            };
            if spec.is_type_only {
                continue;
            }

            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            if let Some(local_name) = arena.identifier_text_owned(local_name_idx)
                && ambient_global_type_only_names.contains(&local_name)
            {
                type_only_nodes.insert(spec_idx);
            }
        }
    }
}

pub(crate) fn normalize_base_url(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() || is_windows_absolute_like(&dir) {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

fn is_windows_absolute_like(path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };

    let bytes = path.as_bytes();
    if bytes.len() < 3 {
        return false;
    }

    (bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\')) || path.starts_with("\\\\")
}

pub(crate) fn normalize_output_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        if dir.is_absolute() {
            canonicalize_with_missing_tail(&dir)
        } else {
            canonicalize_with_missing_tail(&base_dir.join(dir))
        }
    })
}

pub(crate) fn normalize_root_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

pub(crate) fn normalize_root_dirs(base_dir: &Path, roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .map(|root| {
            let resolved = if root.is_absolute() {
                root
            } else {
                base_dir.join(root)
            };
            canonicalize_with_missing_tail(&resolved)
        })
        .collect()
}

pub(crate) fn normalize_type_roots(
    base_dir: &Path,
    roots: Option<Vec<PathBuf>>,
) -> Option<Vec<PathBuf>> {
    let roots = roots?;
    let mut normalized = Vec::new();
    for root in roots {
        let resolved = if root.is_absolute() {
            root
        } else {
            base_dir.join(root)
        };
        // Match tsc: absolute typeRoots paths are used as-is.
        // If the path doesn't exist on disk, it's simply skipped (no fallback).
        let resolved = canonicalize_or_owned(&resolved);
        if resolved.is_dir() {
            normalized.push(resolved);
        }
    }
    Some(normalized)
}

/// Convert config `JsxEmit` to emitter `JsxEmit`.
const fn config_jsx_to_emitter_jsx(jsx: JsxEmit) -> tsz::emitter::JsxEmit {
    match jsx {
        JsxEmit::Preserve => tsz::emitter::JsxEmit::Preserve,
        JsxEmit::React => tsz::emitter::JsxEmit::React,
        JsxEmit::ReactJsx => tsz::emitter::JsxEmit::ReactJsx,
        JsxEmit::ReactJsxDev => tsz::emitter::JsxEmit::ReactJsxDev,
        JsxEmit::ReactNative => tsz::emitter::JsxEmit::ReactNative,
    }
}
