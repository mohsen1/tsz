//! Import declaration validation (`import { X } from "y"`), re-export chain
//! cycle detection, and import resolution helpers.
//!
//! Import-equals validation (`import X = require("y")` / `import X = Namespace`)
//! lives in the sibling `equals` module.

use crate::context::is_declaration_file_name;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Returns `true` if the module specifier looks like it should be rewritten
/// by `rewriteRelativeImportExtensions`.
///
/// Mirrors tsc's `shouldRewriteModuleSpecifier`: the specifier must be a
/// relative path with a TypeScript file extension (.ts/.tsx/.mts/.cts) that
/// is NOT a declaration file (.d.ts/.d.mts/.d.cts).
pub(crate) fn should_rewrite_module_specifier(specifier: &str) -> bool {
    (specifier.starts_with("./") || specifier.starts_with("../"))
        && ts_extension_suffix(specifier).is_some()
}

/// Returns the TypeScript extension suffix (e.g. `".ts"`, `".tsx"`) if the module path
/// ends with a TS-specific extension that requires `allowImportingTsExtensions`.
/// Returns `None` for `.d.ts`/`.d.mts`/`.d.cts` (handled separately by TS2846) and
/// non-TS extensions.
pub(crate) fn ts_extension_suffix(module_name: &str) -> Option<&'static str> {
    // .d.ts/.d.mts/.d.cts are declaration files — handled by TS2846, not TS5097
    if module_name.ends_with(".d.ts")
        || module_name.ends_with(".d.mts")
        || module_name.ends_with(".d.cts")
    {
        return None;
    }
    if module_name.ends_with(".ts") {
        Some(".ts")
    } else if module_name.ends_with(".tsx") {
        Some(".tsx")
    } else if module_name.ends_with(".mts") {
        Some(".mts")
    } else if module_name.ends_with(".cts") {
        Some(".cts")
    } else {
        None
    }
}

