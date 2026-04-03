//! `CommonJS` Module Transform
//!
//! Transforms ES modules to `CommonJS` format:
//!
//! ```typescript
//! import { foo } from "./module";
//! export const bar = 42;
//! export default myFunc;
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! "use strict";
//! Object.defineProperty(exports, "__esModule", { value: true });
//! exports.bar = void 0;
//! var module_1 = require("./module");
//! exports.bar = 42;
//! exports.default = myFunc;
//! ```

use crate::transforms::emit_utils::{
    identifier_text as get_identifier_text, sanitize_module_name, specifier_name_text,
    string_literal_text,
};
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
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if rest.is_empty() {
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
                        );
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
                if arena.has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword) {
                    return;
                }
                if let Some(name) = get_identifier_text(arena, class.name) {
                    exports.push(name);
                }
            }
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            if let Some(func) = arena.get_function(decl_node) {
                if arena.has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword) {
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
                if arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword) {
                    return;
                }
                for &decl_idx in &var_stmt.declarations.nodes {
                    collect_declaration_names(arena, decl_idx, exports);
                }
            }
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enum_decl) = arena.get_enum(decl_node) {
                if arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword) {
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
                if arena.has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword) {
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
                // A string-literal module_specifier means `require("...")` — always a value.
                if let Some(ref_node) = arena.get(import_decl.module_specifier)
                    && ref_node.kind == SyntaxKind::StringLiteral as u16
                {
                    exports.push(name);
                    return;
                }
                if is_import_alias_referencing_value(
                    arena,
                    import_decl.module_specifier,
                    statements,
                    preserve_const_enums,
                ) {
                    exports.push(name);
                }
            }
        }
        _ => {
            // Interface, Type Alias, etc. don't need runtime exports
        }
    }
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
    collect_export_names_with_options(arena, statements, false)
}

