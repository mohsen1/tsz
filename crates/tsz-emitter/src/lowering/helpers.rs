//! Helper methods for the lowering pass.
//!
//! Contains module initialization, modifier checking, helper detection,
//! binding pattern analysis, and this-capture computation.

use super::*;
use crate::emitter::JsxEmit;
use crate::transforms::emit_utils;
use tsz_parser::parser::node::NodeAccess;

impl<'a> LoweringPass<'a> {
    // =========================================================================
    // Helper Methods
    // =========================================================================

    pub(super) fn init_module_state(&mut self, source_file: NodeIndex) {
        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        self.has_export_assignment = self.contains_export_assignment(&source.statements);
        // AMD/UMD wrapper bodies are processed as CJS (the wrapper provides
        // `exports` parameter), so the lowering pass must produce CommonJSExport
        // directives for them just like it does for CommonJS module kind.
        self.commonjs_mode = if self.ctx.is_commonjs()
            || matches!(self.ctx.options.module, ModuleKind::AMD | ModuleKind::UMD)
        {
            true
        } else if self.ctx.auto_detect_module && matches!(self.ctx.options.module, ModuleKind::None)
        {
            self.file_is_module(&source.statements)
        } else {
            false
        };

        // Pre-scan for `export { Name }` re-exports (without module specifier).
        // These names need the IIFE export fold even though their declarations
        // don't have the `export` keyword directly.
        if self.commonjs_mode {
            self.collect_re_exported_names(&source.statements);
            self.collect_all_export_aliases_in_order(&source.statements);
        }
    }

    /// Walk source-order statements once and record every export alias
    /// attached to local enum/namespace IIFE bindings so the emitter can fold
    /// every alias into the IIFE tail.
    fn collect_all_export_aliases_in_order(&mut self, statements: &tsz_parser::parser::NodeList) {
        let mut foldable_locals: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        for &stmt_idx in &statements.nodes {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let foldable_iife_idx = if self.is_foldable_iife_declaration(node) {
                Some(stmt_idx)
            } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.export_decl_wraps_foldable_iife(node)
            } else {
                None
            };
            if let Some(iife_idx) = foldable_iife_idx
                && let Some(local) = self.foldable_iife_local_name(iife_idx)
            {
                foldable_locals.insert(local);
            }
        }

        if foldable_locals.is_empty() {
            return;
        }

