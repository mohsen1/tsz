use crate::transforms::emit_utils::{
    identifier_text as get_identifier_text, is_valid_identifier_name, specifier_name_text,
};

use rustc_hash::FxHashSet;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{Node, NodeArena};

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

/// Emit the `CommonJS` module preamble
///
/// Outputs:
/// ```javascript
/// "use strict";
/// Object.defineProperty(exports, "__esModule", { value: true });
/// ```
pub fn emit_commonjs_preamble(writer: &mut impl std::fmt::Write) -> std::fmt::Result {
    writeln!(writer, "\"use strict\";")?;
    writeln!(
        writer,
        "Object.defineProperty(exports, \"__esModule\", {{ value: true }});"
    )?;
    Ok(())
}

/// Check whether an `import X = Y.Z` entity-name reference targets a value declaration.
fn is_import_alias_referencing_value(
    arena: &NodeArena,
    entity_name_idx: NodeIndex,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> bool {
    let mut parts: Vec<String> = Vec::new();
    fn flatten(arena: &NodeArena, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = arena.get(idx) else { return };
        if let Some(qn) = arena.get_qualified_name(node) {
            flatten(arena, qn.left, parts);
            if let Some(name) = get_identifier_text(arena, qn.right) {
                parts.push(name);
            }
        } else if let Some(name) = get_identifier_text(arena, idx) {
            parts.push(name);
        }
    }
    flatten(arena, entity_name_idx, &mut parts);
    if parts.is_empty() {
        return true;
    }
    resolve_entity_chain_has_value(arena, &parts, statements, preserve_const_enums)
}

fn resolve_entity_chain_has_value(
    arena: &NodeArena,
    parts: &[String],
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> bool {
    if parts.is_empty() {
        return true;
    }
    let target_name = &parts[0];
    let rest = &parts[1..];
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        let inner_node = if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(ed) = arena.get_export_decl(node)
                && !ed.is_type_only
                && ed.module_specifier.is_none()
            {
                arena.get(ed.export_clause)
            } else {
                None
            }
        } else {
            Some(node)
        };
        let Some(inner) = inner_node else {
            continue;
        };
        match inner.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT && rest.is_empty() => {
                if let Some(vs) = arena.get_variable(inner) {
                    let mut names = Vec::new();
                    for &di in &vs.declarations.nodes {
                        collect_declaration_names(arena, di, &mut names);
                    }
                    if names.iter().any(|n| n == target_name) {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(f) = arena.get_function(inner)
                    && let Some(n) = get_identifier_text(arena, f.name)
                    && n == *target_name
                    && rest.is_empty()
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(c) = arena.get_class(inner)
                    && let Some(n) = get_identifier_text(arena, c.name)
                    && n == *target_name
                    && rest.is_empty()
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(e) = arena.get_enum(inner)
                    && let Some(n) = get_identifier_text(arena, e.name)
                    && n == *target_name
                    && rest.is_empty()
                {
                    if arena.has_modifier(&e.modifiers, SyntaxKind::ConstKeyword)
                        && !preserve_const_enums
                    {
                        return false;
                    }
                    return true;
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(m) = arena.get_module(inner)
                    && let Some(n) = get_identifier_text(arena, m.name)
                    && n == *target_name
                {
                    if rest.is_empty() {
                        return super::emit_utils::is_instantiated_module_ext(
                            arena,
                            m.body,
                            preserve_const_enums,
                        ) || module_body_is_empty(arena, m.body);
                    }
                    if let Some(body) = arena.get(m.body)
                        && let Some(block) = arena.get_module_block(body)
                        && let Some(ref stmts) = block.statements
                    {
                        return resolve_entity_chain_has_value(
                            arena,
                            rest,
                            &stmts.nodes,
                            preserve_const_enums,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(i) = arena.get_interface(inner)
                    && let Some(n) = get_identifier_text(arena, i.name)
                    && n == *target_name
                    && rest.is_empty()
                {
                    continue;
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(t) = arena.get_type_alias(inner)
                    && let Some(n) = get_identifier_text(arena, t.name)
                    && n == *target_name
                    && rest.is_empty()
                {
                    continue;
                }
            }
            _ => {}
        }
    }
    let found_type_only = statements.iter().any(|&si| {
        let Some(node) = arena.get(si) else {
            return false;
        };
        let inner = if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(ed) = arena.get_export_decl(node)
                && !ed.is_type_only
                && ed.module_specifier.is_none()
            {
                arena.get(ed.export_clause)
            } else {
                None
            }
        } else {
            Some(node)
        };
        let Some(inner) = inner else {
            return false;
        };
        if inner.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            arena
                .get_interface(inner)
                .and_then(|i| get_identifier_text(arena, i.name))
                .as_deref()
                == Some(target_name)
        } else if inner.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            arena
                .get_type_alias(inner)
                .and_then(|t| get_identifier_text(arena, t.name))
                .as_deref()
                == Some(target_name)
        } else {
            false
        }
    });
    !found_type_only
}

fn module_body_is_empty(arena: &NodeArena, body_idx: NodeIndex) -> bool {
    arena
        .get(body_idx)
        .and_then(|body| arena.get_module_block(body))
        .and_then(|block| block.statements.as_ref())
        .is_none_or(|statements| statements.nodes.is_empty())
}

/// Helper function to collect export name from a single declaration node
/// Walk a qualified `import = X.Y.Z` reference and return true only when the
/// final identifier `Z` resolves to an *exported* type-only member inside the
/// preceding namespace chain. Non-exported type members are not reachable from
/// outside the namespace, so tsc keeps the (broken) runtime emit; mirror that
/// here by returning false for such cases.
pub(crate) fn import_alias_resolves_to_exported_type_only(
    arena: &NodeArena,
    entity_name_idx: NodeIndex,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> bool {
    let mut parts: Vec<String> = Vec::new();
    fn flatten(arena: &NodeArena, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = arena.get(idx) else { return };
        if let Some(qn) = arena.get_qualified_name(node) {
            flatten(arena, qn.left, parts);
            if let Some(name) = get_identifier_text(arena, qn.right) {
                parts.push(name);
            }
        } else if let Some(name) = get_identifier_text(arena, idx) {
            parts.push(name);
        }
    }
    flatten(arena, entity_name_idx, &mut parts);
    if parts.len() < 2 {
        return false;
    }
    chain_resolves_to_exported_type_only(arena, &parts, statements, false, preserve_const_enums)
}

fn import_alias_identifier_resolves_to_exported_type_only_namespace(
    arena: &NodeArena,
    entity_name_idx: NodeIndex,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> bool {
    let Some(alias_target) = get_identifier_text(arena, entity_name_idx) else {
        return false;
    };
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        let inner_node = if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(ed) = arena.get_export_decl(node)
                && !ed.is_type_only
                && ed.module_specifier.is_none()
            {
                arena.get(ed.export_clause)
            } else {
                None
            }
        } else {
            Some(node)
        };
        let Some(inner) = inner_node else {
            continue;
        };
        if inner.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module) = arena.get_module(inner)
            && get_identifier_text(arena, module.name).as_deref() == Some(alias_target.as_str())
            && !super::emit_utils::is_instantiated_module_ext(
                arena,
                module.body,
                preserve_const_enums,
            )
            && module_body_has_exported_type_only_member(arena, module.body)
        {
            return true;
        }
    }
    false
}

fn module_body_has_exported_type_only_member(arena: &NodeArena, module_body: NodeIndex) -> bool {
    let Some(body_node) = arena.get(module_body) else {
        return false;
    };
    if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
        return arena
            .get_module(body_node)
            .is_some_and(|module| module_body_has_exported_type_only_member(arena, module.body));
    }
    let Some(block) = arena.get_module_block(body_node) else {
        return false;
    };
    let Some(ref statements) = block.statements else {
        return false;
    };
    statements.nodes.iter().any(|&stmt_idx| {
        let Some(node) = arena.get(stmt_idx) else {
            return false;
        };
        let (inner_node, has_export_wrapper) = if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(ed) = arena.get_export_decl(node)
                && !ed.is_type_only
                && ed.module_specifier.is_none()
            {
                (arena.get(ed.export_clause), true)
            } else {
                (None, false)
            }
        } else {
            (Some(node), false)
        };
        let Some(inner) = inner_node else {
            return false;
        };
        match inner.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                has_export_wrapper
                    || arena.get_interface(inner).is_some_and(|i| {
                        arena.has_modifier(&i.modifiers, SyntaxKind::ExportKeyword)
                    })
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                has_export_wrapper
                    || arena.get_type_alias(inner).is_some_and(|t| {
                        arena.has_modifier(&t.modifiers, SyntaxKind::ExportKeyword)
                    })
            }
            _ => false,
        }
    })
}

