use crate::context::emit::EmitContext;
use crate::context::transform::{TransformContext, TransformDirective};
use std::sync::Arc;
use tsz_common::ScriptTarget;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{contains_arguments_reference, contains_this_reference};
use tsz_scanner::SyntaxKind;

use crate::transforms::emit_utils;

/// Maximum recursion depth for AST traversal to prevent stack overflow
pub(super) const MAX_AST_DEPTH: u32 = 500;

/// Maximum depth for qualified name recursion (A.B.C.D...)
pub(super) const MAX_QUALIFIED_NAME_DEPTH: u32 = 100;

/// Maximum depth for binding pattern recursion ({a: {b: {c: ...}}})
pub(super) const MAX_BINDING_PATTERN_DEPTH: u32 = 100;

/// Lowering pass - Phase 1 of emission
///
/// Walks the AST and produces transform directives based on compiler options.
pub struct LoweringPass<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) ctx: &'a EmitContext,
    pub(super) transforms: TransformContext,
    pub(super) commonjs_mode: bool,
    pub(super) has_export_assignment: bool,
    /// Current recursion depth for stack overflow protection
    pub(super) visit_depth: u32,
    /// Track declared names for namespace/class/enum/function merging detection
    pub(super) declared_names: rustc_hash::FxHashSet<String>,
    /// Nesting depth of namespace/module declaration bodies.
    /// CommonJS export directives should only be forced at top-level (depth == 0).
    pub(super) namespace_depth: u32,
    /// Depth of arrow functions that capture 'this'
    /// When > 0, 'this' references should be substituted with '_this'
    pub(super) this_capture_level: u32,
    /// Depth of arrow functions that capture 'arguments'
    /// When > 0, 'arguments' references should be substituted with '_arguments'
    pub(super) arguments_capture_level: u32,
    /// Tracks if the current class declaration has an 'extends' clause
    pub(super) current_class_is_derived: bool,
    /// Tracks if we are currently inside a constructor body
    pub(super) in_constructor: bool,
    /// Tracks if we are inside a static class member
    pub(super) in_static_context: bool,
    /// Current class alias name (e.g., "_a") for static members
    pub(super) current_class_alias: Option<String>,
    /// True when visiting the left side of a destructuring assignment
    pub(super) in_assignment_target: bool,
    /// True when inside a class body in ES5 mode.
    /// Arrow functions inside class members should NOT propagate _this capture
    /// to the enclosing scope because the class IIFE creates its own scope
    /// and `class_es5_ir` handles _this capture independently.
    pub(super) in_es5_class: bool,
    /// Names that are re-exported via `export { Name }` (without a module specifier).
    /// Used to determine if a namespace/enum IIFE should fold exports into the
    /// closing argument (e.g., `(A || (exports.A = A = {}))`).
    pub(super) re_exported_names: rustc_hash::FxHashSet<String>,
    /// Stack of enclosing non-arrow function body node indices.
    /// When an arrow function captures `this`, the top of this stack is the
    /// scope that needs `var _this = this;`.
    pub(super) enclosing_function_bodies: Vec<NodeIndex>,
    /// Stack of capture variable names matching `enclosing_function_bodies`.
    /// Each entry is the name to use for `_this` capture in that scope
    /// (e.g., "_this" or "_`this_1`" if there's a collision with a user-defined `_this`).
    pub(super) enclosing_capture_names: Vec<Arc<str>>,
}

impl<'a> LoweringPass<'a> {
    /// Create a new lowering pass
    pub fn new(arena: &'a NodeArena, ctx: &'a EmitContext) -> Self {
        LoweringPass {
            arena,
            ctx,
            transforms: TransformContext::new(),
            commonjs_mode: false,
            has_export_assignment: false,
            visit_depth: 0,
            declared_names: rustc_hash::FxHashSet::default(),
            namespace_depth: 0,
            this_capture_level: 0,
            arguments_capture_level: 0,
            current_class_is_derived: false,
            in_constructor: false,
            in_static_context: false,
            current_class_alias: None,
            in_assignment_target: false,
            in_es5_class: false,
            re_exported_names: rustc_hash::FxHashSet::default(),
            enclosing_function_bodies: Vec::new(),
            enclosing_capture_names: Vec::new(),
        }
    }