pub fn collect_export_names_with_options(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
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
                                    if spec.is_type_only {
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
                    && !arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
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
                    && !arena.has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
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
                    && !arena.has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
                    && let Some(name) = get_identifier_text(arena, class.name)
                {
                    exports.push(name);
                }
            }
            // export enum E {} / export const enum E {} (when preserveConstEnums)
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node)
                    && arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ExportKeyword)
                    && !arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
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
                    && !arena.has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
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
/// Returns (`function_exports`, `other_exports`, `default_func_export`)
/// where `default_func_export` is `Some(local_name)` when the file has
/// `export default function name() {}` — the local function name for the
/// hoisted `exports.default = name;` preamble assignment.
#[allow(clippy::type_complexity)]
pub fn collect_export_names_categorized(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> (Vec<(String, String)>, Vec<String>, Vec<String>) {
    let mut func_exports: Vec<(String, String)> = Vec::new(); // (exported_name, local_name)
    let mut other_exports = Vec::new();
    let mut default_func_exports: Vec<String> = Vec::new();
    let all = collect_export_names_with_options(arena, statements, preserve_const_enums);

    // First pass: collect all function declaration names in the file (including
    // non-exported ones and `declare function` names) so we can resolve
    // `export { f }` specifiers. `declare function` names are included because
    // tsc treats them as hoisted (no `void 0` initialization) — the runtime
    // binding is expected to exist via ambient declaration.
    let mut func_decl_names: Vec<String> = Vec::new();
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = arena.get_function(node)
            && let Some(name) = get_identifier_text(arena, func.name)
            && !func_decl_names.contains(&name)
        {
            func_decl_names.push(name);
        }
        // Also look inside EXPORT_DECLARATION wrappers for function declarations
        // (e.g., `export default function f() {}` wraps FUNCTION_DECLARATION in EXPORT_DECLARATION)
        else if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = arena.get_export_decl(node)
            && export_decl.module_specifier.is_none()
            && let Some(clause_node) = arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = arena.get_function(clause_node)
            && let Some(name) = get_identifier_text(arena, func.name)
            && !func_decl_names.contains(&name)
        {
            func_decl_names.push(name);
        }
    }

    // Second pass: categorize exports as function (hoisted) vs other
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };

        // Direct: export function f() {}
        // Note: overloaded functions produce multiple FUNCTION_DECLARATION nodes
        // with the same name; deduplicate to emit only one `exports.X = X;`.
        // Only count functions that have a body (implementation), not overload
        // signatures.  When an overload signature has `export` but the
        // implementation does not, tsc does NOT export the function.
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = arena.get_function(node)
                && arena.has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                && !arena.has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
                && func.body.is_some()
                && let Some(name) = get_identifier_text(arena, func.name)
                && !func_exports.iter().any(|(e, _)| e == &name)
            {
                func_exports.push((name.clone(), name));
            }
        }
        // Wrapped: ExportDeclaration { clause: FunctionDeclaration }
        // Only include functions with a body (implementation), not overload
        // signatures, matching tsc behavior.
        else if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = arena.get_export_decl(node)
            && !export_decl.is_type_only
            && !export_decl.is_default_export
            && export_decl.module_specifier.is_none()
            && let Some(clause_node) = arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = arena.get_function(clause_node)
            && !arena.has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
            && func.body.is_some()
            && let Some(name) = get_identifier_text(arena, func.name)
            && !func_exports.iter().any(|(e, _)| e == &name)
        {
            func_exports.push((name.clone(), name));
        }
        // Default function export: export default function func() {}
        // tsc hoists `exports.default = func;` to the preamble, just like
        // named function exports, because JS function declarations are hoisted.
        else if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = arena.get_export_decl(node)
            && !export_decl.is_type_only
            && export_decl.is_default_export
            && export_decl.module_specifier.is_none()
            && let Some(clause_node) = arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = arena.get_function(clause_node)
            && !arena.has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
            && func.body.is_some() // skip overload signatures (no body)
            && let Some(name) = get_identifier_text(arena, func.name)
        {
            default_func_exports.push(name);
        }
        // Named export specifiers: export { f } where f is a function declaration
        // JS function declarations are hoisted, so `exports.f = f;` can appear
        // in the preamble (before the function body), matching tsc behavior.
        else if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = arena.get_export_decl(node)
            && !export_decl.is_type_only
            && !export_decl.is_default_export
            && export_decl.module_specifier.is_none()
            && let Some(clause_node) = arena.get(export_decl.export_clause)
            && let Some(named_exports) = arena.get_named_imports(clause_node)
        {
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec) = arena.get_specifier_at(spec_idx)
                    && !spec.is_type_only
                {
                    // The local name is property_name if present, otherwise name
                    // Both can be string literals in ES2022+ arbitrary module namespace identifiers
                    let local_name = if spec.property_name.is_some() {
                        specifier_name_text(arena, spec.property_name)
                    } else {
                        specifier_name_text(arena, spec.name)
                    };
                    let exported_name = specifier_name_text(arena, spec.name);
                    if let (Some(local), Some(exported)) = (local_name, exported_name)
                        && func_decl_names.contains(&local)
                        && !func_exports.iter().any(|(e, _)| e == &exported)
                    {
                        func_exports.push((exported, local));
                    }
                }
            }
        }
    }

    // `other_exports` is the set of names that get `exports.X = void 0;`
    // initialization. Names that are ONLY function exports (hoisted) do not
    // need void 0 because the hoisted `exports.f = f;` suffices. However,
    // names that appear as BOTH a variable and function export (e.g.,
    // `export var a = 10; export function a() {}`) still need void 0 for the
    // variable binding, matching tsc behavior.
    let func_only_names: rustc_hash::FxHashSet<&str> =
        func_exports.iter().map(|(e, _)| e.as_str()).collect();
    for name in all {
        // A name needs void 0 unless it ONLY appears as a function export
        // (i.e., it was collected solely because of a function declaration).
        // If it was collected from both a var statement AND a function, it
        // appears in `all` from the var path and should get void 0.
        if func_only_names.contains(name.as_str()) {
            // Check if this name was also collected from a non-function source.
            // Since `all` deduplicates, we can't tell from `all` alone.
            // Instead, keep it if the name appears in func_exports AND was
            // also listed by a non-function source (the name in `all` came
            // from the function branch at line 434-443, but it could also
            // come from variable/class/enum/namespace/specifier branches).
            // The simplest approach: check if the file has a non-function
            // declaration with this name.
            let has_non_func_source = statements.iter().any(|&stmt_idx| {
                let Some(node) = arena.get(stmt_idx) else {
                    return false;
                };
                // Check if a VARIABLE_STATEMENT contains the target name
                let var_has_name = |n: &Node| -> bool {
                    if n.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        && let Some(var_stmt) = arena.get_variable(n)
                        && !arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
                    {
                        let mut names = Vec::new();
                        for &decl_idx in &var_stmt.declarations.nodes {
                            collect_declaration_names(arena, decl_idx, &mut names);
                        }
                        return names.contains(&name);
                    }
                    false
                };
                match node.kind {
                    // Direct VARIABLE_STATEMENT must be exported to count
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = arena.get_variable(node)
                            && arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                        {
                            var_has_name(node)
                        } else {
                            false
                        }
                    }
                    // EXPORT_DECLARATION wrapping a VARIABLE_STATEMENT is already exported
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        if let Some(export_decl) = arena.get_export_decl(node)
                            && !export_decl.is_type_only
                            && export_decl.module_specifier.is_none()
                            && let Some(clause_node) = arena.get(export_decl.export_clause)
                        {
                            var_has_name(clause_node)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            });
            if has_non_func_source {
                other_exports.push(name);
            }
        } else {
            other_exports.push(name);
        }
    }

    // TypeScript emits void 0 initialization in source order, chunked into
    // groups of 50, with each chunk reversed (via reduceLeft in tsc).
    // We keep source order here and let the emit code handle chunking+reversal.

    (func_exports, other_exports, default_func_exports)
}

