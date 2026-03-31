//! Module and namespace declaration validation (TS2580, TS2668, TS2669, TS2433,
//! TS2434, TS2435, TS1035, TS1235, TS5061, TS2664, TS2666/TS2667).

use crate::declarations::DeclarationChecker;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Check a module/namespace declaration.
    pub fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        if let Some(module) = self.ctx.arena.get_module(node) {
            // Anonymous module declarations (`module { }`) already have TS1437 from the parser.
            // tsc does NOT additionally emit TS2591 for these — the parse error is sufficient.

            // TS2397: Declaration name conflicts with built-in global identifier.
            // Namespaces named `globalThis` conflict with the built-in global.
            // `globalThis` only conflicts in script files (non-modules), since
            // module-scoped declarations don't pollute the global scope.
            // Note: `namespace undefined` is allowed by tsc — TS2397 for `undefined`
            // is only emitted for value declarations (var/let/const), not namespaces.
            if let Some(name_node) = self.ctx.arena.get(module.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();
                let should_emit = name == "globalThis" && !self.ctx.binder.is_external_module();
                if should_emit {
                    let message = format_message(
                        diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                        &[name],
                    );
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        message,
                        diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    );
                }
            }

            // TS2567: Namespace merging with const enum
            if let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(module_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let has_const_enum_decl = symbol.declarations.iter().any(|&decl_idx| {
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && decl_node.kind == syntax_kind_ext::ENUM_DECLARATION
                    {
                        self.ctx.arena.get_enum(decl_node).is_some_and(|enum_data| {
                            self.ctx
                                .arena
                                .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                        })
                    } else {
                        false
                    }
                });
                if has_const_enum_decl {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS.to_string(),
                        diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                    );
                }
            }

            // TS2668: 'export' modifier cannot be applied to ambient modules
            // This only applies to string-literal-named ambient modules (declare module "foo"),
            // not to namespace-form modules (declare namespace Foo)
            // Check this FIRST before early returns so we can emit multiple errors
            let has_declare = self
                .ctx
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword);
            let has_export = self
                .ctx
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword);

            // Only check for TS2668 if this is a string-literal-named module
            let is_string_named = if let Some(name_node) = self.ctx.arena.get(module.name) {
                name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            } else {
                false
            };

            if has_declare && has_export && is_string_named {
                // Find the export modifier position to report error there
                if let Some(ref mods) = module.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                            && mod_node.kind == SyntaxKind::ExportKeyword as u16
                        {
                            self.ctx.error(
                                    mod_node.pos,
                                    mod_node.end - mod_node.pos,
                                    "'export' modifier cannot be applied to ambient modules and module augmentations since they are always visible.".to_string(),
                                    2668, // TS2668
                                );
                            break;
                        }
                    }
                }
            }

            // TS2669/TS2670: Global scope augmentations must be directly nested in
            // external modules or ambient module declarations, and should have `declare`.
            // Only check the GLOBAL_AUGMENTATION flag set by the parser — a plain
            // `namespace global {}` in a non-module file is a regular namespace, not
            // a global augmentation.
            let is_global_augmentation = (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0;
            if is_global_augmentation {
                let mut allowed_context = false;
                if let Some(ext) = self.ctx.arena.get_extended(module_idx) {
                    let parent = ext.parent;
                    if parent.is_some()
                        && let Some(parent_node) = self.ctx.arena.get(parent)
                    {
                        if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                            allowed_context = self.is_external_module();
                        } else if parent_node.kind == syntax_kind_ext::MODULE_BLOCK
                            && let Some(parent_ext) = self.ctx.arena.get_extended(parent)
                        {
                            let gp = parent_ext.parent;
                            if let Some(gp_node) = self.ctx.arena.get(gp)
                                && gp_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                && let Some(gp_module) = self.ctx.arena.get_module(gp_node)
                                && self
                                    .ctx
                                    .arena
                                    .has_modifier(&gp_module.modifiers, SyntaxKind::DeclareKeyword)
                            {
                                let gp_name_node = self.ctx.arena.get(gp_module.name);
                                let gp_is_string_named = gp_name_node.is_some_and(|name_node| {
                                    name_node.kind == SyntaxKind::StringLiteral as u16
                                        || name_node.kind
                                            == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                                });
                                if gp_is_string_named {
                                    allowed_context = true;
                                }
                            }
                        }
                    }
                }

                let error_node = self.ctx.arena.get(module.name).unwrap_or(node);
                if !allowed_context {
                    self.ctx.error(
                        error_node.pos,
                        error_node.end - error_node.pos,
                        diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL.to_string(),
                        diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
                    );
                }
                if !has_declare && !self.is_in_ambient_context(module_idx) {
                    self.ctx.error(
                        error_node.pos,
                        error_node.end - error_node.pos,
                        diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE.to_string(),
                        diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE,
                    );
                }
            }

            // TS1294: erasableSyntaxOnly — non-ambient instantiated modules are not erasable.
            // tsc reports the error at node.name, matching getErrorSpanForNode behavior.
            if self.ctx.compiler_options.erasable_syntax_only
                && !self.ctx.is_ambient_declaration(module_idx)
                && module.body.is_some()
                && self.is_namespace_declaration_instantiated(module_idx)
            {
                let error_node = self.ctx.arena.get(module.name).unwrap_or(node);
                self.ctx.error(
                    error_node.pos,
                    error_node.end - error_node.pos,
                    diagnostic_messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED
                        .to_string(),
                    diagnostic_codes::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED,
                );
            }

            // TS2433/TS2434: Check namespace merging with class/function
            // A namespace declaration cannot be in a different file from a class/function
            // with which it is merged (TS2433), or located prior to the class/function (TS2434).
            // Only check for non-ambient, non-string-named, INSTANTIATED modules.
            // Uninstantiated namespaces (containing only interfaces/type aliases) are allowed
            // to precede a class/function they merge with.
            if !has_declare
                && !is_string_named
                && module.body.is_some()
                && !self.is_in_ambient_context(module_idx)
                && self.is_namespace_declaration_instantiated(module_idx)
            {
                self.check_namespace_merges_with_class_or_function(module_idx, module);
            }

            // TS1035: Only ambient modules can use quoted names.
            // `module "Foo" {}` without `declare` is invalid.
            // Exception: module augmentations inside ambient contexts (e.g., inside
            // `declare module "Map" { module "Observable" { } }`) are already ambient.
            if !has_declare
                && is_string_named
                && !self.is_in_ambient_context(module_idx)
                && let Some(name_node) = self.ctx.arena.get(module.name)
            {
                self.ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    diagnostic_messages::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES.to_string(),
                    diagnostic_codes::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES,
                );
            }

            // TS2435 / TS1234: Ambient modules cannot be nested or placed in wrong context.
            // Check if this is an ambient external module (declare module "string")
            if let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
            {
                // First check: is the direct parent a valid context (SourceFile or ModuleBlock)?
                // If not, emit TS1234 (wrong context takes priority over nesting check).
                // Note: when the module declaration is wrapped in `export`, the immediate
                // parent is EXPORT_DECLARATION. In that case, check the grandparent.
                let is_valid_context = if let Some(ext) = self.ctx.arena.get_extended(module_idx) {
                    let parent = ext.parent;
                    if parent.is_none() {
                        true
                    } else if let Some(parent_node) = self.ctx.arena.get(parent) {
                        if parent_node.kind == syntax_kind_ext::SOURCE_FILE
                            || parent_node.kind == syntax_kind_ext::MODULE_BLOCK
                        {
                            true
                        } else if parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            // Look through export wrapper to check grandparent
                            self.ctx
                                .arena
                                .get_extended(parent)
                                .and_then(|gp_ext| self.ctx.arena.get(gp_ext.parent))
                                .is_none_or(|gp_node| {
                                    gp_node.kind == syntax_kind_ext::SOURCE_FILE
                                        || gp_node.kind == syntax_kind_ext::MODULE_BLOCK
                                })
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                if !is_valid_context {
                    // TS1234: An ambient module declaration is only allowed at the top level in a file.
                    // This fires when `declare module "string"` is inside a block or function body.
                    if !self.ctx.has_syntax_parse_errors {
                        let decl_start = self.ctx.arena.get(module_idx).map(|n| n.pos).unwrap_or(0);
                        let start = if let Some(sf) = self.ctx.arena.source_files.first() {
                            let bytes = sf.text.as_bytes();
                            let mut pos = decl_start as usize;
                            while pos < bytes.len()
                                && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n')
                            {
                                pos += 1;
                            }
                            pos as u32
                        } else {
                            decl_start
                        };
                        self.ctx.error(
                            start,
                            name_node.end - start,
                            diagnostic_messages::AN_AMBIENT_MODULE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_IN_A_FILE.to_string(),
                            diagnostic_codes::AN_AMBIENT_MODULE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_IN_A_FILE,
                        );
                    }
                    // Don't also emit TS2435 when TS1234 fires
                    return;
                }

                // TS2435: Ambient modules cannot be nested in other modules or namespaces.
                // Only check when in a valid syntactic context (ModuleBlock) but nested
                // inside a namespace.
                if self.is_inside_namespace(module_idx) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "Ambient modules cannot be nested in other modules or namespaces."
                            .to_string(),
                        diagnostic_codes::AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES,
                    );
                    return;
                }
            }

            // TS1235: A namespace declaration is only allowed at the top level of a namespace or module.
            // This applies to non-string-named module/namespace declarations that are inside labeled statements
            // or other non-module constructs.
            // Suppressed when file has parse errors (tsc's grammarErrorOnNode checks hasParseDiagnostics).
            if !is_string_named && !self.ctx.has_syntax_parse_errors {
                // Check if the parent is a valid context
                // Valid parents:
                // - SourceFile (top-level namespace)
                // - ModuleBlock (namespace inside namespace body)
                // - ModuleDeclaration (dotted namespace like namespace A.B { })
                // - ExportDeclaration (export namespace X { })
                let is_valid_context = if let Some(ext) = self.ctx.arena.get_extended(module_idx) {
                    let parent = ext.parent;
                    if parent.is_none() {
                        true // Top level is valid
                    } else if let Some(parent_node) = self.ctx.arena.get(parent) {
                        // Valid parents: SourceFile, ModuleBlock, ModuleDeclaration
                        if parent_node.kind == syntax_kind_ext::SOURCE_FILE
                            || parent_node.kind == syntax_kind_ext::MODULE_BLOCK
                            || parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        {
                            true
                        } else if parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            // Check if the export declaration is inside a valid context
                            if let Some(parent_ext) = self.ctx.arena.get_extended(parent) {
                                let grandparent = parent_ext.parent;
                                if let Some(gp_node) = self.ctx.arena.get(grandparent) {
                                    gp_node.kind == syntax_kind_ext::SOURCE_FILE
                                        || gp_node.kind == syntax_kind_ext::MODULE_BLOCK
                                        || gp_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                if !is_valid_context && let Some(name_node) = self.ctx.arena.get(module.name) {
                    // tsc spans from the outermost keyword (`export` or `namespace`)
                    // to the end of the name. If the module is inside an EXPORT_DECLARATION,
                    // use that parent's pos to include the `export` keyword.
                    let decl_pos = self
                        .ctx
                        .arena
                        .get_extended(module_idx)
                        .and_then(|ext| self.ctx.arena.get(ext.parent))
                        .filter(|p| p.kind == syntax_kind_ext::EXPORT_DECLARATION)
                        .map(|p| p.pos)
                        .or_else(|| self.ctx.arena.get(module_idx).map(|n| n.pos))
                        .unwrap_or(name_node.pos);
                    // Skip leading whitespace/newlines to find actual keyword start
                    let start = if let Some(sf) = self.ctx.arena.source_files.first() {
                        let bytes = sf.text.as_bytes();
                        let mut pos = decl_pos as usize;
                        while pos < bytes.len()
                            && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n')
                        {
                            pos += 1;
                        }
                        pos as u32
                    } else {
                        decl_pos
                    };
                    self.ctx.error(
                        start,
                        name_node.end - start,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL,
                    );
                }
            }

            // TS5061: Check for relative module names in ambient declarations
            // declare module "./foo" { } -> Error (only in script/non-module files)
            // In module files, `declare module "./foo"` is a module augmentation, not
            // an ambient module declaration, and relative paths are valid.
            if self
                .ctx
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
                && let Some(lit) = self.ctx.arena.get_literal(name_node)
            {
                // Check TS5061 first - only for true ambient declarations (non-module files)
                if self.is_relative_module_name(&lit.text) && !self.is_external_module() {
                    self.ctx.error(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    diagnostic_messages::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME.to_string(),
                                    diagnostic_codes::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME,
                                );
                }
                // TS2664: Check if the module being augmented exists
                // declare module "nonexistent" { } -> Error if module doesn't exist
                // Only emit TS2664 if:
                // 1. The file is a module file (has import/export statements)
                // 2. The file is not a .d.ts file
                // 3. The module name is not a relative path (relative augmentations
                //    refer to local files which may not be resolved in all contexts)
                // In script files (no imports/exports), declare module "xxx" declares
                // an ambient external module, which is always valid.
                else if !self.module_exists(&lit.text)
                    && !self.is_declaration_file()
                    && self.is_external_module()
                    && !self.is_relative_module_name(&lit.text)
                {
                    let message = format_message(
                        diagnostic_messages::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND,
                        &[&lit.text],
                    );
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        message,
                        diagnostic_codes::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND,
                    );
                } else if self.is_external_module()
                    && self.module_exists(&lit.text)
                    && self.ctx.module_resolves_to_non_module_entity(&lit.text)
                {
                    let has_value_exports = self.module_augmentation_has_value_exports(module.body);
                    let (code, message) = if has_value_exports {
                        (
                            diagnostic_codes::CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT,
                            format_message(
                                diagnostic_messages::CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT,
                                &[&lit.text],
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY,
                            format_message(
                                diagnostic_messages::CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY,
                                &[&lit.text],
                            ),
                        )
                    };
                    self.ctx
                        .error(name_node.pos, name_node.end - name_node.pos, message, code);
                }
            }

            // TS2666/TS2667: Imports/exports are not permitted in module augmentations
            // Only check imports/exports if the augmentation target module actually exists.
            // tsc uses the Transient flag (set only on merged augmentations) to skip
            // this check for unresolved targets — avoiding cascading errors.
            // However, duplicate value declaration tracking (TS2451) must always run.
            let module_augmentation_target_exists = has_declare
                && is_string_named
                && self.is_external_module()
                && self
                    .ctx
                    .arena
                    .get(module.name)
                    .and_then(|n| self.ctx.arena.get_literal(n))
                    .is_some_and(|lit| self.module_exists(&lit.text));
            let should_check_augmentation_body =
                has_declare && is_string_named && self.is_external_module();
            if should_check_augmentation_body {
                let module_specifier = self
                    .ctx
                    .arena
                    .get(module.name)
                    .and_then(|name_node| self.ctx.arena.get_literal(name_node))
                    .map(|lit| lit.text.clone());
                let module_key = module_specifier.as_deref().map_or_else(
                    || "<unknown>".to_string(),
                    |spec| self.normalize_module_augmentation_key(spec),
                );

                let mut value_decl_map = self
                    .ctx
                    .module_augmentation_value_decls
                    .remove(&module_key)
                    .unwrap_or_default();
                let mut reported_import = false;
                let mut reported_export = false;
                if module.body.is_some()
                    && let Some(body_node) = self.ctx.arena.get(module.body)
                    && body_node.kind == syntax_kind_ext::MODULE_BLOCK
                    && let Some(block) = self.ctx.arena.get_module_block(body_node)
                    && let Some(ref stmts) = block.statements
                {
                    let mut register_value_name = |name: &str, name_node: NodeIndex| -> bool {
                        if value_decl_map.contains_key(name) {
                            true
                        } else {
                            value_decl_map.insert(name.to_string(), name_node);
                            false
                        }
                    };
                    for &stmt_idx in &stmts.nodes {
                        if reported_import && reported_export {
                            // Once both import/export errors are reported, we still
                            // need to continue for duplicate value tracking below,
                            // so don't break.
                        }
                        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                            continue;
                        };
                        let kind = stmt_node.kind;
                        if module_augmentation_target_exists
                            && !reported_import
                            && (kind == syntax_kind_ext::IMPORT_DECLARATION
                                || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
                        {
                            self.ctx.error(
                                            stmt_node.pos,
                                            stmt_node.end - stmt_node.pos,
                                            diagnostic_messages::IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN.to_string(),
                                            diagnostic_codes::IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN,
                                        );
                            reported_import = true;
                        } else if module_augmentation_target_exists && !reported_export {
                            let is_forbidden_export = if kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                                || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                            {
                                true
                            } else if kind == syntax_kind_ext::EXPORT_DECLARATION {
                                match self.ctx.arena.get_export_decl(stmt_node) {
                                    Some(export_decl) => {
                                        if export_decl.is_default_export {
                                            // Default export of a type declaration (interface,
                                            // type alias, etc.) is valid in module augmentations
                                            // — it merges with the existing default export.
                                            // Only plain `export default <expr>` is forbidden.
                                            if let Some(clause_node) =
                                                self.ctx.arena.get(export_decl.export_clause)
                                            {
                                                !matches!(
                                                    clause_node.kind,
                                                    syntax_kind_ext::FUNCTION_DECLARATION
                                                        | syntax_kind_ext::CLASS_DECLARATION
                                                        | syntax_kind_ext::INTERFACE_DECLARATION
                                                        | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                                        | syntax_kind_ext::ENUM_DECLARATION
                                                        | syntax_kind_ext::MODULE_DECLARATION
                                                )
                                            } else {
                                                true
                                            }
                                        } else if export_decl.module_specifier.is_some() {
                                            // Re-exports are not permitted in augmentations
                                            true
                                        } else if export_decl.export_clause.is_none() {
                                            true
                                        } else if let Some(clause_node) =
                                            self.ctx.arena.get(export_decl.export_clause)
                                        {
                                            !matches!(
                                                clause_node.kind,
                                                syntax_kind_ext::FUNCTION_DECLARATION
                                                    | syntax_kind_ext::CLASS_DECLARATION
                                                    | syntax_kind_ext::INTERFACE_DECLARATION
                                                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                                    | syntax_kind_ext::ENUM_DECLARATION
                                                    | syntax_kind_ext::MODULE_DECLARATION
                                                    | syntax_kind_ext::VARIABLE_STATEMENT
                                            )
                                        } else {
                                            true
                                        }
                                    }
                                    None => true,
                                }
                            } else {
                                false
                            };
                            if is_forbidden_export {
                                self.ctx.error(
                                                stmt_node.pos,
                                                stmt_node.end - stmt_node.pos,
                                                diagnostic_messages::EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS.to_string(),
                                                diagnostic_codes::EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS,
                                            );
                                reported_export = true;
                            }
                        }

                        if kind == syntax_kind_ext::EXPORT_DECLARATION {
                            let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                            else {
                                continue;
                            };
                            if export_decl.is_default_export
                                || export_decl.module_specifier.is_some()
                                || export_decl.export_clause.is_none()
                            {
                                continue;
                            }
                            let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
                            else {
                                continue;
                            };
                            match clause_node.kind {
                                syntax_kind_ext::VARIABLE_STATEMENT => {
                                    if let Some(var_stmt) = self.ctx.arena.get_variable(clause_node)
                                    {
                                        for &decl_list_idx in &var_stmt.declarations.nodes {
                                            let Some(decl_list_node) =
                                                self.ctx.arena.get(decl_list_idx)
                                            else {
                                                continue;
                                            };
                                            if decl_list_node.kind
                                                == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                            {
                                                if let Some(decl_list) =
                                                    self.ctx.arena.get_variable(decl_list_node)
                                                {
                                                    for &decl_idx in &decl_list.declarations.nodes {
                                                        if let Some(decl_node) =
                                                            self.ctx.arena.get(decl_idx)
                                                            && let Some(decl) = self
                                                                .ctx
                                                                .arena
                                                                .get_variable_declaration(decl_node)
                                                            && let Some(name_node) =
                                                                self.ctx.arena.get(decl.name)
                                                            && let Some(ident) = self
                                                                .ctx
                                                                .arena
                                                                .get_identifier(name_node)
                                                            && register_value_name(
                                                                &ident.escaped_text,
                                                                decl.name,
                                                            )
                                                            && let Some(node) =
                                                                self.ctx.arena.get(decl.name)
                                                        {
                                                            self.ctx.error(
                                                                                    node.pos,
                                                                                    node.end - node.pos,
                                                                                    format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                                );
                                                        }
                                                    }
                                                }
                                            } else if let Some(decl) = self
                                                .ctx
                                                .arena
                                                .get_variable_declaration(decl_list_node)
                                                && let Some(name_node) =
                                                    self.ctx.arena.get(decl.name)
                                                && let Some(ident) =
                                                    self.ctx.arena.get_identifier(name_node)
                                                && register_value_name(
                                                    &ident.escaped_text,
                                                    decl.name,
                                                )
                                                && let Some(node) = self.ctx.arena.get(decl.name)
                                            {
                                                self.ctx.error(
                                                                        node.pos,
                                                                        node.end - node.pos,
                                                                        format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                    );
                                            }
                                        }
                                    }
                                }
                                syntax_kind_ext::FUNCTION_DECLARATION => {
                                    if let Some(func) = self.ctx.arena.get_function(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(func.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, func.name)
                                        && let Some(node) = self.ctx.arena.get(func.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                                syntax_kind_ext::CLASS_DECLARATION => {
                                    if let Some(class) = self.ctx.arena.get_class(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(class.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, class.name)
                                        && let Some(node) = self.ctx.arena.get(class.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                                syntax_kind_ext::ENUM_DECLARATION => {
                                    if let Some(enm) = self.ctx.arena.get_enum(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(enm.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                    {
                                        if let Some(specifier) = module_specifier.as_deref()
                                            && let Some(target_idx) =
                                                self.ctx.resolve_import_target(specifier)
                                            && let Some(target_binder) =
                                                self.ctx.get_binder_for_file(target_idx)
                                        {
                                            let target_arena =
                                                self.ctx.get_arena_for_file(target_idx as u32);
                                            if let Some(source_file) =
                                                target_arena.source_files.first()
                                                && let Some(existing_sym_id) = target_binder
                                                    .resolve_import_if_needed_public(
                                                        &source_file.file_name,
                                                        &ident.escaped_text,
                                                    )
                                                && let Some(symbol) =
                                                    target_binder.get_symbol(existing_sym_id)
                                            {
                                                let allowed = (symbol.flags
                                                    & (symbol_flags::REGULAR_ENUM
                                                        | symbol_flags::CONST_ENUM
                                                        | symbol_flags::MODULE))
                                                    != 0;
                                                if !allowed {
                                                    self.ctx.error(
                                                                        name_node.pos,
                                                                        name_node.end
                                                                            - name_node.pos,
                                                                        diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS.to_string(),
                                                                        diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                                                                    );
                                                }
                                            }
                                        }
                                        if register_value_name(&ident.escaped_text, enm.name)
                                            && let Some(node) = self.ctx.arena.get(enm.name)
                                        {
                                            self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        } else if kind == syntax_kind_ext::VARIABLE_STATEMENT {
                            if let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node)
                                && self
                                    .ctx
                                    .arena
                                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                            {
                                for &decl_list_idx in &var_stmt.declarations.nodes {
                                    let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx)
                                    else {
                                        continue;
                                    };
                                    if decl_list_node.kind
                                        == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                    {
                                        if let Some(decl_list) =
                                            self.ctx.arena.get_variable(decl_list_node)
                                        {
                                            for &decl_idx in &decl_list.declarations.nodes {
                                                if let Some(decl_node) =
                                                    self.ctx.arena.get(decl_idx)
                                                    && let Some(decl) = self
                                                        .ctx
                                                        .arena
                                                        .get_variable_declaration(decl_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(decl.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                    && register_value_name(
                                                        &ident.escaped_text,
                                                        decl.name,
                                                    )
                                                    && let Some(node) =
                                                        self.ctx.arena.get(decl.name)
                                                {
                                                    self.ctx.error(
                                                                            node.pos,
                                                                            node.end - node.pos,
                                                                            format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                            diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                        );
                                                }
                                            }
                                        }
                                    } else if let Some(decl) =
                                        self.ctx.arena.get_variable_declaration(decl_list_node)
                                        && let Some(name_node) = self.ctx.arena.get(decl.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, decl.name)
                                        && let Some(node) = self.ctx.arena.get(decl.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&ident.escaped_text]),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                            }
                        } else if kind == syntax_kind_ext::FUNCTION_DECLARATION {
                            if let Some(func) = self.ctx.arena.get_function(stmt_node)
                                && self
                                    .ctx
                                    .arena
                                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                                && let Some(name_node) = self.ctx.arena.get(func.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, func.name)
                                && let Some(node) = self.ctx.arena.get(func.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    format_message(
                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                        &[&ident.escaped_text],
                                    ),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::CLASS_DECLARATION {
                            if let Some(class) = self.ctx.arena.get_class(stmt_node)
                                && self
                                    .ctx
                                    .arena
                                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                                && let Some(name_node) = self.ctx.arena.get(class.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, class.name)
                                && let Some(node) = self.ctx.arena.get(class.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    format_message(
                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                        &[&ident.escaped_text],
                                    ),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::ENUM_DECLARATION
                            && let Some(enm) = self.ctx.arena.get_enum(stmt_node)
                            && self
                                .ctx
                                .arena
                                .has_modifier(&enm.modifiers, SyntaxKind::ExportKeyword)
                            && let Some(name_node) = self.ctx.arena.get(enm.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                            && register_value_name(&ident.escaped_text, enm.name)
                            && let Some(node) = self.ctx.arena.get(enm.name)
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                format_message(
                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                    &[&ident.escaped_text],
                                ),
                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                            );
                        }
                    }
                }
                self.ctx
                    .module_augmentation_value_decls
                    .insert(module_key, value_decl_map);
            }

            if module.body.is_some() {
                // Check module body (which can be a block or nested module)
                self.check_module_body(module.body);
            }
        }
    }

    // Module resolution helpers (is_declaration_file, is_external_module,
    // module_exists, etc.) are in `declarations_module_helpers.rs`.
    /// Check a module body (block or nested module).
    fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(node)
                && let Some(ref stmts) = block.statements
            {
                let is_ambient = self.is_in_ambient_context(body_idx);
                let mut reported_generic_ambient_statement_error = false;
                for &stmt_idx in &stmts.nodes {
                    if is_ambient {
                        self.check_statement_in_ambient_context(
                            stmt_idx,
                            &mut reported_generic_ambient_statement_error,
                        );
                    }
                    // Also check for nested module declarations in non-ambient context
                    if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                            self.check_module_declaration(stmt_idx);
                        }
                        // Check for export declarations that contain nested modules
                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            && let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                            && let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
                            && clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        {
                            self.check_module_declaration(export_decl.export_clause);
                        }
                    }
                }
            }
        } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested module (for dotted namespace syntax like `namespace A.B { }`)
            self.check_module_declaration(body_idx);
        }
    }

    /// Check TS2433/TS2434: Namespace merging with class/function across files or out of order.
    ///
    /// TS2433: A namespace declaration cannot be in a different file from a class or function
    ///         with which it is merged.
    /// TS2434: A namespace declaration cannot be located prior to a class or function with
    ///         which it is merged.
    ///
    /// This check applies to non-ambient instantiated namespace declarations that have
    /// multiple declarations (merged with a class or function).
    fn check_namespace_merges_with_class_or_function(
        &mut self,
        module_idx: NodeIndex,
        module: &tsz_parser::parser::node::ModuleData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Get the symbol for this module declaration
        let Some(&sym_id) = self.ctx.binder.node_symbols.get(&module_idx.0) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };

        // Only check if the symbol has multiple declarations (merged within the same binder)
        if symbol.declarations.len() <= 1 {
            // For multi-file scenarios: look for a class/function with the same name
            // in other binders. Handles both top-level and nested symbols.
            // In external modules, each file has its own scope — symbols from
            // different files don't merge, so cross-file TS2433 doesn't apply.
            if !self.is_external_module()
                && let Some(all_binders) = &self.ctx.all_binders
            {
                let namespace_name = &symbol.escaped_name;

                // Build enclosing namespace name chain by walking AST parents.
                // For `namespace A { export namespace Point { } }`, when checking Point,
                // this produces ["A"] (outermost first).
                let enclosing_names: Vec<String> = {
                    let mut names = Vec::new();
                    let mut current = module_idx;
                    // Walk up AST parent chain looking for enclosing MODULE_DECLARATION nodes
                    while let Some(ext) = self.ctx.arena.get_extended(current) {
                        current = ext.parent;
                        if current.is_none() {
                            break;
                        }
                        if let Some(node) = self.ctx.arena.get(current)
                            && node.kind == syntax_kind_ext::MODULE_DECLARATION
                            && let Some(mod_data) = self.ctx.arena.get_module(node)
                            && let Some(name_node) = self.ctx.arena.get(mod_data.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            names.push(ident.escaped_text.clone());
                        }
                    }
                    names.reverse(); // outermost first
                    names
                };

                let current_file_idx = self.ctx.current_file_idx;

                // Use global_file_locals_index for O(1) lookup of candidate binders
                // instead of O(N) scan over all binders.
                let lookup_name = if enclosing_names.is_empty() {
                    namespace_name.as_str()
                } else {
                    enclosing_names[0].as_str()
                };

                // Collect candidate (binder_idx, sym_id) pairs from the index
                let candidates: Vec<(usize, tsz_binder::SymbolId)> =
                    if let Some(file_locals_idx) = self.ctx.global_file_locals_index.as_ref() {
                        file_locals_idx
                            .get(lookup_name)
                            .map(|entries| {
                                entries
                                    .iter()
                                    .filter(|&&(idx, _)| idx != current_file_idx)
                                    .map(|&(idx, sym_id)| (idx, sym_id))
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        // Fallback: O(N) scan when index not available
                        all_binders
                            .iter()
                            .enumerate()
                            .filter(|&(idx, _)| idx != current_file_idx)
                            .filter_map(|(idx, binder)| {
                                binder
                                    .file_locals
                                    .get(lookup_name)
                                    .map(|sym_id| (idx, sym_id))
                            })
                            .collect()
                    };

                for (binder_idx, found_sym_id) in candidates {
                    let _ = binder_idx; // used in nested path

                    // For top-level namespaces (no enclosing names), check file_locals
                    if enclosing_names.is_empty() {
                        let other_sym_id = found_sym_id;
                        if other_sym_id != sym_id
                            && let Some(other_symbol) = self.ctx.binder.get_symbol(other_sym_id)
                        {
                            let is_class =
                                (other_symbol.flags & tsz_binder::symbol_flags::CLASS) != 0;
                            let is_function =
                                (other_symbol.flags & tsz_binder::symbol_flags::FUNCTION) != 0;

                            if (is_class || is_function) && !other_symbol.declarations.is_empty() {
                                if let Some(name_node) = self.ctx.arena.get(module.name) {
                                    self.ctx.error(
                                        name_node.pos,
                                        name_node.end - name_node.pos,
                                        diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W.to_string(),
                                        diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W,
                                    );
                                }
                                return;
                            }
                        }
                        continue;
                    }

                    // For nested namespaces, walk down from root through exports.
                    // E.g., for Point inside A, find A in file_locals, then look for
                    // Point in A's exports.
                    let root_sym_id = found_sym_id;

                    // Walk down through intermediate namespace exports
                    let mut current_sym_id = root_sym_id;
                    let mut found = true;
                    for intermediate_name in &enclosing_names[1..] {
                        if let Some(sym) = self.ctx.binder.get_symbol(current_sym_id)
                            && let Some(exports) = &sym.exports
                            && let Some(next_id) = exports.get(intermediate_name.as_str())
                        {
                            current_sym_id = next_id;
                        } else {
                            found = false;
                            break;
                        }
                    }

                    if !found {
                        continue;
                    }

                    // Now look for the target name in the innermost container's exports
                    if let Some(container_sym) = self.ctx.binder.get_symbol(current_sym_id)
                        && let Some(exports) = &container_sym.exports
                        && let Some(target_sym_id) = exports.get(namespace_name.as_str())
                        && target_sym_id != sym_id
                        && let Some(target_sym) = self.ctx.binder.get_symbol(target_sym_id)
                    {
                        let is_class = (target_sym.flags & tsz_binder::symbol_flags::CLASS) != 0;
                        let is_function =
                            (target_sym.flags & tsz_binder::symbol_flags::FUNCTION) != 0;

                        if (is_class || is_function) && !target_sym.declarations.is_empty() {
                            if let Some(name_node) = self.ctx.arena.get(module.name) {
                                self.ctx.error(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W.to_string(),
                                    diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W,
                                );
                            }
                            return;
                        }
                    }
                }
            }
            return;
        }

        // First check if any non-ambient function with a body merges with this namespace.
        // When a function merge exists, the global duplicate-check path in
        // type_checking/global.rs handles TS2434 for the function case, and we
        // should suppress TS2434 for any class that also merges (tsc emits
        // TS2813/TS2814 for the class conflict, not TS2434).
        let has_merged_function = symbol.declarations.iter().any(|&decl_idx| {
            if decl_idx == module_idx {
                return false;
            }
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                return false;
            }
            if self.is_ambient_declaration(decl_idx) {
                return false;
            }
            self.ctx
                .arena
                .get_function(decl_node)
                .is_some_and(|f| f.body.is_some())
        });

        // Check if the merged symbol has CLASS or FUNCTION flags, indicating it merges
        // with a class or function declaration (possibly from another file).
        let has_class_flag = (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0;
        let has_function_flag = (symbol.flags & tsz_binder::symbol_flags::FUNCTION) != 0;

        if !has_class_flag && !has_function_flag {
            return; // No class/function merge, nothing to check
        }

        // Look for same-file class/function declarations among the merged declarations.
        // NOTE: In merged programs, NodeIndex values from different files can collide,
        // so self.ctx.arena.get(decl_idx) may return a wrong node for cross-file decls.
        // We verify the node kind matches CLASS_DECLARATION or FUNCTION_DECLARATION to
        // filter out collisions.
        let mut found_same_file = false;
        for &decl_idx in &symbol.declarations {
            if decl_idx == module_idx {
                continue;
            }
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let is_class = decl_node.kind == syntax_kind_ext::CLASS_DECLARATION;
            let is_function = decl_node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
            if !is_class && !is_function {
                continue;
            }
            let is_ambient = self.is_ambient_declaration(decl_idx);
            if is_ambient {
                // Ambient (declare) class/function: counts as same-file merge
                // (no TS2433) but doesn't trigger TS2434 ordering check.
                found_same_file = true;
                continue;
            }
            if is_function
                && let Some(func) = self.ctx.arena.get_function(decl_node)
                && func.body.is_none()
            {
                continue;
            }

            // Found a same-file non-ambient class or function.
            // Check TS2434: namespace comes before class/function.
            found_same_file = true;

            // Function-order TS2434 is already handled by the global duplicate-check path.
            if is_function {
                continue;
            }
            // Skip class-order TS2434 when the namespace also merges with a function;
            // tsc emits TS2813/TS2814 for the class conflict, not TS2434.
            if is_class && has_merged_function {
                continue;
            }

            let namespace_pos = self.ctx.arena.get(module_idx).map_or(0, |n| n.pos);
            let class_or_func_pos = self.ctx.arena.get(decl_idx).map_or(0, |n| n.pos);

            if namespace_pos < class_or_func_pos
                && let Some(name_node) = self.ctx.arena.get(module.name)
            {
                self.ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC.to_string(),
                    diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                );
            }
            break;
        }

        if !found_same_file {
            // Before emitting TS2433, check if all class/function declarations we CAN
            // resolve in the current arena are ambient (e.g., `declare function $()`).
            // If they are all ambient, the namespace merge is valid and TS2433 should
            // not fire. If none are resolvable (cross-file), we still emit TS2433
            // because the symbol has CLASS/FUNCTION flags from another file.
            let mut found_any_class_or_function_in_arena = false;
            let mut all_ambient_or_bodyless = true;
            for &decl_idx in &symbol.declarations {
                if decl_idx == module_idx {
                    continue;
                }
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let is_class = decl_node.kind == syntax_kind_ext::CLASS_DECLARATION;
                let is_function = decl_node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
                if !is_class && !is_function {
                    continue;
                }
                found_any_class_or_function_in_arena = true;
                if self.is_ambient_declaration(decl_idx) {
                    continue;
                }
                if is_function
                    && let Some(func) = self.ctx.arena.get_function(decl_node)
                    && func.body.is_none()
                {
                    continue;
                }
                // Found a non-ambient class/function in the arena
                all_ambient_or_bodyless = false;
                break;
            }

            // Suppress TS2433 only when we found class/function declarations in the
            // current arena AND they are all ambient/bodyless. If no class/function
            // was found in the arena, the merged symbol's CLASS/FUNCTION flags must
            // come from another file, so TS2433 should fire.
            let suppress = found_any_class_or_function_in_arena && all_ambient_or_bodyless;
            if !suppress && let Some(name_node) = self.ctx.arena.get(module.name) {
                self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W,
                    );
            }
        }
    }

    fn is_namespace_declaration_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        self.ctx.arena.is_namespace_instantiated(namespace_idx)
    }
}