    /// Run the lowering pass on a source file and return the transform context
    pub fn run(mut self, source_file: NodeIndex) -> TransformContext {
        self.init_module_state(source_file);
        // Push source file as the top-level _this capture scope
        if self.ctx.target_es5 {
            let capture_name = self.compute_this_capture_name(source_file);
            self.enclosing_function_bodies.push(source_file);
            self.enclosing_capture_names.push(capture_name);
        }
        self.visit(source_file);
        if self.ctx.target_es5 {
            self.enclosing_function_bodies.pop();
            self.enclosing_capture_names.pop();
        }
        self.maybe_wrap_module(source_file);
        self.transforms.mark_helpers_populated();

        if tracing::enabled!(tracing::Level::DEBUG) {
            let arrow_captures = self
                .transforms
                .iter()
                .filter_map(|(idx, directive)| match directive {
                    TransformDirective::ES5ArrowFunction {
                        arrow_node: _,
                        captures_this,
                        captures_arguments: _,
                        class_alias: _,
                    } => Some((idx, *captures_this)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            tracing::debug!(
                "[lowering] source={} arrow directives: {arrow_captures:?}",
                source_file.0
            );
            if let Some(capture_name) = self.transforms.this_capture_name(source_file) {
                tracing::debug!(
                    "[lowering] source {} this capture: {capture_name}",
                    source_file.0
                );
            } else {
                tracing::debug!("[lowering] source {} no this capture scope", source_file.0);
            }
        }

        self.transforms
    }

    /// Visit a node and its children
    pub(super) fn visit(&mut self, idx: NodeIndex) {
        // Stack overflow protection: limit recursion depth
        if self.visit_depth >= MAX_AST_DEPTH {
            return;
        }
        self.visit_depth += 1;

        let Some(node) = self.arena.get(idx) else {
            self.visit_depth -= 1;
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => self.visit_class_declaration(node, idx),
            k if k == syntax_kind_ext::CLASS_EXPRESSION => self.visit_class_expression(idx),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.visit_function_declaration(node, idx);
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.visit_function_expression(node, idx);
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.visit_arrow_function(node, idx),
            k if k == syntax_kind_ext::CONSTRUCTOR => self.visit_constructor(node, idx),
            k if k == syntax_kind_ext::CALL_EXPRESSION => self.visit_call_expression(node, idx),
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.visit_new_expression(node),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.visit_variable_statement(node, idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => self.visit_enum_declaration(node, idx),
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.visit_module_declaration(node, idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.visit_export_declaration(node, idx);
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.visit_import_declaration(node, idx);
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => self.visit_for_in_statement(node),
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => self.visit_for_of_statement(node, idx),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // If we're inside a capturing arrow function, substitute 'this' with '_this'
                if self.this_capture_level > 0 {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteThis { capture_name });
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if self.this_capture_level > 0
                    && let Some(text) = self.get_identifier_text_ref(idx)
                    && text == "this"
                {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteThis { capture_name });
                }

                // Check if this is the 'arguments' identifier
                if self.arguments_capture_level > 0
                    && let Some(text) = self.get_identifier_text_ref(idx)
                    && text == "arguments"
                {
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteArguments);
                }
            }
            _ => self.visit_children(idx),
        }

        self.visit_depth -= 1;
    }

    fn visit_for_in_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.visit(for_in_of.initializer);
        self.visit(for_in_of.expression);
        self.visit(for_in_of.statement);
    }

    fn visit_for_of_statement(&mut self, node: &Node, idx: NodeIndex) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };
        let should_lower_for_of_sync = self.ctx.target_es5 && !for_in_of.await_modifier;
        let should_lower_for_await_of =
            for_in_of.await_modifier && !self.ctx.options.target.supports_es2018();

        if should_lower_for_of_sync || should_lower_for_await_of {
            self.transforms
                .insert(idx, TransformDirective::ES5ForOf { for_of_node: idx });
            if for_in_of.await_modifier {
                self.transforms.helpers_mut().async_values = true;
            } else if self.ctx.options.downlevel_iteration {
                self.transforms.helpers_mut().values = true;
            }
        }

        // Check if initializer contains destructuring pattern
        // For-of initializer can be VARIABLE_DECLARATION_LIST with binding patterns
        let init_has_binding_pattern =
            self.for_of_initializer_has_binding_pattern(for_in_of.initializer);

