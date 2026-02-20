//! Module/Namespace Declaration Checking
//!
//! Extracted from `declarations.rs`: module and namespace declaration validation
//! including TS2580, TS2668, TS2669, TS2433, TS2434, TS2435, TS1035, TS1235,
//! TS5061, TS2664, TS2666/TS2667, and namespace merge checks.

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

        if let Some(module) = self.ctx.arena.get_module(node)
            && let Some(name_node) = self.ctx.arena.get(module.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && let Some(module_exports) = self.ctx.binder.module_exports.get(&self.ctx.file_name)
            && module_exports.has(&ident.escaped_text)
        {
            self.ctx.error(
                name_node.pos,
                name_node.end - name_node.pos,
                diagnostic_messages::DUPLICATE_IDENTIFIER.to_string(),
                diagnostic_codes::DUPLICATE_IDENTIFIER,
            );
            return;
        };

        let Some(_node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        let Some(node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        if let Some(module) = self.ctx.arena.get_module(node)
            && let Some(name_node) = self.ctx.arena.get(module.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && self.ctx.binder.file_locals.has(&ident.escaped_text)
        {
            self.ctx.error(
                name_node.pos,
                name_node.end - name_node.pos,
                diagnostic_messages::DUPLICATE_IDENTIFIER.to_string(),
                diagnostic_codes::DUPLICATE_IDENTIFIER,
            );
            return;
        }

        let Some(node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        if let Some(module) = self.ctx.arena.get_module(node) {
            // TS2580: Anonymous module declaration with `module` keyword (not `namespace`)
            // When `module {` is parsed as a module declaration with a missing name,
            // TSC also emits TS2580 because `module` could be a Node.js identifier reference.
            let is_namespace = (node.flags as u32) & node_flags::NAMESPACE != 0;

            if !is_namespace
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text.is_empty()
            {
                // Detailed node types error (TS2591) is preferred in recent TS versions.
                let code =
                    diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2;
                let message = format_message(
                    diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                    &["module"],
                );

                self.ctx.error(node.pos, 6, message, code);
            }

            // TS2668: 'export' modifier cannot be applied to ambient modules
            // This only applies to string-literal-named ambient modules (declare module "foo"),
            // not to namespace-form modules (declare namespace Foo)
            // Check this FIRST before early returns so we can emit multiple errors
            let has_declare = self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16);
            let has_export = self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword as u16);

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
            // external modules or ambient module declarations, and should have `declare`
            let is_global_augmentation = (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
                || self
                    .ctx
                    .arena
                    .get(module.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == "global");
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
                                && self.ctx.has_modifier(
                                    &gp_module.modifiers,
                                    SyntaxKind::DeclareKeyword as u16,
                                )
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
            if !has_declare
                && is_string_named
                && let Some(name_node) = self.ctx.arena.get(module.name)
            {
                self.ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    diagnostic_messages::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES.to_string(),
                    diagnostic_codes::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES,
                );
            }

            // TS2435: Ambient modules cannot be nested in other modules or namespaces
            // Check if this is an ambient external module (declare module "string")
            // inside another namespace/module
            if let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
            {
                // This is an ambient external module with a string name
                // Check if it's nested inside a namespace
                if self.is_inside_namespace(module_idx) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "Ambient modules cannot be nested in other modules or namespaces."
                            .to_string(),
                        diagnostic_codes::AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES,
                    );
                    return; // Don't emit other errors for nested ambient modules
                }
            }

            // TS1235: A namespace declaration is only allowed at the top level of a namespace or module.
            // This applies to non-string-named module/namespace declarations that are inside labeled statements
            // or other non-module constructs.
            if !is_string_named {
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
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
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
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16)
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
            if has_declare && is_string_named && self.is_external_module() {
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
                            break;
                        }
                        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                            continue;
                        };
                        let kind = stmt_node.kind;
                        if !reported_import
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
                        } else if !reported_export {
                            let is_forbidden_export = if kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                                || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                            {
                                true
                            } else if kind == syntax_kind_ext::EXPORT_DECLARATION {
                                match self.ctx.arena.get_export_decl(stmt_node) {
                                    Some(export_decl) => {
                                        if export_decl.is_default_export {
                                            true
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
                                                                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
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
                                                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
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
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
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
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
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
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        } else if kind == syntax_kind_ext::VARIABLE_STATEMENT {
                            if let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node)
                                && self.ctx.has_modifier(
                                    &var_stmt.modifiers,
                                    SyntaxKind::ExportKeyword as u16,
                                )
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
                                                                            diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
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
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                            }
                        } else if kind == syntax_kind_ext::FUNCTION_DECLARATION {
                            if let Some(func) = self.ctx.arena.get_function(stmt_node)
                                && self
                                    .ctx
                                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword as u16)
                                && let Some(name_node) = self.ctx.arena.get(func.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, func.name)
                                && let Some(node) = self.ctx.arena.get(func.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                        .to_string(),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::CLASS_DECLARATION {
                            if let Some(class) = self.ctx.arena.get_class(stmt_node)
                                && self.ctx.has_modifier(
                                    &class.modifiers,
                                    SyntaxKind::ExportKeyword as u16,
                                )
                                && let Some(name_node) = self.ctx.arena.get(class.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, class.name)
                                && let Some(node) = self.ctx.arena.get(class.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                        .to_string(),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::ENUM_DECLARATION
                            && let Some(enm) = self.ctx.arena.get_enum(stmt_node)
                            && self
                                .ctx
                                .has_modifier(&enm.modifiers, SyntaxKind::ExportKeyword as u16)
                            && let Some(name_node) = self.ctx.arena.get(enm.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                            && register_value_name(&ident.escaped_text, enm.name)
                            && let Some(node) = self.ctx.arena.get(enm.name)
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                    .to_string(),
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
                for &stmt_idx in &stmts.nodes {
                    if is_ambient {
                        self.check_statement_in_ambient_context(stmt_idx);
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

        // Only check if the symbol has multiple declarations (merged)
        if symbol.declarations.len() <= 1 {
            return;
        }

        // Look for a non-ambient class or function declaration among the merged declarations
        for &decl_idx in &symbol.declarations {
            if decl_idx == module_idx {
                continue; // Skip the current namespace declaration
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let is_class = decl_node.kind == syntax_kind_ext::CLASS_DECLARATION;
            let is_function = decl_node.kind == syntax_kind_ext::FUNCTION_DECLARATION;

            if !is_class && !is_function {
                continue;
            }

            // Check if the declaration is ambient: `declare class`, or inside
            // an ambient context (e.g. `declare module 'M' { class C {} }`)
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }

            // For functions, they must have a body to be considered a value declaration
            if is_function
                && let Some(func) = self.ctx.arena.get_function(decl_node)
                && func.body.is_none()
            {
                continue; // Function overload signature, not an implementation
            }

            // Found a non-ambient class or function declaration
            // Now check if they're in different files (TS2433) or namespace is prior (TS2434)

            // Get the source file of the current namespace declaration
            let current_file = self.get_source_file_of_node(module_idx);
            let other_file = self.get_source_file_of_node(decl_idx);

            if current_file != other_file {
                // TS2433: Different files
                if let Some(name_node) = self.ctx.arena.get(module.name) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W,
                    );
                }
            } else {
                // TS2434: Namespace comes before class/function in the same file
                // Compare positions - only emit error if namespace is before class/function
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
            }

            // Only report error once (for the first matching class/function)
            break;
        }
    }

    /// Check if a namespace declaration is instantiated (contains runtime code).
    /// Uninstantiated namespaces only contain interfaces, type aliases, etc.
    fn is_namespace_declaration_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(namespace_node) = self.ctx.arena.get(namespace_idx) else {
            return false;
        };
        if namespace_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.ctx.arena.get_module(namespace_node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.is_namespace_declaration_instantiated(body_idx);
        }
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }
        let Some(module_block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };
        for &statement_idx in &statements.nodes {
            let Some(statement_node) = self.ctx.arena.get(statement_idx) else {
                continue;
            };
            // Check the effective kind - for EXPORT_DECLARATION, look at the inner declaration
            let effective_kind = if statement_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_data) = self.ctx.arena.get_export_decl(statement_node) {
                    self.ctx
                        .arena
                        .get(export_data.export_clause)
                        .map_or(statement_node.kind, |inner| inner.kind)
                } else {
                    statement_node.kind
                }
            } else {
                statement_node.kind
            };
            match effective_kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    let ns_idx = if statement_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                        self.ctx
                            .arena
                            .get_export_decl(statement_node)
                            .map_or(statement_idx, |d| d.export_clause)
                    } else {
                        statement_idx
                    };
                    if self.is_namespace_declaration_instantiated(ns_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Get the source file path of a node's declaration.
    /// Returns the file name if we can determine it, or empty string if unknown.
    fn get_source_file_of_node(&self, node_idx: NodeIndex) -> String {
        // Walk up to find the source file
        let mut current = node_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::SOURCE_FILE
            {
                // Found the source file - return the file name from context
                return self.ctx.file_name.clone();
            }
            current = parent;
        }
        // Fallback to current file name
        self.ctx.file_name.clone()
    }
}
