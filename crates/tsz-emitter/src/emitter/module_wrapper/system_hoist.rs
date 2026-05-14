use super::super::{JsxEmit, Printer};
use super::{SystemDependencyAction, SystemDependencyPlan};
use crate::emitter::declarations::class::class_has_self_references;
use std::collections::HashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn collect_system_hoisted_names(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        system_plan: &SystemDependencyPlan,
    ) -> Vec<String> {
        self.system_empty_binding_temps.clear();
        let mut names = Vec::new();
        let mut deferred_named_export_names = Vec::new();
        let mut seen_deferred_named_export_names = HashSet::new();
        let mut seen = HashSet::new();
        let mut seen_top_level_using = false;
        let has_top_level_using = !self.ctx.options.target.supports_es2025()
            && source
                .statements
                .nodes
                .iter()
                .filter_map(|&stmt_idx| self.arena.get(stmt_idx))
                .any(|stmt_node| self.statement_is_top_level_using(stmt_node));

        let source_import_vars: HashSet<String> =
            system_plan.import_vars.values().cloned().collect();

        // Synthetic assignments, such as automatic JSX runtime imports, are
        // not backed by a source statement and still need to precede source
        // declaration hoists. Source-backed import temps are inserted below
        // when their import statements are encountered, preserving tsc's
        // mixed local/import var order.
        for actions in system_plan.actions.values() {
            for action in actions {
                if let SystemDependencyAction::Assign(name) = action
                    && !source_import_vars.contains(name)
                    && seen.insert(name.clone())
                {
                    names.push(name.clone());
                }
            }
        }

        if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev)
            && system_plan
                .actions
                .keys()
                .any(|dep| dep.ends_with("/jsx-dev-runtime"))
            && seen.insert("_jsxFileName".to_string())
        {
            names.push("_jsxFileName".to_string());
        }

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if let Some(dep_var) = system_plan.import_vars.get(&stmt_node.pos)
                && seen.insert(dep_var.clone())
            {
                names.push(dep_var.clone());
            }
            let stmt_is_top_level_using = self.statement_is_top_level_using(stmt_node);
            if stmt_is_top_level_using {
                seen_top_level_using = true;
            }
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && let Some(class_decl) = self.arena.get_class(stmt_node)
                {
                    let class_name = self.get_identifier_text_idx(class_decl.name);
                    if !class_name.is_empty() && seen.insert(class_name.clone()) {
                        names.push(class_name);
                    }
                    if has_top_level_using
                        && seen_top_level_using
                        && self
                            .arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::ExportKeyword)
                        && self
                            .arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::DefaultKeyword)
                        && class_decl.name.is_some()
                        && seen.insert("_default".to_string())
                    {
                        // Only hoist `_default` when the default-exported
                        // class lives AFTER the top-level `using` in source
                        // and so is reached from inside the synthesized
                        // try/catch. When the class precedes the using,
                        // the class name itself is the live binding and
                        // tsc emits no `_default`.
                        names.push("_default".to_string());
                    }
                }
                if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && let Some(import_decl) = self.arena.get_import_decl(stmt_node)
                    && self.import_decl_has_runtime_value(import_decl)
                {
                    let local_name = self.get_identifier_text_idx(import_decl.import_clause);
                    if !local_name.is_empty() && seen.insert(local_name.clone()) {
                        names.push(local_name);
                    }
                }
                if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_decl) = self.arena.get_module(stmt_node)
                {
                    // Skip ambient/declare module declarations — they don't
                    // produce runtime code and shouldn't be hoisted.
                    // e.g., `declare global { interface ImportMeta {...} }`
                    let is_ambient = self
                        .arena
                        .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword);
                    if !is_ambient {
                        if !self.is_instantiated_module(module_decl.body) {
                            continue;
                        }
                        let module_name = self.get_identifier_text_idx(module_decl.name);
                        if !module_name.is_empty() && seen.insert(module_name.clone()) {
                            names.push(module_name);
                        }
                    }
                }
                if stmt_node.kind == syntax_kind_ext::ENUM_DECLARATION
                    && let Some(enum_decl) = self.arena.get_enum(stmt_node)
                {
                    let is_erased = self
                        .arena
                        .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                        || (self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                            && !self.ctx.options.preserve_const_enums);
                    if !is_erased {
                        let enum_name = self.get_identifier_text_idx(enum_decl.name);
                        if !enum_name.is_empty() && seen.insert(enum_name.clone()) {
                            names.push(enum_name);
                        }
                    }
                }
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                    && export_decl.module_specifier.is_none()
                    && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                {
                    // `export default class {}` or `export default class Foo {}` —
                    // hoist `var default_1;` or `var Foo;` respectively.
                    if export_decl.is_default_export
                        && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        if let Some(class_decl) = self.arena.get_class(clause_node) {
                            let class_name = self.get_identifier_text_idx(class_decl.name);
                            if has_top_level_using && class_name.is_empty() {
                                if (self.ctx.target_es5 || self.ctx.options.legacy_decorators)
                                    && seen.insert("default_1".to_string())
                                {
                                    names.push("default_1".to_string());
                                }
                                if seen_top_level_using && seen.insert("_default".to_string()) {
                                    names.push("_default".to_string());
                                }
                                continue;
                            }
                            let name = if class_name.is_empty() {
                                "default_1".to_string()
                            } else {
                                class_name
                            };
                            if let Some(alias) = self.system_hoist_legacy_decorated_class_alias(
                                &name,
                                &class_decl.members.nodes,
                                &class_decl.modifiers,
                            ) && seen.insert(alias.clone())
                            {
                                names.push(alias);
                            }
                            if seen.insert(name.clone()) {
                                names.push(name);
                            }
                            if has_top_level_using
                                && seen_top_level_using
                                && seen.insert("_default".to_string())
                            {
                                // Only hoist `_default` when this default-
                                // exported class lives AFTER the top-level
                                // `using` in source (so it's reached from
                                // within the synthesized try/catch).
                                // Named classes that PRECEDE the using don't
                                // need the tracker — the class identifier
                                // is already the live binding.
                                names.push("_default".to_string());
                            }
                        }
                        continue;
                    }
                    if has_top_level_using
                        && export_decl.is_default_export
                        && clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
                        && clause_node.kind != syntax_kind_ext::CLASS_DECLARATION
                        && seen.insert("_default".to_string())
                    {
                        let insert_at = names.len().saturating_sub(1);
                        names.insert(insert_at, "_default".to_string());
                        continue;
                    }
                    if has_top_level_using
                        && seen_top_level_using
                        && export_decl.is_default_export
                        && (clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                            || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION)
                    {
                        // Only hoist `_default` when this default-export
                        // sits AFTER the top-level `using` in source — that
                        // means the class/function is reached from inside
                        // the synthesized try/catch and needs the tracker
                        // for the export call. When the default-export
                        // comes BEFORE the using, the class name (or
                        // `default_1` placeholder) is the live binding
                        // and no separate tracker is needed; tsc does not
                        // hoist `_default` in that case.
                        let has_local_name =
                            if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                self.arena
                                    .get_function(clause_node)
                                    .and_then(|func| self.get_identifier_text_opt(func.name))
                                    .is_some_and(|name| !name.is_empty())
                            } else {
                                self.arena
                                    .get_class(clause_node)
                                    .and_then(|class| self.get_identifier_text_opt(class.name))
                                    .is_some_and(|name| !name.is_empty())
                            };
                        if has_local_name && seen.insert("_default".to_string()) {
                            let insert_at = names.len().saturating_sub(1);
                            names.insert(insert_at, "_default".to_string());
                        }
                    }
                    if export_decl.is_default_export {
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        self.collect_system_empty_binding_temps_from_variable_statement(
                            clause_node,
                            true,
                            &mut names,
                            &mut seen,
                        );
                        for name in self.collect_variable_names_from_node(clause_node) {
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                        continue;
                    }
                    if has_top_level_using
                        && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                        && let Some(named_exports) = self.arena.get_named_imports(clause_node)
                    {
                        for &spec_idx in &named_exports.elements.nodes {
                            if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                                let local_name = if spec.property_name.is_some() {
                                    self.get_identifier_text_idx(spec.property_name)
                                } else {
                                    self.get_identifier_text_idx(spec.name)
                                };
                                if !local_name.is_empty()
                                    && seen_deferred_named_export_names.insert(local_name.clone())
                                {
                                    deferred_named_export_names.push(local_name);
                                }
                            }
                        }
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.arena.get_import_decl(clause_node)
                        && self.import_decl_has_runtime_value(import_decl)
                    {
                        let name = self.get_identifier_text_idx(import_decl.import_clause);
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(class_decl) = self.arena.get_class(clause_node)
                    {
                        let name = self.get_identifier_text_idx(class_decl.name);
                        if let Some(alias) = self.system_hoist_legacy_decorated_class_alias(
                            &name,
                            &class_decl.members.nodes,
                            &class_decl.modifiers,
                        ) && seen.insert(alias.clone())
                        {
                            names.push(alias);
                        }
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                    if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module_decl) = self.arena.get_module(clause_node)
                    {
                        let is_ambient = self
                            .arena
                            .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword);
                        if !is_ambient {
                            let name = self.get_identifier_text_idx(module_decl.name);
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                    }
                    if clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                    {
                        let is_erased = self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                            || (self
                                .arena
                                .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                                && !self.ctx.options.preserve_const_enums);
                        if !is_erased {
                            let name = self.get_identifier_text_idx(enum_decl.name);
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                    }
                }
                // Hoist `var` declarations from for/for-in/for-of loop initializers.
                // In System modules, `for (var x in ...)` becomes `var x;` at the
                // module scope and `for (x in ...)` inside execute().
                if (stmt_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                    || stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
                    && let Some(for_data) = self.arena.get_for_in_of(stmt_node)
                {
                    self.collect_var_names_from_initializer(
                        for_data.initializer,
                        &mut names,
                        &mut seen,
                    );
                }
                if stmt_node.kind == syntax_kind_ext::FOR_STATEMENT
                    && let Some(loop_data) = self.arena.get_loop(stmt_node)
                {
                    self.collect_var_names_from_initializer(
                        loop_data.initializer,
                        &mut names,
                        &mut seen,
                    );
                }
                let names_before_nested = names.len();
                self.collect_system_nested_top_level_var_hoisted_names(
                    stmt_idx, &mut names, &mut seen,
                );
                // tsc places `env_1` IMMEDIATELY before the first
                // nested-hoisted var (a `var` declared inside an `if` /
                // `for` / `try` / etc. that gets hoisted to the System
                // closure scope). Without this insertion the helper
                // sits at the end of the var list, which produces
                // `var z, y, env_1;` instead of tsc's
                // `var z, env_1, y;` for sources like
                // `using z = ...; if (false) { var y = 1; }`.
                if has_top_level_using
                    && seen_top_level_using
                    && names.len() > names_before_nested
                    && seen.insert("env_1".to_string())
                {
                    names.insert(names_before_nested, "env_1".to_string());
                }
                if has_top_level_using {
                    let needs_default_temp = (stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        && self
                            .arena
                            .get_export_assignment(stmt_node)
                            .is_some_and(|export_assignment| !export_assignment.is_export_equals))
                        || (stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            && self
                                .arena
                                .get_export_decl(stmt_node)
                                .is_some_and(|export_decl| {
                                    export_decl.is_default_export
                                        && export_decl.module_specifier.is_none()
                                        && self.arena.get(export_decl.export_clause).is_some_and(
                                            |clause_node| {
                                                clause_node.kind
                                                    != syntax_kind_ext::FUNCTION_DECLARATION
                                                    && clause_node.kind
                                                        != syntax_kind_ext::CLASS_DECLARATION
                                            },
                                        )
                                }));
                    if needs_default_temp && seen.insert("_default".to_string()) {
                        let insert_at = names.len().saturating_sub(1);
                        names.insert(insert_at, "_default".to_string());
                    }
                }
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let is_exported_variable = self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);

            for &decl_list_idx in &var_stmt.declarations.nodes {
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

                    if decl.initializer.is_some() && self.binding_pattern_is_empty(decl.name) {
                        let source_temp = self.make_unique_name();
                        if seen.insert(source_temp.clone()) {
                            names.push(source_temp.clone());
                        }
                        let export_temp = if is_exported_variable && self.ctx.target_es5 {
                            let name = self.make_unique_name();
                            if seen.insert(name.clone()) {
                                names.push(name.clone());
                            }
                            Some(name)
                        } else {
                            None
                        };
                        if let Some(name_node) = self.arena.get(decl.name) {
                            self.system_empty_binding_temps
                                .insert(name_node.pos, (source_temp, export_temp));
                        }
                        continue;
                    }

                    let mut binding_names = Vec::new();
                    self.collect_binding_names(decl.name, &mut binding_names);
                    for name in binding_names {
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                }
            }
        }

        // tsc places the using-block helper `env_1` at the *end* of the
        // closure's `var` list when nothing nested-hoisted appears between
        // the using statement and the rest of the closure. The earlier
        // nested-walk insertion (above) already places `env_1` immediately
        // before any `var` hoisted from inside an `if` / `for` / `try`,
        // matching tsc's `var z, env_1, y;` shape for sources like
        // `using z = ...; if (false) { var y = 1; }`. If no nested-hoisted
        // var appeared, the `env_1` slot stays free and we land it here at
        // the trailing position — matching tsc's
        // `var x, z, y, _default, w, env_1;` shape.
        if has_top_level_using && seen.insert("env_1".to_string()) {
            names.push("env_1".to_string());
        }

        for name in deferred_named_export_names {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }

        names
    }

    fn system_hoist_legacy_decorated_class_alias(
        &self,
        class_name: &str,
        members: &[NodeIndex],
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<String> {
        if class_name.is_empty() || !self.ctx.options.legacy_decorators {
            return None;
        }
        let has_class_or_ctor_param_decorators =
            !self.collect_class_decorators(modifiers).is_empty()
                || !self
                    .collect_constructor_param_decorators(members)
                    .is_empty();
        if !has_class_or_ctor_param_decorators {
            return None;
        }
        class_has_self_references(self.arena, self.source_text_for_map(), class_name, members)
            .then(|| format!("{class_name}_1"))
    }

    pub(super) fn add_system_jsx_runtime_dependency(
        &mut self,
        dependencies: &[String],
        system_plan: &mut SystemDependencyPlan,
    ) {
        let Some(runtime_dep) = dependencies.iter().find(|dep| {
            matches!(dep.as_str(), "react/jsx-runtime" | "react/jsx-dev-runtime")
                || dep.ends_with("/jsx-runtime")
                || dep.ends_with("/jsx-dev-runtime")
        }) else {
            return;
        };
        if system_plan.actions.contains_key(runtime_dep) {
            return;
        }
        let dep_var = if runtime_dep.ends_with("/jsx-dev-runtime") {
            "jsx_dev_runtime_1"
        } else {
            "jsx_runtime_1"
        };
        system_plan
            .actions
            .entry(runtime_dep.clone())
            .or_default()
            .push(SystemDependencyAction::Assign(dep_var.to_string()));
    }

    fn collect_system_empty_binding_temps_from_variable_statement(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        is_exported: bool,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
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
                if decl.initializer.is_none() || !self.binding_pattern_is_empty(decl.name) {
                    continue;
                }
                let source_temp = self.make_unique_name();
                if seen.insert(source_temp.clone()) {
                    names.push(source_temp.clone());
                }
                let export_temp = if is_exported && self.ctx.target_es5 {
                    let name = self.make_unique_name();
                    if seen.insert(name.clone()) {
                        names.push(name.clone());
                    }
                    Some(name)
                } else {
                    None
                };
                if let Some(name_node) = self.arena.get(decl.name) {
                    self.system_empty_binding_temps
                        .insert(name_node.pos, (source_temp, export_temp));
                }
            }
        }
    }

    fn collect_system_nested_top_level_var_hoisted_names(
        &mut self,
        idx: NodeIndex,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if self.top_level_hoisted_var_statement_is_var(node) {
                    self.collect_system_variable_hoisted_names(node, names, seen);
                }
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    let statements = block.statements.nodes.clone();
                    for stmt in statements {
                        self.collect_system_nested_top_level_var_hoisted_names(stmt, names, seen);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.collect_system_nested_top_level_var_hoisted_names(
                        if_stmt.then_statement,
                        names,
                        seen,
                    );
                    self.collect_system_nested_top_level_var_hoisted_names(
                        if_stmt.else_statement,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.collect_var_names_from_initializer(loop_data.initializer, names, seen);
                    self.collect_system_nested_top_level_var_hoisted_names(
                        loop_data.statement,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_data) = self.arena.get_for_in_of(node) {
                    self.collect_var_names_from_initializer(for_data.initializer, names, seen);
                    self.collect_system_nested_top_level_var_hoisted_names(
                        for_data.statement,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_stmt) = self.arena.get_switch(node) {
                    self.collect_system_nested_top_level_var_hoisted_names(
                        switch_stmt.case_block,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    let statements = clause.statements.nodes.clone();
                    for stmt in statements {
                        self.collect_system_nested_top_level_var_hoisted_names(stmt, names, seen);
                    }
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.collect_system_nested_top_level_var_hoisted_names(
                        try_stmt.try_block,
                        names,
                        seen,
                    );
                    self.collect_system_nested_top_level_var_hoisted_names(
                        try_stmt.catch_clause,
                        names,
                        seen,
                    );
                    self.collect_system_nested_top_level_var_hoisted_names(
                        try_stmt.finally_block,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(node) {
                    self.collect_system_nested_top_level_var_hoisted_names(
                        catch_clause.block,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    if self.collect_system_labeled_variable_names(labeled.statement, names, seen) {
                        return;
                    }
                    self.collect_system_nested_top_level_var_hoisted_names(
                        labeled.statement,
                        names,
                        seen,
                    );
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    self.collect_system_nested_top_level_var_hoisted_names(
                        with_stmt.then_statement,
                        names,
                        seen,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_system_labeled_variable_names(
        &self,
        stmt_idx: NodeIndex,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let variable_node = if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            stmt_node
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                return false;
            };
            if export_decl.module_specifier.is_some() {
                return false;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                return false;
            };
            if clause_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                return false;
            }
            clause_node
        } else {
            return false;
        };

        for name in self.collect_variable_names_from_node(variable_node) {
            if !name.is_empty() && seen.insert(name.clone()) {
                names.push(name);
            }
        }
        true
    }

    fn top_level_hoisted_var_statement_is_var(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        if self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
        {
            return false;
        }
        var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena.get(decl_list_idx).is_some_and(|decl_list_node| {
                !tsz_parser::parser::node_flags::is_let_or_const(decl_list_node.flags as u32)
            })
        })
    }

    fn collect_system_variable_hoisted_names(
        &self,
        node: &tsz_parser::parser::node::Node,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
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

                let mut binding_names = Vec::new();
                self.collect_binding_names(decl.name, &mut binding_names);
                for name in binding_names {
                    if !name.is_empty() && seen.insert(name.clone()) {
                        names.push(name);
                    }
                }
            }
        }
    }

    /// Collect variable names from a for/for-in/for-of initializer that is a
    /// `var` declaration list.  `let`/`const` are block-scoped and are NOT hoisted.
    fn collect_var_names_from_initializer(
        &self,
        initializer: NodeIndex,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return;
        }
        // Only hoist `var` declarations (not `let`/`const`)
        let is_var = (init_node.flags as u32
            & (tsz_parser::parser::node_flags::LET | tsz_parser::parser::node_flags::CONST))
            == 0;
        if !is_var {
            return;
        }
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return;
        };
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            let mut binding_names = Vec::new();
            self.collect_binding_names(decl.name, &mut binding_names);
            for name in binding_names {
                if !name.is_empty() && seen.insert(name.clone()) {
                    names.push(name);
                }
            }
        }
    }
}