        if init_has_binding_pattern {
            // Mark __read helper when destructuring is used with downlevelIteration
            // TypeScript emits __read to convert iterator results to arrays for destructuring
            if self.ctx.target_es5 && self.ctx.options.downlevel_iteration {
                self.transforms.helpers_mut().read = true;
            }
            // Set in_assignment_target to prevent spread in destructuring from triggering __spreadArray
            let prev = self.in_assignment_target;
            self.in_assignment_target = true;
            self.visit(for_in_of.initializer);
            self.in_assignment_target = prev;
        } else {
            self.visit(for_in_of.initializer);
        }
        self.visit(for_in_of.expression);
        self.visit(for_in_of.statement);
    }

    /// Visit a class declaration
    fn visit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_class_declaration(node, idx, false, false);
    }

    /// Visit a class expression.
    fn visit_class_expression(&mut self, idx: NodeIndex) {
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();

        self.in_static_context = false;
        self.visit_children(idx);

        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;
    }

    fn visit_enum_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_enum_declaration(node, idx, false);
    }

    fn visit_module_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_module_declaration(node, idx, false);
    }

    fn visit_import_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(import_decl) = self.arena.get_import_decl(node) else {
            return;
        };

        // Detect CommonJS helpers needed for imports
        if self.is_commonjs()
            && let Some(clause_node) = self.arena.get(import_decl.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
            && !clause.is_type_only
        {
            let empty_named_import_preserves_side_effects = clause.name.is_none()
                && clause.named_bindings.is_some()
                && self
                    .arena
                    .get(clause.named_bindings)
                    .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                    .is_some_and(|named_imports| {
                        named_imports.name.is_none() && named_imports.elements.nodes.is_empty()
                    });
            if !empty_named_import_preserves_side_effects
                && !self.ctx.options.verbatim_module_syntax
                && !self.import_has_value_usage_after_node(node, clause)
            {
                if import_decl.import_clause.is_some() {
                    self.visit(import_decl.import_clause);
                }
                return;
            }
            // __importDefault and __importStar helpers are only needed when
            // esModuleInterop is enabled.  Without it, namespace imports
            // compile to plain `require()` calls and default imports use
            // direct property access on the module object.
            if self.ctx.options.es_module_interop {
                let has_default = clause.name.is_some();
                let has_named_bindings = clause.named_bindings.is_some()
                    && self.arena.get(clause.named_bindings).is_some_and(|n| {
                        // Check for true named bindings (not namespace import)
                        n.kind != syntax_kind_ext::NAMESPACE_IMPORT
                            && self.arena.get_named_imports(n).is_some_and(|ni| {
                                ni.name.is_none() || !ni.elements.nodes.is_empty()
                            })
                    });

                // Combined default + named import (e.g., `import foo, {bar} from "mod"`)
                // requires __importStar to wrap the require call so both .default
                // and named exports are accessible.
                if has_default && has_named_bindings {
                    let helpers = self.transforms.helpers_mut();
                    helpers.import_star = true;
                    helpers.create_binding = true;
                } else if has_default {
                    // Default-only import: import d from "mod" -> needs __importDefault
                    let helpers = self.transforms.helpers_mut();
                    helpers.import_default = true;
                }

                // Namespace import: import * as ns from "mod" -> needs __importStar
                if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                    // NAMESPACE_IMPORT = 275
                    if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                        let helpers = self.transforms.helpers_mut();
                        helpers.import_star = true;
                        helpers.create_binding = true;
                    } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                        && named_imports.name.is_some()
                        && named_imports.elements.nodes.is_empty()
                    {
                        let helpers = self.transforms.helpers_mut();
                        helpers.import_star = true;
                        helpers.create_binding = true;
                    } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                    {
                        let has_default_named_import =
                            named_imports.elements.nodes.iter().any(|&spec_idx| {
                                self.arena.get(spec_idx).is_some_and(|spec_node| {
                                    self.arena.get_specifier(spec_node).is_some_and(|spec| {
                                        if spec.is_type_only {
                                            return false;
                                        }
                                        let import_name = if spec.property_name.is_some() {
                                            self.arena
                                                .get(spec.property_name)
                                                .and_then(|prop_node| {
                                                    self.arena.get_identifier(prop_node)
                                                })
                                                .map(|id| id.escaped_text.as_str())
                                        } else {
                                            self.arena
                                                .get(spec.name)
                                                .and_then(|name_node| {
                                                    self.arena.get_identifier(name_node)
                                                })
                                                .map(|id| id.escaped_text.as_str())
                                        };
                                        import_name == Some("default")
                                    })
                                })
                            });
                        if has_default_named_import {
                            let helpers = self.transforms.helpers_mut();
                            helpers.import_default = true;
                        }
                    }
                }
            }
        }

        // Continue traversal
        if import_decl.import_clause.is_some() {
            self.visit(import_decl.import_clause);
        }
    }

    fn import_has_value_usage_after_node(
        &self,
        node: &Node,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        let mut names = Vec::new();
        if clause.name.is_some() {
            let default_name = emit_utils::identifier_text_or_empty(self.arena, clause.name);
            if !default_name.is_empty() {
                names.push(default_name);
            }
        }
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
        {
            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                let ns_name = emit_utils::identifier_text_or_empty(self.arena, named_imports.name);
                if !ns_name.is_empty() {
                    names.push(ns_name);
                }
            } else {
                for &spec_idx in &named_imports.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if spec.is_type_only {
                        continue;
                    }
                    let local_name = emit_utils::identifier_text_or_empty(self.arena, spec.name);
                    if !local_name.is_empty() {
                        names.push(local_name);
                    }
                }
            }
        }
        if names.is_empty() {
            return true;
        }
        let Some(source_text) = self.arena.source_files.iter().find_map(|sf| {
            if (node.pos as usize) < sf.text.len() {
                Some(sf.text.as_ref())
            } else {
                None
            }
        }) else {
            return true;
        };
        // Use the module specifier end as the base offset, since node.end may
        // include trailing trivia that extends into the next statement.
        let mut start = if let Some(import_decl) = self.arena.get_import_decl(node)
            && let Some(module_node) = self.arena.get(import_decl.module_specifier)
        {
            module_node.end as usize
        } else {
            node.end as usize
        };
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        // Skip past the entire import line (including trailing comments)
        // to avoid matching identifiers in trailing comments like
        // `import { yield } from "m"; // error to use default as binding name`
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        let haystack = &source_text[start..];
        // Strip type-only content so identifiers in type positions
        // (type aliases, interfaces, type annotations, etc.) don't
        // cause unnecessary helper emission.
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);
        names
            .iter()
            .any(|name| crate::import_usage::contains_identifier_occurrence(&value_haystack, name))
    }

    fn visit_export_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return;
        };

        // Skip type-only exports
        if export_decl.is_type_only {
            return;
        }

        // Detect CommonJS helpers: export * from "mod"
        if self.is_commonjs()
            && export_decl.module_specifier.is_some()
            && export_decl.export_clause.is_none()
        {
            let helpers = self.transforms.helpers_mut();
            helpers.export_star = true;
            helpers.create_binding = true; // __exportStar depends on __createBinding
        }

        // Detect CommonJS helpers: export * as ns from "mod"
        // In CJS with esModuleInterop, this needs __importStar + __createBinding.
        if self.is_commonjs()
            && self.ctx.options.es_module_interop
            && export_decl.module_specifier.is_some()
            && export_decl.export_clause.is_some()
            && self.arena.get(export_decl.export_clause).is_some_and(|n| {
                n.kind != syntax_kind_ext::NAMED_EXPORTS
                    && n.kind != syntax_kind_ext::NAMESPACE_EXPORT
                    && n.kind != syntax_kind_ext::NAMED_IMPORTS
            })
        {
            let helpers = self.transforms.helpers_mut();
            helpers.import_star = true;
            helpers.create_binding = true;
        }

        // Detect CommonJS helpers: export { default } from "mod" or export { default as X } from "mod"
        // In CJS with esModuleInterop, re-exporting `default` needs __importDefault.
        if self.is_commonjs()
            && self.ctx.options.es_module_interop
            && export_decl.module_specifier.is_some()
            && let Some(clause_node) = self.arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named_exports) = self.arena.get_named_imports(clause_node)
        {
            let has_default_specifier = named_exports.elements.nodes.iter().any(|&spec_idx| {
                self.arena.get(spec_idx).is_some_and(|spec_node| {
                    self.arena.get_specifier(spec_node).is_some_and(|spec| {
                        if spec.is_type_only {
                            return false;
                        }
                        // For export specifiers, check property_name first (original name),
                        // then fall back to name (when there's no rename, name IS the original)
                        let check_idx = if spec.property_name.is_some()
                            && self.arena.get(spec.property_name).is_some()
                        {
                            spec.property_name
                        } else {
                            spec.name
                        };
                        self.arena.get(check_idx).is_some_and(|check_node| {
                            if check_node.kind == SyntaxKind::DefaultKeyword as u16 {
                                return true;
                            }
                            self.arena
                                .get_identifier(check_node)
                                .is_some_and(|id| id.escaped_text == "default")
                        })
                    })
                })
            });
            if has_default_specifier {
                let helpers = self.transforms.helpers_mut();
                helpers.import_default = true;
            }
        }

        if export_decl.export_clause.is_none() {
            return;
        }

        if export_decl.is_default_export
            && self.is_commonjs()
            && let Some(export_node) = self.arena.get(export_decl.export_clause)
        {
            if export_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.arena.get_function(export_node)
            {
                let is_anonymous = {
                    let func_name = self.get_identifier_text_ref(func.name).unwrap_or("");
                    func_name == "function" || !emit_utils::is_valid_identifier_name(func_name)
                };
                if is_anonymous {
                    let directive = self.commonjs_default_export_function_directive(
                        export_decl.export_clause,
                        func,
                    );
                    self.transforms.insert(export_decl.export_clause, directive);

                    if let Some(mods) = &func.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }

                    for &param_idx in &func.parameters.nodes {
                        self.visit(param_idx);
                    }

                    if func.body.is_some() {
                        self.visit(func.body);
                    }

                    return;
                }
            }

            if export_node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class) = self.arena.get_class(export_node)
            {
                let is_anonymous = {
                    let class_name = self.get_identifier_text_ref(class.name).unwrap_or("");
                    !emit_utils::is_valid_identifier_name(class_name)
                };
                if is_anonymous {
                    let heritage = self.get_extends_heritage(&class.heritage_clauses);
                    let directive = if self.ctx.target_es5 {
                        self.mark_class_helpers(export_decl.export_clause, heritage);
                        TransformDirective::CommonJSExportDefaultClassES5 {
                            class_node: export_decl.export_clause,
                        }
                    } else {
                        if self.ctx.needs_es2022_lowering && self.class_has_private_members(class) {
                            self.mark_class_helpers(export_decl.export_clause, heritage);
                        }
                        TransformDirective::CommonJSExportDefaultExpr
                    };
                    self.transforms.insert(export_decl.export_clause, directive);

                    if let Some(mods) = &class.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }

                    for &member_idx in &class.members.nodes {
                        self.visit(member_idx);
                    }

                    return;
                }
            }
        }

        let force_module_export = self.namespace_depth == 0;
        if let Some(export_node) = self.arena.get(export_decl.export_clause) {
            if export_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                self.lower_class_declaration(
                    export_node,
                    export_decl.export_clause,
                    force_module_export,
                    export_decl.is_default_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                self.lower_function_declaration(
                    export_node,
                    export_decl.export_clause,
                    force_module_export,
                    export_decl.is_default_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.lower_variable_statement(
                    export_node,
                    export_decl.export_clause,
                    force_module_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                self.lower_enum_declaration(
                    export_node,
                    export_decl.export_clause,
                    force_module_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.lower_module_declaration(
                    export_node,
                    export_decl.export_clause,
                    force_module_export,
                );
                return;
            }
        }

        self.visit(export_decl.export_clause);
    }

    fn commonjs_default_export_function_directive(
        &mut self,
        function_node: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> TransformDirective {
        let mut directives = Vec::new();
        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                // Async generators (async function*) need additional helpers
                if func.asterisk_token {
                    self.mark_async_generator_helpers();
                }
                directives.push(TransformDirective::ES5AsyncFunction { function_node });
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
                // Mark rest helper if parameters have rest
                if self.function_parameters_need_rest_helper(&func.parameters) {
                    self.transforms.helpers_mut().rest = true;
                }
                directives.push(TransformDirective::ES5FunctionParameters { function_node });
            }
        } else if self.ctx.needs_async_lowering && func.is_async {
            // ES2015/ES2016: async functions need __awaiter (generators are native)
            self.mark_async_helpers();
            if func.asterisk_token {
                self.mark_async_generator_helpers();
            }
        }

        directives.push(TransformDirective::CommonJSExportDefaultExpr);

        if directives.len() == 1 {
            directives
                .pop()
                .expect("commonjs default export directive should not be empty")
        } else {
            TransformDirective::Chain(directives)
        }
    }

    fn lower_class_declaration(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        force_export: bool,
        force_default: bool,
    ) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        if let Some(mods) = &class.modifiers {
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
        }

        // Skip ambient declarations (declare class)
        if self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
        {
            return;
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword));

        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let is_default = if force_export {
            force_default
        } else {
            self.arena
                .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword)
        };

        // Get class name only if we might need it for exports.
        let class_name = if is_exported && class.name.is_some() {
            self.get_identifier_id(class.name)
        } else {
            None
        };

        // Track class name for namespace/class merging detection
        if let Some(name) = self.get_identifier_text_ref(class.name) {
            self.declared_names.insert(name.to_string());
        }

        let heritage = self.get_extends_heritage(&class.heritage_clauses);
        if self.ctx.target_es5
            || (self.ctx.needs_es2022_lowering
                && (self.class_has_auto_accessor_members(class)
                    || self.class_has_private_members(class)))
        {
            self.mark_class_helpers(idx, heritage);
        }

        // TC39 (non-legacy) decorator detection
        // At ESNext, TC39 decorators are native syntax — no transform needed.
        let target_supports_native_decorators = self.ctx.options.target == ScriptTarget::ESNext;
        let has_tc39_decorators = !self.ctx.options.legacy_decorators
            && !target_supports_native_decorators
            && self.class_has_decorators(class);
        if has_tc39_decorators {
            let needs_prop_key = self.class_has_computed_decorated_member(class);
            let needs_set_function_name = self.class_has_private_decorated_member(class);
            // __setFunctionName is needed when there are class-level decorators
            // AND we're in ES2015 mode (IIFE pattern with __setFunctionName call).
            // In ES2022+ mode, it's not used for class decorators.
            let needs_class_set_fn_name = (self.ctx.target_es5 || self.ctx.needs_es2022_lowering)
                && class.modifiers.as_ref().is_some_and(|mods| {
                    mods.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                });
            let helpers = self.transforms.helpers_mut();
            helpers.es_decorate = true;
            helpers.run_initializers = true;
            if needs_prop_key {
                helpers.prop_key = true;
            }
            if needs_set_function_name || needs_class_set_fn_name {
                helpers.set_function_name = true;
            }
        }

        // Determine the base transform
        let needs_es5_transform = self.ctx.target_es5;
        let can_use_simple_es5_tc39 = has_tc39_decorators
            && needs_es5_transform
            && class.members.nodes.is_empty()
            && class.heritage_clauses.is_none();
        let base_directive =
            if has_tc39_decorators && (!needs_es5_transform || can_use_simple_es5_tc39) {
                // TC39 decorator transform (ES2015+ targets, below ESNext)
                TransformDirective::TC39Decorators {
                    class_node: idx,
                    function_name: None,
                }
            } else if needs_es5_transform {
                // ES5 class transform
                TransformDirective::ES5Class {
                    class_node: idx,
                    heritage,
                }
            } else {
                // No transform needed for ES6+ targets
                TransformDirective::Identity
            };

        // Wrap with CommonJS export if needed
        let final_directive = if is_exported {
            if let Some(export_name) = class_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        // Only register non-identity transforms
        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        // Save and set current_class_is_derived state for super detection
        let prev_is_derived = self.current_class_is_derived;
        self.current_class_is_derived = heritage.is_some();

        // Generate class alias for static members (e.g., "_a" for "Vector")
        let class_alias = if self.ctx.target_es5 {
            self.get_identifier_text_ref(class.name).map(|name| {
                // Generate a unique alias based on class name
                // For now, use the first letter + underscore pattern
                let first_char = name.chars().next().unwrap_or('_');
                format!("_{}", first_char.to_lowercase().collect::<String>())
            })
        } else {
            None
        };

        // Save previous static context
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();

        // Nested classes create a fresh `this`/class-alias boundary. Only the nested
        // class's own static members should re-enable static context while traversing.
        self.in_static_context = false;
        self.current_class_alias = None;

        // In ES5 mode, class members are emitted inside a class IIFE.
        // Arrow functions in property initializers/methods should NOT propagate
        // _this capture to the enclosing scope — the class_es5_ir handles
        // _this capture independently within the constructor/method bodies.
        let prev_in_es5_class = self.in_es5_class;
        let prev_capture_level = self.this_capture_level;
        let prev_args_capture_level = self.arguments_capture_level;
        if self.ctx.target_es5 {
            self.in_es5_class = true;
            self.this_capture_level = 0;
            self.arguments_capture_level = 0;
        }

        // Visit children (members) with static context tracking
        for &member_idx in &class.members.nodes {
            // Check if this member is static
            let is_static = self.is_static_member(member_idx);

            if is_static {
                self.in_static_context = true;
                self.current_class_alias = class_alias.clone();
            }

            self.visit(member_idx);

            if is_static {
                self.in_static_context = false;
                self.current_class_alias.take();
            }
        }

        // When a class has class-level legacy decorators and emitDecoratorMetadata is
        // enabled, the __metadata helper is needed for constructor paramtypes even if
        // no individual member is decorated. The member-level decorator visitor only
        // sets helpers.metadata for decorated properties/methods, so we must also
        // check here for the class-level decorator + constructor case.
        if self.ctx.options.legacy_decorators
            && self.ctx.options.emit_decorator_metadata
            && class.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            })
            && class.members.nodes.iter().any(|&m_idx| {
                self.arena
                    .get(m_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CONSTRUCTOR)
            })
        {
            self.transforms.helpers_mut().metadata = true;
        }

        // Restore previous state
        self.current_class_is_derived = prev_is_derived;
        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;

        // Restore _this capture state (undo the class barrier)
        if self.ctx.target_es5 {
            self.in_es5_class = prev_in_es5_class;
            self.this_capture_level = prev_capture_level;
            self.arguments_capture_level = prev_args_capture_level;
        }
    }

    fn lower_function_declaration(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        force_export: bool,
        force_default: bool,
    ) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Save and reset in_constructor state for nested function scope
        // Regular functions create a new scope, so in_constructor should be false inside them
        let prev_in_constructor = self.in_constructor;
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();
        self.in_constructor = false;
        self.in_static_context = false;

        if let Some(mods) = &func.modifiers {
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || self
                    .arena
                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword));
        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let is_default = if force_export {
            force_default
        } else {
            self.arena
                .has_modifier(&func.modifiers, SyntaxKind::DefaultKeyword)
        };

        let func_name = if is_exported && func.name.is_some() {
            self.get_identifier_id(func.name)
        } else {
            None
        };

        // Track function name for namespace/function merging detection
        if let Some(name) = self.get_identifier_text_ref(func.name) {
            self.declared_names.insert(name.to_string());
        }

        // Check if this is an async function needing lowering (target < ES2017)
        let base_directive = if self.ctx.needs_async_lowering && self.has_async_modifier(idx) {
            if func.asterisk_token {
                // Async generators: at ES2015+ use __asyncGenerator + __await (no __awaiter).
                // At ES5, also need __awaiter + __generator for the outer wrapper.
                if self.ctx.target_es5 {
                    self.mark_async_helpers();
                }
                self.mark_async_generator_helpers();
            } else {
                self.mark_async_helpers();
            }
            TransformDirective::ES5AsyncFunction { function_node: idx }
        } else if self.ctx.target_es5
            && self.function_parameters_need_es5_transform(&func.parameters)
        {
            // Mark rest helper if parameters have rest
            if self.function_parameters_need_rest_helper(&func.parameters) {
                self.transforms.helpers_mut().rest = true;
            }
            TransformDirective::ES5FunctionParameters { function_node: idx }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = func_name {
                if is_default {
                    // Default exports need explicit exports.default = name;
                    let export_directive = TransformDirective::CommonJSExport {
                        names: Arc::from(vec![export_name]),
                        is_default,
                        inner: Box::new(TransformDirective::Identity),
                    };

                    match base_directive {
                        TransformDirective::Identity => export_directive,
                        other => TransformDirective::Chain(vec![other, export_directive]),
                    }
                } else {
                    // Named function exports: emit exports.f = f; after the declaration
                    let export_directive = TransformDirective::CommonJSExport {
                        names: Arc::from(vec![export_name]),
                        is_default: false,
                        inner: Box::new(TransformDirective::Identity),
                    };

                    match base_directive {
                        TransformDirective::Identity => export_directive,
                        other => TransformDirective::Chain(vec![other, export_directive]),
                    }
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if func.body.is_some() {
            // Track this function body as a potential _this capture scope
            if self.ctx.target_es5 {
                let cn =
                    self.compute_this_capture_name_with_params(func.body, Some(&func.parameters));
                self.enclosing_function_bodies.push(func.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(func.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore in_constructor state
        self.in_constructor = prev_in_constructor;
        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;
    }

    fn lower_enum_declaration(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(enum_decl) = self.arena.get_enum(node) else {
            return;
        };

        // Skip ambient and const enums (declare/const enums are erased)
        if self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
            || self.has_const_modifier(&enum_decl.modifiers)
        {
            return;
        }

        // Check if exported directly, via force_export, or via re-export (`export { Name }`)
        let re_exported = self
            .get_identifier_text_ref(enum_decl.name)
            .is_some_and(|n| self.re_exported_names.contains(n));
        let is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || re_exported
                || self
                    .arena
                    .has_modifier(&enum_decl.modifiers, SyntaxKind::ExportKeyword));

        let enum_name = if is_exported && enum_decl.name.is_some() {
            self.get_identifier_id(enum_decl.name)
        } else {
            None
        };

        // Track enum name for namespace/enum merging detection
        if let Some(name) = self.get_identifier_text_ref(enum_decl.name) {
            self.declared_names.insert(name.to_string());
        }

        let base_directive = if self.ctx.target_es5 {
            TransformDirective::ES5Enum { enum_node: idx }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = enum_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default: false,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        for &member_idx in &enum_decl.members.nodes {
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.visit(member.name);
                if member.initializer.is_some() {
                    self.visit(member.initializer);
                }
            }
        }
    }

    fn lower_module_declaration(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(module_decl) = self.arena.get_module(node) else {
            return;
        };

        // Skip ambient declarations (declare namespace/module)
        if self
            .arena
            .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
        {
            return;
        }

        // Get the namespace root name for merging detection
        let namespace_name = self.get_module_root_name_text(module_decl.name);

        // Check if this name has already been declared (class/enum/function/namespace)
        // If so, we should NOT emit 'var' for this namespace
        let should_declare_var = if let Some(ref name) = namespace_name {
            !self.declared_names.contains(name)
        } else {
            true
        };

        // Check if exported via re-export (`export { Name }`)
        let re_exported = namespace_name
            .as_ref()
            .is_some_and(|n| self.re_exported_names.contains(n));

        // Track this name as declared
        if let Some(name) = namespace_name {
            self.declared_names.insert(name);
        }
        let is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || re_exported
                || self
                    .arena
                    .has_modifier(&module_decl.modifiers, SyntaxKind::ExportKeyword));

        let module_name = if is_exported {
            self.get_module_root_name(module_decl.name)
        } else {
            None
        };

        let base_directive = if self.ctx.target_es5 {
            TransformDirective::ES5Namespace {
                namespace_node: idx,
                should_declare_var,
            }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = module_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default: false,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        // Recurse into namespace body to detect helpers needed by nested declarations
        // (e.g., classes with extends need __extends, async functions need __awaiter)
        self.namespace_depth += 1;
        self.visit_module_body(module_decl.body);
        self.namespace_depth -= 1;
    }

    /// Recursively visit module/namespace body statements to detect helper requirements
    fn visit_module_body(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };

        if let Some(block_data) = self.arena.get_module_block(body_node) {
            if let Some(ref stmts) = block_data.statements {
                for &stmt_idx in &stmts.nodes {
                    self.visit(stmt_idx);
                }
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested namespace: `namespace A.B { ... }` — recurse into inner body
            if let Some(inner_module) = self.arena.get_module(body_node) {
                self.visit_module_body(inner_module.body);
            }
        }
    }

    /// Visit a function declaration
    fn visit_function_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_function_declaration(node, idx, false, false);
    }

    /// Visit an arrow function
    fn visit_arrow_function(&mut self, node: &Node, idx: NodeIndex) {
        let Some(arrow) = self.arena.get_function(node) else {
            return;
        };

        if self.ctx.target_es5 {
            let malformed_return_type = arrow.type_annotation.is_some()
                && self
                    .arena
                    .get(arrow.type_annotation)
                    .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);

            if self.is_recovery_malformed_arrow(node) || malformed_return_type {
                for &param_idx in &arrow.parameters.nodes {
                    self.visit(param_idx);
                }
                if arrow.body.is_some() {
                    self.visit(arrow.body);
                }
                return;
            }

            let captures_this = contains_this_reference(self.arena, idx);
            let captures_arguments = contains_arguments_reference(self.arena, idx);

            tracing::debug!(
                "[lowering][arrow] idx={} captures_this={captures_this} is_async={}",
                idx.0,
                arrow.is_async
            );

            // For static members, use class alias capture instead of IIFE
            let class_alias = if self.in_static_context && captures_this {
                self.current_class_alias.clone()
            } else {
                None
            };

            self.transforms.insert(
                idx,
                TransformDirective::ES5ArrowFunction {
                    arrow_node: idx,
                    captures_this,
                    captures_arguments,
                    class_alias: class_alias.map(std::convert::Into::into),
                },
            );

            if arrow.is_async {
                self.mark_async_helpers();
            }

            // If this arrow function captures 'this', increment the capture level
            // so that nested 'this' references get substituted.
            // Also mark the enclosing function body so the emitter inserts
            // `var _this = this;` at the start of that scope.
            // But NOT when inside an ES5 class — class_es5_ir handles _this
            // capture independently within constructor/method bodies.
            if captures_this {
                self.this_capture_level += 1;
                if !self.in_es5_class
                    && let Some(&enclosing_body) = self.enclosing_function_bodies.last()
                {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .mark_this_capture_scope(enclosing_body, capture_name);
                }
            }

            // If this arrow function captures 'arguments', increment the capture level
            // so that nested 'arguments' references get substituted
            if captures_arguments {
                self.arguments_capture_level += 1;
            }
        } else if self.ctx.needs_async_lowering && arrow.is_async {
            // ES2015/ES2016: arrow syntax is native but async needs lowering
            self.mark_async_helpers();
        }

        for &param_idx in &arrow.parameters.nodes {
            self.visit(param_idx);
        }

        if arrow.body.is_some() {
            self.visit(arrow.body);
        }

        // Restore capture level after visiting the arrow function body
        if self.ctx.target_es5 {
            let captures_this = contains_this_reference(self.arena, idx);
            if captures_this {
                self.this_capture_level -= 1;
            }

            let captures_arguments = contains_arguments_reference(self.arena, idx);
            if captures_arguments {
                self.arguments_capture_level -= 1;
            }
        }
    }

    fn is_recovery_malformed_arrow(&self, node: &Node) -> bool {
        let start = node.pos as usize;
        let end = node.end as usize;

        self.arena.source_files.iter().any(|sf| {
            if start < sf.text.len() && start < end {
                let window_start = start.saturating_sub(8);
                let window_end = (end + 8).min(sf.text.len());
                let slice = &sf.text[window_start..window_end];
                slice.contains("): =>") || slice.contains("):=>")
            } else {
                false
            }
        })
    }

    /// Visit a constructor declaration
    fn visit_constructor(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(ctor) = self.arena.get_constructor(node) else {
            return;
        };

        // Save previous state
        let prev_in_constructor = self.in_constructor;
        // Set new state - we're now inside a constructor
        self.in_constructor = true;

        // Visit children (modifiers, parameters, body).
        // Save/restore the decorate flag — constructor decorators are errors and
        // tsc doesn't emit __decorate helpers for them.
        if let Some(mods) = &ctor.modifiers {
            let prev_decorate = self.transforms.helpers().decorate;
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
            self.transforms.helpers_mut().decorate = prev_decorate;
        }
        for &param_idx in &ctor.parameters.nodes {
            self.visit(param_idx);
        }
        if ctor.body.is_some() {
            if self.ctx.target_es5 {
                let cn = self.compute_this_capture_name(ctor.body);
                self.enclosing_function_bodies.push(ctor.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(ctor.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore state
        self.in_constructor = prev_in_constructor;
    }

    /// Visit a call expression and detect `super()` calls
    fn visit_call_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Check if this is a super() call
        let is_super_call = if let Some(expr_node) = self.arena.get(call.expression) {
            expr_node.kind == SyntaxKind::SuperKeyword as u16
        } else {
            false
        };

        // Emit directive if conditions met:
        // 1. This is a super(...) call
        // 2. Target is ES5
        // 3. We're inside a constructor
        // 4. The current class has a base class (is_derived)
        if is_super_call
            && self.ctx.target_es5
            && self.in_constructor
            && self.current_class_is_derived
        {
            self.transforms
                .insert(idx, TransformDirective::ES5SuperCall);
        }

        // CJS dynamic import: import("mod") needs __importStar helper
        // This applies regardless of esModuleInterop setting.
        // Skip for node module CJS files where native import() is supported.
        if self.commonjs_mode
            && !self.ctx.options.resolved_node_module_to_cjs
            && !is_super_call
            && let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            let helpers = self.transforms.helpers_mut();
            helpers.import_star = true;
            helpers.create_binding = true;
        }

        // Check if call has spread arguments and needs ES5 transformation
        if self.ctx.target_es5
            && !is_super_call
            && let Some(ref args) = call.arguments
        {
            let has_spread = args
                .nodes
                .iter()
                .any(|&arg_idx| emit_utils::is_spread_element(self.arena, arg_idx));
            if has_spread {
                self.transforms
                    .insert(idx, TransformDirective::ES5CallSpread { call_expr: idx });
                // __spreadArray is only needed when spread arguments must be merged
                // with additional segments (not for plain foo(...args)).
                if self.call_spread_needs_spread_array(args.nodes.as_slice()) {
                    self.transforms.helpers_mut().spread_array = true;
                    // When downlevelIteration is enabled, spread on iterables
                    // needs __read to convert iterator results to arrays.
                    if self.ctx.options.downlevel_iteration {
                        self.transforms.helpers_mut().read = true;
                    }
                }
            }
        }

        // Continue traversal
        self.visit(call.expression);
        if let Some(ref args) = call.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a new expression and traverse callee + arguments for nested transforms.
    fn visit_new_expression(&mut self, node: &Node) {
        let Some(new_expr) = self.arena.get_call_expr(node) else {
            return;
        };

        self.visit(new_expr.expression);
        if let Some(ref args) = new_expr.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a variable statement
    fn visit_variable_statement(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_variable_statement(node, idx, false);
    }

    fn lower_variable_statement(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };

        let is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword));

        if is_exported {
            let export_names = self.collect_variable_names(&var_stmt.declarations);
            if !export_names.is_empty() {
                self.transforms.insert(
                    idx,
                    TransformDirective::CommonJSExport {
                        names: Arc::from(export_names),
                        is_default: false,
                        inner: Box::new(TransformDirective::Identity),
                    },
                );
            }
        }

        // Visit each declaration
        for &decl in &var_stmt.declarations.nodes {
            self.visit(decl);
        }
    }

    fn visit_function_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Save and reset in_constructor state for nested function scope
        let prev_in_constructor = self.in_constructor;
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();
        self.in_constructor = false;
        self.in_static_context = false;

        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                if func.asterisk_token {
                    self.mark_async_generator_helpers();
                }
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5AsyncFunction { function_node: idx },
                );
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
                // Mark rest helper if parameters have rest
                if self.function_parameters_need_rest_helper(&func.parameters) {
                    self.transforms.helpers_mut().rest = true;
                }
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5FunctionParameters { function_node: idx },
                );
            }
        } else if self.ctx.needs_async_lowering && func.is_async {
            if func.asterisk_token {
                // ES2015+: async generators need __asyncGenerator + __await helpers
                self.mark_async_generator_helpers();
            } else {
                // ES2015/ES2016: non-generator async functions need __awaiter
                self.mark_async_helpers();
            }
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if func.body.is_some() {
            // Track this function body as a potential _this capture scope
            if self.ctx.target_es5 {
                let cn =
                    self.compute_this_capture_name_with_params(func.body, Some(&func.parameters));
                self.enclosing_function_bodies.push(func.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(func.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore in_constructor state
        self.in_constructor = prev_in_constructor;
        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;
    }
}

#[cfg(test)]
#[path = "../../tests/lowering_pass.rs"]
mod tests;
