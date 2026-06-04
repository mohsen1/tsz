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
            re_exported_export_names: rustc_hash::FxHashMap::default(),
            all_export_aliases_in_order: rustc_hash::FxHashMap::default(),
            enclosing_function_bodies: Vec::new(),
            enclosing_capture_names: Vec::new(),
            current_source_text: None,
            current_jsx_pragmas: JsxPragmaFacts::default(),
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

        // Forward the foldable alias map so re-export clauses emitted before
        // their enum declaration suppress the would-be `exports.<alias> = X;`
        // line that otherwise reads `X` in its TDZ window.
        if self.commonjs_mode && !self.all_export_aliases_in_order.is_empty() {
            let folded: rustc_hash::FxHashMap<String, Vec<String>> = self
                .all_export_aliases_in_order
                .iter()
                .filter_map(|(local, alias_ids)| {
                    let aliases: Vec<String> = alias_ids
                        .iter()
                        .filter_map(|id| {
                            self.arena
                                .identifiers
                                .get(*id as usize)
                                .map(|ident| ident.escaped_text.clone())
                        })
                        .filter(|name| !name.is_empty())
                        .collect();
                    if aliases.is_empty() {
                        None
                    } else {
                        Some((local.clone(), aliases))
                    }
                })
                .collect();
            self.transforms.set_cjs_iife_folded_bindings(folded);
        }

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

    /// Run the emit planning pass on a source file.
    ///
    /// This is the direct-to-target planning boundary. It currently wraps the
    /// existing transform directives while follow-up work migrates helper,
    /// hoist, export, temp, and region facts into `EmitPlan`.
    pub fn run_plan(self, source_file: NodeIndex) -> EmitPlan {
        let options = self.ctx.options.clone();
        let transforms = self.run(source_file);
        EmitPlanBuilder::new(&options)
            .with_transforms(transforms)
            .build()
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
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.visit_new_expression(node, idx),
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
                self.transforms.helpers_mut().mark_async_values();
            } else if self.ctx.options.downlevel_iteration {
                self.transforms.helpers_mut().mark_values();
            }
        }
        if !self.ctx.options.target.supports_es2025()
            && crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
                .is_some()
        {
            self.transforms.helpers_mut().add_disposable_resource = true;
            self.transforms.helpers_mut().dispose_resources = true;
        }

        // Check if initializer contains destructuring pattern
        // For-of initializer can be VARIABLE_DECLARATION_LIST with binding patterns
        let init_has_binding_pattern =
            self.for_of_initializer_has_binding_pattern(for_in_of.initializer);

        // A for-of whose initializer is a bare expression (not a
        // VARIABLE_DECLARATION_LIST) is a destructuring-ASSIGNMENT target, e.g.
        // `for ([a, ...rest] of xs)` / `for ({ a = d } of xs)`. The LHS
        // array/object literal is a pattern, not a fresh array/object, so the
        // in-assignment-target flag must be set during its visit. Otherwise a
        // spread element like `[...rest]` is mis-classified as array
        // construction and spuriously pulls in the `__spreadArray` helper.
        let init_is_assignment_target =
            self.for_of_initializer_is_assignment_target(for_in_of.initializer);

        if init_has_binding_pattern || init_is_assignment_target {
            // Mark __read helper when binding-pattern destructuring is used with
            // downlevelIteration. TypeScript emits __read to convert iterator
            // results to arrays for binding-pattern destructuring. (Bare
            // assignment targets keep the existing array-indexing lowering and
            // do not add __read here.)
            if init_has_binding_pattern
                && self.ctx.target_es5
                && self.ctx.options.downlevel_iteration
            {
                self.transforms.helpers_mut().mark_read();
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

    fn visit_export_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return;
        };

        // Skip type-only exports
        if export_decl.is_type_only {
            return;
        }

        let is_top_level_export = self.namespace_depth == 0;

        // Detect CommonJS helpers: export * from "mod"
        if is_top_level_export
            && self.is_commonjs()
            && export_decl.module_specifier.is_some()
            && export_decl.export_clause.is_none()
        {
            let helpers = self.transforms.helpers_mut();
            helpers.export_star = true;
            helpers.create_binding = true; // __exportStar depends on __createBinding
        }

        // Detect CommonJS helpers: export * as ns from "mod"
        // In CJS with esModuleInterop, this needs __importStar + __createBinding.
        if is_top_level_export
            && self.is_commonjs()
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
        if is_top_level_export
            && self.is_commonjs()
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
                helpers.mark_import_default();
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
                        let target_supports_native_decorators = self.ctx.options.target
                            == ScriptTarget::ESNext
                            && self.ctx.options.use_define_for_class_fields;
                        if !self.ctx.options.legacy_decorators
                            && !target_supports_native_decorators
                            && self.class_has_decorators(class)
                        {
                            self.mark_tc39_decorator_helpers(class);
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
                self.mark_function_parameter_transform_helpers(&func.parameters);
                if func.asterisk_token {
                    self.mark_async_generator_helpers();
                } else {
                    self.mark_async_helpers();
                }
                directives.push(TransformDirective::ES5AsyncFunction { function_node });
            } else if func.asterisk_token {
                self.transforms.helpers_mut().generator = true;
                self.mark_function_parameter_transform_helpers(&func.parameters);
                directives.push(TransformDirective::ES5GeneratorFunction { function_node });
            } else if self.function_parameters_need_body_prologue_transform(&func.parameters) {
                self.mark_function_parameter_transform_helpers(&func.parameters);
                directives.push(TransformDirective::ES5FunctionParameters { function_node });
            }
        } else if func.is_async
            && ((func.asterisk_token && self.ctx.needs_es2018_lowering)
                || (!func.asterisk_token && self.ctx.needs_async_lowering))
        {
            if func.asterisk_token {
                self.mark_async_generator_helpers();
            } else {
                // ES2015/ES2016: async functions need __awaiter (generators are native)
                self.mark_async_helpers();
            }
        } else if self.function_parameters_need_body_prologue_transform(&func.parameters) {
            self.mark_function_parameter_transform_helpers(&func.parameters);
            directives.push(TransformDirective::ES5FunctionParameters { function_node });
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
        if self.arena.is_declare(&class.modifiers) {
            return;
        }

        let re_exported = self.ctx.options.target == ScriptTarget::ES5
            && self
                .get_identifier_text_ref(class.name)
                .is_some_and(|n| self.re_exported_names.contains(n));

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || re_exported
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

        // TC39 (non-legacy) decorator detection.
        // At ESNext, TC39 decorators are native syntax only when class fields
        // can stay native too. With useDefineForClassFields=false, class
        // initialization semantics still need lowering, so decorators must be
        // lowered with the class elements they initialize.
        let target_supports_native_decorators = self.ctx.options.target == ScriptTarget::ESNext
            && self.ctx.options.use_define_for_class_fields;
        let has_tc39_decorators = !self.ctx.options.legacy_decorators
            && !target_supports_native_decorators
            && self.class_has_decorators(class);
        let has_tc39_class_decorators = has_tc39_decorators
            && class.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
        let has_legacy_class_decorators = self.ctx.options.legacy_decorators
            && class.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
        let target_needs_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;
        let has_lowered_static_field = target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member_node| {
                    member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self
                            .arena
                            .get_property_decl(member_node)
                            .is_some_and(|prop| {
                                self.has_class_member_modifier(
                                    &prop.modifiers,
                                    SyntaxKind::StaticKeyword as u16,
                                ) && !self.has_class_member_modifier(
                                    &prop.modifiers,
                                    SyntaxKind::AbstractKeyword as u16,
                                ) && !self.has_class_member_modifier(
                                    &prop.modifiers,
                                    SyntaxKind::DeclareKeyword as u16,
                                ) && !prop.initializer.is_none()
                            })
                })
            });
        if has_legacy_class_decorators
            && is_default
            && class.name.is_none()
            && has_lowered_static_field
        {
            self.transforms.helpers_mut().set_function_name = true;
        }
        if has_tc39_decorators {
            self.mark_tc39_decorator_helpers(class);
            if self.ctx.target_es5
                && !has_tc39_class_decorators
                && self.class_has_static_tc39_public_field_decorator(class)
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
                let local_name = self.get_identifier_text_ref(class.name);
                let export_names = self.commonjs_export_names_for_local(local_name, export_name);
                let export_directive = TransformDirective::CommonJSExport {
                    names: export_names,
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
        let base_directive = if self.has_async_modifier(idx)
            && ((func.asterisk_token && self.ctx.needs_es2018_lowering)
                || (!func.asterisk_token && self.ctx.needs_async_lowering))
        {
            if func.asterisk_token {
                self.mark_async_generator_helpers();
            } else {
                self.mark_async_helpers();
            }
            self.mark_function_parameter_transform_helpers(&func.parameters);
            TransformDirective::ES5AsyncFunction { function_node: idx }
        } else if self.ctx.target_es5 && func.asterisk_token {
            self.transforms.helpers_mut().generator = true;
            self.mark_function_parameter_transform_helpers(&func.parameters);
            TransformDirective::ES5GeneratorFunction { function_node: idx }
        } else if self.function_parameters_need_body_prologue_transform(&func.parameters) {
            self.mark_function_parameter_transform_helpers(&func.parameters);
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
        if self.arena.is_declare(&enum_decl.modifiers)
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
                // Carry every export alias attached to this enum's local name
                // (direct `export enum X` + later `export { X as Y }` clauses)
                // so the IIFE-tail fold can build the full
                // `(X || (exports.Y = exports.X = X = {}))` chain.
                let local_name = self.get_identifier_text_ref(enum_decl.name);
                let export_names = self.commonjs_export_names_for_local(local_name, export_name);
                let export_directive = TransformDirective::CommonJSExport {
                    names: export_names,
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
        if self.arena.is_declare(&module_decl.modifiers) {
            return;
        }

        // Get the namespace root name for merging detection
        let namespace_name = self.get_module_root_name_text(module_decl.name);
        let namespace_has_runtime_value =
            emit_utils::module_body_has_runtime_value_declarations(self.arena, module_decl.body);

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
        if namespace_has_runtime_value && let Some(ref name) = namespace_name {
            self.declared_names.insert(name.clone());
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
                let export_names =
                    self.commonjs_export_names_for_local(namespace_name.as_deref(), export_name);
                let export_directive = TransformDirective::CommonJSExport {
                    names: export_names,
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
        // (e.g., classes with extends need __extends, async functions need __awaiter).
        // Save/restore `declared_names`: each namespace IIFE creates a new function
        // scope, so names declared inside (nested namespaces, enums, etc.) must not
        // leak out and suppress outer-scope `var` declarations of same-named siblings.
        self.namespace_depth += 1;
        let prev_declared = std::mem::take(&mut self.declared_names);
        self.visit_module_body(module_decl.body);
        self.declared_names = prev_declared;
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

        let mut es5_captures_this = false;
        let mut es5_captures_arguments = false;

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

            let contains_this = arrow_captures_lexical_this(self.arena, idx);
            let async_arrow_needs_awaiter_this =
                arrow.is_async && self.enclosing_function_bodies.len() > 1;
            let captures_this = contains_this || async_arrow_needs_awaiter_this;
            let captures_arguments = contains_arguments_reference(self.arena, idx);
            es5_captures_this = captures_this;
            es5_captures_arguments = captures_arguments;

            tracing::debug!(
                "[lowering][arrow] idx={} contains_this={contains_this} captures_this={captures_this} is_async={}",
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
            self.mark_function_parameter_transform_helpers(&arrow.parameters);

            // If this arrow function captures lexical `this`, increment the
            // capture level so that nested `this` references get substituted.
            // Also mark the enclosing function body so the emitter inserts
            // `var _this = this;` at the start of that scope.
            // Async arrows need the lexical thisArg passed to `__awaiter` even
            // when the generator body does not spell `this`.
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
        } else if !arrow.is_async
            && self.function_parameters_need_body_prologue_transform(&arrow.parameters)
        {
            self.mark_function_parameter_transform_helpers(&arrow.parameters);
            self.transforms.insert(
                idx,
                TransformDirective::ES5FunctionParameters { function_node: idx },
            );
        }

        for &param_idx in &arrow.parameters.nodes {
            self.visit(param_idx);
        }

        if arrow.body.is_some() {
            self.visit(arrow.body);
        }

        // Restore capture level after visiting the arrow function body
        if self.ctx.target_es5 {
            if es5_captures_this {
                self.this_capture_level -= 1;
            }

            if es5_captures_arguments {
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
        if ctor.body.is_some() {
            self.mark_function_parameter_transform_helpers(&ctor.parameters);
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
}