        self.collect_foldable_export_aliases(statements, &foldable_locals);
    }

    /// Return the inner declaration index when `export_decl_node` wraps a
    /// foldable enum or instantiated namespace IIFE declaration.
    fn export_decl_wraps_foldable_iife(
        &self,
        export_decl_node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        let export_decl = self.arena.get_export_decl(export_decl_node)?;
        if export_decl.module_specifier.is_some()
            || export_decl.is_type_only
            || export_decl.is_default_export
        {
            return None;
        }
        let inner_node = self.arena.get(export_decl.export_clause)?;
        self.is_foldable_iife_declaration(inner_node)
            .then_some(export_decl.export_clause)
    }

    fn is_foldable_iife_declaration(&self, node: &tsz_parser::parser::node::Node) -> bool {
        match node.kind {
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|enum_decl| {
                    !self.arena.is_declare(&enum_decl.modifiers)
                        && !self.has_const_modifier(&enum_decl.modifiers)
                })
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(node).is_some_and(|module_decl| {
                    !self.arena.is_declare(&module_decl.modifiers)
                        && emit_utils::module_body_has_runtime_value_declarations(
                            self.arena,
                            module_decl.body,
                        )
                })
            }
            _ => false,
        }
    }

    fn foldable_iife_local_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(node)?;
                self.get_identifier_text_ref(enum_decl.name)
                    .map(str::to_string)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module_decl = self.arena.get_module(node)?;
                self.get_module_root_name_text(module_decl.name)
            }
            _ => None,
        }
    }

    fn foldable_iife_export_id(&self, idx: NodeIndex) -> Option<IdentifierId> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(node)?;
                self.get_identifier_id(enum_decl.name)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module_decl = self.arena.get_module(node)?;
                self.get_module_root_name(module_decl.name)
            }
            _ => None,
        }
    }

    fn node_has_export_modifier(&self, node: &tsz_parser::parser::node::Node) -> bool {
        match node.kind {
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|decl| {
                    self.arena
                        .has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(node).is_some_and(|decl| {
                    self.arena
                        .has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            _ => false,
        }
    }

    fn collect_foldable_export_aliases(
        &mut self,
        statements: &tsz_parser::parser::NodeList,
        foldable_locals: &rustc_hash::FxHashSet<String>,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                _ if self.is_foldable_iife_declaration(node) => {
                    if !self.node_has_export_modifier(node) {
                        continue;
                    }
                    if let Some(name_id) = self.foldable_iife_export_id(stmt_idx)
                        && let Some(local_name) = self.foldable_iife_local_name(stmt_idx)
                    {
                        let entry = self
                            .all_export_aliases_in_order
                            .entry(local_name)
                            .or_default();
                        if !entry.contains(&name_id) {
                            entry.push(name_id);
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export_decl) = self.arena.get_export_decl(node) else {
                        continue;
                    };
                    if export_decl.is_type_only {
                        continue;
                    }
                    if let Some(inner_iife_idx) = self.export_decl_wraps_foldable_iife(node) {
                        if let Some(name_id) = self.foldable_iife_export_id(inner_iife_idx)
                            && let Some(local_name) = self.foldable_iife_local_name(inner_iife_idx)
                        {
                            let entry = self
                                .all_export_aliases_in_order
                                .entry(local_name)
                                .or_default();
                            if !entry.contains(&name_id) {
                                entry.push(name_id);
                            }
                        }
                        continue;
                    }
                    if export_decl.module_specifier.is_some() {
                        continue;
                    }
                    let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                        continue;
                    };
                    let Some(named) = self.arena.get_named_imports(clause_node) else {
                        continue;
                    };
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec_node) = self.arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(spec) = self.arena.get_specifier(spec_node) else {
                            continue;
                        };
                        if spec.is_type_only {
                            continue;
                        }
                        // Local name is property_name when aliased, otherwise name.
                        let local_name_idx = if spec.property_name.is_some() {
                            spec.property_name
                        } else {
                            spec.name
                        };
                        let Some(local_name) = self
                            .get_identifier_text_ref(local_name_idx)
                            .map(str::to_string)
                        else {
                            continue;
                        };
                        // Only record this alias when `local_name` actually
                        // names a foldable enum/namespace — `export { x as y }`
                        // for a `const x` must still emit the regular
                        // `exports.y = x;` line, not be folded into a
                        // (non-existent) IIFE tail.
                        if !foldable_locals.contains(&local_name) {
                            continue;
                        }
                        let Some(export_name_id) = self.get_identifier_id(spec.name) else {
                            continue;
                        };
                        let entry = self
                            .all_export_aliases_in_order
                            .entry(local_name)
                            .or_default();
                        if !entry.contains(&export_name_id) {
                            entry.push(export_name_id);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect names from `export { Name }` statements (without a module specifier).
    fn collect_re_exported_names(&mut self, statements: &tsz_parser::parser::NodeList) {
        for &stmt_idx in &statements.nodes {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(node) else {
                continue;
            };
            // Only local re-exports (no module specifier)
            if export_decl.module_specifier.is_some() || export_decl.is_type_only {
                continue;
            }
            // The export_clause for `export { A }` is a NAMED_EXPORTS node
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                // The local name (property_name if aliased, otherwise name)
                let local_name_idx = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                if let Some(name) = self.get_identifier_text_ref(local_name_idx) {
                    let local_name = name.to_string();
                    self.re_exported_names.insert(local_name.clone());
                    if let Some(export_name_id) = self.get_identifier_id(spec.name) {
                        self.re_exported_export_names
                            .entry(local_name)
                            .or_default()
                            .push(export_name_id);
                    }
                }
            }
        }
    }

    /// Every CommonJS export alias attached to `local_name` in source order,
    /// falling back to `[fallback_name]` when nothing has been recorded.
    pub(super) fn commonjs_export_names_for_local(
        &self,
        local_name: Option<&str>,
        fallback_name: IdentifierId,
    ) -> Arc<[IdentifierId]> {
        if let Some(local_name) = local_name {
            if let Some(all_aliases) = self.all_export_aliases_in_order.get(local_name)
                && !all_aliases.is_empty()
            {
                return Arc::from(all_aliases.clone());
            }
            if let Some(re_exports) = self.re_exported_export_names.get(local_name)
                && !re_exports.is_empty()
            {
                return Arc::from(re_exports.clone());
            }
        }

        Arc::from(vec![fallback_name])
    }

    pub(super) const fn is_commonjs(&self) -> bool {
        self.commonjs_mode
    }

    /// Check if a modifier list contains the 'const' keyword
    pub(super) fn has_const_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.arena.has_modifier(modifiers, SyntaxKind::ConstKeyword)
    }

    /// Check if a class member (method, property, accessor) is static
    pub(super) fn is_static_member(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };

        let modifiers = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .and_then(|m| m.modifiers.as_ref()),
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT =>
            {
                self.arena
                    .get_property_assignment(member_node)
                    .and_then(|p| p.modifiers.as_ref())
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .and_then(|a| a.modifiers.as_ref()),
            _ => None,
        };

        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16)
        })
    }

    pub(super) fn get_extends_heritage(
        &self,
        heritage_clauses: &Option<NodeList>,
    ) -> Option<NodeIndex> {
        let clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &clauses.nodes {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return Some(clause_idx);
            }
        }

        None
    }

    /// Check if a function has the 'async' modifier
    pub(super) fn has_async_modifier(&self, func_idx: NodeIndex) -> bool {
        let Some(func_node) = self.arena.get(func_idx) else {
            return false;
        };

        let Some(func) = self.arena.get_function(func_node) else {
            return false;
        };

        if func.is_async {
            return true;
        }

        let Some(mods) = &func.modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
        })
    }

    pub(super) const fn mark_async_helpers(&mut self) {
        let helpers = self.transforms.helpers_mut();
        helpers.awaiter = true;
        // __generator is only needed for ES5 (ES2015+ has native generators)
        if self.ctx.target_es5 {
            helpers.generator = true;
        }
    }

    /// Mark helpers needed for async generator functions (async function*).
    pub(super) fn mark_async_generator_helpers(&mut self) {
        let helpers = self.transforms.helpers_mut();
        helpers.mark_await_helper();
        helpers.mark_async_generator();
        if self.ctx.target_es5 {
            helpers.generator = true;
        }
    }

    pub(super) fn has_class_member_modifier(
        &self,
        modifiers: &Option<NodeList>,
        modifier: u16,
    ) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes
            .iter()
            .any(|&mod_idx| self.arena.get(mod_idx).is_some_and(|n| n.kind == modifier))
    }

    /// Check if a class has any decorators (class-level or member-level)
    pub(super) fn class_has_decorators(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // Check class-level decorators
        if let Some(mods) = &class_data.modifiers
            && mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
        {
            return true;
        }
        // Check member-level decorators
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let mods = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|p| p.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => None,
            };
            if let Some(mods) = mods
                && mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            {
                return true;
            }
        }
        false
    }

    pub(super) fn class_has_static_tc39_public_field_decorator(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                return false;
            };
            self.has_class_member_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16)
                && !self
                    .has_class_member_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16)
                && !self
                    .has_class_member_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword as u16)
                && !self
                    .has_class_member_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword as u16)
                && self
                    .arena
                    .get(prop.name)
                    .is_none_or(|name| name.kind != SyntaxKind::PrivateIdentifier as u16)
                && prop.modifiers.as_ref().is_some_and(|mods| {
                    mods.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                })
        })
    }

    /// Check if a class has any decorated members with computed property names
    pub(super) fn class_has_computed_decorated_member(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let (mods, name_idx) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(m) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (m.modifiers.as_ref(), m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(p) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    (p.modifiers.as_ref(), p.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(a) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (a.modifiers.as_ref(), a.name)
                }
                _ => continue,
            };
            // Check if member has decorators
            let has_decorators = mods.is_some_and(|m| {
                m.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
            if !has_decorators {
                continue;
            }
            // Check if name is computed (but not a string literal)
            if let Some(name_node) = self.arena.get(name_idx)
                && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.arena.get_computed_property(name_node)
                && let Some(expr_node) = self.arena.get(computed.expression)
                && expr_node.kind != SyntaxKind::StringLiteral as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a class has any decorated private members
    pub(super) fn class_has_private_decorated_member(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let (mods, name_idx) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(m) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (m.modifiers.as_ref(), m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(p) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    (p.modifiers.as_ref(), p.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(a) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (a.modifiers.as_ref(), a.name)
                }
                _ => continue,
            };
            let has_decorators = mods.is_some_and(|m| {
                m.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
            if !has_decorators {
                continue;
            }
            if let Some(name_node) = self.arena.get(name_idx)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
        }
        false
    }

    pub(super) fn needs_es5_object_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements.iter().any(|&idx| {
            if emit_utils::is_computed_property_member(self.arena, idx)
                || emit_utils::is_spread_element(self.arena, idx)
            {
                return true;
            }

            let Some(node) = self.arena.get(idx) else {
                return false;
            };

            // Shorthand properties are ES2015+ syntax and don't need lowering for ES2015+ targets
            // Only method declarations need lowering (computed property names are checked above)
            node.kind == syntax_kind_ext::METHOD_DECLARATION
        })
    }

    /// Check if an array literal needs ES5 transformation (has spread elements)
    pub(super) fn needs_es5_array_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements
            .iter()
            .any(|&idx| emit_utils::is_spread_element(self.arena, idx))
    }

    pub(super) fn function_parameters_need_es5_transform(&self, params: &NodeList) -> bool {
        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            param.dot_dot_dot_token
                || param.initializer.is_some()
                || self.is_binding_pattern_idx(param.name)
        })
    }

    /// Check if function parameters have rest that needs __rest helper.
    /// Only object rest patterns need __rest. Function rest params use arguments loop,
    /// and array rest elements use .`slice()`.
    pub(super) fn function_parameters_need_rest_helper(&self, params: &NodeList) -> bool {
        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            // Function rest parameters (...args) do NOT need __rest helper.
            // They are lowered using an arguments loop, not __rest.

            // Check if binding patterns contain object rest
            if self.is_binding_pattern_idx(param.name) {
                self.binding_pattern_has_object_rest(param.name)
            } else {
                false
            }
        })
    }

    /// Check if a binding pattern (recursively) has an object rest element.
    /// Only object rest patterns need the __rest helper. Array rest uses .`slice()`.
    pub(super) fn binding_pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            && node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return false;
        };

        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return false;
        };

        let is_object = node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;

        pattern.elements.nodes.iter().any(|&elem_idx| {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            // Rest in object pattern needs __rest
            if is_object && elem.dot_dot_dot_token {
                return true;
            }
            // Recursively check nested binding patterns
            self.binding_pattern_has_object_rest(elem.name)
        })
    }

    /// Check if an assignment destructuring pattern has object rest.
    ///
    /// Assignment destructuring uses object/array literal nodes rather than
    /// binding-pattern nodes, but it still lowers object rest through `__rest`.
    pub(super) fn assignment_pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                match elem_node.kind {
                    k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => true,
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_property_assignment(elem_node)
                        .is_some_and(|prop| {
                            self.assignment_pattern_has_object_rest(prop.initializer)
                        }),
                    _ => false,
                }
            });
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread) = self.arena.get_spread(elem_node)
                {
                    return self.assignment_pattern_has_object_rest(spread.expression);
                }
                self.assignment_pattern_has_object_rest(elem_idx)
            });
        }

        false
    }

    pub(super) fn is_binding_pattern_idx(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|n| n.is_binding_pattern())
    }

    pub(super) fn call_spread_needs_spread_array(&self, args: &[NodeIndex]) -> bool {
        let mut spread_count = 0usize;
        let mut real_arg_count = 0usize;

        for &idx in args {
            if idx.is_none() {
                continue;
            }
            real_arg_count += 1;
            if emit_utils::is_spread_element(self.arena, idx) {
                spread_count += 1;
            }
        }

        // No spread means no spread helper.
        if spread_count == 0 {
            return false;
        }

        // Exactly one spread and no other args: foo(...arr) -> foo.apply(void 0, arr)
        // This does not require __spreadArray.
        if spread_count == 1 && real_arg_count == 1 {
            return false;
        }

        true
    }

    /// Check if a for-of initializer contains binding patterns (destructuring)
    /// Initializer can be `VARIABLE_DECLARATION_LIST` with declarations that have binding patterns
    pub(super) fn for_of_initializer_has_binding_pattern(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        // Check if initializer is a variable declaration list
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(var_data) = self.arena.get_variable(init_node)
        {
            // Check each declaration in the list
            for &decl_idx in &var_data.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl_data) = self.arena.get_variable_declaration(decl_node)
                    && let Some(name_node) = self.arena.get(decl_data.name)
                {
                    // Check if name is an ARRAY binding pattern
                    // __read helper is only needed for array destructuring, not object destructuring
                    // Object destructuring accesses properties by name, not by iterator position
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub(super) fn get_identifier_id(&self, idx: NodeIndex) -> Option<IdentifierId> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        Some(node.data_index)
    }

    pub(super) fn get_identifier_text_ref(&self, idx: NodeIndex) -> Option<&str> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(node)?;
        Some(&ident.escaped_text)
    }

    pub(super) fn resolve_class_expr_binding_name(&self, class_idx: NodeIndex) -> Option<&str> {
        let mut current = class_idx;
        let mut hops = 0;

        while hops < 8 {
            let parent_idx = self.arena.get_extended(current)?.parent;
            if parent_idx.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;

            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let paren = self.arena.get_parenthesized(parent_node)?;
                    if paren.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::TYPE_ASSERTION
                | syntax_kind_ext::AS_EXPRESSION
                | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    let assertion = self.arena.get_type_assertion(parent_node)?;
                    if assertion.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::NON_NULL_EXPRESSION => {
                    let non_null = self.arena.get_unary_expr_ex(parent_node)?;
                    if non_null.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.arena.get_variable_declaration(parent_node)?;
                    if decl.initializer != current {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(decl.name)
                        .filter(|name| !name.is_empty());
                }
                syntax_kind_ext::PARAMETER => {
                    let param = self.arena.get_parameter(parent_node)?;
                    if param.initializer != current {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(param.name)
                        .filter(|name| !name.is_empty());
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || binary.operator_token != SyntaxKind::EqualsToken as u16
                    {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(binary.left)
                        .filter(|name| !name.is_empty());
                }
                _ => return None,
            }
        }

        None
    }

    pub(super) fn get_module_root_name(&self, name_idx: NodeIndex) -> Option<IdentifierId> {
        self.get_module_root_name_inner(name_idx, 0)
    }

    pub(super) fn get_module_root_name_inner(
        &self,
        name_idx: NodeIndex,
        depth: u32,
    ) -> Option<IdentifierId> {
        // Stack overflow protection for qualified names
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return None;
        }

        if name_idx.is_none() {
            return None;
        }

        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(node.data_index);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.qualified_names.get(node.data_index as usize)
        {
            return self.get_module_root_name_inner(qn.left, depth + 1);
        }

        None
    }

    /// Get the root name of a module as a String for merging detection
    pub(super) fn get_module_root_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let id = self.get_module_root_name(name_idx)?;
        let ident = self.arena.identifiers.get(id as usize)?;
        Some(ident.escaped_text.clone())
    }

    pub(super) fn get_block_like(
        &self,
        node: &Node,
    ) -> Option<&tsz_parser::parser::node::BlockData> {
        if node.kind == syntax_kind_ext::BLOCK
            || node.kind == syntax_kind_ext::CASE_BLOCK
            || node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
        {
            self.arena.blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    pub(super) fn collect_variable_names(&self, declarations: &NodeList) -> Vec<IdentifierId> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    pub(super) fn collect_binding_names(&self, name_idx: NodeIndex, names: &mut Vec<IdentifierId>) {
        self.collect_binding_names_inner(name_idx, names, 0);
    }

    pub(super) fn collect_binding_names_inner(
        &self,
        name_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection for deeply nested binding patterns
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            names.push(node.data_index);
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element_inner(elem_idx, names, depth + 1);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names_inner(elem.name, names, depth + 1);
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_binding_names_from_element_inner(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names_inner(elem.name, names, depth + 1);
        }
    }

    pub(super) fn maybe_wrap_module(&mut self, source_file: NodeIndex) {
        let format = match self.ctx.options.module {
            ModuleKind::AMD => ModuleFormat::AMD,
            ModuleKind::System => ModuleFormat::System,
            ModuleKind::UMD => ModuleFormat::UMD,
            _ => return,
        };

        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        if !self.file_is_module(&source.statements) {
            return;
        }

        let dependencies = Arc::from(self.collect_module_dependencies(&source.statements.nodes));
        self.transforms.insert(
            source_file,
            TransformDirective::ModuleWrapper {
                format,
                dependencies,
            },
        );
    }

    pub(super) fn file_is_module(&self, statements: &NodeList) -> bool {
        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }
        if self.jsx_automatic_runtime_makes_module() {
            return true;
        }
        // Node16/NodeNext resolved to ESM: file is definitively a module
        if self.ctx.options.resolved_node_module_to_esm {
            return true;
        }
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                    {
                        if let Some(import_decl) = self.arena.get_import_decl(node)
                            && self.import_has_runtime_dependency(import_decl)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                    {
                        // Any export declaration (even ambient / type-only) makes the
                        // file a module.  tsc wraps AMD/UMD/System output even when
                        // all exports are `export declare`.  The runtime-value filter
                        // is for *emitting* exports, not for module detection.
                        return true;
                    }
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self
                                .arena
                                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self
                                .arena
                                .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self
                                .arena
                                .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self
                                .arena
                                .has_modifier(&enum_decl.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&enum_decl.modifiers)
                            && !self.has_const_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self
                                .arena
                                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if matches!(
            self.ctx.options.module,
            ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System
        ) && self.source_has_dynamic_import_call(statements)
        {
            return true;
        }
        if self.contains_import_meta(statements) {
            return true;
        }
        false
    }

    fn source_has_dynamic_import_call(&self, statements: &NodeList) -> bool {
        let mut stack: Vec<NodeIndex> = statements.nodes.clone();
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.arena.get_call_expr(node)
                && let Some(expr_node) = self.arena.get(call.expression)
                && expr_node.kind == SyntaxKind::ImportKeyword as u16
            {
                return true;
            }
            for child in self.arena.get_children(idx) {
                stack.push(child);
            }
        }
        false
    }

    fn contains_import_meta(&self, statements: &NodeList) -> bool {
        let mut stack: Vec<NodeIndex> = statements.nodes.clone();
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(node)
                && let Some(expr_node) = self.arena.get(access.expression)
                && expr_node.kind == SyntaxKind::ImportKeyword as u16
                && self
                    .arena
                    .get(access.name_or_argument)
                    .and_then(|name_node| self.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text.as_str() == "meta")
            {
                return true;
            }
            for child in self.arena.get_children(idx) {
                stack.push(child);
            }
        }
        false
    }

    fn jsx_automatic_runtime_makes_module(&self) -> bool {
        if self.ctx.options.module_detection_legacy {
            return false;
        }
        if !matches!(
            self.ctx.options.jsx,
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev
        ) {
            return false;
        }
        (0..self.arena.len()).any(|idx| {
            self.arena.get(NodeIndex(idx as u32)).is_some_and(|node| {
                node.kind == syntax_kind_ext::JSX_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_FRAGMENT
            })
        })
    }

    pub(super) fn contains_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                return true;
            }
        }
        false
    }

    pub(super) fn collect_module_dependencies(&self, statements: &[NodeIndex]) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_should_schedule_runtime_dependency(node, import_decl) {
                        continue;
                    }
                    if let Some(text) =
                        emit_utils::module_specifier_text(self.arena, import_decl.module_specifier)
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_has_runtime_dependency(export_decl) {
                    continue;
                }
                if let Some(text) =
                    emit_utils::module_specifier_text(self.arena, export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        if self.jsx_automatic_runtime_makes_module() {
            let source = self
                .ctx
                .options
                .jsx_import_source
                .as_deref()
                .unwrap_or("react");
            let runtime = if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
                format!("{source}/jsx-dev-runtime")
            } else {
                format!("{source}/jsx-runtime")
            };
            if !deps.contains(&runtime) {
                deps.push(runtime);
            }
        }

        deps
    }

    pub(super) fn import_has_runtime_dependency(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return self.import_equals_has_external_module(import_decl.module_specifier);
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if clause.name.is_some() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    pub(super) fn import_should_schedule_runtime_dependency(
        &self,
        node: &tsz_parser::parser::node::Node,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if !self.import_has_runtime_dependency(import_decl) {
            return false;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };
        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return true;
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };
        if clause.is_type_only {
            return false;
        }
        if self.ctx.options.verbatim_module_syntax {
            return true;
        }
        if self.import_clause_is_empty_named_import(clause) {
            return false;
        }
        if self.import_clause_is_namespace_only(clause)
            && self.import_references_type_only_export_equals_module(import_decl)
        {
            return false;
        }

        self.import_has_value_usage_after_node(node, clause)
    }

    fn import_clause_is_namespace_only(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named| named.name.is_some() && named.elements.nodes.is_empty())
    }

    fn import_clause_is_empty_named_import(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named| named.name.is_none() && named.elements.nodes.is_empty())
    }

    fn import_references_type_only_export_equals_module(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(module_node) = self.arena.get(import_decl.module_specifier) else {
            return false;
        };
        let Some(lit) = self.arena.get_literal(module_node) else {
            return false;
        };
        self.ctx
            .options
            .type_only_export_equals_modules
            .contains(lit.text.as_str())
    }

    pub(super) fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            // require(nonStringLiteral) — specifier failed to parse as string literal,
            // but the `import = require(...)` form still indicates an external module
            return true;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return true;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    #[allow(dead_code)]
    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        crate::transforms::emit_utils::export_decl_has_runtime_value(
            self.arena,
            export_decl,
            self.ctx.options.preserve_const_enums,
        )
    }

    pub(super) fn export_has_runtime_dependency(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.module_specifier.is_none() {
            return false;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return true;
        };

        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
#[path = "../../tests/lowering_helpers.rs"]
mod tests;