/// Check if a module specifier refers to a Node.js built-in module.
/// Handles both bare names ("fs") and the `node:` prefix ("node:fs").
pub(crate) fn is_node_builtin_module(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    matches!(
        bare,
        "assert"
            | "assert/strict"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "dns/promises"
            | "domain"
            | "events"
            | "fs"
            | "fs/promises"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "inspector/promises"
            | "module"
            | "net"
            | "os"
            | "path"
            | "path/posix"
            | "path/win32"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "readline/promises"
            | "repl"
            | "stream"
            | "stream/consumers"
            | "stream/promises"
            | "stream/web"
            | "string_decoder"
            | "sys"
            | "timers"
            | "timers/promises"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "util/types"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

fn imported_types_package_target(module_name: &str) -> Option<String> {
    let package = module_name.strip_prefix("@types/")?;
    if package.is_empty() {
        return None;
    }
    if let Some((scope, name)) = package.split_once("__") {
        if !scope.is_empty() && !name.is_empty() {
            return Some(format!("@{scope}/{name}"));
        }
    }
    Some(package.to_string())
}

impl<'a> CheckerState<'a> {
    fn source_file_has_syntactic_module_indicator(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
        }

        false
    }

    fn source_file_has_top_level_global_augmentation(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if (stmt.flags as u32) & node_flags::GLOBAL_AUGMENTATION == 0 {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::GlobalKeyword as u16
                || arena
                    .get_identifier(name_node)
                    .is_some_and(|ident| ident.escaped_text == "global")
            {
                return true;
            }
        }

        false
    }

    /// Check if a source file contains a module augmentation (not global augmentation).
    /// A module augmentation is a `declare module "X" { ... }` statement that extends
    /// an existing module's type definitions. Files with only module augmentations
    /// (and no regular exports) should not trigger TS2307 because they serve a valid
    /// purpose in the type system.
    fn source_file_has_module_augmentation(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            // Must NOT be a global augmentation
            if (stmt.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0 {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            // Module augmentation has a string literal name (not "global" keyword)
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }

        false
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    fn maybe_emit_imported_global_augmentation_errors(&mut self, target_idx: usize) {
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return;
        };
        if self.source_file_has_syntactic_module_indicator(arena, source_file) {
            return;
        }
        // Collect positions first to avoid borrowing arena and self simultaneously
        let mut error_positions: Vec<(u32, u32)> = Vec::new();
        let file_name = source_file.file_name.clone();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if (stmt.flags as u32) & node_flags::GLOBAL_AUGMENTATION == 0 {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            let is_global = name_node.kind == SyntaxKind::GlobalKeyword as u16
                || arena
                    .get_identifier(name_node)
                    .is_some_and(|ident| ident.escaped_text == "global");
            if !is_global {
                continue;
            }

            error_positions.push((name_node.pos, name_node.end.saturating_sub(name_node.pos)));
        }

        for (start, length) in error_positions {
            self.error_at_position_in_file(
                file_name.clone(),
                start,
                length,
                diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
                diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
            );
        }
    }

    /// TS1214: Check import binding names for strict-mode reserved words.
    /// Import declarations make the file a module (always strict mode), so TS1214 applies.
    /// Matches tsc's binder: `checkContextualIdentifier` is guarded by
    /// `!file.parseDiagnostics.length`, so strict-mode checks are skipped
    /// entirely when the file has any parser errors.
    fn check_import_binding_reserved_words(&mut self, import_clause_idx: NodeIndex) {
        // Skip when there are parser errors (matches tsc binder behavior)
        if self.ctx.has_parse_errors {
            return;
        }

        use crate::state_checking::is_strict_mode_reserved_name;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        // Check default import name: `import package from "./mod"`
        if clause.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(clause.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && is_strict_mode_reserved_name(&ident.escaped_text)
        {
            self.emit_module_strict_mode_reserved_word_error(clause.name, &ident.escaped_text);
        }

        // Check named bindings (namespace import or named imports)
        if clause.named_bindings.is_none() {
            return;
        }
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };

        if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
            // `import * as package from "./mod"` — check the alias name
            if let Some(ns_data) = self.ctx.arena.get_named_imports(bindings_node)
                && ns_data.name.is_some()
                && let Some(name_node) = self.ctx.arena.get(ns_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && is_strict_mode_reserved_name(&ident.escaped_text)
            {
                self.emit_module_strict_mode_reserved_word_error(ns_data.name, &ident.escaped_text);
            }
        } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS {
            // `import { foo as package } from "./mod"` — check each specifier's local name
            if let Some(named_data) = self.ctx.arena.get_named_imports(bindings_node) {
                let elements: Vec<_> = named_data.elements.nodes.to_vec();
                for elem_idx in elements {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(spec) = self.ctx.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    // The local binding name is `spec.name`
                    let name_to_check = spec.name;
                    if let Some(name_node) = self.ctx.arena.get(name_to_check)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && is_strict_mode_reserved_name(&ident.escaped_text)
                    {
                        self.emit_module_strict_mode_reserved_word_error(
                            name_to_check,
                            &ident.escaped_text,
                        );
                    }
                }
            }
        }
    }

    /// TS18058/TS18059: Check that deferred imports only use namespace binding.
    /// Deferred imports (`import defer ...`) must use `* as ns` form.
    /// Default imports and named imports are not allowed.
    pub(crate) fn check_deferred_import_restrictions(&mut self, import_clause_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

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
        if attrs_data.token == tsz_scanner::SyntaxKind::AssertKeyword as u16
            && self
                .ctx
                .capabilities
                .check_import_assert_deprecated()
                .is_some()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
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
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                diagnostic_codes::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
            );
        }
    }

    /// TS2322: Check that import attribute values are assignable to the global `ImportAttributes`
    /// interface.
    ///
    /// For `import ... with { type: "json" }`, builds an object type from the attribute
    /// entries and checks it against the global `ImportAttributes` interface. If the user
    /// has augmented `ImportAttributes` (e.g., `interface ImportAttributes { type: "json" }`),
    /// mismatched values will produce TS2322.
    pub(crate) fn check_import_attributes_assignability(&mut self, attributes_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;
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

            // Get the value type — import attribute values are always string literals.
            // Use string literal types directly (not widened) to match TSC behavior.
            let value_type = if let Some(val_node) = self.ctx.arena.get(attr_data.value)
                && let Some(lit) = self.ctx.arena.get_literal(val_node)
            {
                self.ctx.types.factory().literal_string(&lit.text)
            } else {
                // Fallback for non-literal values (should not happen for valid attributes)
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

        // Check assignability — emit TS2322 at the attributes node if not assignable
        self.check_assignable_or_report_at(
            source_type,
            import_attributes_type,
            attributes_idx,
            attributes_idx,
        );
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

    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        let is_type_only_import = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|clause_node| self.ctx.arena.get_import_clause(clause_node))
            .is_some_and(|clause| clause.is_type_only);

        // Suppress semantic diagnostics (TS2307, TS2823, TS2322) when the import
        // statement has parse errors. Matches TSC: syntax errors take priority.
        use tsz_parser::parser::node_flags;
        let has_parse_errors =
            (node.flags as u32 & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                || self.ctx.has_real_syntax_errors;

        // TS18058/TS18059: Validate deferred import binding restrictions.
        // Deferred imports only allow namespace imports: `import defer * as ns from "..."`
        self.check_deferred_import_restrictions(import.import_clause);

        // TS1363: A type-only import can specify a default import or named bindings, but not both.
        // e.g., `import type A, { B } from '...'` is invalid.
        if let Some(clause_node) = self.ctx.arena.get(import.import_clause)
            && let Some(clause) = self.ctx.arena.get_import_clause(clause_node)
            && clause.is_type_only
            && clause.name.is_some()
            && clause.named_bindings.is_some()
        {
            self.error_at_node(
                        import.import_clause,
                        "A type-only import can specify a default import or named bindings, but not both.",
                        diagnostic_codes::A_TYPE_ONLY_IMPORT_CAN_SPECIFY_A_DEFAULT_IMPORT_OR_NAMED_BINDINGS_BUT_NOT_BOTH,
                    );
        }

        // TS2880: Warn about deprecated `assert` keyword
        self.check_import_attributes_deprecated_assert(import.attributes);

        if !has_parse_errors {
            self.check_type_only_resolution_mode_attribute_grammar(
                import.attributes,
                is_type_only_import,
            );

            // TS2823: Import attributes require specific module options
            self.check_import_attributes_module_option(import.attributes, is_type_only_import);

            // TS2322: Check import attribute values against global ImportAttributes interface
            self.check_import_attributes_assignability(import.attributes);
        }

        // TS1214/TS1212: Check import binding names for strict mode reserved words.
        // Import declarations make the file a module, so it's always strict mode → TS1214.
        self.check_import_binding_reserved_words(import.import_clause);

        if import.import_clause.is_some() {
            self.check_import_declaration_conflicts(stmt_idx, import.import_clause);
        }

        // Skip semantic import diagnostics when the import has parse errors.
        if has_parse_errors {
            return;
        }

        // Extract module specifier data eagerly so direct import diagnostics like
        // TS6137 can run even when unresolved-import reporting is disabled.
        let module_specifier_idx = import.module_specifier;
        let import_clause_idx = import.import_clause;

        let Some(spec_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = spec_node.pos;
        let spec_length = spec_node.end.saturating_sub(spec_node.pos);

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;
        // tsc emits TS2307 independently per import declaration, even when multiple
        // imports reference the same module.  Clear the per-module dedup entry so
        // this declaration gets its own chance to report a module-not-found error.
        // The within-declaration dedup (resolution-error path vs fallback path)
        // is preserved because both paths insert the key before returning.
        self.ctx
            .modules_with_ts2307_emitted
            .remove(module_name.as_str());
        let has_import_clause = self.ctx.arena.get(import_clause_idx).is_some();
        let is_side_effect_import = !has_import_clause;
        // Note: side-effect imports may return early in the resolution error check below
        // when no_unchecked_side_effect_imports=false (silently ignoring unresolved modules).
        if !is_type_only_import && let Some(suggested) = imported_types_package_target(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF,
                &[&suggested, module_name],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF,
            );
            return;
        }

        // Skip module resolution checks when unresolved-import reporting is disabled.
        if !self.ctx.report_unresolved_imports {
            return;
        }
        // Track whether TS2846/TS5097 extension diagnostics were emitted.
        // When these fire, TS2307 from module resolution should be suppressed
        // (tsc prioritizes extension-specific diagnostics over "cannot find module").
        let mut emitted_extension_diagnostic = false;

        let dts_ext = if module_name.ends_with(".d.ts") {
            Some((".d.ts", ".ts", ".js"))
        } else if module_name.ends_with(".d.mts") {
            Some((".d.mts", ".mts", ".mjs"))
        } else if module_name.ends_with(".d.cts") {
            Some((".d.cts", ".cts", ".cjs"))
        } else {
            None
        };
        if let Some((dts_suffix, ts_ext, js_ext)) = dts_ext
            && !is_type_only_import
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let base = module_name.trim_end_matches(dts_suffix);
            let suggested = if self.ctx.compiler_options.allow_importing_ts_extensions {
                format!("{base}{ts_ext}")
            } else {
                // For CommonJS-like module kinds, extensionless imports are valid.
                // For ESM-like module kinds, append .js/.mjs/.cjs extension.
                use tsz_common::common::ModuleKind;
                match self.ctx.compiler_options.module {
                    ModuleKind::CommonJS
                    | ModuleKind::AMD
                    | ModuleKind::UMD
                    | ModuleKind::System
                    | ModuleKind::None => base.to_string(),
                    _ => format!("{base}{js_ext}"),
                }
            };
            let message = format_message(
                diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                &[&suggested],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
            );
            emitted_extension_diagnostic = true;
        }

        // TS5097: Check for .ts/.tsx/.mts/.cts extensions when allowImportingTsExtensions is disabled.
        // rewriteRelativeImportExtensions also suppresses this error (tsc utilities.ts:9045).
        // tsc does not emit TS5097 inside declaration files (.d.ts).
        // When the resolver reports TS6142 (jsx not set), tsc does not also emit TS5097.
        let has_jsx_not_set_error = self.ctx.get_resolution_error(module_name).is_some_and(|e| {
            e.code
                == crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
        });
        if !self.ctx.compiler_options.allow_importing_ts_extensions
            && !self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && !has_jsx_not_set_error
            && let Some(ext) = ts_extension_suffix(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                    diagnostic_messages::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                    &[ext],
                );
            self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                );
            emitted_extension_diagnostic = true;
        }

        // TS2876: rewriteRelativeImportExtensions — specifier looks like a file name
        // (e.g. `./foo.ts`) but actually resolves to a directory index file
        // (e.g. `./foo.ts/index.ts`), making extension rewriting unsafe.
        // tsc checks `!resolvedModule.resolvedUsingTsExtension && shouldRewrite`.
        if !emitted_extension_diagnostic
            && self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && should_rewrite_module_specifier(module_name)
            && self.resolved_via_directory_index(module_name)
        {
            let resolved_display = self.resolved_file_display_path(module_name);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME,
                &[&resolved_display],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME,
            );
            emitted_extension_diagnostic = true;
        }

        // TS2877: rewriteRelativeImportExtensions — non-relative imports with
        // a TypeScript extension that resolve to an input TypeScript file are not
        // rewritten during emit.
        if !emitted_extension_diagnostic
            && self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && !should_rewrite_module_specifier(module_name)
            && !self.resolved_via_directory_index(module_name)
            && self.module_target_is_typescript_input_file(module_name)
            && let Some(ext) = ts_extension_suffix(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
                &[ext],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
            );
            emitted_extension_diagnostic = true;
        }

        if self.would_create_cycle(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: cycle detected");
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular import detected: {cycle_str}");

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        self.ctx.import_resolution_stack.push(module_name.clone());

        // Node.js built-in modules (e.g. "fs", "path", "node:fs") should not
        // trigger TS2307/TS2882 when using Node module resolution. TSC resolves
        // these via @types/node; our single-file checker lacks this, so we
        // suppress resolution errors for known built-in names.
        let is_node_builtin = self.ctx.compiler_options.module.is_node_module()
            && is_node_builtin_module(module_name);

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || error_code == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
            {
                // When TS2846 or TS5097 was already emitted for this import,
                // suppress TS2307/TS2792. tsc prioritizes extension-specific
                // diagnostics over "cannot find module" errors.
                // Also suppress TS2307 for .d.ts type-only imports — tsc does
                // not validate module existence for `import type` from .d.ts.
                if emitted_extension_diagnostic || (is_type_only_import && dts_ext.is_some()) {
                    self.ctx.import_resolution_stack.pop();
                    return;
                }
                // Node.js built-in modules: suppress TS2307/TS2882 entirely.
                if is_node_builtin {
                    self.ctx.import_resolution_stack.pop();
                    return;
                }
                // Side-effect imports use TS2882 instead of TS2307/TS2792,
                // but only when noUncheckedSideEffectImports is enabled.
                // When disabled, side-effect imports with resolution errors are silently ignored.
                if is_side_effect_import {
                    // When the driver has already reported a resolution error for this side-effect import,
                    // emit it as TS2882 even when noUncheckedSideEffectImports is disabled.
                    // This ensures module-not-found errors are not silently swallowed (matches tsc behavior).
                    if error_code
                        == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                        || error_code
                            == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        error_code = diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF;
                        error_message = format_message(
                            diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                            &[module_name],
                        );
                    } else if !self.ctx.compiler_options.no_unchecked_side_effect_imports {
                        self.ctx.import_resolution_stack.pop();
                        return;
                    }
                } else {
                    let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                    error_code = fallback_code;
                    error_message = fallback_message;
                }
            }
            tracing::trace!(%module_name, error_code, "check_import_declaration: resolution error found");
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            if error_code
                != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // Ambient module declarations still suppress TS2307 when the driver
        // did not report a concrete resolution failure for this import.
        if self.is_ambient_module_match(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: ambient module match, returning");
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Use global declared modules index for O(1) lookup
        {
            let found = if let Some(declared) = &self.ctx.global_declared_modules {
                let normalized = module_name.trim_matches('"').trim_matches('\'');
                declared.exact.contains(normalized)
            } else if let Some(binders) = &self.ctx.all_binders {
                binders.iter().any(|binder| {
                    binder.declared_modules.contains(module_name)
                        || binder.shorthand_ambient_modules.contains(module_name)
                })
            } else {
                false
            };
            if found {
                tracing::trace!(%module_name, "check_import_declaration: found in declared/shorthand modules, returning");
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // For side-effect imports (import "module") in default mode (no_unchecked_side_effect_imports=false),
        // we only check resolution errors (TS2882 above). Skip member/export validation which
        // requires import bindings. If we reach here, the module resolved successfully.
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if module was successfully resolved
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
                let resolution_mode =
                    self.requested_resolution_mode(import.attributes, is_type_only_import);
                let has_typed_export_surface = self
                    .resolve_effective_module_exports_with_mode(module_name, resolution_mode)
                    .is_some();
                // If the module resolved to a target file but has no typed export surface,
                // the module is effectively not found. Emit TS2307 for this case.
                // This handles cases like symlinked workspace dependencies where the
                // package exists but doesn't have valid exports.
                if !has_typed_export_surface {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let is_json_module = file_name.ends_with(".json")
                            && self.ctx.compiler_options.resolve_json_module;
                        // Check if this is a .d.ts file with only `export=` (no named exports).
                        // Such files should NOT emit TS2307 here because they have a valid
                        // export surface via the export assignment.
                        let is_dts_with_only_export_assignment = if file_name.ends_with(".d.ts") {
                            if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                                if let Some(exports) = binder.module_exports.get(file_name) {
                                    exports.has("export=") && exports.len() == 1
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        // Check if the file contains module augmentation (e.g., `declare module "X" { ... }`).
                        // Files with module augmentations serve a valid purpose (extending module types)
                        // and should not trigger TS2307 even if they have no regular exports.
                        let has_module_augmentation =
                            self.source_file_has_module_augmentation(arena, source_file);
                        // For non-JS, non-JSON files without export surface, emit TS2307
                        // BUT skip if it's a .d.ts file with only export= (no named exports)
                        // OR if it's a file with module augmentation
                        if !is_js_like
                            && !is_json_module
                            && !is_dts_with_only_export_assignment
                            && !has_module_augmentation
                        {
                            let (message, code) = self.module_not_found_diagnostic(module_name);
                            if !self.ctx.modules_with_ts2307_emitted.contains(module_name) {
                                self.ctx
                                    .modules_with_ts2307_emitted
                                    .insert(module_name.to_string());
                                self.error_at_position(spec_start, spec_length, &message, code);
                            }
                            self.ctx.import_resolution_stack.pop();
                            return;
                        }
                    }
                }
                let mut skip_export_checks = false;
                // Extract data we need before any mutable borrows
                let (_target_is_declaration_file, file_info) = {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let skip_exports = is_js_like
                            && !source_file.is_declaration_file
                            && !has_typed_export_surface;
                        // Determine if target file is ESM. .mjs/.mts are always ESM.
                        // For .js/.ts targets, also check package.json "type" field via
                        // file_is_esm_map. TSC does not emit TS1479 when a .js source file
                        // imports a .js ESM target — only when the target is .mjs/.mts
                        // (unambiguously ESM). However, .cjs files are unambiguously CJS,
                        // so they DO get TS1479 when importing .js ESM targets.
                        // JSON files are data, not modules — they can always be
                        // require()'d and never count as ESM for TS1479.
                        let target_is_json = file_name.ends_with(".json");
                        let target_ext_is_esm = !target_is_json
                            && (file_name.ends_with(".mjs") || file_name.ends_with(".mts"));
                        // Skip file_is_esm_map check only for ambiguous JS sources (.js/.jsx).
                        // .cjs is unambiguously CJS, so it should check file_is_esm_map
                        // to detect .js targets that are ESM via package.json "type".
                        let skip_esm_map = target_is_json
                            || self.ctx.file_name.ends_with(".js")
                            || self.ctx.file_name.ends_with(".jsx")
                            || self.ctx.file_name.ends_with(".mjs");
                        let target_is_esm = target_ext_is_esm
                            || (!skip_esm_map
                                && self
                                    .ctx
                                    .file_is_esm_map
                                    .as_ref()
                                    .and_then(|m| m.get(file_name))
                                    .copied()
                                    .unwrap_or(false));
                        let is_dts = source_file.is_declaration_file;
                        (is_dts, Some((skip_exports, target_is_esm)))
                    } else {
                        (false, None)
                    }
                };

                if let Some((should_skip_exports, target_is_esm)) = file_info {
                    if should_skip_exports {
                        skip_export_checks = true;
                    }

                    // TS1479: Check if CommonJS file is importing an ES module.
                    // In TypeScript 6.0+, TSC only emits TS1479 for Node16/Node18
                    // module kinds. Node20 and NodeNext (targeting Node 22+) support
                    // `require()` of ESM modules, so the diagnostic is suppressed.
                    // For ESNext, Preserve, bundler, and other module kinds, the
                    // import interop is handled by the bundler/runtime.
                    let is_node_module_kind =
                        self.ctx.compiler_options.module.is_node16_or_node18();
                    let current_is_commonjs = is_node_module_kind && {
                        let current_file = &self.ctx.file_name;
                        // .cts/.cjs are always CommonJS
                        let is_commonjs_file =
                            current_file.ends_with(".cts") || current_file.ends_with(".cjs");
                        // .mts/.mjs are always ESM
                        let is_esm_file =
                            current_file.ends_with(".mts") || current_file.ends_with(".mjs");
                        if is_commonjs_file {
                            true
                        } else if is_esm_file {
                            false
                        } else if let Some(is_esm) = self.ctx.file_is_esm {
                            // Driver-provided per-file module kind from package.json
                            // "type" field (Node16/NodeNext resolution)
                            !is_esm
                        } else {
                            // Fallback: global module kind heuristic
                            !self.ctx.compiler_options.module.is_es_module()
                        }
                    };

                    // TSC suppresses TS1479 for .cjs/.cts files with relative imports.
                    // These explicitly-CJS files only get TS1479 for non-relative
                    // (package) imports where Node's runtime resolution would fail loading
                    // an ESM module via require(). Relative imports within the project are
                    // handled by tsc's output processing, not Node's runtime loader.
                    let is_explicit_cjs_file = self.ctx.file_name.ends_with(".cjs")
                        || self.ctx.file_name.ends_with(".cts");
                    let is_relative_import =
                        module_name.starts_with("./") || module_name.starts_with("../");
                    let suppress_for_cjs_relative = is_explicit_cjs_file && is_relative_import;

                    // TS1479 only applies under Node16/Node18 module kinds where
                    // CJS/ESM interop boundaries exist at runtime. Node20/NodeNext,
                    // bundler resolution, and pure ESM module kinds handle interop
                    // transparently.
                    let module_has_cjs_esm_boundary =
                        self.ctx.compiler_options.module.is_node16_or_node18();

                    if current_is_commonjs
                        && target_is_esm
                        && module_has_cjs_esm_boundary
                        && !is_type_only_import
                        && !suppress_for_cjs_relative
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                            &[module_name],
                        );
                        self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                        );
                    }
                }

                // TS2846 for resolved .d.ts files is only emitted when the import
                // specifier explicitly uses a .d.ts extension (handled above at the
                // dts_ext check). TSC does NOT emit TS2846 when an import like
                // "./foo" resolves to "foo.d.ts" — even under verbatimModuleSyntax.
                self.maybe_emit_imported_global_augmentation_errors(target_idx);
                if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                    let normalized_module_name = module_name.trim_matches('"').trim_matches('\'');
                    // Side-effect imports (`import "x"`) never require the target
                    // to be a module — they just execute the file.  Skip TS2306
                    // regardless of the noUncheckedSideEffectImports setting.
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    let source_file = arena.source_files.first();
                    let target_is_global_augmentation_dts =
                        source_file.is_some_and(|source_file| {
                            source_file.file_name.ends_with(".d.ts")
                                && !self
                                    .source_file_has_syntactic_module_indicator(arena, source_file)
                                && self.source_file_has_top_level_global_augmentation(
                                    arena,
                                    source_file,
                                )
                        });
                    if !is_side_effect_import
                        && (!binder.is_external_module || target_is_global_augmentation_dts)
                        && !self.is_ambient_module_match(module_name)
                        && !binder.declared_modules.contains(normalized_module_name)
                        && let Some(source_file) = source_file
                    {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let is_json_module = file_name.ends_with(".json")
                            && self.ctx.compiler_options.resolve_json_module;
                        if !is_js_like && !is_json_module {
                            use crate::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let message = format_message(
                                diagnostic_messages::FILE_IS_NOT_A_MODULE,
                                &[&source_file.file_name],
                            );
                            self.error_at_position(
                                spec_start,
                                spec_length,
                                &message,
                                diagnostic_codes::FILE_IS_NOT_A_MODULE,
                            );
                            self.ctx.import_resolution_stack.pop();
                            return;
                        }
                    }
                }
                if !skip_export_checks {
                    self.check_imported_members(import, module_name);
                }
            } else {
                self.check_imported_members(import, module_name);
            }

            // TS1484/TS1485: verbatimModuleSyntax import checks
            self.check_verbatim_module_syntax_imports(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: found in module_exports, checking members");
            self.check_imported_members(import, module_name);

            // TS1484/TS1485: verbatimModuleSyntax import checks
            self.check_verbatim_module_syntax_imports(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Node.js built-in modules: suppress fallback TS2307/TS2882 too.
        if is_node_builtin {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        tracing::trace!(%module_name, "check_import_declaration: fallback - emitting module-not-found error");
        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.modules_with_ts2307_emitted.insert(module_key);
            // Side-effect imports (bare `import "module"`) use TS2882 instead of TS2307
            let (message, code) = if is_side_effect_import {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                (
                    format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                        &[module_name],
                    ),
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                )
            } else {
                self.module_not_found_diagnostic(module_name)
            };
            // Use pre-extracted position instead of error_at_node to avoid
            // silent failures when get_node_span returns None
            self.error_at_position(spec_start, spec_length, &message, code);
        }

        self.ctx.import_resolution_stack.pop();
    }

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Check re-export chains for circular dependencies.
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut FxHashSet<String>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if visited.contains(module_name) {
            let cycle_path: Vec<&str> = visited
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!(
                "{}: {}",
                diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                cycle_str
            );

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error(
                    0,
                    0,
                    message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        visited.insert(module_name.to_string());

        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        visited.remove(module_name);
    }

    /// Check if adding a module to the resolution path would create a cycle.
    pub(crate) fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }

    // =========================================================================
    // Re-export Resolution Helpers
    // =========================================================================

    /// Try to resolve an import through the target module's binder re-export chains.
    /// Traverses across binder boundaries by resolving each re-export source
    /// to its target file and checking that file's binder.
    pub(crate) fn resolve_import_via_target_binder(
        &self,
        module_name: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        };
        if let Some(target_idx) = target_idx {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_in_file(target_idx, import_name, &mut visited);
        }
        false
    }

    /// Try to resolve an import by searching binders' re-export chains.
    ///
    /// Uses `global_module_binder_index` for O(1) candidate lookup when available,
    /// falling back to an O(N) scan of all binders otherwise.
    pub(crate) fn resolve_import_via_all_binders(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        let Some(all_binders) = &self.ctx.all_binders else {
            return false;
        };
        // Use global module binder index for O(1) candidate lookup.
        if let Some(ref idx) = self.ctx.global_module_binder_index {
            let candidate_indices = idx
                .get(module_name)
                .into_iter()
                .flatten()
                .chain(idx.get(normalized).into_iter().flatten());
            let mut seen = FxHashSet::default();
            for &binder_idx in candidate_indices {
                if !seen.insert(binder_idx) {
                    continue;
                }
                if let Some(binder) = all_binders.get(binder_idx)
                    && (binder
                        .resolve_import_if_needed_public(module_name, import_name)
                        .is_some()
                        || binder
                            .resolve_import_if_needed_public(normalized, import_name)
                            .is_some())
                {
                    return true;
                }
            }
            return false;
        }
        // Fallback: O(N) scan when index not built.
        for binder in all_binders.iter() {
            if binder
                .resolve_import_if_needed_public(module_name, import_name)
                .is_some()
                || binder
                    .resolve_import_if_needed_public(normalized, import_name)
                    .is_some()
            {
                return true;
            }
        }
        false
    }

    /// Resolve an import by checking a specific file's exports and following
    /// re-export chains across binder boundaries. Each file has its own binder
    /// in multi-file mode, so we traverse wildcard/named re-exports by resolving
    /// each source specifier to its target file and checking that file's binder.
    fn resolve_import_in_file(
        &self,
        file_idx: usize,
        import_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> bool {
        if !visited.insert(file_idx) {
            return false; // Cycle detection
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && exports.has(import_name)
        {
            return true;
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = reexports.get(import_name)
        {
            let name = original_name.as_deref().unwrap_or(import_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && self.resolve_import_in_file(source_idx, name, visited)
            {
                return true;
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && self.resolve_import_in_file(source_idx, import_name, visited)
                {
                    return true;
                }
            }
        }

        false
    }

    fn check_import_declaration_conflicts(&mut self, stmt_idx: NodeIndex, clause_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        let mut bindings_to_check = Vec::new();

        if clause.name.is_some() {
            bindings_to_check.push((clause_idx, clause.name));
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
        {
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                {
                    bindings_to_check.push((clause.named_bindings, ns.name));
                }
            } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
                && let Some(named) = self.ctx.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &named.elements.nodes {
                    if let Some(spec_node) = self.ctx.arena.get(spec_idx)
                        && let Some(spec) = self.ctx.arena.get_specifier(spec_node)
                    {
                        let name_idx = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if name_idx.is_some() {
                            bindings_to_check.push((spec_idx, name_idx));
                        }
                    }
                }
            }
        }

        for (binding_node_idx, name_idx) in bindings_to_check {
            if let Some(name_node) = self.ctx.arena.get(name_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let sym_id_opt = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&binding_node_idx.0)
                    .copied();
                if let Some(sym_id) = sym_id_opt {
                    let mut has_conflict = false;
                    if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                        if sym.is_type_only {
                            continue;
                        }

                        let mut import_has_value = false;
                        let mut import_has_type = false;
                        let mut visited = Vec::new();
                        if let Some(resolved_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                            // When resolve_alias_symbol returns the SAME symbol, it
                            // means resolution failed (e.g. unresolved external module).
                            // The symbol's flags include merged local declarations,
                            // which would give a false positive.
                            && resolved_id != sym_id
                            && let Some(resolved_sym) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(resolved_id, &self.get_lib_binders())
                        {
                            let mut has_value = (resolved_sym.flags
                                & (symbol_flags::VALUE | symbol_flags::EXPORT_VALUE))
                                != 0;
                            if has_value
                                && (resolved_sym.flags & symbol_flags::VALUE_MODULE) != 0
                                && (resolved_sym.flags
                                    & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE))
                                    == 0
                            {
                                let mut any_instantiated = false;
                                for &decl_idx in &resolved_sym.declarations {
                                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
                                                        if self.is_namespace_declaration_instantiated(decl_idx) {
                                                            any_instantiated = true;
                                                            break;
                                                        }
                                                    } else {
                                                        any_instantiated = true;
                                                        break;
                                                    }
                                    }
                                }
                                has_value = any_instantiated;
                            }
                            import_has_value = has_value;
                            // Check if the imported symbol carries type semantics
                            // (e.g. enum, class, interface). When it does, local type
                            // aliases or interfaces with the same name conflict.
                            if (resolved_sym.flags & symbol_flags::TYPE) != 0 {
                                import_has_type = true;
                            }
                            if (resolved_sym.flags & symbol_flags::ALIAS) != 0
                                && sym.import_module.is_some()
                                && sym.import_name.is_none()
                            {
                                import_has_value = true;
                            }
                        }

                        // Cross-file fallback: when resolve_alias_symbol returns the alias
                        // itself (can't resolve cross-file), check the exported symbol's
                        // flags directly in the target file's binder.
                        if (!import_has_value || !import_has_type)
                            && let Some(ref module_name) = sym.import_module
                        {
                            let export_name = sym.import_name.as_deref().unwrap_or(&name);
                            // Try declared modules (module_exports)
                            // Use global_module_binder_index for O(1) lookup instead of O(N) binder scan
                            if let Some(binders) = &self.ctx.all_binders {
                                let candidate_indices = self
                                    .ctx
                                    .global_module_binder_index
                                    .as_ref()
                                    .and_then(|idx| idx.get(module_name.as_str()));
                                if let Some(indices) = candidate_indices {
                                    for &binder_idx in indices {
                                        if let Some(binder) = binders.get(binder_idx)
                                            && let Some(exports) =
                                                binder.module_exports.get(module_name.as_str())
                                            && let Some(target_sym_id) = exports.get(export_name)
                                            && let Some(target_sym) =
                                                binder.symbols.get(target_sym_id)
                                        {
                                            if (target_sym.flags
                                                & (symbol_flags::VALUE
                                                    | symbol_flags::EXPORT_VALUE))
                                                != 0
                                            {
                                                import_has_value = true;
                                            }
                                            if (target_sym.flags & symbol_flags::TYPE) != 0 {
                                                import_has_type = true;
                                            }
                                            if import_has_value {
                                                break;
                                            }
                                        }
                                    }
                                } else {
                                    for binder in binders.iter() {
                                        if let Some(exports) =
                                            binder.module_exports.get(module_name.as_str())
                                            && let Some(target_sym_id) = exports.get(export_name)
                                            && let Some(target_sym) =
                                                binder.symbols.get(target_sym_id)
                                        {
                                            if (target_sym.flags
                                                & (symbol_flags::VALUE
                                                    | symbol_flags::EXPORT_VALUE))
                                                != 0
                                            {
                                                import_has_value = true;
                                            }
                                            if (target_sym.flags & symbol_flags::TYPE) != 0 {
                                                import_has_type = true;
                                            }
                                            if import_has_value {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            // Try regular file exports: follow re-export chains
                            // (module_exports → named re-exports → wildcard re-exports)
                            // to find the actual exported symbol.  Using file_locals directly
                            // would pick up globals leaked by create_binder_from_bound_file.
                            if (!import_has_value || !import_has_type)
                                && let Some(target_idx) =
                                    self.ctx.resolve_import_target(module_name)
                            {
                                let mut visited = FxHashSet::default();
                                if let Some((resolved_sym_id, resolved_file_idx)) = self
                                    .resolve_export_in_file(target_idx, export_name, &mut visited)
                                {
                                    let resolved_binder =
                                        self.ctx.get_binder_for_file(resolved_file_idx);
                                    if let Some(resolved_sym) =
                                        resolved_binder.and_then(|b| b.symbols.get(resolved_sym_id))
                                    {
                                        if (resolved_sym.flags
                                            & (symbol_flags::VALUE | symbol_flags::EXPORT_VALUE))
                                            != 0
                                        {
                                            import_has_value = true;
                                        }
                                        if (resolved_sym.flags & symbol_flags::TYPE) != 0 {
                                            import_has_type = true;
                                        }
                                        // Non-type-only re-export aliases forward values
                                        if !import_has_value
                                            && (resolved_sym.flags & symbol_flags::ALIAS) != 0
                                            && !resolved_sym.is_type_only
                                        {
                                            import_has_value = true;
                                        }
                                        // When a type alias shadows an import alias,
                                        // follow alias_partners to the partner ALIAS
                                        // and check its import chain for value semantics.
                                        if !import_has_value
                                            && (resolved_sym.flags & symbol_flags::TYPE_ALIAS) != 0
                                            && !resolved_sym.is_type_only
                                            && let Some(resolved_binder) =
                                                self.ctx.get_binder_for_file(resolved_file_idx)
                                            && let Some(&partner_id) =
                                                resolved_binder.alias_partners.get(&resolved_sym_id)
                                            && let Some(partner) =
                                                resolved_binder.symbols.get(partner_id)
                                            && (partner.flags & symbol_flags::ALIAS) != 0
                                            && !partner.is_type_only
                                            && let Some(ref src_module) = partner.import_module
                                        {
                                            let src_name = partner
                                                .import_name
                                                .as_deref()
                                                .unwrap_or(export_name);
                                            if let Some(src_idx) =
                                                self.ctx.resolve_import_target_from_file(
                                                    resolved_file_idx,
                                                    src_module,
                                                )
                                            {
                                                let mut inner_visited = FxHashSet::default();
                                                if let Some((src_sym_id, src_file_idx)) = self
                                                    .resolve_export_in_file(
                                                        src_idx,
                                                        src_name,
                                                        &mut inner_visited,
                                                    )
                                                    && let Some(src_binder) =
                                                        self.ctx.get_binder_for_file(src_file_idx)
                                                    && let Some(src_sym) =
                                                        src_binder.symbols.get(src_sym_id)
                                                    && (src_sym.flags
                                                        & (symbol_flags::VALUE
                                                            | symbol_flags::EXPORT_VALUE))
                                                        != 0
                                                {
                                                    import_has_value = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Namespace imports (`import * as X`) always create a
                        // value binding (the module namespace object), even when
                        // the target module can't be resolved.
                        if !import_has_value
                            && let Some(binding_node) = self.ctx.arena.get(binding_node_idx)
                            && binding_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                        {
                            import_has_value = true;
                        }

                        if !import_has_value {
                            continue;
                        }

                        // Use the import STATEMENT's enclosing scope — the scope
                        // the import lives in (e.g. module scope).  We avoid using
                        // the import-specifier's scope because `find_enclosing_scope`
                        // may differ from the statement scope when the specifier is
                        // inside a NamedImports node that happens to be scope-creating.
                        let import_scope = self
                            .ctx
                            .binder
                            .find_enclosing_scope(self.ctx.arena, stmt_idx);

                        // Check 1: merged declarations on the import's own symbol.
                        has_conflict = sym.declarations.iter().any(|&decl_idx| {
                            if decl_idx == binding_node_idx
                                || decl_idx == clause_idx
                                || decl_idx == stmt_idx
                            {
                                return false;
                            }
                            let is_current_file_decl =
                                self.ctx.binder.node_symbols.contains_key(&decl_idx.0);
                            if !is_current_file_decl {
                                return false;
                            }
                            // Skip declarations inside module augmentations
                            // (`declare module "./foo" { ... }`).  The binder may
                            // not create a separate scope for the augmentation block,
                            // so the scope check alone can't detect this.
                            if self.is_inside_module_augmentation(decl_idx) {
                                return false;
                            }
                            // Scope check: the declaration must be in the same
                            // logical scope as the import.  We compare scopes by
                            // checking if they are the same ScopeId OR if they
                            // share the same container symbol (merged namespace
                            // blocks create separate scopes but share one symbol).
                            // Use the PARENT's scope for scope-creating nodes
                            // (e.g. function/class declarations create a body
                            // scope, but they *live in* the parent scope).
                            let decl_containing_scope =
                                self.ctx.arena.get_extended(decl_idx).and_then(|ext| {
                                    let parent = ext.parent;
                                    if parent.is_some() {
                                        self.ctx.binder.find_enclosing_scope(self.ctx.arena, parent)
                                    } else {
                                        self.ctx
                                            .binder
                                            .find_enclosing_scope(self.ctx.arena, decl_idx)
                                    }
                                });
                            let in_same_scope = match (import_scope, decl_containing_scope) {
                                (Some(a), Some(b)) if a == b => true,
                                (Some(a), Some(b)) => {
                                    // Merged namespace: check if both scopes'
                                    // container nodes map to the same symbol.
                                    let sym_a =
                                        self.ctx.binder.scopes.get(a.0 as usize).and_then(|s| {
                                            self.ctx.binder.node_symbols.get(&s.container_node.0)
                                        });
                                    let sym_b =
                                        self.ctx.binder.scopes.get(b.0 as usize).and_then(|s| {
                                            self.ctx.binder.node_symbols.get(&s.container_node.0)
                                        });
                                    sym_a.is_some() && sym_a == sym_b
                                }
                                _ => true,
                            };
                            if !in_same_scope {
                                return false;
                            }

                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                if matches!(
                                    decl_node.kind,
                                    syntax_kind_ext::IMPORT_CLAUSE
                                        | syntax_kind_ext::NAMESPACE_IMPORT
                                        | syntax_kind_ext::IMPORT_SPECIFIER
                                        | syntax_kind_ext::NAMED_IMPORTS
                                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                        | syntax_kind_ext::IMPORT_DECLARATION
                                        // Re-exports (`export { x } from "./b"`) don't
                                        // introduce local bindings, so they must not
                                        // conflict with imports.
                                        | syntax_kind_ext::EXPORT_SPECIFIER
                                        | syntax_kind_ext::EXPORT_DECLARATION
                                ) {
                                    return false;
                                }
                                // Type aliases and interfaces live in the type declaration
                                // space. They only conflict with imports that also carry
                                // type semantics (e.g. enums, classes).
                                if !import_has_type
                                    && matches!(
                                        decl_node.kind,
                                        syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                            | syntax_kind_ext::INTERFACE_DECLARATION
                                    )
                                {
                                    return false;
                                }
                                // Non-import, non-type local declarations (var, function,
                                // class, namespace, enum) conflict with value imports.
                                // Type declarations conflict when the import has type meaning.
                                true
                            } else {
                                false
                            }
                        });

                        // Check 2: separate symbols with the same name (binder may
                        // create distinct symbols instead of merging declarations).
                        if !has_conflict {
                            let all_symbols = self.ctx.binder.symbols.find_all_by_name(&name);
                            for &other_sym_id in all_symbols {
                                if other_sym_id == sym_id {
                                    continue;
                                }
                                if let Some(other_sym) = self.ctx.binder.symbols.get(other_sym_id) {
                                    // Skip if the other symbol is purely an alias (another import)
                                    if (other_sym.flags & symbol_flags::ALIAS) != 0
                                        && (other_sym.flags & !symbol_flags::ALIAS) == 0
                                    {
                                        continue;
                                    }
                                    // Skip type-only symbols (type aliases, interfaces) — they
                                    // live in the type declaration space and don't conflict
                                    // with value-only imports. When the import also carries
                                    // type semantics (e.g. enum, class), they DO conflict.
                                    if !import_has_type {
                                        let type_only_flags = symbol_flags::TYPE_ALIAS
                                            | symbol_flags::INTERFACE
                                            | symbol_flags::TYPE_PARAMETER;
                                        if (other_sym.flags & type_only_flags) != 0
                                            && (other_sym.flags & symbol_flags::VALUE) == 0
                                        {
                                            continue;
                                        }
                                    }
                                    // Must have a declaration in the same scope
                                    let decl_in_same_scope =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            let decl_containing =
                                                self.ctx.arena.get_extended(decl_idx).and_then(
                                                    |ext| {
                                                        let parent = ext.parent;
                                                        if parent.is_some() {
                                                            self.ctx.binder.find_enclosing_scope(
                                                                self.ctx.arena,
                                                                parent,
                                                            )
                                                        } else {
                                                            self.ctx.binder.find_enclosing_scope(
                                                                self.ctx.arena,
                                                                decl_idx,
                                                            )
                                                        }
                                                    },
                                                );

                                            match (import_scope, decl_containing) {
                                                (Some(a), Some(b)) => a == b,
                                                _ => true,
                                            }
                                        });
                                    if !decl_in_same_scope {
                                        continue;
                                    }
                                    // Must be in the current file and not an
                                    // import/export specifier (re-exports like
                                    // `export { x } from "./b"` don't create local
                                    // bindings and must not conflict with imports).
                                    let has_local_decl =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            if self.ctx.binder.node_symbols.get(&decl_idx.0)
                                                != Some(&other_sym_id)
                                            {
                                                return false;
                                            }
                                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                                if matches!(
                                                    decl_node.kind,
                                                    syntax_kind_ext::EXPORT_SPECIFIER
                                                        | syntax_kind_ext::EXPORT_DECLARATION
                                                        | syntax_kind_ext::IMPORT_CLAUSE
                                                        | syntax_kind_ext::NAMESPACE_IMPORT
                                                        | syntax_kind_ext::IMPORT_SPECIFIER
                                                        | syntax_kind_ext::NAMED_IMPORTS
                                                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                                        | syntax_kind_ext::IMPORT_DECLARATION
                                                ) {
                                                    return false;
                                                }
                                                // Type declarations only conflict when
                                                // the import also carries type semantics.
                                                if !import_has_type
                                                    && matches!(
                                                        decl_node.kind,
                                                        syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                                            | syntax_kind_ext::INTERFACE_DECLARATION
                                                    )
                                                {
                                                    return false;
                                                }
                                                true
                                            } else {
                                                false
                                            }
                                        });
                                    if has_local_decl {
                                        has_conflict = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if has_conflict {
                        let message = format_message(
                                diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                                &[&name],
                            );
                        self.error_at_node(
                                name_idx,
                                &message,
                                diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            );
                        // Record so TS2456 can be suppressed for type aliases
                        // whose apparent circularity is caused by this conflict.
                        self.ctx.import_conflict_names.insert(name.clone());
                    }
                }
            }
        }
    }

    /// Returns `true` if `specifier` resolves via directory-index probing rather
    /// than direct TS-extension file matching.
    ///
    /// This mirrors tsc's `!resolvedModule.resolvedUsingTsExtension`:
    /// if the specifier is `./foo.ts` but the resolved file is
    /// `foo.ts/index.ts`, the TS extension in the specifier was NOT used to
    /// find the file — directory probing found it instead.
    pub(crate) fn resolved_via_directory_index(&self, specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return false;
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return false;
        };
        let Some(sf) = target_arena.source_files.first() else {
            return false;
        };
        // Extract the stem (without extension) from the specifier basename.
        let spec_file = specifier
            .rsplit_once('/')
            .map_or(specifier, |(_, file)| file);
        let spec_stem = spec_file.rfind('.').map_or(spec_file, |i| &spec_file[..i]);
        // Extract the stem from the resolved file's basename.
        // For declaration files like "foo.d.ts", strip all declaration
        // suffixes to get the base stem "foo".
        let resolved_file = sf
            .file_name
            .rsplit_once('/')
            .map_or(sf.file_name.as_str(), |(_, file)| file);
        let resolved_stem = resolved_file
            .strip_suffix(".d.ts")
            .or_else(|| resolved_file.strip_suffix(".d.mts"))
            .or_else(|| resolved_file.strip_suffix(".d.cts"))
            .or_else(|| resolved_file.rfind('.').map(|i| &resolved_file[..i]))
            .unwrap_or(resolved_file);
        // If the stems match, the resolution used the TS extension directly
        // (e.g., ./obj.ts → obj.d.ts). If stems differ, it went through
        // directory probing (e.g., ./foo.ts → foo.ts/index.d.ts).
        resolved_stem != spec_stem
    }

    /// Returns a relative display path for the resolved target of `specifier`,
    /// suitable for the TS2876 diagnostic message argument.
    pub(crate) fn resolved_file_display_path(&self, specifier: &str) -> String {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return specifier.to_string();
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return specifier.to_string();
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return specifier.to_string();
        };
        let Some(sf) = target_arena.source_files.first() else {
            return specifier.to_string();
        };
        // Return a relative path with "./" prefix, matching tsc's output format.
        let resolved = &sf.file_name;
        if resolved.starts_with("./") || resolved.starts_with("../") {
            resolved.clone()
        } else {
            format!("./{resolved}")
        }
    }

    /// Returns `true` if `specifier` resolves to a non-declaration TypeScript input
    /// file (`.ts`, `.tsx`, `.mts`, `.cts`) that can participate in emit rewriting.
    fn module_target_is_typescript_input_file(&self, specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return false;
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return false;
        };
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();
        if is_declaration_file_name(file_name) {
            return false;
        }

        file_name.ends_with(".ts")
            || file_name.ends_with(".tsx")
            || file_name.ends_with(".mts")
            || file_name.ends_with(".cts")
    }
}

#[cfg(test)]
mod tests {
    use super::ts_extension_suffix;

    #[test]
    fn ts_extension_detects_ts() {
        assert_eq!(ts_extension_suffix("./foo.ts"), Some(".ts"));
    }

    #[test]
    fn ts_extension_detects_tsx() {
        assert_eq!(ts_extension_suffix("./foo.tsx"), Some(".tsx"));
    }

    #[test]
    fn ts_extension_detects_mts() {
        assert_eq!(ts_extension_suffix("./foo.mts"), Some(".mts"));
    }

    #[test]
    fn ts_extension_detects_cts() {
        assert_eq!(ts_extension_suffix("./foo.cts"), Some(".cts"));
    }

    #[test]
    fn ts_extension_ignores_dts() {
        assert_eq!(ts_extension_suffix("./foo.d.ts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_mts() {
        assert_eq!(ts_extension_suffix("./foo.d.mts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_cts() {
        assert_eq!(ts_extension_suffix("./foo.d.cts"), None);
    }

    #[test]
    fn ts_extension_ignores_js() {
        assert_eq!(ts_extension_suffix("./foo.js"), None);
    }

    #[test]
    fn ts_extension_ignores_no_ext() {
        assert_eq!(ts_extension_suffix("./foo"), None);
    }

    #[test]
    fn ts_extension_ignores_json() {
        assert_eq!(ts_extension_suffix("./data.json"), None);
    }
}