/// Collect names from inline-exported variable declarations (`export let/const/var`).
///
/// In CJS mode, tsc substitutes ALL identifier references to these variables
/// with `exports.X` (both reads and writes).  This does NOT apply to classes,
/// functions, enums, namespaces, or re-exports (`export { y }`).
pub fn collect_inline_exported_var_names(
    arena: &NodeArena,
    statements: &[NodeIndex],
) -> Vec<String> {
    let mut names = Vec::new();
    for &stmt_idx in statements {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        // Direct: export let/const/var x = ...
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_stmt) = arena.get_variable(node)
                && arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                && !arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
            {
                for &decl_idx in &var_stmt.declarations.nodes {
                    collect_declaration_names(arena, decl_idx, &mut names);
                }
            }
        }
        // Wrapped: ExportDeclaration { clause: VariableStatement }
        else if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = arena.get_export_decl(node)
            && !export_decl.is_type_only
            && export_decl.module_specifier.is_none()
            && let Some(clause_node) = arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = arena.get_variable(clause_node)
            && !arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
        {
            for &decl_idx in &var_stmt.declarations.nodes {
                collect_declaration_names(arena, decl_idx, &mut names);
            }
        }
    }
    names
}

/// Emit the exports initialization line
///
/// ```javascript
/// exports.foo = exports.bar = void 0;
/// ```
pub fn emit_exports_init(
    writer: &mut impl std::fmt::Write,
    exports: &[String],
) -> std::fmt::Result {
    if exports.is_empty() {
        return Ok(());
    }

    for chunk in exports.chunks(50) {
        for (i, name) in chunk.iter().enumerate() {
            if i > 0 {
                write!(writer, " = ")?;
            }
            write!(writer, "exports.{name}")?;
        }
        writeln!(writer, " = void 0;")?;
    }

    Ok(())
}

/// Transform an import declaration to `CommonJS` require
///
/// ```typescript
/// import { foo, bar } from "./module";
/// ```
/// Becomes:
/// ```javascript
/// var module_1 = require("./module");
/// ```
pub fn transform_import_to_require(
    arena: &NodeArena,
    node: &Node,
    module_counter: &mut u32,
) -> Option<(String, String)> {
    let import = arena.get_import_decl(node)?;

    // Get module specifier text
    let module_spec = string_literal_text(arena, import.module_specifier)?;

    // Generate module variable name (e.g., module_1, module_2)
    *module_counter += 1;
    let var_name = format!("{}_1", sanitize_module_name(&module_spec));

    // Return (var_name, require_statement)
    let require_stmt = format!("var {var_name} = require(\"{module_spec}\");");

    Some((var_name, require_stmt))
}

/// Transform import bindings to variable declarations
///
/// For:
/// ```typescript
/// import { foo, bar as baz } from "./module";
/// ```
///
/// After `var module_1 = require("./module");`:
/// We don't need separate var declarations - just use `module_1.foo` directly
///
/// For default imports:
/// ```typescript
/// import myDefault from "./module";
/// ```
/// Becomes:
/// ```javascript
/// var myDefault = module_1.default;
/// ```
pub fn get_import_bindings(
    arena: &NodeArena,
    node: &Node,
    module_var: &str,
    es_module_interop: bool,
) -> Vec<String> {
    let mut bindings = Vec::new();

    let Some(import) = arena.get_import_decl(node) else {
        return bindings;
    };

    let Some(clause_node) = arena.get(import.import_clause) else {
        return bindings;
    };

    let Some(clause) = arena.get_import_clause(clause_node) else {
        return bindings;
    };

    if clause.is_type_only {
        return bindings;
    }

    // Default import: import foo from "..."
    if clause.name.is_some()
        && let Some(name) = get_identifier_text(arena, clause.name)
    {
        // Bind to the default value directly so local identifier references
        // preserve TS-style runtime behavior.
        bindings.push(format!("var {name} = {module_var}.default;"));
    }

    // Named bindings: import { a, b as c } from "..." or import * as ns from "..."
    if clause.named_bindings.is_some()
        && let Some(named_node) = arena.get(clause.named_bindings)
    {
        // NamedImportsData handles both namespace and named imports
        if let Some(named_imports) = arena.get_named_imports(named_node) {
            // Check if it's a namespace import: import * as ns from "..."
            // Namespace imports have a name but no elements
            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                if let Some(name) = get_identifier_text(arena, named_imports.name) {
                    if es_module_interop {
                        // Use __importStar helper for namespace imports
                        bindings.push(format!("var {name} = __importStar({module_var});"));
                    } else {
                        // Without esModuleInterop, namespace import is just an alias
                        bindings.push(format!("var {name} = {module_var};"));
                    }
                }
            } else {
                // Named imports (`import { a, b as c } from "..."`) should not emit
                // local alias vars in CommonJS output; call sites are rewritten to
                // property accesses on the module temp (`module_1.a`), matching tsc.
            }
        }
    }

    bindings
}

