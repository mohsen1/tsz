pub fn collect_export_names_categorized(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
    type_only_nodes: &FxHashSet<NodeIndex>,
) -> CategorizedExports {
    let mut func_exports: Vec<(String, String)> = Vec::new(); // (exported_name, local_name)
    let mut other_exports = Vec::new();
    let mut default_func_exports: Vec<String> = Vec::new();
    // Counter for synthetic names used by anonymous `export default function`
    // declarations. tsc emits `default_1`, `default_2`, ... in source order
    // when more than one anonymous default function appears (an error case
    // surfaced by `exportDefaultInterfaceAndTwoFunctions`); a single shared
    // `"default_1"` would collide and produce identical helper names.
    let mut anonymous_default_counter: u32 = 0;
    let all =
        collect_export_names_with_options(arena, statements, preserve_const_enums, type_only_nodes);
    let mut reserved_default_names: FxHashSet<String> = arena
        .identifiers
        .iter()
        .map(|ident| ident.escaped_text.clone())
        .collect();

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
                && !arena.is_declare(&func.modifiers)
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
            && !arena.is_declare(&func.modifiers)
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
            && !arena.is_declare(&func.modifiers)
            && func.body.is_some()
        // skip overload signatures (no body)
        {
            let name = get_identifier_text(arena, func.name)
                .filter(|name| {
                    !name.is_empty() && name != "function" && is_valid_identifier_name(name)
                })
                .unwrap_or_else(|| {
                    loop {
                        anonymous_default_counter += 1;
                        let candidate = format!("default_{anonymous_default_counter}");
                        if reserved_default_names.insert(candidate.clone()) {
                            break candidate;
                        }
                    }
                });
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
                        && !arena.is_declare(&var_stmt.modifiers)
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

    CategorizedExports {
        function_exports: func_exports,
        other_exports,
        default_function_exports: default_func_exports,
    }
}

/// Collect names from inline-exported variable declarations (`export let/const/var`).
///
/// In CJS mode, tsc substitutes ALL identifier references to these variables
/// with `exports.X` (both reads and writes).  This does NOT apply to classes,
/// functions, enums, namespaces, or re-exports (`export { y }`).
pub fn collect_inline_exported_var_names(
    arena: &NodeArena,
    statements: &[NodeIndex],
    preserve_const_enums: bool,
) -> Vec<String> {
    let mut names = Vec::new();
    for &stmt_idx in statements {
        collect_inline_exported_var_names_from_statement(
            arena,
            stmt_idx,
            statements,
            preserve_const_enums,
            &mut names,
        );
    }
    names
}

fn collect_inline_exported_var_names_from_statement(
    arena: &NodeArena,
    stmt_idx: NodeIndex,
    source_statements: &[NodeIndex],
    preserve_const_enums: bool,
    names: &mut Vec<String>,
) {
    let Some(node) = arena.get(stmt_idx) else {
        return;
    };

    match node.kind {
        // Direct: export let/const/var x = ... (including `export declare const`)
        // In CJS, all exported names become `exports.X` properties, even recovered
        // exported variables under control-flow wrappers. tsc qualifies reads of
        // these names as `exports.X` and wraps calls as `(0, exports.X)()`.
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(var_stmt) = arena.get_variable(node)
                && arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                for &decl_idx in &var_stmt.declarations.nodes {
                    collect_declaration_names(arena, decl_idx, names);
                }
            }
        }
        // Wrapped: ExportDeclaration { clause: ... }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            let Some(export_decl) = arena.get_export_decl(node) else {
                return;
            };
            if export_decl.is_type_only || export_decl.module_specifier.is_some() {
                return;
            }
            let Some(clause_node) = arena.get(export_decl.export_clause) else {
                return;
            };
            if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt) = arena.get_variable(clause_node)
            {
                for &decl_idx in &var_stmt.declarations.nodes {
                    collect_declaration_names(arena, decl_idx, names);
                }
            }
            // ExportDeclaration { clause: ImportEqualsDeclaration }
            // `export import b = a.foo` — the alias name becomes `exports.b`
            else if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import_decl) = arena.get_import_decl(clause_node)
                && let Some(name) = get_identifier_text(arena, import_decl.import_clause)
            {
                if import_decl.is_type_only {
                    return;
                }
                if import_alias_resolves_to_exported_type_only(
                    arena,
                    import_decl.module_specifier,
                    source_statements,
                    preserve_const_enums,
                ) {
                    return;
                }
                names.push(name);
            }
        }
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &inner_idx in &block.statements.nodes {
                    collect_inline_exported_var_names_from_statement(
                        arena,
                        inner_idx,
                        source_statements,
                        preserve_const_enums,
                        names,
                    );
                }
            }
        }
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    if_stmt.then_statement,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
                collect_inline_exported_var_names_from_statement(
                    arena,
                    if_stmt.else_statement,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::LABELED_STATEMENT => {
            if let Some(labeled) = arena.get_labeled_statement(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    labeled.statement,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::TRY_STATEMENT => {
            if let Some(try_stmt) = arena.get_try(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    try_stmt.try_block,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
                collect_inline_exported_var_names_from_statement(
                    arena,
                    try_stmt.catch_clause,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
                collect_inline_exported_var_names_from_statement(
                    arena,
                    try_stmt.finally_block,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::CATCH_CLAUSE => {
            if let Some(catch_clause) = arena.get_catch_clause(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    catch_clause.block,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    loop_data.statement,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::FOR_IN_STATEMENT || k == syntax_kind_ext::FOR_OF_STATEMENT => {
            if let Some(for_in_of) = arena.get_for_in_of(node) {
                collect_inline_exported_var_names_from_statement(
                    arena,
                    for_in_of.statement,
                    source_statements,
                    preserve_const_enums,
                    names,
                );
            }
        }
        k if k == syntax_kind_ext::SWITCH_STATEMENT => {
            if let Some(switch_stmt) = arena.get_switch(node)
                && let Some(case_block_node) = arena.get(switch_stmt.case_block)
                && let Some(block_data) = arena.blocks.get(case_block_node.data_index as usize)
            {
                for &clause_idx in &block_data.statements.nodes {
                    collect_inline_exported_var_names_from_statement(
                        arena,
                        clause_idx,
                        source_statements,
                        preserve_const_enums,
                        names,
                    );
                }
            }
        }
        k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
            if let Some(clause) = arena.get_case_clause(node) {
                for &inner_idx in &clause.statements.nodes {
                    collect_inline_exported_var_names_from_statement(
                        arena,
                        inner_idx,
                        source_statements,
                        preserve_const_enums,
                        names,
                    );
                }
            }
        }
        // Function, class, and namespace bodies have their own binding/export scopes.
        _ => {}
    }
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
