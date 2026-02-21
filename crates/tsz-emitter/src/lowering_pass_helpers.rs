//! Helper methods for the lowering pass.
//!
//! Contains module initialization, modifier checking, helper detection,
//! binding pattern analysis, and this-capture computation.

use super::*;

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
        self.commonjs_mode = if self.ctx.is_commonjs() {
            true
        } else if self.ctx.auto_detect_module && matches!(self.ctx.options.module, ModuleKind::None)
        {
            self.file_is_module(&source.statements)
        } else {
            false
        };
    }

    pub(super) const fn is_commonjs(&self) -> bool {
        self.commonjs_mode
    }

    /// Check if a modifier list contains the 'declare' keyword
    pub(super) fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::DeclareKeyword as u16)
        })
    }

    /// Check if a modifier list contains the 'const' keyword
    pub(super) fn has_const_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::ConstKeyword as u16)
        })
    }

    /// Check if a modifier list contains the 'export' keyword
    pub(super) fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::ExportKeyword as u16)
        })
    }

    /// Check if a modifier list contains the 'default' keyword
    pub(super) fn has_default_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::DefaultKeyword as u16)
        })
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

    pub(super) fn mark_class_helpers(
        &mut self,
        class_node: NodeIndex,
        heritage: Option<NodeIndex>,
    ) {
        if heritage.is_some() {
            self.transforms.helpers_mut().extends = true;
        }

        let Some(class_node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        if self.class_has_private_members(class_data) {
            let helpers = self.transforms.helpers_mut();
            helpers.class_private_field_get = true;
            helpers.class_private_field_set = true;
        }
    }

    pub(super) fn class_has_private_members(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node)
                        && is_private_identifier(self.arena, prop.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(member_node)
                        && is_private_identifier(self.arena, method.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.arena.get_accessor(member_node)
                        && is_private_identifier(self.arena, accessor.name)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub(super) fn needs_es5_object_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements.iter().any(|&idx| {
            if self.is_computed_property_member(idx) || self.is_spread_element(idx) {
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
        elements.iter().any(|&idx| self.is_spread_element(idx))
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

    pub(super) fn is_binding_pattern_idx(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        })
    }

    pub(super) fn is_computed_property_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        }

        false
    }

    pub(super) fn is_spread_element(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            || node.kind == syntax_kind_ext::SPREAD_ELEMENT
    }

    pub(super) fn call_spread_needs_spread_array(&self, args: &[NodeIndex]) -> bool {
        let mut spread_count = 0usize;
        let mut real_arg_count = 0usize;

        for &idx in args {
            if idx.is_none() {
                continue;
            }
            real_arg_count += 1;
            if self.is_spread_element(idx) {
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

    pub(super) fn is_valid_identifier_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first == '$' || first.is_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
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
        if node.kind == syntax_kind_ext::BLOCK || node.kind == syntax_kind_ext::CASE_BLOCK {
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
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        if let Some(export_decl) = self.arena.get_export_decl(node)
                            && self.export_decl_has_runtime_value(export_decl)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self.has_export_modifier(&var_stmt.modifiers)
                            && !self.has_declare_modifier(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self.has_export_modifier(&func.modifiers)
                            && !self.has_declare_modifier(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self.has_export_modifier(&class.modifiers)
                            && !self.has_declare_modifier(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self.has_export_modifier(&enum_decl.modifiers)
                            && !self.has_declare_modifier(&enum_decl.modifiers)
                            && !self.has_const_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self.has_export_modifier(&module.modifiers)
                            && !self.has_declare_modifier(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
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
                    if !self.import_has_runtime_dependency(import_decl) {
                        continue;
                    }
                    if let Some(text) = self.get_module_specifier_text(import_decl.module_specifier)
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
                if let Some(text) = self.get_module_specifier_text(export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
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

    pub(super) fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            return false;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return false;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.is_default_export {
            return true;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return false;
        };

        if let Some(named) = self.arena.get_named_imports(clause_node) {
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

            return false;
        }

        if self.export_clause_is_type_only(clause_node) {
            return false;
        }

        true
    }

    pub(super) fn export_clause_is_type_only(&self, clause_node: &Node) -> bool {
        match clause_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let Some(enum_decl) = self.arena.get_enum(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&enum_decl.modifiers)
                    || self.has_const_modifier(&enum_decl.modifiers)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let Some(class_decl) = self.arena.get_class(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&class_decl.modifiers)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let Some(func_decl) = self.arena.get_function(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&func_decl.modifiers)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_decl) = self.arena.get_variable(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&var_decl.modifiers)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let Some(module_decl) = self.arena.get_module(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&module_decl.modifiers)
            }
            _ => false,
        }
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

    pub(super) fn get_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }

        let node = self.arena.get(specifier)?;
        let literal = self.arena.get_literal(node)?;

        Some(literal.text.clone())
    }

    /// Compute the capture variable name for `_this` in a given scope.
    /// If the scope contains a variable declaration or function parameter named `_this`,
    /// returns `_this_1`. Otherwise returns `_this`.
    pub(super) fn compute_this_capture_name(&self, body_idx: NodeIndex) -> Arc<str> {
        self.compute_this_capture_name_with_params(body_idx, None)
    }

    /// Compute capture name, also checking function parameters for collision.
    pub(super) fn compute_this_capture_name_with_params(
        &self,
        body_idx: NodeIndex,
        params: Option<&NodeList>,
    ) -> Arc<str> {
        if self.scope_has_name(body_idx, "_this") || self.params_have_name(params, "_this") {
            Arc::from("_this_1")
        } else {
            Arc::from("_this")
        }
    }

    /// Check if any parameter in the list has the given name.
    pub(super) fn params_have_name(&self, params: Option<&NodeList>, name: &str) -> bool {
        let Some(params) = params else {
            return false;
        };
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            if let Some(param) = self.arena.get_parameter(param_node)
                && self.get_identifier_text_ref(param.name) == Some(name)
            {
                return true;
            }
        }
        false
    }

    /// Check if a function body (block or source file) contains a variable
    /// declaration or parameter with the given name at its direct scope level.
    pub(super) fn scope_has_name(&self, body_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(body_idx) else {
            return false;
        };

        // Get statements from block or source file
        let statements = if let Some(block) = self.arena.get_block(node) {
            &block.statements
        } else if let Some(sf) = self.arena.get_source_file(node) {
            &sf.statements
        } else {
            return false;
        };

        // Check each statement for variable declarations with the given name
        for &stmt_idx in &statements.nodes {
            let Some(stmt) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                // VariableStatement → VariableData.declarations contains a VariableDeclarationList
                if let Some(var_stmt_data) = self.arena.get_variable(stmt) {
                    for &decl_list_idx in &var_stmt_data.declarations.nodes {
                        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                            continue;
                        };
                        // VariableDeclarationList → VariableData.declarations contains VariableDeclarations
                        if let Some(decl_list_data) = self.arena.get_variable(decl_list_node) {
                            for &decl_idx in &decl_list_data.declarations.nodes {
                                let Some(decl_node) = self.arena.get(decl_idx) else {
                                    continue;
                                };
                                if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                                    && self.get_identifier_text_ref(decl.name) == Some(name)
                                {
                                    return true;
                                }
                            }
                        }
                        // Also handle VariableDeclaration directly (in case it's not nested)
                        if let Some(decl) = self.arena.get_variable_declaration(decl_list_node)
                            && self.get_identifier_text_ref(decl.name) == Some(name)
                        {
                            return true;
                        }
                    }
                }
            }
            // Also check function declarations (their name occupies the scope)
            if stmt.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.arena.get_function(stmt)
                && self.get_identifier_text_ref(func.name) == Some(name)
            {
                return true;
            }
        }

        false
    }
}