fn chain_resolves_to_exported_type_only(
    arena: &NodeArena,
    parts: &[String],
    statements: &[NodeIndex],
    require_export: bool,
    preserve_const_enums: bool,
) -> bool {
    if parts.is_empty() {
        return false;
    }
    let target_name = &parts[0];
    let rest = &parts[1..];
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        let (inner_node, has_export_decl_wrapper) =
            if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(ed) = arena.get_export_decl(node)
                    && !ed.is_type_only
                    && ed.module_specifier.is_none()
                {
                    (arena.get(ed.export_clause), true)
                } else {
                    continue;
                }
            } else {
                (Some(node), false)
            };
        let Some(inner) = inner_node else {
            continue;
        };
        let has_export_modifier = match inner.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => arena
                .get_interface(inner)
                .map(|i| arena.has_modifier(&i.modifiers, SyntaxKind::ExportKeyword))
                .unwrap_or(false),
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                .get_type_alias(inner)
                .map(|t| arena.has_modifier(&t.modifiers, SyntaxKind::ExportKeyword))
                .unwrap_or(false),
            k if k == syntax_kind_ext::MODULE_DECLARATION => arena
                .get_module(inner)
                .map(|m| arena.has_modifier(&m.modifiers, SyntaxKind::ExportKeyword))
                .unwrap_or(false),
            _ => false,
        } || has_export_decl_wrapper;
        match inner.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if rest.is_empty()
                    && arena
                        .get_interface(inner)
                        .and_then(|i| get_identifier_text(arena, i.name))
                        .as_deref()
                        == Some(target_name.as_str())
                    && (!require_export || has_export_modifier)
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if rest.is_empty()
                    && arena
                        .get_type_alias(inner)
                        .and_then(|t| get_identifier_text(arena, t.name))
                        .as_deref()
                        == Some(target_name.as_str())
                    && (!require_export || has_export_modifier)
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(m) = arena.get_module(inner)
                    && let Some(n) = get_identifier_text(arena, m.name)
                    && n == *target_name
                    && (!require_export || has_export_modifier)
                {
                    if rest.is_empty() {
                        return !super::emit_utils::is_instantiated_module_ext(
                            arena,
                            m.body,
                            preserve_const_enums,
                        );
                    }
                    if let Some(body) = arena.get(m.body)
                        && let Some(block) = arena.get_module_block(body)
                        && let Some(ref stmts) = block.statements
                    {
                        // Inside the namespace body, members must be exported to
                        // be reachable from the outer alias chain.
                        return chain_resolves_to_exported_type_only(
                            arena,
                            rest,
                            &stmts.nodes,
                            true,
                            preserve_const_enums,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Helper function to collect export name from a single declaration node
fn collect_export_name_from_declaration(
    arena: &NodeArena,
    decl_node: &Node,
    exports: &mut Vec<String>,
    preserve_const_enums: bool,
    statements: &[NodeIndex],
) {
    match decl_node.kind {
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(decl_node) {
                if arena.is_declare(&class.modifiers) {
                    return;
                }
                if let Some(name) = get_identifier_text(arena, class.name) {
                    exports.push(name);
                }
            }
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            if let Some(func) = arena.get_function(decl_node) {
                if arena.is_declare(&func.modifiers) {
                    return;
                }
                // Skip overload signatures (no body) — if the implementation
                // also has `export`, it will be collected separately.
                if func.body.is_none() {
                    return;
                }
                if let Some(name) = get_identifier_text(arena, func.name)
                    && !exports.contains(&name)
                {
                    exports.push(name);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(var_stmt) = arena.get_variable(decl_node) {
                if arena.is_declare(&var_stmt.modifiers) {
                    return;
                }
                for &decl_idx in &var_stmt.declarations.nodes {
                    collect_declaration_names(arena, decl_idx, exports);
                }
            }
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enum_decl) = arena.get_enum(decl_node) {
                if arena.is_declare(&enum_decl.modifiers) {
                    return;
                }
                if arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                    && !preserve_const_enums
                {
                    return;
                }
                if let Some(name) = get_identifier_text(arena, enum_decl.name) {
                    exports.push(name);
                }
            }
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            if let Some(module) = arena.get_module(decl_node) {
                if arena.is_declare(&module.modifiers) {
                    return;
                }
                if !super::emit_utils::is_instantiated_module_ext(
                    arena,
                    module.body,
                    preserve_const_enums,
                ) {
                    return;
                }
                if let Some(name) = get_identifier_text(arena, module.name) {
                    exports.push(name);
                }
            }
        }
        k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
            if let Some(import_decl) = arena.get_import_decl(decl_node)
                && let Some(name) = get_identifier_text(arena, import_decl.import_clause)
            {
                if import_decl.is_type_only {
                    return;
                }
                if import_equals_uses_external_module_ref(arena, import_decl.module_specifier) {
                    return;
                }
                // For a qualified `export import A = X.Y` chain, the alias
                // is type-only when *the exported member* `Y` of namespace
                // `X` is itself an interface or type alias. A non-exported
                // type member inside `X` cannot be reached from outside, so
                // tsc resolves the chain to nothing and preserves the
                // (broken-at-runtime) `exports.A = X.Y;`. Mirror that:
                // only elide when the inner member is an *exported*
                // type-only declaration.
                if import_alias_resolves_to_exported_type_only(
                    arena,
                    import_decl.module_specifier,
                    statements,
                    preserve_const_enums,
                ) {
                    return;
                }
                if arena
                    .get(import_decl.module_specifier)
                    .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
                    && import_alias_identifier_resolves_to_exported_type_only_namespace(
                        arena,
                        import_decl.module_specifier,
                        statements,
                        preserve_const_enums,
                    )
                {
                    return;
                }
                exports.push(name);
            }
        }
        _ => {
            // Interface, Type Alias, etc. don't need runtime exports
        }
    }
}

fn import_equals_uses_external_module_ref(arena: &NodeArena, module_specifier: NodeIndex) -> bool {
    arena.get(module_specifier).is_some_and(|node| {
        node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
    })
}

/// Build a set of names that have runtime value declarations in the file.
///
/// This is used to syntactically determine whether `export { x }` refers to
/// a runtime value or a type-only declaration (interface, type alias, etc.).
/// Names with at least one value declaration are considered "has value".
///
/// Value declarations: variables, functions, classes, non-const enums,
/// instantiated namespaces, import-equals, import bindings.
/// Also includes `declare` value declarations (ambient values exist at runtime).
/// Type-only: interfaces, type aliases, const enums (when not preserving),
/// non-instantiated namespaces.
pub fn build_value_declaration_names(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> rustc_hash::FxHashSet<String> {
    let mut value_names = rustc_hash::FxHashSet::default();

    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };

        match node.kind {
            // Variables (including `declare const x`) are value declarations
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node) {
                    for &decl_idx in &var_stmt.declarations.nodes {
                        let mut names = Vec::new();
                        collect_declaration_names(arena, decl_idx, &mut names);
                        value_names.extend(names);
                    }
                }
            }
            // Functions (including `declare function f()`) are value declarations
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && let Some(name) = get_identifier_text(arena, func.name)
                {
                    value_names.insert(name);
                }
            }
            // Classes (including `declare class C`) are value declarations
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node)
                    && let Some(name) = get_identifier_text(arena, class.name)
                {
                    value_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node)
                    && (preserve_const_enums
                        || !arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword))
                    && let Some(name) = get_identifier_text(arena, enum_decl.name)
                {
                    value_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node)
                    && super::emit_utils::is_instantiated_module_ext(
                        arena,
                        module.body,
                        preserve_const_enums,
                    )
                    && let Some(name) = get_identifier_text(arena, module.name)
                {
                    value_names.insert(name);
                }
            }
            // Also handle wrapped export declarations (export class C {}, etc.)
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = arena.get_export_decl(node)
                    && !export_decl.is_type_only
                    && export_decl.module_specifier.is_none()
                    && let Some(clause_node) = arena.get(export_decl.export_clause)
                {
                    collect_value_names_from_declaration(
                        arena,
                        clause_node,
                        &mut value_names,
                        preserve_const_enums,
                        statements,
                    );
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_decl) = arena.get_import_decl(node)
                    && let Some(name) = get_identifier_text(arena, import_decl.import_clause)
                    && !import_decl.is_type_only
                {
                    if let Some(ref_node) = arena.get(import_decl.module_specifier)
                        && ref_node.kind == SyntaxKind::StringLiteral as u16
                    {
                        value_names.insert(name);
                    } else if is_import_alias_referencing_value(
                        arena,
                        import_decl.module_specifier,
                        statements,
                        preserve_const_enums,
                    ) {
                        value_names.insert(name);
                    }
                }
            }
            // Import bindings create value names (unless `import type`)
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import_decl) = arena.get_import_decl(node)
                    && !import_decl.is_type_only
                    && let Some(clause_node) = arena.get(import_decl.import_clause)
                    && let Some(clause) = arena.get_import_clause(clause_node)
                    && !clause.is_type_only
                {
                    // Default import: `import d from "mod"` → d is a value
                    if let Some(name) = get_identifier_text(arena, clause.name) {
                        value_names.insert(name);
                    }
                    // Named/namespace bindings
                    if let Some(nb_node) = arena.get(clause.named_bindings) {
                        if nb_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                            // `import * as M from "mod"` → M is a value
                            // NAMESPACE_IMPORT uses NamedImportsData with the name field
                            if let Some(ns) = arena.get_named_imports(nb_node)
                                && let Some(name) = get_identifier_text(arena, ns.name)
                            {
                                value_names.insert(name);
                            }
                        } else if nb_node.kind == syntax_kind_ext::NAMED_IMPORTS {
                            // `import { a, b } from "mod"` → a, b are values
                            if let Some(named) = arena.get_named_imports(nb_node) {
                                for &spec_idx in &named.elements.nodes {
                                    if let Some(spec) = arena.get_specifier_at(spec_idx)
                                        && !spec.is_type_only
                                        && let Some(name) = get_identifier_text(arena, spec.name)
                                    {
                                        value_names.insert(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    value_names
}

/// Build a set of local names whose declarations emit runtime bindings.
///
/// This is intentionally stricter than `build_value_declaration_names`: ambient
/// declarations are value-space declarations for type checking and export
/// initialization, but they do not emit local JavaScript bindings.
pub fn build_runtime_declaration_names(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> rustc_hash::FxHashSet<String> {
    let mut runtime_names = rustc_hash::FxHashSet::default();

    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };

        let decl_node = if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export_decl) = arena.get_export_decl(node) else {
                continue;
            };
            if export_decl.is_type_only || export_decl.module_specifier.is_some() {
                continue;
            }
            arena.get(export_decl.export_clause)
        } else {
            Some(node)
        };
        let Some(decl_node) = decl_node else {
            continue;
        };

        match decl_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(decl_node) {
                    if arena.is_declare(&var_stmt.modifiers) {
                        continue;
                    }
                    for &decl_idx in &var_stmt.declarations.nodes {
                        let mut names = Vec::new();
                        collect_declaration_names(arena, decl_idx, &mut names);
                        runtime_names.extend(names);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(decl_node)
                    && !arena.is_declare(&func.modifiers)
                    && func.body.is_some()
                    && let Some(name) = get_identifier_text(arena, func.name)
                {
                    runtime_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(decl_node)
                    && !arena.is_declare(&class.modifiers)
                    && let Some(name) = get_identifier_text(arena, class.name)
                {
                    runtime_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(decl_node)
                    && !arena.is_declare(&enum_decl.modifiers)
                    && (preserve_const_enums
                        || !arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword))
                    && let Some(name) = get_identifier_text(arena, enum_decl.name)
                {
                    runtime_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(decl_node)
                    && !arena.is_declare(&module.modifiers)
                    && super::emit_utils::is_instantiated_module_ext(
                        arena,
                        module.body,
                        preserve_const_enums,
                    )
                    && let Some(name) = get_identifier_text(arena, module.name)
                {
                    runtime_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_decl) = arena.get_import_decl(decl_node)
                    && let Some(name) = get_identifier_text(arena, import_decl.import_clause)
                    && !import_decl.is_type_only
                    && let Some(ref_node) = arena.get(import_decl.module_specifier)
                {
                    if ref_node.kind == SyntaxKind::StringLiteral as u16
                        || is_import_alias_referencing_value(
                            arena,
                            import_decl.module_specifier,
                            statements,
                            preserve_const_enums,
                        )
                    {
                        runtime_names.insert(name);
                    }
                }
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import_decl) = arena.get_import_decl(decl_node)
                    && !import_decl.is_type_only
                    && let Some(clause_node) = arena.get(import_decl.import_clause)
                    && let Some(clause) = arena.get_import_clause(clause_node)
                    && !clause.is_type_only
                {
                    if let Some(name) = get_identifier_text(arena, clause.name) {
                        runtime_names.insert(name);
                    }
                    if let Some(nb_node) = arena.get(clause.named_bindings) {
                        if nb_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                            if let Some(ns) = arena.get_named_imports(nb_node)
                                && let Some(name) = get_identifier_text(arena, ns.name)
                            {
                                runtime_names.insert(name);
                            }
                        } else if nb_node.kind == syntax_kind_ext::NAMED_IMPORTS
                            && let Some(named) = arena.get_named_imports(nb_node)
                        {
                            for &spec_idx in &named.elements.nodes {
                                if let Some(spec) = arena.get_specifier_at(spec_idx)
                                    && !spec.is_type_only
                                    && let Some(name) = get_identifier_text(arena, spec.name)
                                {
                                    runtime_names.insert(name);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    runtime_names
}

/// Build a set of names that are only type-level declarations (interface, type alias)
/// in the current file. Used to distinguish "confirmed type-only" from "cross-file
/// reference" when deciding whether to skip `export { X }` from void 0 initialization.
pub fn build_type_only_declaration_names(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> rustc_hash::FxHashSet<String> {
    let mut type_only_names = rustc_hash::FxHashSet::default();

    // Helper to classify a declaration node as type-only
    let mut add_type_only = |decl_node: &Node| {
        match decl_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = arena.get_interface(decl_node)
                    && let Some(name) = get_identifier_text(arena, iface.name)
                {
                    type_only_names.insert(name);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(type_alias) = arena.get_type_alias(decl_node)
                    && let Some(name) = get_identifier_text(arena, type_alias.name)
                {
                    type_only_names.insert(name);
                }
            }
            // Const enums without preserveConstEnums have no runtime value
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if !preserve_const_enums
                    && let Some(enum_decl) = arena.get_enum(decl_node)
                    && arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                    && let Some(name) = get_identifier_text(arena, enum_decl.name)
                {
                    type_only_names.insert(name);
                }
            }
            // Non-instantiated namespaces (type-only content) have no runtime value
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(decl_node)
                    && !super::emit_utils::is_instantiated_module_ext(
                        arena,
                        module.body,
                        preserve_const_enums,
                    )
                    && let Some(name) = get_identifier_text(arena, module.name)
                {
                    type_only_names.insert(name);
                }
            }
            _ => {}
        }
    };

    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        match node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::MODULE_DECLARATION =>
            {
                add_type_only(node);
            }
            // Also handle wrapped export declarations
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = arena.get_export_decl(node)
                    && export_decl.module_specifier.is_none()
                    && let Some(clause_node) = arena.get(export_decl.export_clause)
                {
                    add_type_only(clause_node);
                }
            }
            _ => {}
        }
    }

    type_only_names
}

/// Helper: collect value names from a declaration node inside an export.
fn collect_value_names_from_declaration(
    arena: &NodeArena,
    decl_node: &Node,
    value_names: &mut rustc_hash::FxHashSet<String>,
    preserve_const_enums: bool,
    statements: &[NodeIndex],
) {
    match decl_node.kind {
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(var_stmt) = arena.get_variable(decl_node) {
                for &decl_idx in &var_stmt.declarations.nodes {
                    let mut names = Vec::new();
                    collect_declaration_names(arena, decl_idx, &mut names);
                    value_names.extend(names);
                }
            }
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            if let Some(func) = arena.get_function(decl_node)
                && let Some(name) = get_identifier_text(arena, func.name)
            {
                value_names.insert(name);
            }
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(decl_node)
                && let Some(name) = get_identifier_text(arena, class.name)
            {
                value_names.insert(name);
            }
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enum_decl) = arena.get_enum(decl_node)
                && (preserve_const_enums
                    || !arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword))
                && let Some(name) = get_identifier_text(arena, enum_decl.name)
            {
                value_names.insert(name);
            }
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            if let Some(module) = arena.get_module(decl_node)
                && (super::emit_utils::is_instantiated_module_ext(
                    arena,
                    module.body,
                    preserve_const_enums,
                ) || module_body_is_empty(arena, module.body))
                && let Some(name) = get_identifier_text(arena, module.name)
            {
                value_names.insert(name);
            }
        }
        k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
            if let Some(import_decl) = arena.get_import_decl(decl_node)
                && let Some(name) = get_identifier_text(arena, import_decl.import_clause)
                && !import_decl.is_type_only
            {
                if let Some(ref_node) = arena.get(import_decl.module_specifier)
                    && ref_node.kind == SyntaxKind::StringLiteral as u16
                {
                    value_names.insert(name);
                } else if is_import_alias_referencing_value(
                    arena,
                    import_decl.module_specifier,
                    statements,
                    preserve_const_enums,
                ) {
                    value_names.insert(name);
                }
            }
        }
        _ => {
            // Interface, Type Alias → type-only, no value
        }
    }
}

/// Collect all export names from a source file for the exports initialization
///
/// Returns a list of exported names (e.g., ["foo", "bar"])
pub fn collect_export_names(arena: &NodeArena, statements: &[NodeIndex]) -> Vec<String> {
    collect_export_names_with_options(arena, statements, false, &FxHashSet::default())
}

pub fn collect_export_names_with_options(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
    type_only_nodes: &FxHashSet<NodeIndex>,
) -> Vec<String> {
    let mut exports = Vec::new();

    // Build declaration name sets lazily — only needed when we see named export specifiers.
    // `value_names`: names with runtime value (var, function, class, enum, namespace, import)
    // `type_only_names`: names that are ONLY interfaces/type aliases (no value binding)
    // We skip export specifiers only when the local name is confirmed type-only in the
    // current file.  Cross-file references (not in either set) get void 0 by default.
    let mut value_names: Option<rustc_hash::FxHashSet<String>> = None;
    let mut type_only_names: Option<rustc_hash::FxHashSet<String>> = None;

    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };

        match node.kind {
            // export class C {} / export function f() {} / export { x } / export default ...
            // These are wrapped in EXPORT_DECLARATION nodes
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = arena.get_export_decl(node) {
                    if export_decl.is_type_only {
                        continue;
                    }
                    if export_decl.is_default_export {
                        // Skip default exports from void 0 initialization -
                        // TypeScript doesn't emit `exports.default = void 0;`
                        // Default exports are always assigned inline
                        continue;
                    }

                    if let Some(clause_node) = arena.get(export_decl.export_clause) {
                        // For re-exports with named specifiers (e.g., export { "<X>" as "<Y>" } from "mod"),
                        // also collect their exported names for the preamble void 0 initialization.
                        // tsc gathers all export void 0s (both local and re-export) into one chained line.
                        if export_decl.module_specifier.is_some() {
                            if let Some(named_exports) = arena.get_named_imports(clause_node) {
                                for &spec_idx in &named_exports.elements.nodes {
                                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                                        continue;
                                    };
                                    if spec.is_type_only || type_only_nodes.contains(&spec_idx) {
                                        continue;
                                    }
                                    if let Some(name) = specifier_name_text(arena, spec.name) {
                                        exports.push(name);
                                    }
                                }
                            }
                            // Also collect `export * as "name" from "mod"`
                            else if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS
                                && let Some(name) =
                                    specifier_name_text(arena, export_decl.export_clause)
                            {
                                exports.push(name);
                            }
                            continue;
                        }

                        if let Some(named_exports) = arena.get_named_imports(clause_node) {
                            // Lazily build name sets on first use
                            let vn = value_names.get_or_insert_with(|| {
                                build_value_declaration_names(
                                    arena,
                                    statements,
                                    preserve_const_enums,
                                )
                            });
                            let ton = type_only_names.get_or_insert_with(|| {
                                build_type_only_declaration_names(
                                    arena,
                                    statements,
                                    preserve_const_enums,
                                )
                            });
                            for &spec_idx in &named_exports.elements.nodes {
                                let Some(spec) = arena.get_specifier_at(spec_idx) else {
                                    continue;
                                };
                                if spec.is_type_only {
                                    continue;
                                }
                                if type_only_nodes.contains(&spec_idx) {
                                    continue;
                                }
                                // The local name is property_name if present, otherwise name
                                let local_name = if spec.property_name.is_some() {
                                    get_identifier_text(arena, spec.property_name)
                                } else {
                                    get_identifier_text(arena, spec.name)
                                };
                                // Skip specifiers that refer to confirmed type-only
                                // declarations (interface / type alias) in the current
                                // file with NO value binding.  Cross-file references
                                // (not in either set) get void 0 by default.
                                if let Some(ref local) = local_name
                                    && ton.contains(local)
                                    && !vn.contains(local)
                                {
                                    continue;
                                }
                                // Use the exported name (name), not the local name (property_name)
                                // The exported name can be a string literal (e.g., export { x as "<X>" })
                                if let Some(name) = specifier_name_text(arena, spec.name) {
                                    exports.push(name);
                                }
                            }
                        } else {
                            collect_export_name_from_declaration(
                                arena,
                                clause_node,
                                &mut exports,
                                preserve_const_enums,
                                statements,
                            );
                        }
                    }
                }
            }
            // export const foo = ...
            // export let bar = ...
            // export var baz = ...
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node)
                    && arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.is_declare(&var_stmt.modifiers)
                {
                    for &decl_idx in &var_stmt.declarations.nodes {
                        collect_declaration_names(arena, decl_idx, &mut exports);
                    }
                }
            }
            // export function foo() {}
            // Note: overloaded functions produce multiple FUNCTION_DECLARATION nodes
            // with the same name; deduplicate to avoid repeated exports.
            // Skip overload signatures (no body) — if the implementation also has
            // `export`, it will be added when we encounter it.
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && arena.has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.is_declare(&func.modifiers)
                    && func.body.is_some()
                    && let Some(name) = get_identifier_text(arena, func.name)
                    && !exports.contains(&name)
                {
                    exports.push(name);
                }
            }
            // export class Foo {}
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node)
                    && arena.has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.is_declare(&class.modifiers)
                    && let Some(name) = get_identifier_text(arena, class.name)
                {
                    exports.push(name);
                }
            }
            // export import Foo = require("foo")
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_decl) = arena.get_import_decl(node)
                    && arena.has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword)
                {
                    collect_export_name_from_declaration(
                        arena,
                        node,
                        &mut exports,
                        preserve_const_enums,
                        statements,
                    );
                }
            }
            // export enum E {} / export const enum E {} (when preserveConstEnums)
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node)
                    && arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.is_declare(&enum_decl.modifiers)
                    && (preserve_const_enums
                        || !arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword))
                    && let Some(name) = get_identifier_text(arena, enum_decl.name)
                {
                    exports.push(name);
                }
            }
            // export namespace N {}
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node)
                    && arena.has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.is_declare(&module.modifiers)
                    && super::emit_utils::is_instantiated_module_ext(
                        arena,
                        module.body,
                        preserve_const_enums,
                    )
                    && let Some(name) = get_identifier_text(arena, module.name)
                {
                    exports.push(name);
                }
            }
            _ => {}
        }
    }

    // Deduplicate: merged declarations (e.g., two `export namespace N {}` blocks)
    // or `export class Foo {}` + `export { Foo }` can produce duplicate names.
    // tsc emits each name exactly once in the void 0 initialization.
    let mut seen = std::collections::HashSet::new();
    exports.retain(|name| seen.insert(name.clone()));

    exports
}

/// Collect export names, categorized into function declarations (hoisted)
/// and other declarations (non-hoisted).
/// Categorized exports from a source file, grouped by how `CommonJS` lowering
/// should emit their initialization.
pub struct CategorizedExports {
    /// Function exports as `(exported_name, local_name)` pairs. These are
    /// hoisted so `exports.foo = foo;` can be emitted before the function body.
    pub function_exports: Vec<(String, String)>,
    /// Non-function exports that need `exports.foo = void 0;` initialization.
    pub other_exports: Vec<String>,
    /// Local names of `export default function name() {}` declarations,
    /// used for the hoisted `exports.default = name;` preamble assignment.
    pub default_function_exports: Vec<String>,
}