/// Generate export assignment for a name
///
/// ```javascript
/// exports.foo = foo;
/// ```
pub fn emit_export_assignment(name: &str) -> String {
    format!("exports.{name} = {name};")
}

/// Generate Object.defineProperty for re-exports
///
/// For:
/// ```typescript
/// export { foo } from "./module";
/// ```
/// Becomes:
/// ```javascript
/// Object.defineProperty(exports, "foo", { enumerable: true, get: function () { return module_1.foo; } });
/// ```
pub fn emit_reexport_property(export_name: &str, module_var: &str, import_name: &str) -> String {
    format!(
        "Object.defineProperty(exports, \"{export_name}\", {{ enumerable: true, get: function () {{ return {module_var}.{import_name}; }} }});"
    )
}

/// Collect exported names from a variable declaration (identifier or binding pattern).
fn collect_declaration_names(arena: &NodeArena, decl_idx: NodeIndex, exports: &mut Vec<String>) {
    let Some(decl_node) = arena.get(decl_idx) else {
        return;
    };

    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(decl_list) = arena.get_variable(decl_node) {
            for &inner_decl_idx in &decl_list.declarations.nodes {
                collect_declaration_names(arena, inner_decl_idx, exports);
            }
        }
        return;
    }

    if let Some(decl) = arena.get_variable_declaration(decl_node) {
        collect_binding_names(arena, decl.name, exports);
    }
}

fn collect_binding_names(arena: &NodeArena, name_idx: NodeIndex, exports: &mut Vec<String>) {
    if name_idx.is_none() {
        return;
    }

    let Some(node) = arena.get(name_idx) else {
        return;
    };

    if node.kind == SyntaxKind::Identifier as u16 {
        if let Some(id) = arena.get_identifier(node) {
            exports.push(id.escaped_text.clone());
        }
        return;
    }

    match node.kind {
        k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
        {
            if let Some(pattern) = arena.get_binding_pattern(node) {
                for &elem_idx in &pattern.elements.nodes {
                    collect_binding_names_from_element(arena, elem_idx, exports);
                }
            }
        }
        k if k == syntax_kind_ext::BINDING_ELEMENT => {
            if let Some(elem) = arena.get_binding_element(node) {
                collect_binding_names(arena, elem.name, exports);
            }
        }
        _ => {}
    }
}

fn collect_binding_names_from_element(
    arena: &NodeArena,
    elem_idx: NodeIndex,
    exports: &mut Vec<String>,
) {
    if elem_idx.is_none() {
        return;
    }

    let Some(elem_node) = arena.get(elem_idx) else {
        return;
    };

    if let Some(elem) = arena.get_binding_element(elem_node) {
        collect_binding_names(arena, elem.name, exports);
    }
}

#[cfg(test)]
mod tests {
    use super::collect_export_names;
    use tsz_parser::ParserState;

    /// When a module has two `export namespace N {}` blocks (merged declarations),
    /// `collect_export_names` must return `N` only once, matching tsc's behavior
    /// for the `exports.N = void 0` initialization line.
    #[test]
    fn collect_export_names_deduplicates_merged_namespaces() {
        let source = "export namespace N { export const a = 1; }\nexport namespace N { export const b = 2; }\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let sf_node = parser.arena.get(root).unwrap();
        let stmts = parser.arena.get_source_file(sf_node).unwrap();
        let names = collect_export_names(&parser.arena, &stmts.statements.nodes);

        let n_count = names.iter().filter(|n| n.as_str() == "N").count();
        assert_eq!(
            n_count, 1,
            "Merged namespace declarations should produce exactly one export name, got: {names:?}"
        );
    }

    /// When exports are unique, deduplication should not remove anything.
    #[test]
    fn collect_export_names_preserves_unique_names() {
        let source = "export const a = 1;\nexport const b = 2;\nexport function c() {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let sf_node = parser.arena.get(root).unwrap();
        let stmts = parser.arena.get_source_file(sf_node).unwrap();
        let names = collect_export_names(&parser.arena, &stmts.statements.nodes);

        assert_eq!(
            names.len(),
            3,
            "All unique names should be preserved: {names:?}"
        );
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
        assert!(names.contains(&"c".to_string()));
    }
}
