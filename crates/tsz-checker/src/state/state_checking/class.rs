//! Class declaration, expression, property initialization, and decorator checking.

use crate::EnclosingClassInfo;
use crate::context::TypingRequest;
use crate::flow_analysis::PropertyKey;
use crate::query_boundaries::class_type as class_query;
use crate::query_boundaries::definite_assignment::check_constructor_property_use_before_assignment;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check a class declaration.
    pub(crate) fn check_class_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::class_inheritance::ClassInheritanceChecker;
        use crate::diagnostics::diagnostic_codes;
        use crate::diagnostics::diagnostic_messages;

        // Optimization: Skip if already fully checked
        if self.ctx.checked_classes.contains(&stmt_idx) {
            return;
        }

        // Recursion guard: if we're already checking this class, return early.
        // This handles complex cycles where class checking triggers type resolution
        // (e.g. for method return types) that references the class itself or its base.
        if !self.ctx.checking_classes.insert(stmt_idx) {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return;
        };

        let Some(class) = self.ctx.arena.get_class(node) else {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return;
        };

        // TS1211: A class declaration without the 'default' modifier must have a name.
        // Only applies to class declarations, not class expressions (which are allowed to be anonymous).
        // Also skip when `default` is present as a modifier on the class itself (e.g. `default class {}`
        // without `export` — that's a TS1029 error, not TS1211).
        //
        // Also skip when the parser already emitted TS1005 for a reserved word in the name
        // position (e.g. `class void {}`). In that case `name` is None but tsc only emits
        // TS1005, not TS1211. We detect this by checking if there's a non-whitespace token
        // between the `class` keyword and `{` — that means a keyword was parsed and rejected.
        let parser_already_reported_name_error = class.name.is_none() && {
            if let Some(sf) = self.ctx.arena.source_files.first() {
                let src = sf.text.as_ref();
                let start = node.pos as usize;
                // Find "class" in the source at node start, then check what follows
                let after_class = src.get(start..).and_then(|s| {
                    let class_kw = s.find("class")?;
                    Some(&s[class_kw + 5..])
                });
                if let Some(rest) = after_class {
                    // Check if there's a non-whitespace char before `{`
                    let before_brace = rest.split('{').next().unwrap_or("");
                    !before_brace.trim().is_empty()
                } else {
                    false
                }
            } else {
                false
            }
        };
        if class.name.is_none()
            && node.kind == syntax_kind_ext::CLASS_DECLARATION
            && !self.has_modifier_kind(&class.modifiers, tsz_scanner::SyntaxKind::DefaultKeyword)
            && !parser_already_reported_name_error
        {
            // The parser consumes `default` before parsing the class, so it won't
            // appear in the class's own modifiers — check the parent export node.
            let parent_export = self.ctx.arena.get_extended(stmt_idx).and_then(|ext| {
                let parent = self.ctx.arena.get(ext.parent)?;
                let export_data = self.ctx.arena.get_export_decl(parent)?;
                Some((ext.parent, export_data.is_default_export))
            });
            match parent_export {
                Some((_, true)) => {} // `export default class {}` — allowed
                Some((export_idx, false)) => {
                    // `export class {}` — report on export node (tsc points at `export`)
                    self.error_at_node(
                        export_idx,
                        "A class declaration without the 'default' modifier must have a name.",
                        diagnostic_codes::A_CLASS_DECLARATION_WITHOUT_THE_DEFAULT_MODIFIER_MUST_HAVE_A_NAME,
                    );
                }
                None => {
                    // bare `class {}` — report on class node
                    self.error_at_node(
                        stmt_idx,
                        "A class declaration without the 'default' modifier must have a name.",
                        diagnostic_codes::A_CLASS_DECLARATION_WITHOUT_THE_DEFAULT_MODIFIER_MUST_HAVE_A_NAME,
                    );
                }
            }
        }

        // TS1042: async modifier cannot be used on class declarations
        self.check_async_modifier_on_declaration(&class.modifiers);

        let mut experimental_class_decorators = Vec::new();

        // Evaluate class-level decorator expressions to trigger definite-assignment
        // checks (TS2454) and other diagnostics. tsc evaluates decorator expressions
        // even if the class has other errors.
        if let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR
                    && let Some(decorator) = self.ctx.arena.get_decorator(mod_node)
                {
                    // TS1497: Check decorator expression grammar
                    self.check_grammar_decorator(decorator.expression);

                    let decorator_type = self.compute_type_of_node(decorator.expression);

                    // TS1238: Validate class decorator call signature.
                    if self.ctx.compiler_options.experimental_decorators {
                        // Experimental class decorators receive the class constructor
                        // value. Save the expression type and validate it after the
                        // class value side has been refreshed; doing it here can see a
                        // provisional/re-entrant constructor shape and miss TS1238.
                        experimental_class_decorators.push((decorator.expression, decorator_type));
                    } else {
                        // ES decorators: tsc anchors TS1238 at the whole decorator
                        // (including `@`) when the factory requires too many args, but
                        // at the expression alone when the factory has zero parameters.
                        self.check_es_class_decorator_arity(
                            mod_idx,
                            decorator.expression,
                            decorator_type,
                        );
                    }
                }
            }
        }

        // CRITICAL: Check for circular inheritance using InheritanceGraph
        // This prevents stack overflow from infinite recursion in get_class_instance_type
        // Must be done BEFORE any type checking to catch cycles early
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        let _has_inheritance_cycle = checker.check_class_inheritance_cycle(stmt_idx, class);

        // TS1213: Check class name for strict mode reserved words.
        // Class definitions are automatically in strict mode, so class names
        // always get TS1213 (class context), not TS1212.
        self.check_class_name_strict_mode_reserved(class.name);

        // Check for reserved class names (error 2414)
        // tsc's checkTypeNameIsReserved forbids predefined type names.
        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && crate::error_reporter::assignability::is_reserved_type_name(
                ident.escaped_text.as_str(),
            )
        {
            self.error_at_node(
                class.name,
                &format!("Class name cannot be '{}'.", ident.escaped_text),
                diagnostic_codes::CLASS_NAME_CANNOT_BE,
            );
        }

        // TS2725: Class name cannot be 'Object' when targeting ES5 and above with module X
        // Applies to non-ES module kinds (CommonJS, AMD, UMD, System) and non-ambient classes.
        // For Node16/NodeNext/Node18/Node20, only applies when the file is CJS format
        // (determined by package.json "type" field and file extension).
        if class.name.is_some()
            && !self.has_declare_modifier(&class.modifiers)
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == "Object"
        {
            use tsz_common::common::ModuleKind;
            let module = self.ctx.compiler_options.module;
            let module_name = match module {
                ModuleKind::CommonJS => Some("CommonJS"),
                ModuleKind::AMD => Some("AMD"),
                ModuleKind::UMD => Some("UMD"),
                ModuleKind::System => Some("System"),
                // For Node module kinds, only emit for CJS-format files
                ModuleKind::Node16
                | ModuleKind::Node18
                | ModuleKind::Node20
                | ModuleKind::NodeNext => {
                    let file_is_cjs = match self.ctx.file_is_esm {
                        Some(true) => false,
                        Some(false) => true,
                        None => {
                            // Fallback: use file extension heuristic
                            let f = &self.ctx.file_name;
                            !f.ends_with(".mjs") && !f.ends_with(".mts")
                        }
                    };
                    if file_is_cjs {
                        match module {
                            ModuleKind::Node16 => Some("Node16"),
                            ModuleKind::Node18 => Some("Node18"),
                            ModuleKind::Node20 => Some("Node20"),
                            ModuleKind::NodeNext => Some("NodeNext"),
                            _ => unreachable!(),
                        }
                    } else {
                        None
                    }
                }
                _ => None, // ES modules and None don't trigger this error
            };
            if let Some(module_name) = module_name {
                self.error_at_node(
                    class.name,
                    &format!(
                        "Class name cannot be 'Object' when targeting ES5 and above with module {module_name}."
                    ),
                    diagnostic_codes::CLASS_NAME_CANNOT_BE_OBJECT_WHEN_TARGETING_ES5_AND_ABOVE_WITH_MODULE,
                );
            }
        }

        // Check if this is a declared class (ambient declaration)
        let is_declared = self.is_ambient_class_declaration(stmt_idx);

        // Check if this class is abstract
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        // Push type parameters BEFORE checking heritage clauses and abstract members
        // This allows heritage clauses and member checks to reference the class's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        self.check_duplicate_type_parameters(&class.type_parameters);
        let class_name_str = self
            .ctx
            .arena
            .get(class.name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.to_string());
        if let Some(ref name) = class_name_str {
            self.check_type_parameters_for_missing_names_with_enclosing(
                &class.type_parameters,
                name,
            );
        } else {
            self.check_type_parameters_for_missing_names(&class.type_parameters);
        }

        // Collect class type parameter names for TS2302 checking in static members
        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&class.type_parameters, stmt_idx);
        // In JS files, @template type parameters come from JSDoc, not AST.
        if class.type_parameters.is_none() {
            self.check_unused_jsdoc_template_type_params(stmt_idx);
        }

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &class.heritage_clauses,
            true,
            &class_type_param_names,
        );

        // Check for abstract members in non-abstract class (error 1253),
        // private identifiers in ambient classes (error 2819),
        // and private identifiers when targeting ES5 or lower (error 18028)
        for &member_idx in &class.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                // Get member name for private identifier checks
                let member_name_idx = match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .map(|p| p.name),
                    syntax_kind_ext::METHOD_DECLARATION => {
                        self.ctx.arena.get_method_decl(member_node).map(|m| m.name)
                    }
                    syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                        self.ctx.arena.get_accessor(member_node).map(|a| a.name)
                    }
                    _ => None,
                };
                let Some(member_name_idx) = member_name_idx else {
                    continue;
                };

                // Check if member has a private identifier name
                let is_private_identifier =
                    self.ctx.arena.get(member_name_idx).is_some_and(|node| {
                        node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    });

                if is_private_identifier {
                    use crate::context::ScriptTarget;

                    // TS18028: Check for private identifiers when targeting ES5 or lower
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );
                    if is_es5_or_lower {
                        self.error_at_node(
                            member_name_idx,
                            diagnostic_messages::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                            diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                        );
                    }

                    // Get member modifiers for TS18010/TS18019 checks
                    let member_modifiers: Option<&Option<tsz_parser::parser::NodeList>> =
                        match member_node.kind {
                            syntax_kind_ext::PROPERTY_DECLARATION => self
                                .ctx
                                .arena
                                .get_property_decl(member_node)
                                .map(|p| &p.modifiers),
                            syntax_kind_ext::METHOD_DECLARATION => self
                                .ctx
                                .arena
                                .get_method_decl(member_node)
                                .map(|m| &m.modifiers),
                            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                                .ctx
                                .arena
                                .get_accessor(member_node)
                                .map(|a| &a.modifiers),
                            _ => None,
                        };

                    if let Some(modifiers) = member_modifiers {
                        // TS18010: An accessibility modifier cannot be used with a private identifier.
                        // tsc points the error at the modifier node, not the member.
                        let accessibility_modifier = self
                            .ctx
                            .arena
                            .find_modifier(modifiers, tsz_scanner::SyntaxKind::PublicKeyword)
                            .or_else(|| {
                                self.ctx.arena.find_modifier(
                                    modifiers,
                                    tsz_scanner::SyntaxKind::PrivateKeyword,
                                )
                            })
                            .or_else(|| {
                                self.ctx.arena.find_modifier(
                                    modifiers,
                                    tsz_scanner::SyntaxKind::ProtectedKeyword,
                                )
                            });
                        // In JS files, accessibility modifiers come from JSDoc tags
                        // (@public, @private, @protected) rather than AST modifiers.
                        let has_jsdoc_accessibility = accessibility_modifier.is_none()
                            && self.is_js_file()
                            && self.has_jsdoc_accessibility_modifier(member_idx);
                        if let Some(mod_idx) = accessibility_modifier {
                            self.error_at_node(
                                mod_idx,
                                diagnostic_messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                                diagnostic_codes::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            );
                        } else if has_jsdoc_accessibility {
                            self.error_at_node(
                                member_idx,
                                diagnostic_messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                                diagnostic_codes::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            );
                        }

                        // TS18019: 'declare'/'abstract' modifier cannot be used with a private identifier.
                        // Only applies to property declarations. tsc points at the modifier node.
                        if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                            if let Some(mod_idx) = self
                                .ctx
                                .arena
                                .find_modifier(modifiers, tsz_scanner::SyntaxKind::DeclareKeyword)
                            {
                                self.error_at_node_msg(
                                    mod_idx,
                                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                                    &["declare"],
                                );
                            }
                            if let Some(mod_idx) = self
                                .ctx
                                .arena
                                .find_modifier(modifiers, tsz_scanner::SyntaxKind::AbstractKeyword)
                            {
                                self.error_at_node_msg(
                                    mod_idx,
                                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                                    &["abstract"],
                                );
                            }
                        }
                    }
                }

                // TS1024: 'readonly' modifier can only appear on a property declaration or index signature.
                {
                    let has_readonly_on_non_property = match member_node.kind {
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                                self.has_readonly_modifier(&method.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                                self.has_readonly_modifier(&accessor.modifiers)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if has_readonly_on_non_property {
                        self.error_at_node(
                            member_idx,
                            diagnostic_messages::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                            diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                        );
                    }
                }

                // Check for abstract members in non-abstract class
                if !is_abstract_class {
                    let member_has_abstract = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => {
                            if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                                self.has_abstract_modifier(&prop.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                                self.has_abstract_modifier(&method.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                                self.has_abstract_modifier(&accessor.modifiers)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if member_has_abstract {
                        // TS1244 for methods/accessors, TS1253 for properties.
                        // tsc anchors at the start of the declaration (including
                        // the `abstract` modifier), not at the member name.
                        // error_at_node trims to the name, so use
                        // error_at_position with the raw member-node span.
                        let is_method = matches!(
                            member_node.kind,
                            syntax_kind_ext::METHOD_DECLARATION
                                | syntax_kind_ext::GET_ACCESSOR
                                | syntax_kind_ext::SET_ACCESSOR
                        );
                        let (start, length) = (member_node.pos, member_node.end - member_node.pos);
                        if is_method {
                            self.error_at_position(
                                start,
                                length,
                                "Abstract methods can only appear within an abstract class.",
                                diagnostic_codes::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                            );
                        } else {
                            self.error_at_position(
                                start,
                                length,
                                "Abstract properties can only appear within an abstract class.",
                                diagnostic_codes::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                            );
                        }
                    }
                }

                // TS1267: Abstract property cannot have an initializer
                if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                {
                    if self.has_abstract_modifier(&prop.modifiers) && prop.initializer.is_some() {
                        let name = self.get_member_name_text(prop.name).unwrap_or_default();
                        self.error_at_node_msg(
                                prop.name,
                                diagnostic_codes::PROPERTY_CANNOT_HAVE_AN_INITIALIZER_BECAUSE_IT_IS_MARKED_ABSTRACT,
                                &[&name],
                            );
                    }

                    let name = self.get_member_name_text(prop.name).unwrap_or_default();

                    // TS18006: Classes may not have a field named 'constructor'
                    if name == "constructor" {
                        self.error_at_node(
                                prop.name,
                                crate::diagnostics::diagnostic_messages::CLASSES_MAY_NOT_HAVE_A_FIELD_NAMED_CONSTRUCTOR,
                                diagnostic_codes::CLASSES_MAY_NOT_HAVE_A_FIELD_NAMED_CONSTRUCTOR,
                            );
                    }

                    // TS2699: Static property 'prototype' conflicts with Function.prototype
                    // Not reported in ambient contexts (declare class).
                    if name == "prototype"
                        && self.has_static_modifier(&prop.modifiers)
                        && !is_declared
                    {
                        let class_name = self.get_class_name_from_decl(stmt_idx);
                        self.error_at_node_msg(
                                prop.name,
                                diagnostic_codes::STATIC_PROPERTY_CONFLICTS_WITH_BUILT_IN_PROPERTY_FUNCTION_OF_CONSTRUCTOR_FUNCTIO,
                                &["prototype", &class_name],
                            );
                    }
                }

                // TS2699/TS2300: Static method/accessor named 'prototype' conflicts
                // with Function.prototype and is a duplicate identifier.
                if matches!(
                    member_node.kind,
                    syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR
                ) {
                    let (name_idx, modifiers) = match member_node.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(member_node)
                            .map(|m| (m.name, &m.modifiers)),
                        _ => self
                            .ctx
                            .arena
                            .get_accessor(member_node)
                            .map(|a| (a.name, &a.modifiers)),
                    }
                    .unzip();
                    if let (Some(name_idx), Some(modifiers)) = (name_idx, modifiers) {
                        let name = self.get_member_name_text(name_idx).unwrap_or_default();
                        if name == "prototype"
                            && self.has_static_modifier(modifiers)
                            && !is_declared
                        {
                            let class_name = self.get_class_name_from_decl(stmt_idx);
                            // TS2300: Duplicate identifier 'prototype'
                            self.error_at_node_msg(
                                name_idx,
                                diagnostic_codes::DUPLICATE_IDENTIFIER,
                                &["prototype"],
                            );
                            // TS2699: Static property conflicts with Function.prototype
                            self.error_at_node_msg(
                                name_idx,
                                diagnostic_codes::STATIC_PROPERTY_CONFLICTS_WITH_BUILT_IN_PROPERTY_FUNCTION_OF_CONSTRUCTOR_FUNCTIO,
                                &["prototype", &class_name],
                            );
                        }
                    }
                }
            }
        }

        // Collect class name
        let class_name = self.get_class_name_from_decl(stmt_idx);

        // Save previous enclosing class and set current.
        // Push the outer class onto the chain so protected access checks can
        // walk up to find the correct enclosing class in the inheritance hierarchy.
        let prev_enclosing_class = self.ctx.enclosing_class.take();
        if let Some(ref prev) = prev_enclosing_class {
            self.ctx.enclosing_class_chain.push(prev.class_idx);
        }
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx: stmt_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared,
            in_static_property_initializer: false,
            in_static_member: false,
            has_super_call_in_current_constructor: false,
            cached_instance_this_type: None,
            type_param_names: class_type_param_names,
            class_type_parameters: _type_params,
        });

        // Drop any value-side or instance-side class shape cached during the
        // earlier environment-building pass. Member checking needs a fresh view
        // so `this` inside methods observes the checked class shape rather than
        // a provisional snapshot.
        //
        // For the constructor type cache, we save and temporarily restore it
        // rather than clearing entirely. This prevents a cycle when a generic
        // class has a static member whose type references itself (e.g.,
        // `private static instance: Bar<string>`). Without a cached
        // constructor type, recomputation during member body checking can
        // re-enter `get_class_constructor_type` and hit cycle detection,
        // returning the instance type as a fallback instead of the correct
        // constructor type. The cache is definitively cleared and refreshed
        // after member checking completes (see below).
        self.ctx.class_instance_type_cache.remove(&stmt_idx);
        // Clear the constructor type cache for a fresh view. Save the old
        // value so it can be temporarily restored during member checking to
        // prevent cycles. When a generic class has a private static member
        // whose type references itself (e.g., `private static instance:
        // Bar<string>`), recomputing the class type during method body
        // checking can re-enter get_class_constructor_type and hit cycle
        // detection. Without a valid fallback, the cycle returns the instance
        // type instead of the constructor type, causing false TS2339 errors.
        self.ctx.class_constructor_type_cache.remove(&stmt_idx);
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(stmt_idx) {
            self.ctx.symbol_types.remove(&sym_id);
        }
        if class.name.is_some()
            && let Some(ident) = self.ctx.arena.get_identifier_at(class.name)
            && let Some(name_sym) = self.ctx.binder.file_locals.get(&ident.escaped_text)
        {
            self.ctx.symbol_types.remove(&name_sym);
        }

        // Class bodies reset the async context — field initializers and static blocks
        // don't inherit async from the enclosing function. Methods define their own context.
        let saved_async_depth = self.ctx.async_depth;
        self.ctx.async_depth = 0;

        // Check each class member
        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        self.ctx.async_depth = saved_async_depth;

        // Check for duplicate member names (TS2300, TS2393)
        self.check_duplicate_class_members(&class.members.nodes);

        // Check for missing method/constructor implementations (2389, 2390, 2391)
        // Skip for declared classes (ambient declarations don't need implementations)
        if !is_declared {
            self.check_class_member_implementations(&class.members.nodes);

            // Check static/instance consistency for method overloads (TS2387, TS2388)
            // In `declare class`, static and instance methods with the same name are
            // separate declarations, not overload signatures.
            self.check_static_instance_overload_consistency(&class.members.nodes);
        }

        // Check abstract consistency for method overloads (TS2512)
        self.check_abstract_overload_consistency(&class.members.nodes);

        // Check consecutive abstract declarations (TS2516)
        self.check_abstract_method_consecutive_declarations(&class.members.nodes);

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check getter/setter type compatibility when getter type is inferred (TS2322).
        // TS 5.1+ allows unrelated types only when both are explicitly annotated.
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check strict property initialization (TS2564)
        self.check_property_initialization(stmt_idx, class, is_declared, is_abstract_class);

        // TS2417 (classExtendsNull2): a class that extends `null` and merges with an
        // interface that has heritage must report static-side incompatibility with `null`.
        if self.class_extends_null(class) && self.class_has_merged_interface_extends(class) {
            let class_name = if let Some(name_node) = self.ctx.arena.get(class.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map_or_else(|| "<anonymous>".to_string(), |id| id.escaped_text.clone())
            } else {
                "<anonymous>".to_string()
            };
            self.error_at_node(
                class.name,
                &format!(
                    "Class static side 'typeof {class_name}' incorrectly extends base class static side 'null'."
                ),
                diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
            );
        }

        // TS2509: Base constructor return type is not an object type or intersection of
        // object types with statically known members.
        self.check_base_constructor_return_type(class);

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(stmt_idx, class);

        // TS2797: A mixin class that extends from a type variable containing an
        // abstract construct signature must also be declared 'abstract'.
        if !is_abstract_class {
            self.check_mixin_abstract_construct_constraint(stmt_idx, class);
        }

        // TS2545: A mixin class must have a constructor with a single rest parameter
        // of type 'any[]'.
        self.check_mixin_constructor_rest_parameter(stmt_idx, class);

        // Check that non-abstract class implements all abstract members from base class (error 2654)
        self.check_abstract_member_implementations(stmt_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(stmt_idx, class);

        // Check JSDoc @implements tags (JS files only)
        self.check_jsdoc_implements_clauses(stmt_idx, class);

        // Check JSDoc @extends/@augments name matches actual extends clause (TS8023, JS files only)
        self.check_jsdoc_extends_name_mismatch(stmt_idx, class);

        // Check that class properties are compatible with index signatures (TS2411)
        // Get the class instance type (not constructor type) to access instance index signatures
        let class_instance_type = self.get_class_instance_type(stmt_idx, class);
        self.check_index_signature_compatibility(
            &class.members.nodes,
            class_instance_type,
            stmt_idx,
        );

        for &member_idx in &class.members.nodes {
            self.check_index_signature_parameter_type(member_idx);
        }

        self.check_class_declaration(stmt_idx);

        // TS4094: Property of exported anonymous class type may not be private or protected.
        // When `declaration: true`, anonymous class types in exported positions cannot have
        // private/protected members represented in .d.ts files.
        // Anchor at the export statement, not the class keyword — tsc reports at the
        // `export` position (col 1), which is the parent when class is a ClassExpression.
        if self.ctx.emit_declarations() && !self.ctx.is_declaration_file() {
            if class.name.is_none() {
                if let Some(report_at) =
                    self.get_anonymous_class_export_anchor(stmt_idx, &class.modifiers)
                {
                    // Use the solver's ObjectShape to get ALL properties including inherited
                    // ones, not just the direct AST members.
                    self.report_instance_type_private_members_as_ts4094(
                        report_at,
                        class_instance_type,
                    );
                }
            } else {
                // Named exported class extending an anonymous class base: the base's
                // private/protected members appear in the .d.ts type literal for the
                // anonymous heritage type.  Report at the named class's name node.
                //
                // Two patterns for exported named classes:
                // 1. `export class Foo` — TSZ wraps CLASS_DECLARATION in an EXPORT_DECLARATION
                //    node; the class's own `modifiers` list is empty, so we check the parent.
                // 2. `class Foo` with `export` in modifiers — less common but possible.
                let is_exported = self
                    .ctx
                    .arena
                    .has_modifier(&class.modifiers, tsz_scanner::SyntaxKind::ExportKeyword)
                    || self
                        .ctx
                        .arena
                        .get_extended(stmt_idx)
                        .and_then(|ext| self.ctx.arena.get(ext.parent))
                        .is_some_and(|parent| parent.kind == syntax_kind_ext::EXPORT_DECLARATION);
                if is_exported {
                    self.check_ts4094_named_class_anonymous_heritage(stmt_idx, class);
                }
            }
        }

        self.check_inherited_properties_against_index_signatures(
            class_instance_type,
            &class.members.nodes,
            stmt_idx,
        );

        // Check for decorator-related global types (TS2318)
        // When experimentalDecorators is enabled and a method/accessor has decorators,
        // TypedPropertyDescriptor must be available
        self.check_decorator_global_types(&class.members.nodes);

        // Restore previous enclosing class and pop the chain
        self.ctx.enclosing_class = prev_enclosing_class;
        if self.ctx.enclosing_class.is_some() {
            self.ctx.enclosing_class_chain.pop();
        }

        // Check variance annotations match actual usage (TS2636)
        self.check_variance_annotations(stmt_idx, &class.type_parameters);

        self.pop_type_parameters(type_param_updates);

        let mut refresh_symbols = Vec::new();
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(stmt_idx) {
            refresh_symbols.push(sym_id);
        }
        if class.name.is_some()
            && let Some(ident) = self.ctx.arena.get_identifier_at(class.name)
            && let Some(name_sym) = self.ctx.binder.file_locals.get(&ident.escaped_text)
            && !refresh_symbols.contains(&name_sym)
        {
            refresh_symbols.push(name_sym);
        }

        self.ctx.checked_classes.insert(stmt_idx);
        self.ctx.checking_classes.remove(&stmt_idx);

        // Class value-side constructor shapes may be cached during
        // build_type_environment before JSDoc/template/member inference stabilizes.
        // Refresh them after the checked pass so following statements observe the
        // finalized constructor signatures and instance return types.
        self.ctx.class_constructor_type_cache.remove(&stmt_idx);
        for sym_id in refresh_symbols {
            self.ctx.symbol_types.remove(&sym_id);
            let _ = self.get_type_of_symbol(sym_id);
        }

        for (decorator_expression, decorator_type) in experimental_class_decorators {
            // Experimental decorators: tsc anchors TS1238 at the expression (after @).
            self.check_class_decorator_call_signature(
                decorator_expression,
                decorator_type,
                stmt_idx,
                class,
            );
        }
    }

    #[allow(dead_code)]
    pub(crate) fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        self.check_class_expression_with_request(class_idx, class, &TypingRequest::NONE);
    }

    pub(crate) fn check_class_expression_with_request(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        request: &TypingRequest,
    ) {
        // TS1206: With --experimentalDecorators, decorators on class expressions
        // are not valid. Only ES decorators (TC39 Stage 3) support class expressions.
        if self.ctx.compiler_options.experimental_decorators
            && let Some(modifiers) = &class.modifiers
        {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        mod_idx,
                        "Decorators are not valid here.",
                        diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                    );
                }
            }
        }

        // TS8004: Type parameters on class expression in JS files
        if self.is_js_file() {
            self.error_if_ts_only_type_params(&class.type_parameters);

            // Also check members for JS grammar errors
            for &member_idx in &class.members.nodes {
                self.check_js_grammar_class_member(member_idx);
            }
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        self.check_duplicate_type_parameters(&class.type_parameters);
        let class_name = self.get_class_name_from_decl(class_idx);
        if class.name != NodeIndex::NONE && !class_name.is_empty() {
            self.check_type_parameters_for_missing_names_with_enclosing(
                &class.type_parameters,
                &class_name,
            );
        } else {
            self.check_type_parameters_for_missing_names(&class.type_parameters);
        }

        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check heritage clauses for primitive type keywords (TS2863/TS2864).
        // Uses the lightweight check to avoid triggering constructor accessibility (TS2675)
        // side effects that the full check_heritage_clauses_for_unresolved_names would cause
        // via get_type_of_node on extends expressions (e.g., nested class extending private ctor).
        self.check_heritage_clauses_for_primitive_types(&class.heritage_clauses);

        let class_name = self.get_class_name_from_decl(class_idx);
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        let prev_enclosing_class = self.ctx.enclosing_class.take();
        if let Some(ref prev) = prev_enclosing_class {
            self.ctx.enclosing_class_chain.push(prev.class_idx);
        }
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared: false,
            in_static_property_initializer: false,
            in_static_member: false,
            has_super_call_in_current_constructor: false,
            cached_instance_this_type: None,
            type_param_names: class_type_param_names,
            class_type_parameters: _type_params,
        });

        // Class bodies reset the async context — field initializers don't
        // inherit async from the enclosing function.
        let saved_async_depth = self.ctx.async_depth;
        self.ctx.async_depth = 0;

        for &member_idx in &class.members.nodes {
            self.check_class_member_with_request(member_idx, request);

            // Check for abstract members in non-abstract class expressions (TS1253/TS1244)
            if !is_abstract_class && let Some(member_node) = self.ctx.arena.get(member_idx) {
                use crate::diagnostics::diagnostic_codes;

                let member_has_abstract = match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                            self.has_abstract_modifier(&prop.modifiers)
                        } else {
                            false
                        }
                    }
                    syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                            self.has_abstract_modifier(&method.modifiers)
                        } else {
                            false
                        }
                    }
                    syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                        if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                            self.has_abstract_modifier(&accessor.modifiers)
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                if member_has_abstract {
                    let is_method = matches!(
                        member_node.kind,
                        syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::GET_ACCESSOR
                            | syntax_kind_ext::SET_ACCESSOR
                    );
                    // tsc anchors TS1244/TS1253 at the start of the
                    // declaration (including the `abstract` modifier),
                    // not at the member name.
                    let (start, length) = (member_node.pos, member_node.end - member_node.pos);
                    if is_method {
                        self.error_at_position(
                            start,
                            length,
                            "Abstract methods can only appear within an abstract class.",
                            diagnostic_codes::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                        );
                    } else {
                        self.error_at_position(
                            start,
                            length,
                            "Abstract properties can only appear within an abstract class.",
                            diagnostic_codes::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                        );
                    }
                }
            }
        }

        self.ctx.async_depth = saved_async_depth;

        // Check strict property initialization (TS2564) for class expressions
        // Class expressions should have the same property initialization checks as class declarations
        self.check_property_initialization(class_idx, class, false, is_abstract_class);

        // Check for duplicate member names (TS2300, TS2393)
        self.check_duplicate_class_members(&class.members.nodes);

        // Check for missing method/constructor implementations (2389, 2390, 2391)
        self.check_class_member_implementations(&class.members.nodes);

        // Check static/instance consistency for method overloads (TS2387, TS2388)
        self.check_static_instance_overload_consistency(&class.members.nodes);

        // Check abstract consistency for method overloads (TS2512)
        self.check_abstract_overload_consistency(&class.members.nodes);

        // Check consecutive abstract declarations (TS2516)
        self.check_abstract_method_consecutive_declarations(&class.members.nodes);

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check getter/setter type compatibility when getter type is inferred (TS2322).
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(class_idx, class);

        // TS2545: A mixin class must have a constructor with a single rest parameter
        // of type 'any[]'.
        self.check_mixin_constructor_rest_parameter(class_idx, class);

        // Check that non-abstract class implements all abstract members from base class (error 2653, 2656)
        self.check_abstract_member_implementations(class_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(class_idx, class);

        // Check JSDoc @implements tags (JS files only)
        self.check_jsdoc_implements_clauses(class_idx, class);

        // Check JSDoc @extends/@augments name matches actual extends clause (TS8023, JS files only)
        self.check_jsdoc_extends_name_mismatch(class_idx, class);

        // Check that class properties are compatible with index signatures (TS2411)
        // Get the class instance type (not constructor type) to access instance index signatures
        let class_instance_type = self.get_class_instance_type(class_idx, class);
        self.check_index_signature_compatibility(
            &class.members.nodes,
            class_instance_type,
            class_idx,
        );

        // Check for decorator-related global types (TS2318)
        self.check_decorator_global_types(&class.members.nodes);

        self.ctx.enclosing_class = prev_enclosing_class;
        if self.ctx.enclosing_class.is_some() {
            self.ctx.enclosing_class_chain.pop();
        }

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_property_initialization(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        is_declared: bool,
        _is_abstract: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip TS2564 for declared classes (ambient declarations) and .d.ts files.
        // In tsc, .d.ts files are inherently ambient even without the `declare` keyword.
        // Note: Abstract classes DO get TS2564 errors - they can have constructors
        // and properties must be initialized either with defaults or in the constructor
        if is_declared || self.ctx.is_declaration_file() {
            return;
        }

        // Only check property initialization when strictPropertyInitialization is enabled
        // tsc also requires strictNullChecks to be enabled for TS2564
        if !self.ctx.strict_property_initialization() || !self.ctx.strict_null_checks() {
            return;
        }

        // tsc suppresses TS2564 when the file contains structural parse errors
        // (errors that set `containsParseError` in tsc's parser). In tsc,
        // `containsParseError` propagates up through the parent chain to the
        // source file node, and tsc checks this flag at the source-file level.
        //
        // We use `has_structural_parse_errors` which specifically tracks parse
        // errors that cause AST malformation (TS1005, TS1068, TS1109, etc.)
        // as opposed to grammar checks (TS1101 "with" in strict mode) that
        // don't affect AST structure and don't set `containsParseError` in tsc.
        if self.ctx.has_structural_parse_errors {
            return;
        }

        // Check if this is a derived class (has base class)
        let summary = self.summarize_class_initialization(class_idx, class);
        if summary.required_instance_fields.is_empty() {
            return;
        }

        for field in &summary.required_instance_fields {
            let Some(key) = field.key.as_ref() else {
                continue;
            };
            // Property is assigned if it's in the constructor-assigned set.
            // Note: parameter properties (e.g. `constructor(public y: number)`) do NOT
            // count as initialization of a separate explicit property declaration with
            // the same name. In tsc, `y: number;` + `constructor(public y: number)`
            // produces both TS2300 (duplicate) AND TS2564 (not initialized).
            if summary.constructor_assigned_fields.contains(key) {
                continue;
            }
            use crate::diagnostics::format_message;

            // Use TS2524 if there's a constructor (definite assignment analysis)
            // Use TS2564 if no constructor (just missing initializer)
            let (message, code) = (
                diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
            );

            self.error_at_node(
                field.name_idx,
                &format_message(message, &[field.display_name.as_str()]),
                code,
            );
        }

        // Check for TS2565 (Property used before being assigned in constructor)
        if let Some(body_idx) = summary.constructor_body {
            check_constructor_property_use_before_assignment(
                self,
                body_idx,
                &summary.required_instance_field_keys,
                summary.requires_super,
            );
        }
    }

    pub(crate) fn property_requires_initialization(
        &mut self,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        _is_derived_class: bool,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        if prop.initializer.is_some()
            || prop.question_token
            || prop.exclamation_token
            || self.has_static_modifier(&prop.modifiers)
            || self.has_abstract_modifier(&prop.modifiers)
            || self.has_declare_modifier(&prop.modifiers)
        {
            return false;
        }

        // Stage 3 (ES) decorated properties don't require initialization — the
        // decorator can intercept the property definition and provide an initial
        // value at runtime. TSC suppresses TS2564 for decorated properties when
        // using ES decorators (experimentalDecorators is NOT enabled).
        // With legacy experimentalDecorators, decorators are metadata-only and
        // don't affect initialization, so TS2564 is still required.
        if !self.ctx.compiler_options.experimental_decorators
            && let Some(ref modifiers) = prop.modifiers
        {
            let has_decorator = modifiers.nodes.iter().any(|&mod_idx| {
                self.ctx
                    .arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::DECORATOR)
            });
            if has_decorator {
                return false;
            }
        }

        // Properties with string or numeric literal names are not checked for strict property initialization
        // Example: class C { "b": number; 0: number; }  // These are not checked
        let Some(name_node) = self.ctx.arena.get(prop.name) else {
            return false;
        };
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        let prop_type = if let Some(declared_type) =
            self.effective_class_property_declared_type(member_idx, prop)
        {
            declared_type
        } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ANY
        };

        // Property initialization checking:
        // 1. ANY/UNKNOWN types don't need initialization
        // 2. Union types with undefined don't need initialization
        // 3. Optional types don't need initialization
        // 4. Type parameters (unconstrained or constrained to allow undefined)
        if prop_type == TypeId::ANY || prop_type == TypeId::UNKNOWN {
            return false;
        }

        // ERROR types also don't need initialization - these indicate parsing/binding errors
        if prop_type == TypeId::ERROR {
            return false;
        }

        // Check if undefined is assignable to the property type.
        // This handles: union types with undefined, type parameters with
        // unconstrained or undefined-allowing constraints (mirrors tsc's
        // `isTypeAssignableTo(undefinedType, type)` check for TS2564).
        !class_query::undefined_is_assignable_to(self.ctx.types, prop_type)
    }

    /// Check for TS2565: Properties used before being assigned in the constructor.
    ///
    /// This function analyzes the constructor body to detect when a property
    /// is accessed (via `this.X`) before it has been assigned a value.
    pub(crate) fn check_properties_used_before_assigned(
        &mut self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) {
        if body_idx.is_none() {
            return;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        let start_idx = if require_super {
            self.find_super_statement_start(&block.statements.nodes)
                .unwrap_or(0)
        } else {
            0
        };

        let mut assigned = FxHashSet::default();

        // Track parameter properties as already assigned
        for _key in tracked {
            // Parameter properties are assigned in the parameter list
            // We'll collect them separately if needed
        }

        // Analyze statements in order, checking for property accesses before assignment
        for &stmt_idx in block.statements.nodes.iter().skip(start_idx) {
            self.check_statement_for_early_property_access(stmt_idx, &mut assigned, tracked);
        }
    }

    /// Check a single statement for property accesses that occur before assignment.
    /// Returns true if the statement definitely assigns to the tracked property.
    pub(crate) fn check_statement_for_early_property_access(
        &mut self,
        stmt_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> bool {
        if stmt_idx.is_none() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement_for_early_property_access(stmt_idx, assigned, tracked);
                    }
                }
                false
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.check_expression_for_early_property_access(
                        expr_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    // Check the condition expression for property accesses
                    self.check_expression_for_early_property_access(
                        if_stmt.expression,
                        assigned,
                        tracked,
                    );
                    // Check both branches
                    let mut then_assigned = assigned.clone();
                    let mut else_assigned = assigned.clone();
                    self.check_statement_for_early_property_access(
                        if_stmt.then_statement,
                        &mut then_assigned,
                        tracked,
                    );
                    if if_stmt.else_statement.is_some() {
                        self.check_statement_for_early_property_access(
                            if_stmt.else_statement,
                            &mut else_assigned,
                            tracked,
                        );
                    }
                    // Properties assigned in both branches are considered assigned
                    *assigned = then_assigned
                        .intersection(&else_assigned)
                        .cloned()
                        .collect();
                }
                false
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret_stmt) = self.ctx.arena.get_return_statement(node)
                    && ret_stmt.expression.is_some()
                {
                    self.check_expression_for_early_property_access(
                        ret_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                // For loops, we conservatively don't track assignments across iterations
                // This is a simplified approach - the full TypeScript implementation is more complex
                false
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.ctx.arena.get_try(node) {
                    self.check_statement_for_early_property_access(
                        try_stmt.try_block,
                        assigned,
                        tracked,
                    );
                    // Check catch and finally blocks
                    // ...
                }
                false
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    // var_stmt.declarations contains VariableDeclarationList nodes,
                    // each of which in turn contains the actual VariableDeclaration nodes.
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx)
                            && let Some(decl_list) = self.ctx.arena.get_variable(decl_list_node)
                        {
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && let Some(decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    && decl.initializer.is_some()
                                {
                                    self.check_expression_for_early_property_access(
                                        decl.initializer,
                                        assigned,
                                        tracked,
                                    );
                                }
                            }
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Check for decorator-related global types (TS2318).
    ///
    /// When experimentalDecorators is enabled and a method or accessor has decorators,
    /// TypeScript requires the `TypedPropertyDescriptor` type to be available.
    /// If it's not available (e.g., with noLib), we emit TS2318.
    pub(crate) fn check_decorator_global_types(&mut self, members: &[NodeIndex]) {
        // Only check if experimentalDecorators is enabled
        if !self.ctx.compiler_options.experimental_decorators {
            return;
        }

        // Check if any method or accessor has decorators
        let mut has_method_or_accessor_decorator = false;
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            let modifiers = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => continue,
            };

            if let Some(mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                        && mod_node.kind == syntax_kind_ext::DECORATOR
                    {
                        has_method_or_accessor_decorator = true;
                        break;
                    }
                }
            }
            if has_method_or_accessor_decorator {
                break;
            }
        }

        if !has_method_or_accessor_decorator {
            return;
        }

        // Check if TypedPropertyDescriptor is available
        let type_name = "TypedPropertyDescriptor";
        if self.ctx.has_name_in_lib(type_name) {
            return; // Type is available from lib
        }
        if self.ctx.binder.file_locals.has(type_name) {
            return; // Type is declared locally
        }

        // TypedPropertyDescriptor is not available - emit TS2318
        // TSC emits this error twice for method decorators
        let file_name = self.ctx.file_name.clone();
        self.error_global_type_missing_at_position(type_name, file_name.clone(), 0, 0);
        self.error_global_type_missing_at_position(type_name, file_name, 0, 0);
    }

    /// TS1238: Check that a class decorator expression has a compatible call signature.
    ///
    /// For experimental decorators, the decorator is called as `decoratorExpr(classConstructor)`.
    /// If the decorator type has no call signatures, or if call resolution against the class
    /// constructor type fails, emit TS1238.
    fn check_class_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::query_boundaries::common::call_signatures_for_type;

        // Skip validation for error types or any — these won't produce meaningful diagnostics
        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        // Resolve Lazy(DefId) references and evaluate applications so that
        // type queries can see the underlying type shape.
        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);

        // After evaluation, any/unknown/error → skip
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // Check if the decorator type is callable.
        // TypeData::Function has a single call signature (function declarations/expressions).
        // TypeData::Callable has overloaded call/construct signatures (interfaces).
        let has_call_signatures = class_query::has_function_shape(self.ctx.types, resolved)
            || call_signatures_for_type(self.ctx.types, resolved)
                .is_some_and(|sigs| !sigs.is_empty());

        if !has_call_signatures {
            // No call signatures at all (e.g., a class used as decorator — has construct
            // signatures but no call signatures). Emit TS1238.
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
            return;
        }

        // Has call signatures — try to resolve the call with the class constructor type.
        // resolve_call handles both Function and Callable types internally.
        // If resolution fails (type mismatch, arity error), emit TS1238.
        let class_constructor_type = self.get_class_constructor_type(class_idx, class);
        if class_constructor_type == TypeId::ERROR {
            return;
        }

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[class_constructor_type],
            false,
            None,
            None,
        );

        if !matches!(
            result,
            crate::query_boundaries::common::CallResult::Success(_)
        ) {
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
        }
    }

    /// TS1238 for ES decorators: check that a class decorator doesn't require
    /// more than 2 parameters.
    ///
    /// ES decorators receive `(value, context)` — at most 2 arguments.
    /// If the decorator function's call signature has more than 2 required
    /// parameters, the call will fail. Emit TS1238.
    fn check_es_class_decorator_arity(
        &mut self,
        decorator_node: NodeIndex,
        decorator_expression: NodeIndex,
        decorator_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // Check the function shape for required parameter count
        if let Some(shape) =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, resolved)
        {
            let required_params = shape
                .params
                .iter()
                .filter(|p| !p.optional && !p.rest)
                .count();
            // ES decorators are invoked with `(value, context)`.
            //
            // * When the factory has no parameters at all, the runtime call
            //   `f(value, context)` passes extra args; tsc anchors the error
            //   at the decorator expression (excluding `@`).
            // * When the factory requires more than two parameters, the call
            //   cannot supply them; tsc anchors the error at the whole
            //   decorator (including `@`).
            if shape.params.is_empty() {
                self.error_at_node(
                    decorator_expression,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                );
            } else if required_params > 2 {
                self.error_at_node(
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                );
            }
        }
    }

    /// TS2509: Check that the base constructor return type is an object type or
    /// intersection of object types with statically known members.
    ///
    /// When a class extends another via a heritage clause, the return type of
    /// the base constructor must be valid. For example, if `Mix(Private, Private2)`
    /// returns an intersection that reduces to `never`, this is not a valid base type.
    fn check_base_constructor_return_type(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            let (expr_idx, type_arguments) = if let Some(type_node) = self.ctx.arena.get(type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                (
                    expr_type_args.expression,
                    expr_type_args.type_arguments.as_ref(),
                )
            } else {
                (type_idx, None)
            };

            // Get the base instance type (constructor return type)
            let Some(base_type) = self.base_instance_type_from_expression(expr_idx, type_arguments)
            else {
                continue;
            };

            // Skip for any/error/null — these are permissive.
            // `class extends null` is valid TS (produces a class with no prototype).
            if base_type == TypeId::ANY || base_type == TypeId::ERROR || base_type == TypeId::NULL {
                continue;
            }

            // Skip for null — `class C extends null` is valid in TypeScript.
            // tsc does not emit TS2509 for null base types; instead, it only
            // checks for TS17005 (super call in null-extending class).
            if base_type == TypeId::NULL {
                continue;
            }

            // Skip union base types. When a constructor has multiple construct
            // signatures (e.g., `Array` with `new(): any[]` and `new<T>(): T[]`),
            // the resolved base instance type can be a union like `any[] | T[]`.
            // tsc resolves these to the correct specific return type; our resolution
            // currently produces the union. Since all constituent types in such
            // unions are valid object types, suppress TS2509 for unions.
            if crate::query_boundaries::common::is_union_type(self.ctx.types, base_type) {
                continue;
            }

            // Check if the base type is a valid base type
            if !crate::query_boundaries::class::is_valid_base_type(self.ctx.types, base_type) {
                let type_name = self.format_type(base_type);
                let message = format_message(
                    diagnostic_messages::BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP,
                    &[&type_name],
                );
                self.error_at_node(
                    expr_idx,
                    &message,
                    diagnostic_codes::BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP,
                );
            }

            break; // Only check the first extends clause
        }
    }

    /// TS2797: A mixin class that extends from a type variable containing an
    /// abstract construct signature must also be declared 'abstract'.
    ///
    /// When a non-abstract class extends from a type variable (type parameter)
    /// whose constraint includes `abstract new (...)`, the class must be abstract.
    /// This is the mixin pattern: `class C extends baseClass` where `baseClass: T`
    /// and `T extends abstract new (...args: any) => any`.
    fn check_mixin_abstract_construct_constraint(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the extends expression
            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            // Try get_type_of_node first; if it returns a usable type, use it.
            // Otherwise, fall back to resolving the parameter's declared type
            // annotation directly (workaround for name-merging issues where
            // get_type_of_node returns ANY).
            let base_type = self.resolve_heritage_expr_declared_type(expr_idx);
            if base_type == TypeId::ERROR {
                return;
            }

            // Check if the base type is a type parameter with a constraint
            let Some(constraint_type) =
                class_query::type_parameter_constraint(self.ctx.types, base_type)
            else {
                return;
            };

            // Check if the constraint has abstract construct signatures
            if self.constraint_has_abstract_construct(constraint_type) {
                let error_node = if class_data.name.is_some() {
                    class_data.name
                } else {
                    class_idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT,
                    diagnostic_codes::A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT,
                );
            }

            return;
        }
    }

    /// TS2545: A mixin class must have a constructor with a single rest parameter
    /// of type 'any[]'.
    ///
    /// When a class extends a type variable (type parameter), the construct
    /// signatures of the constraint must each have a single rest parameter whose
    /// type is `any[]` or `readonly any[]`.  If not, emit TS2545.
    fn check_mixin_constructor_rest_parameter(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            let base_type = self.resolve_heritage_expr_declared_type(expr_idx);
            if base_type == TypeId::ERROR {
                return;
            }

            // Only applies when the base type is a type parameter (mixin pattern)
            let Some(constraint_type) =
                class_query::type_parameter_constraint(self.ctx.types, base_type)
            else {
                return;
            };

            // Evaluate the constraint type (may be a Lazy type alias or Application)
            let evaluated = self.evaluate_type_for_assignability(constraint_type);

            // Get construct signatures from the evaluated constraint
            let construct_sigs = self.collect_construct_signatures_from_evaluated(evaluated);
            if construct_sigs.is_empty() {
                return;
            }

            // Check if any construct signature with parameters has invalid mixin form.
            // tsc skips signatures with 0 parameters (they are not problematic).
            let has_invalid_sig = construct_sigs.iter().any(|sig| {
                if sig.params.is_empty() {
                    return false;
                }
                let valid = sig.params.len() == 1
                    && sig.params[0].rest
                    && !sig.params[0].optional
                    && self.is_valid_mixin_rest_param_type(sig.params[0].type_id);
                !valid
            });

            if has_invalid_sig {
                self.error_at_node(
                    class_idx,
                    diagnostic_messages::A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY,
                    diagnostic_codes::A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY,
                );
            }

            return;
        }
    }

    /// Collect construct signatures from an already-evaluated type.
    fn collect_construct_signatures_from_evaluated(
        &self,
        type_id: TypeId,
    ) -> Vec<tsz_solver::CallSignature> {
        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return sigs;
        }

        // Intersection: collect from all members
        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            let mut all_sigs = Vec::new();
            for &member in members.iter() {
                if let Some(sigs) =
                    class_query::construct_signatures_for_type(self.ctx.types, member)
                {
                    all_sigs.extend(sigs);
                }
            }
            return all_sigs;
        }

        Vec::new()
    }

    /// Check if a rest parameter type is valid for a mixin constructor.
    /// Accepts `any`, `any[]`, or `readonly any[]`.
    fn is_valid_mixin_rest_param_type(&self, type_id: TypeId) -> bool {
        // `any` is valid for mixin rest parameters (e.g., `...args: any`)
        if type_id == TypeId::ANY {
            return true;
        }
        // `any[]` or `readonly any[]`
        matches!(
            class_query::array_element_type(self.ctx.types, type_id),
            Some(elem) if elem == TypeId::ANY
        )
    }

    /// Resolve the declared type for a heritage expression identifier.
    /// First tries `get_type_of_node`. If that returns ANY (which can happen
    /// due to symbol name merging), falls back to resolving the identifier's
    /// symbol, finding its parameter declaration, and evaluating the type
    /// annotation directly.
    fn resolve_heritage_expr_declared_type(&mut self, expr_idx: NodeIndex) -> TypeId {
        let base_type = self.get_type_of_node(expr_idx);
        if base_type != TypeId::ANY {
            return base_type;
        }

        // Fallback: resolve via parameter declaration's type annotation
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return TypeId::ANY;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return TypeId::ANY;
        };
        let Some(&decl_idx) = symbol.declarations.first() else {
            return TypeId::ANY;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::ANY;
        };
        let Some(param_data) = self.ctx.arena.get_parameter(decl_node) else {
            return TypeId::ANY;
        };
        let type_ann = param_data.type_annotation;
        if type_ann == NodeIndex::NONE {
            return TypeId::ANY;
        }
        self.get_type_from_type_node(type_ann)
    }

    /// Check if a constraint type (or any member of an intersection constraint)
    /// contains abstract construct signatures.
    fn constraint_has_abstract_construct(&self, constraint_type: TypeId) -> bool {
        // Direct callable check
        if let Some(callable) =
            class_query::callable_shape_for_type(self.ctx.types, constraint_type)
            && callable.is_abstract
            && !callable.construct_signatures.is_empty()
        {
            return true;
        }

        // Intersection: check each member
        if let Some(members) = class_query::intersection_members(self.ctx.types, constraint_type) {
            for &member in members.iter() {
                if let Some(callable) = class_query::callable_shape_for_type(self.ctx.types, member)
                    && callable.is_abstract
                    && !callable.construct_signatures.is_empty()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if an anonymous class is exported (via export default modifier or parent export node).
    fn is_class_exported_default(
        &self,
        class_idx: NodeIndex,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        use tsz_scanner::SyntaxKind;
        // Check for export + default modifiers on the class itself
        let has_export = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword);
        let has_default = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::DefaultKeyword);
        if has_export && has_default {
            return true;
        }
        // Check if parent is an export default (ExportDeclaration with is_default_export)
        if let Some(ext) = self.ctx.arena.get_extended(class_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            if parent.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.ctx.arena.get_export_decl(parent)
                && export_data.is_default_export
            {
                return true;
            }
            // Also check for ExportAssignment (export = class { ... })
            if parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return true;
            }
        }
        false
    }

    /// Return the node index to anchor TS4094 at for an exported anonymous class.
    ///
    /// tsc reports TS4094 at the `export` keyword (col 1), not the `class` keyword.
    /// When the class is an expression inside an export statement, the parent node
    /// starts at `export`. When it's a `ClassDeclaration` with own `export default`
    /// modifiers, the first modifier starts before the class keyword.
    fn get_anonymous_class_export_anchor(
        &self,
        class_idx: NodeIndex,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        use tsz_scanner::SyntaxKind;
        let has_export = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword);
        let has_default = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::DefaultKeyword);
        if has_export && has_default {
            // ClassDeclaration with `export default` modifiers. Use the first modifier
            // node as the anchor so we report at `export` (col 1), not `class`.
            if let Some(mods) = modifiers
                && let Some(&first_mod_idx) = mods.nodes.first()
            {
                return Some(first_mod_idx);
            }
            return Some(class_idx);
        }
        // ClassExpression in `export default class` or `export = class`.
        // The parent export-statement node starts at the `export` keyword.
        if let Some(ext) = self.ctx.arena.get_extended(class_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            if parent.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.ctx.arena.get_export_decl(parent)
                && export_data.is_default_export
            {
                return Some(ext.parent);
            }
            if parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return Some(ext.parent);
            }
        }
        None
    }

    /// TS4094: Named exported class whose `extends` heritage resolves to an anonymous
    /// class type.  The anonymous base's private/protected members appear in the .d.ts
    /// type literal and must be reported.  Errors are anchored at the named class's
    /// name identifier (matching tsc's anchor position).
    fn check_ts4094_named_class_anonymous_heritage(
        &mut self,
        _class_idx: tsz_parser::parser::NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        let Some(ref heritage_list) = class.heritage_clauses else {
            return;
        };
        for &clause_idx in &heritage_list.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            // Only `extends` clauses carry the anonymous constructor type.
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_ref_idx in &heritage.types.nodes {
                let Some(type_ref_node) = self.ctx.arena.get(type_ref_idx) else {
                    continue;
                };
                // Mirror the existing heritage-resolution pattern: if the node is an
                // ExpressionWithTypeArguments, unpack it; otherwise treat the node itself
                // as the expression (handles bare identifier heritage like `extends Foo`).
                let (expr_idx, type_args) =
                    if let Some(eta) = self.ctx.arena.get_expr_type_args(type_ref_node) {
                        (eta.expression, eta.type_arguments.as_ref())
                    } else {
                        (type_ref_idx, None)
                    };
                let Some(base_instance_type) =
                    self.base_instance_type_from_expression(expr_idx, type_args)
                else {
                    continue;
                };
                if self.instance_type_is_from_anonymous_class(base_instance_type) {
                    self.report_instance_type_private_members_as_ts4094(
                        class.name,
                        base_instance_type,
                    );
                }
            }
        }
    }

    /// TS1497: Check that a decorator expression follows the valid grammar.
    ///
    /// Valid decorator expressions are:
    /// - `@identifier`
    /// - `@identifier.name.name`  (property access chain)
    /// - `@identifier.name()`     (single call at the top)
    /// - `@(expression)`          (parenthesized)
    ///
    /// Invalid (TS1497) examples: `@x().y`, `@new x`, `@x?.y`, @x\`\`,
    /// `@x?.()`, `@x?.["y"]`, `@x["y"]`.
    ///
    /// Matches tsc's `checkGrammarDecorator`. Only checked when the source file
    /// has no parse diagnostics.
    pub(crate) fn check_grammar_decorator(&mut self, expression_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip if the source file has parse diagnostics (matches tsc's hasParseDiagnostics gate)
        if self.ctx.has_parse_errors {
            return;
        }

        let Some(expr_node) = self.ctx.arena.get(expression_idx) else {
            return;
        };

        // DecoratorParenthesizedExpression: ( Expression )
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return;
        }

        let mut current = expression_idx;
        let mut can_have_call = true;
        let mut error_node: Option<NodeIndex> = None;

        while let Some(node) = self.ctx.arena.get(current) {
            // Allow TS syntax: ExpressionWithTypeArguments, NonNullExpression
            if node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                if let Some(data) = self.ctx.arena.get_expr_type_args(node) {
                    current = data.expression;
                    continue;
                }
                break;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
                if let Some(data) = self.ctx.arena.get_unary_expr_ex(node) {
                    current = data.expression;
                    continue;
                }
                break;
            }

            // DecoratorCallExpression: DecoratorMemberExpression Arguments
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                if !can_have_call {
                    error_node = Some(current);
                }
                // Check for optional chaining on call: x?.()
                if node.is_optional_chain() {
                    // Optional chaining — always an error, even if we already have one
                    error_node = Some(current);
                }
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    current = call.expression;
                    can_have_call = false;
                    continue;
                }
                break;
            }

            // DecoratorMemberExpression: DecoratorMemberExpression . IdentifierName
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    if access.question_dot_token {
                        // Optional chaining — always error
                        error_node = Some(current);
                    }
                    current = access.expression;
                    can_have_call = false;
                    continue;
                }
                break;
            }

            // If it's not an identifier, it's invalid
            if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                error_node = Some(current);
            }

            break;
        }

        if error_node.is_some() {
            self.error_at_node(
                expression_idx,
                diagnostic_messages::EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR,
                diagnostic_codes::EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source, check_source_diagnostics};

    fn check_with_declaration(source: &str) -> Vec<u32> {
        check_source(
            source,
            "test.ts",
            CheckerOptions {
                emit_declarations: true,
                ..CheckerOptions::default()
            },
        )
        .iter()
        .map(|d| d.code)
        .collect()
    }

    // TS4094 — property of exported anonymous class type may not be private or protected

    #[test]
    fn ts4094_export_default_anon_class_private_member() {
        // `export default class { private x() {} }` — tsc emits TS4094 for `x`.
        let codes = check_with_declaration(
            r#"
export default class {
    private x() {}
    protected y() {}
    public z() {}
}
"#,
        );
        assert!(
            codes.contains(&4094),
            "TS4094 expected for private/protected in exported anonymous class, got: {codes:?}"
        );
    }

    #[test]
    fn ts4094_no_error_for_named_export_own_class() {
        // `export class Foo { private x() {} }` — tsc does NOT emit TS4094 because
        // the named class's private members are stripped in the .d.ts.
        let codes = check_with_declaration(
            r#"
export class Foo {
    private x() {}
}
"#,
        );
        assert!(
            !codes.contains(&4094),
            "TS4094 should NOT fire for named exported class with own private members, got: {codes:?}"
        );
    }

    #[test]
    fn ts4094_export_default_mixin_call_anon_class() {
        // `export default mix(AnonClass)` where mix<T>(x:T):T returns the anonymous
        // constructor — tsc emits TS4094 for private/protected members.
        let codes = check_with_declaration(
            r#"
declare function mix<TMix>(mixin: TMix): TMix;
const AnonBase = class {
    protected _onDispose() {}
    private _assertIsStripped() {}
};
export default mix(AnonBase);
"#,
        );
        assert!(
            codes.contains(&4094),
            "TS4094 expected for `export default mix(AnonClass)`, got: {codes:?}"
        );
    }

    #[test]
    fn ts4094_named_class_extending_mixin_anon_class() {
        // `export class Monitor extends mix(AnonBase)` — tsc emits TS4094 at Monitor's
        // name because the anonymous base's private/protected appear in the .d.ts.
        let codes = check_with_declaration(
            r#"
declare function mix<TMix>(mixin: TMix): TMix;
const AnonBase = class {
    protected _onDispose() {}
    private _assertIsStripped() {}
};
export class Monitor extends mix(AnonBase) {
    protected _onDispose() {}
}
"#,
        );
        assert!(
            codes.contains(&4094),
            "TS4094 expected for named exported class extending mixin of anonymous class, got: {codes:?}"
        );
    }

    #[test]
    fn ts4094_no_error_without_declaration_flag() {
        // Without `declaration: true`, TS4094 should not be emitted.
        let codes: Vec<u32> = check_source_diagnostics(
            r#"
export default class {
    private x() {}
}
"#,
        )
        .iter()
        .map(|d| d.code)
        .collect();
        assert!(
            !codes.contains(&4094),
            "TS4094 should NOT fire without declaration flag, got: {codes:?}"
        );
    }

    #[test]
    fn ts1267_abstract_property_with_initializer() {
        let diags = check_source_diagnostics(
            r#"
abstract class C {
    abstract x: number = 1;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 1267).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS1267, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(matching[0].message_text.contains("'x'"));
    }

    #[test]
    fn ts1267_abstract_property_without_initializer_no_error() {
        let diags = check_source_diagnostics(
            r#"
abstract class C {
    abstract x: number;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 1267).collect();
        assert_eq!(matching.len(), 0, "Expected no TS1267, got: {matching:?}");
    }

    #[test]
    fn ts18006_field_named_constructor() {
        let diags = check_source_diagnostics(
            r#"
class C {
    "constructor" = 3;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 18006).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS18006, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts2699_static_property_named_prototype() {
        let diags = check_source_diagnostics(
            r#"
class C {
    static prototype: number;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2699).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2699, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts2699_non_static_prototype_no_error() {
        let diags = check_source_diagnostics(
            r#"
class C {
    prototype: number;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2699).collect();
        assert_eq!(
            matching.len(),
            0,
            "Expected no TS2699 for non-static prototype, got: {matching:?}"
        );
    }

    #[test]
    fn ts2797_mixin_extending_abstract_type_variable() {
        let diags = check_source_diagnostics(
            r#"
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass) {
    class MixinClass extends baseClass {
    }
    return MixinClass;
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2797).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2797 for mixin extending abstract type variable, got codes: {all_codes:?}"
        );
    }

    #[test]
    fn ts2797_mixin_with_implements_clause() {
        // TS2797 should still fire when the mixin class has an implements clause
        // (previously broken due to name-merging between function Mixin and interface Mixin)
        let diags = check_source_diagnostics(
            r#"
interface Mixin {
    mixinMethod(): void;
}
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2797).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2797 for mixin with implements clause, got codes: {all_codes:?}"
        );
    }

    #[test]
    fn ts2515_expression_based_heritage() {
        // Non-abstract class extending expression-based heritage (mixin pattern)
        // should report TS2515 for unimplemented abstract members
        let diags = check_source_diagnostics(
            r#"
interface Mixin {
    mixinMethod(): void;
}
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}

abstract class AbstractBase {
    abstract abstractBaseMethod(): void;
}

const MixedBase = Mixin(AbstractBase);

class DerivedFromAbstract extends MixedBase {
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        let ts2515: Vec<_> = diags.iter().filter(|d| d.code == 2515).collect();
        assert_eq!(
            ts2515.len(),
            1,
            "Expected 1 TS2515 for missing abstract member, got codes: {all_codes:?}"
        );
        // Verify the message mentions the correct base class name
        let msg = &ts2515[0].message_text;
        assert!(
            msg.contains("AbstractBase & Mixin"),
            "TS2515 message should reference 'AbstractBase & Mixin', got: {msg}"
        );
    }

    #[test]
    fn double_mixin_conditional_type_base_class_has_no_extra_ts2345() {
        let diags = check_source_diagnostics(
            r#"
type Constructor = new (...args: any[]) => {};
declare const Object: Constructor;

const Mixin1 = <C extends Constructor>(Base: C) => class extends Base { private _fooPrivate!: {}; };

type FooConstructor = typeof Mixin1 extends (a: Constructor) => infer Cls ? Cls : never;
const Mixin2 = <C extends FooConstructor>(Base: C) => class extends Base {};

class C extends Mixin2(Mixin1(Object)) {}
"#,
        );
        let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&2345),
            "Expected no TS2345 for double mixin conditional base class, got codes: {codes:?}"
        );
    }

    #[test]
    fn ts2545_mixin_with_optional_rest_parameter() {
        // TS2545: A mixin class must have a constructor with a single rest
        // parameter of type 'any[]'. Optional rest parameters are invalid.
        let diags = check_source_diagnostics(
            r#"
type Constructor<T = {}> = new (...args?: any[]) => T;

function Mixin<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
    };
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2545).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2545 for optional rest param in mixin constructor, got codes: {all_codes:?}"
        );
    }

    #[test]
    fn ts2545_no_error_for_valid_mixin_constructor() {
        // Valid mixin pattern: `...args: any[]` without optional should NOT emit TS2545.
        let diags = check_source_diagnostics(
            r#"
type Constructor = new (...args: any[]) => {};

function Mixin<TBase extends Constructor>(Base: TBase) {
    return class extends Base {};
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        assert!(
            !all_codes.contains(&2545),
            "Should NOT emit TS2545 for valid mixin constructor pattern, got codes: {all_codes:?}"
        );
    }

    #[test]
    fn ts2545_no_error_for_bare_any_rest_parameter() {
        // `...args: any` (bare any, not any[]) is also valid for mixin constructors.
        let diags = check_source_diagnostics(
            r#"
function Mixin<TBase extends abstract new (...args: any) => any>(baseClass: TBase) {
    abstract class MixinClass extends baseClass {}
    return MixinClass;
}
"#,
        );
        let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        assert!(
            !all_codes.contains(&2545),
            "Should NOT emit TS2545 for bare `any` rest param type, got codes: {all_codes:?}"
        );
    }
}
