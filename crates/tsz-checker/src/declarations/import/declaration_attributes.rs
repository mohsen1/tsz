//! Import attribute and deferred-import validation helpers.
//!
//! Contains checks for:
//! - Deferred imports (`import defer * as ns from "..."`)
//! - Import attributes / assertions (`with { ... }` / `assert { ... }`)
//! - JSON ESM import attribute requirements
//! - Type-only resolution-mode attribute grammar

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// TS18058/TS18059: Check that deferred imports only use namespace binding.
    /// Deferred imports (`import defer ...`) must use `* as ns` form.
    /// Default imports and named imports are not allowed.
    pub(crate) fn check_deferred_import_restrictions(&mut self, import_clause_idx: NodeIndex) {
        let Some(clause_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        if !clause.is_deferred {
            return;
        }

        // The import clause node starts at the `defer` keyword position.
        // Use that as the error location (matching TSC behavior).
        let defer_pos = clause_node.pos;
        let defer_len = 5u32; // length of "defer"

        // TS18058: Default imports are not allowed in a deferred import.
        if clause.name.is_some() {
            self.error_at_position(
                defer_pos,
                defer_len,
                diagnostic_messages::DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT,
                diagnostic_codes::DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT,
            );
        }

        // TS18059: Named imports are not allowed in a deferred import.
        if let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
            && bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
        {
            self.error_at_position(
                defer_pos,
                defer_len,
                diagnostic_messages::NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT,
                diagnostic_codes::NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT,
            );
        }
    }

    /// TS2880: Check that `assert` keyword is not used (deprecated in favor of `with`).
    pub(crate) fn check_import_attributes_deprecated_assert(&mut self, attributes_idx: NodeIndex) {
        if attributes_idx.is_none() {
            return;
        }

        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };

        let Some(attrs_data) = self.ctx.arena.get_import_attributes_data(attr_node) else {
            return;
        };

        // token stores the SyntaxKind of the keyword used (AssertKeyword vs WithKeyword)
        // Route through the capability boundary to check ignore_deprecations.
        if attrs_data.token == SyntaxKind::AssertKeyword as u16
            && self
                .ctx
                .capabilities
                .check_import_assert_deprecated()
                .is_some()
        {
            // Error spans the `assert` keyword (6 characters), positioned at the node start
            self.error_at_position(
                attr_node.pos,
                6, // length of "assert"
                diagnostic_messages::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
                diagnostic_codes::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
            );
        }
    }

    /// TS2823: Check that import attributes are only used with supported module options.
    ///
    /// Routes through the environment capability boundary (`check_feature_gate`)
    /// to determine whether a diagnostic should be emitted.
    pub(crate) fn check_import_attributes_module_option(
        &mut self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) {
        if attributes_idx.is_none() {
            return;
        }

        use crate::query_boundaries::capabilities::FeatureGate;
        if self
            .ctx
            .capabilities
            .check_feature_gate(FeatureGate::ImportAttributes)
            .is_some()
            && !self.resolution_mode_override_is_effective(attributes_idx, declaration_is_type_only)
            && let Some(attr_node) = self.ctx.arena.get(attributes_idx)
        {
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                diagnostic_codes::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
            );
        }
    }

    pub(crate) fn check_import_attributes_commonjs_or_type_only(
        &mut self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) {
        if attributes_idx.is_none() {
            return;
        }

        if self.resolution_mode_override_is_effective(attributes_idx, declaration_is_type_only) {
            return;
        }

        use crate::query_boundaries::capabilities::FeatureGate;
        if self
            .ctx
            .capabilities
            .check_feature_gate(FeatureGate::ImportAttributes)
            .is_some()
        {
            return;
        }

        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };

        if declaration_is_type_only {
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS,
                diagnostic_codes::IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS,
            );
            return;
        }

        if self.import_declaration_emits_commonjs() {
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE,
                diagnostic_codes::IMPORT_ATTRIBUTES_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE,
            );
        }
    }

    pub(crate) fn import_declaration_emits_commonjs(&self) -> bool {
        use tsz_common::common::ModuleKind;

        match self.ctx.compiler_options.module {
            ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext => {
                let current_file = self.ctx.file_name.as_str();
                if current_file.ends_with(".cts") || current_file.ends_with(".cjs") {
                    return true;
                }
                if current_file.ends_with(".mts") || current_file.ends_with(".mjs") {
                    return false;
                }
                self.ctx.file_is_esm.is_some_and(|is_esm| !is_esm)
            }
            ModuleKind::CommonJS => true,
            _ => false,
        }
    }

    pub(crate) fn current_file_uses_esm_import_syntax(&self) -> bool {
        match self.ctx.compiler_options.module {
            tsz_common::common::ModuleKind::Node16
            | tsz_common::common::ModuleKind::Node18
            | tsz_common::common::ModuleKind::Node20
            | tsz_common::common::ModuleKind::NodeNext => {
                let current_file = self.ctx.file_name.as_str();
                if current_file.ends_with(".cts") || current_file.ends_with(".cjs") {
                    return false;
                }
                if current_file.ends_with(".mts") || current_file.ends_with(".mjs") {
                    return true;
                }
                self.ctx.file_is_esm.unwrap_or(false)
            }
            module => module.is_es_module(),
        }
    }

    pub(crate) const fn module_kind_display_name(&self) -> &'static str {
        match self.ctx.compiler_options.module {
            tsz_common::common::ModuleKind::Node16 => "Node16",
            tsz_common::common::ModuleKind::Node18 => "Node18",
            tsz_common::common::ModuleKind::Node20 => "Node20",
            tsz_common::common::ModuleKind::NodeNext => "NodeNext",
            tsz_common::common::ModuleKind::ESNext => "ESNext",
            tsz_common::common::ModuleKind::Preserve => "Preserve",
            tsz_common::common::ModuleKind::CommonJS => "CommonJS",
            tsz_common::common::ModuleKind::AMD => "AMD",
            tsz_common::common::ModuleKind::UMD => "UMD",
            tsz_common::common::ModuleKind::System => "System",
            tsz_common::common::ModuleKind::ES2015 => "ES2015",
            tsz_common::common::ModuleKind::ES2020 => "ES2020",
            tsz_common::common::ModuleKind::ES2022 => "ES2022",
            tsz_common::common::ModuleKind::None => "None",
        }
    }

    pub(crate) fn import_has_type_json_attribute(&self, attributes_idx: NodeIndex) -> bool {
        if attributes_idx.is_none() {
            return false;
        }
        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return false;
        };
        let Some(attrs_data) = self.ctx.arena.get_import_attributes_data(attr_node) else {
            return false;
        };

        attrs_data.elements.nodes.iter().any(|&elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            let Some(attr_data) = self.ctx.arena.get_import_attribute_data(elem_node) else {
                return false;
            };
            let name_is_type = self
                .ctx
                .arena
                .get_literal_text(attr_data.name)
                .map(|name| name.trim_matches('"').trim_matches('\'') == "type")
                .or_else(|| {
                    self.ctx
                        .arena
                        .get(attr_data.name)
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str() == "type")
                })
                .unwrap_or(false);
            let value_is_json = self
                .ctx
                .arena
                .get_literal_text(attr_data.value)
                .is_some_and(|value| value.trim_matches('"').trim_matches('\'') == "json");
            name_is_type && value_is_json
        })
    }

    pub(crate) fn import_attributes_enable_json_module(&self, attributes_idx: NodeIndex) -> bool {
        self.import_has_type_json_attribute(attributes_idx)
            && matches!(
                self.ctx.compiler_options.module,
                tsz_common::common::ModuleKind::Node18
                    | tsz_common::common::ModuleKind::Node20
                    | tsz_common::common::ModuleKind::NodeNext
            )
            && self.current_file_uses_esm_import_syntax()
    }

    pub(crate) fn maybe_emit_json_esm_import_attribute_required(
        &mut self,
        import: &tsz_parser::parser::node::ImportDeclData,
        target_idx: usize,
        spec_start: u32,
        spec_length: u32,
        is_type_only_import: bool,
    ) {
        if is_type_only_import
            || !matches!(
                self.ctx.compiler_options.module,
                tsz_common::common::ModuleKind::Node18
                    | tsz_common::common::ModuleKind::Node20
                    | tsz_common::common::ModuleKind::NodeNext
            )
            || !self.current_file_uses_esm_import_syntax()
            || self.import_has_type_json_attribute(import.attributes)
        {
            return;
        }

        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let file_name = source_file.file_name.as_str();
        if !file_name.ends_with(".json") && !file_name.ends_with(".d.json.ts") {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };
        // Emit TS1543 for default imports (`import x from "./f.json"`) and namespace imports
        // (`import * as x from "./f.json"`). Named imports are handled separately by TS1544
        // in import_members.rs, and side-effect imports have no import clause.
        let has_default_binding = clause.name.is_some();
        let has_namespace_binding = self
            .ctx
            .arena
            .get(clause.named_bindings)
            .is_some_and(|bindings_node| bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT);
        if !has_default_binding && !has_namespace_binding {
            return;
        }

        let module_kind = self.module_kind_display_name();
        let message = crate::diagnostics::format_message(
            diagnostic_messages::IMPORTING_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_REQUIRES_A_TYPE_JSON_IMPORT_ATTR,
            &[module_kind],
        );
        self.error_at_position(
            spec_start,
            spec_length,
            &message,
            diagnostic_codes::IMPORTING_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_REQUIRES_A_TYPE_JSON_IMPORT_ATTR,
        );
    }

    /// TS2322: Check that import attribute values are assignable to the global `ImportAttributes`
    /// interface.
    ///
    /// For `import ... with { type: "json" }`, builds an object type from the attribute
    /// entries and checks it against the global `ImportAttributes` interface. If the user
    /// has augmented `ImportAttributes` (e.g., `interface ImportAttributes { type: "json" }`),
    /// mismatched values will produce TS2322.
    pub(crate) fn check_import_attributes_assignability(&mut self, attributes_idx: NodeIndex) {
        use tsz_solver::TypeId;

        if attributes_idx.is_none() {
            return;
        }

        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };

        let Some(attrs_data) = self.ctx.arena.get_import_attributes_data(attr_node) else {
            return;
        };

        let elements: Vec<NodeIndex> = attrs_data.elements.nodes.clone();

        if elements.is_empty() {
            return;
        }

        // Resolve the global ImportAttributes interface type (including user augmentations).
        let Some(import_attributes_type) = self.resolve_lib_type_by_name("ImportAttributes") else {
            return;
        };

        // Build an object type from the import attribute entries
        let mut properties = Vec::new();
        for &elem_idx in &elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind != syntax_kind_ext::IMPORT_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_import_attribute_data(elem_node) else {
                continue;
            };

            // Get the attribute name (identifier or string literal)
            let name = if let Some(name_node) = self.ctx.arena.get(attr_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    Some(ident.escaped_text.clone())
                } else {
                    self.ctx
                        .arena
                        .get_literal(name_node)
                        .map(|lit| lit.text.clone())
                }
            } else {
                None
            };

            let Some(name) = name else {
                continue;
            };

            // TS2858: import attribute values must be string literal expressions.
            // For TS2322 display parity, keep top-level literal primitives (e.g. `0`)
            // but widen nested object-literal members (e.g. `{ a: 0 }` -> `{ a: number }`).
            let value_type = if let Some(val_node) = self.ctx.arena.get(attr_data.value) {
                if val_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
                    if let Some(lit) = self.ctx.arena.get_literal(val_node) {
                        self.ctx.types.factory().literal_string(&lit.text)
                    } else {
                        self.get_type_of_node(attr_data.value)
                    }
                } else {
                    self.error_at_position(
                        val_node.pos,
                        val_node.end.saturating_sub(val_node.pos),
                        crate::diagnostics::diagnostic_messages::IMPORT_ATTRIBUTE_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS,
                        crate::diagnostics::diagnostic_codes::IMPORT_ATTRIBUTE_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS,
                    );
                    if val_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        let object_type = self.get_type_of_node(attr_data.value);
                        let widened_object = crate::query_boundaries::common::widen_type(
                            self.ctx.types,
                            object_type,
                        );
                        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            widened_object,
                        ) {
                            self.ctx
                                .types
                                .store_display_properties(widened_object, shape.properties.clone());
                        }
                        widened_object
                    } else if let Some(literal_type) =
                        self.literal_type_from_initializer(attr_data.value)
                    {
                        literal_type
                    } else {
                        self.get_type_of_node(attr_data.value)
                    }
                }
            } else {
                self.get_type_of_node(attr_data.value)
            };

            let name_atom = self.ctx.types.intern_string(&name);
            properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
        }

        if properties.is_empty() {
            return;
        }

        let source_type = self.ctx.types.factory().object(properties);

        // Don't check if source or target are any/error
        if source_type == TypeId::ANY
            || source_type == TypeId::ERROR
            || import_attributes_type == TypeId::ANY
            || import_attributes_type == TypeId::ERROR
        {
            return;
        }
        let related = self
            .assign_relation_outcome(source_type, import_attributes_type)
            .related;
        if !related {
            use crate::diagnostics::format_message;
            let source_str = self.format_type(source_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, "ImportAttributes"],
            );
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    /// TS1453/TS1455/TS1456/TS1463/TS1464: validate type-only `resolution-mode`
    /// import attributes before the regular module-option and assignability checks.
    ///
    /// This mirrors tsc's `getResolutionModeOverride(..., grammarErrorOnNode)` path:
    /// whole-declaration type-only imports get extra grammar validation for the
    /// `resolution-mode` attribute shape, key, and literal value.
    pub(crate) fn check_type_only_resolution_mode_attribute_grammar(
        &mut self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) {
        if attributes_idx.is_none() || !declaration_is_type_only {
            return;
        }

        // In tsc, the resolution-mode attribute check only applies when using
        // Node16/NodeNext module resolution. For other module modes (es2015,
        // esnext, bundler, etc.), type-only imports can have any attributes
        // without triggering TS1463/TS1453.
        if !self.ctx.compiler_options.module.is_node_module() {
            return;
        }

        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs_data) = self.ctx.arena.get_import_attributes_data(attr_node) else {
            return;
        };

        let uses_with_keyword = attrs_data.token == SyntaxKind::WithKeyword as u16;
        let (invalid_key_message, invalid_key_code, invalid_shape_message, invalid_shape_code) =
            if uses_with_keyword {
                (
                    diagnostic_messages::RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ATTRIBUTES,
                    diagnostic_codes::RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ATTRIBUTES,
                    diagnostic_messages::TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                    diagnostic_codes::TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                )
            } else {
                (
                    diagnostic_messages::RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ASSERTIONS,
                    diagnostic_codes::RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ASSERTIONS,
                    diagnostic_messages::TYPE_IMPORT_ASSERTIONS_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                    diagnostic_codes::TYPE_IMPORT_ASSERTIONS_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                )
            };

        if attrs_data.elements.nodes.len() != 1 {
            self.error_at_node(attributes_idx, invalid_shape_message, invalid_shape_code);
            return;
        }

        let elem_idx = attrs_data.elements.nodes[0];
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };
        let Some(attr_data) = self.ctx.arena.get_import_attribute_data(elem_node) else {
            return;
        };

        let name = if let Some(name_node) = self.ctx.arena.get(attr_data.name) {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                Some(ident.escaped_text.as_str())
            } else {
                self.ctx
                    .arena
                    .get_literal_text(attr_data.name)
                    .map(|lit| lit.trim_matches('"').trim_matches('\''))
            }
        } else {
            None
        };

        if name != Some("resolution-mode") {
            let is_json_type_attribute = name == Some("type")
                && self
                    .ctx
                    .arena
                    .get_literal_text(attr_data.value)
                    .is_some_and(|value| value.trim_matches('"').trim_matches('\'') == "json");
            if is_json_type_attribute {
                return;
            }
            self.error_at_node(attr_data.name, invalid_key_message, invalid_key_code);
            return;
        }

        let Some(value_text) = self.ctx.arena.get_literal_text(attr_data.value) else {
            return;
        };
        let value_text = value_text.trim_matches('"').trim_matches('\'');
        if value_text != "import" && value_text != "require" {
            self.error_at_node(
                attr_data.value,
                diagnostic_messages::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT,
                diagnostic_codes::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT,
            );
        }
    }
}
