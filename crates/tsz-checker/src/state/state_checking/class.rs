//! Class declaration, expression, property initialization, and decorator checking.

use crate::EnclosingClassInfo;
use crate::flow_analysis::{ComputedKey, PropertyKey};
use crate::query_boundaries::class_type as class_query;
use crate::query_boundaries::definite_assignment::{
    check_constructor_property_use_before_assignment, constructor_assigned_properties,
};
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
                    before_brace.trim().len() > 0
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

        // Evaluate class-level decorator expressions to trigger definite-assignment
        // checks (TS2454) and other diagnostics. tsc evaluates decorator expressions
        // even if the class has other errors.
        if let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR
                    && let Some(decorator) = self.ctx.arena.get_decorator(mod_node)
                {
                    let decorator_type = self.compute_type_of_node(decorator.expression);

                    // TS1238: Validate class decorator call signature.
                    // For experimental decorators, the decorator is called as
                    // decoratorExpr(classConstructor). If no call signature exists
                    // or call resolution fails, emit TS1238.
                    if self.ctx.compiler_options.experimental_decorators {
                        self.check_class_decorator_call_signature(
                            decorator.expression,
                            decorator_type,
                            stmt_idx,
                            class,
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
        // TSC forbids using predefined type names as class names.
        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && matches!(
                ident.escaped_text.as_str(),
                "any" | "number" | "boolean" | "string" | "undefined"
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

        // Collect class type parameter names for TS2302 checking in static members
        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&class.type_parameters, stmt_idx);

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
                    use crate::diagnostics::diagnostic_messages;

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

                    // Note: In tsc 6.0, private identifiers (#name) ARE allowed
                    // in ambient (declare) classes. TS18019 is only for member-level
                    // 'declare' modifier on private-named members, not class-level.
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
                        // TS1244 for methods/accessors, TS1253 for properties
                        let is_method = matches!(
                            member_node.kind,
                            syntax_kind_ext::METHOD_DECLARATION
                                | syntax_kind_ext::GET_ACCESSOR
                                | syntax_kind_ext::SET_ACCESSOR
                        );
                        if is_method {
                            self.error_at_node(
                                member_idx,
                                "Abstract methods can only appear within an abstract class.",
                                diagnostic_codes::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                            );
                        } else {
                            self.error_at_node(
                                member_idx,
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
            }
        }

        // Collect class name
        let class_name = self.get_class_name_from_decl(stmt_idx);

        // Save previous enclosing class and set current
        let prev_enclosing_class = self.ctx.enclosing_class.take();
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

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(stmt_idx, class);

        // TS2797: A mixin class that extends from a type variable containing an
        // abstract construct signature must also be declared 'abstract'.
        if !is_abstract_class {
            self.check_mixin_abstract_construct_constraint(stmt_idx, class);
        }

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

        self.check_inherited_properties_against_index_signatures(
            class_instance_type,
            &class.members.nodes,
            stmt_idx,
        );

        // Check for decorator-related global types (TS2318)
        // When experimentalDecorators is enabled and a method/accessor has decorators,
        // TypedPropertyDescriptor must be available
        self.check_decorator_global_types(&class.members.nodes);

        // Restore previous enclosing class
        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);

        self.ctx.checked_classes.insert(stmt_idx);
        self.ctx.checking_classes.remove(&stmt_idx);
    }

    pub(crate) fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
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
            self.check_class_member(member_idx);
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

        // tsc suppresses TS2564 per-node via its containsParseError flag propagation.
        // A parse error only affects the containing node and its ancestors, not the
        // entire file. We approximate this by checking if any *real* syntax error
        // position falls within the class node's span. Grammar-level parse errors
        // (e.g., TS1030 "modifier already seen") don't trigger containsParseError
        // in tsc, so we use real_syntax_error_positions which only includes actual
        // parse failures (TS1005, TS1109, TS1128, etc.).
        if self.ctx.has_real_syntax_errors
            && let Some(class_node) = self.ctx.arena.get(class_idx)
        {
            let class_start = class_node.pos;
            let class_end = class_node.end;
            let class_has_parse_error = self
                .ctx
                .real_syntax_error_positions
                .iter()
                .any(|&pos| pos >= class_start && pos < class_end);
            if class_has_parse_error {
                return;
            }
        }

        // Check if this is a derived class (has base class)
        let is_derived_class = self.class_has_base(class);

        let mut properties = Vec::new();
        let mut tracked = FxHashSet::default();
        let mut parameter_properties = FxHashSet::default();

        // First pass: collect parameter properties from constructor
        // Parameter properties are always definitely assigned
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };

            // Collect parameter properties from constructor parameters
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Parameter properties have modifiers (public/private/protected/readonly)
                if param.modifiers.is_some()
                    && let Some(key) = self.property_key_from_name(param.name)
                {
                    parameter_properties.insert(key.clone());
                }
            }
        }

        // Second pass: collect class properties that need initialization
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(node) else {
                continue;
            };

            if !self.property_requires_initialization(member_idx, prop, is_derived_class) {
                continue;
            }

            let Some(key) = self.property_key_from_name(prop.name).or_else(|| {
                let name_node = self.ctx.arena.get(prop.name)?;
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return None;
                }
                let raw = self.node_text(prop.name)?;
                let normalized = raw.trim_end_matches(':').trim();
                let inner = normalized
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(normalized)
                    .trim()
                    .to_string();
                Some(PropertyKey::Computed(ComputedKey::Ident(inner)))
            }) else {
                continue;
            };

            // Get property name for error message.
            // For computed properties, always use key-based formatting to preserve brackets
            // (tsc displays computed properties as [x], ["h"], etc. in diagnostics).
            let format_key_name = |key: &PropertyKey| -> String {
                match key {
                    PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{s}]"),
                    PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{s}\"]"),
                    PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{n}]"),
                    PropertyKey::Private(s) => {
                        // The scanner stores private identifiers with the `#` prefix
                        if s.starts_with('#') {
                            s.clone()
                        } else {
                            format!("#{s}")
                        }
                    }
                    PropertyKey::Ident(s) => s.clone(),
                }
            };
            let is_computed = self
                .ctx
                .arena
                .get(prop.name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let name = if is_computed {
                format_key_name(&key)
            } else {
                self.get_property_name(prop.name)
                    .unwrap_or_else(|| format_key_name(&key))
            };

            tracked.insert(key.clone());
            properties.push((key, name, prop.name));
        }

        if properties.is_empty() {
            return;
        }

        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);
        let assigned = if let Some(body_idx) = constructor_body {
            constructor_assigned_properties(self, body_idx, &tracked, requires_super)
        } else {
            FxHashSet::default()
        };

        for (key, name, name_node) in properties {
            // Property is assigned if it's in the assigned set OR it's a parameter property
            if assigned.contains(&key) || parameter_properties.contains(&key) {
                continue;
            }
            use crate::diagnostics::format_message;

            // Use TS2524 if there's a constructor (definite assignment analysis)
            // Use TS2564 if no constructor (just missing initializer)
            let (message, code) = (
                diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
            );

            self.error_at_node(name_node, &format_message(message, &[&name]), code);
        }

        // Check for TS2565 (Property used before being assigned in constructor)
        if let Some(body_idx) = constructor_body {
            check_constructor_property_use_before_assignment(
                self,
                body_idx,
                &tracked,
                requires_super,
            );
        }
    }

    pub(crate) fn property_requires_initialization(
        &mut self,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        is_derived_class: bool,
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

        // tsc suppresses TS2564 when the property's type annotation already has errors
        // (e.g. TS2314 "A generic type requires type arguments").  The type may resolve
        // to a valid TypeId even when there's an error on the type reference, so we check
        // whether any existing diagnostic falls within the type annotation's span.
        // IMPORTANT: Only check the type annotation span, not the entire member span,
        // to avoid suppressing TS2564 due to unrelated errors like TS2300 (duplicate
        // identifier) or TS2717 (incompatible property types).
        if !self.is_js_file()
            && prop.type_annotation.is_some()
            && let Some(type_node) = self.ctx.arena.get(prop.type_annotation)
        {
            let type_start = type_node.pos;
            let type_end = type_node.end;
            let has_type_annotation_errors = self
                .ctx
                .diagnostics
                .iter()
                .any(|d| d.start >= type_start && d.start < type_end && d.code != 2564);
            if has_type_annotation_errors {
                return false;
            }
        }

        // Enhanced property initialization checking:
        // 1. ANY/UNKNOWN types don't need initialization
        // 2. Union types with undefined don't need initialization
        // 3. Optional types don't need initialization
        if prop_type == TypeId::ANY || prop_type == TypeId::UNKNOWN {
            return false;
        }

        // ERROR types also don't need initialization - these indicate parsing/binding errors
        if prop_type == TypeId::ERROR {
            return false;
        }

        // For derived classes, be more strict about definite assignment
        // Properties in derived classes that redeclare base class properties need initialization
        // This catches cases like: class B extends A { property: any; } where A has property
        if is_derived_class {
            // In derived classes, properties without definite assignment assertions
            // need initialization unless they include undefined in their type
            return !class_query::type_includes_undefined(self.ctx.types, prop_type);
        }

        !class_query::type_includes_undefined(self.ctx.types, prop_type)
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
        use tsz_binder::lib_loader::emit_error_global_type_missing;
        let diag = emit_error_global_type_missing(type_name, self.ctx.file_name.clone(), 0, 0);
        self.ctx.push_diagnostic(diag.clone());
        self.ctx.push_diagnostic(diag);
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

        if !matches!(result, tsz_solver::CallResult::Success(_)) {
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
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
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;

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
}
