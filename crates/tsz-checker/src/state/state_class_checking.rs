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

        // TS1042: async modifier cannot be used on class declarations
        self.check_async_modifier_on_declaration(&class.modifiers);

        // CRITICAL: Check for circular inheritance using InheritanceGraph
        // This prevents stack overflow from infinite recursion in get_class_instance_type
        // Must be done BEFORE any type checking to catch cycles early
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if checker.check_class_inheritance_cycle(stmt_idx, class) {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return; // Cycle detected - error already emitted, skip all type checking
        }

        // TS1212: Check class name for strict mode reserved words
        self.check_strict_mode_reserved_name_at(class.name, stmt_idx);

        // Check for reserved class names (error 2414)
        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == "any"
        {
            self.error_at_node(
                class.name,
                "Class name cannot be 'any'.",
                diagnostic_codes::CLASS_NAME_CANNOT_BE,
            );
        }

        // TS2725: Class name cannot be 'Object' when targeting ES5 and above with module X
        // Only applies to non-ES module kinds (CommonJS, AMD, UMD, System) and non-ambient classes
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

                    // TS18019: Check for private identifiers in ambient classes
                    if is_declared {
                        self.error_at_node(
                            member_name_idx,
                            diagnostic_messages::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
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
        }

        // Check static/instance consistency for method overloads (TS2387, TS2388)
        self.check_static_instance_overload_consistency(&class.members.nodes);

        // Check abstract consistency for method overloads (TS2512)
        self.check_abstract_overload_consistency(&class.members.nodes);

        // Check consecutive abstract declarations (TS2516)
        self.check_abstract_method_consecutive_declarations(&class.members.nodes);

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check for accessor type compatibility (TS2322)
        // TS 5.1+ allows divergent types ONLY if both have explicit annotations.
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

        // Check that non-abstract class implements all abstract members from base class (error 2654)
        self.check_abstract_member_implementations(stmt_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(stmt_idx, class);

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
        // TS8004: Type parameters on class expression in JS files
        if self.is_js_file() {
            if let Some(ref type_params) = class.type_parameters
                && !type_params.nodes.is_empty()
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                        type_params.nodes[0],
                        diagnostic_messages::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
            }

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

        // Check for accessor type compatibility (TS2322)
        // TS 5.1+ allows divergent types ONLY if both have explicit annotations.
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(class_idx, class);

        // Check that non-abstract class implements all abstract members from base class (error 2653, 2656)
        self.check_abstract_member_implementations(class_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(class_idx, class);

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
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        is_declared: bool,
        _is_abstract: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip TS2564 for declared classes (ambient declarations) and .d.ts files.
        // In tsc, .d.ts files are inherently ambient even without the `declare` keyword.
        // Note: Abstract classes DO get TS2564 errors - they can have constructors
        // and properties must be initialized either with defaults or in the constructor
        if is_declared || self.ctx.file_name.ends_with(".d.ts") {
            return;
        }

        // Only check property initialization when strictPropertyInitialization is enabled
        // tsc also requires strictNullChecks to be enabled for TS2564
        if !self.ctx.strict_property_initialization() || !self.ctx.strict_null_checks() {
            return;
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

            let Some(key) = self.property_key_from_name(prop.name) else {
                continue;
            };

            // Get property name for error message. Use fallback for complex computed properties.
            let name = self.get_property_name(prop.name).unwrap_or_else(|| {
                // For complex computed properties (e.g., [getKey()]), use a descriptive fallback
                match &key {
                    PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{s}]"),
                    PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{s}\"]"),
                    PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{n}]"),
                    PropertyKey::Private(s) => format!("#{s}"),
                    PropertyKey::Ident(s) => s.clone(),
                }
            });

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

        let prop_type = if prop.type_annotation.is_some() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ANY
        };

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

    // Note: class_has_base, type_includes_undefined, find_constructor_body are in type_checking.rs

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
                    for &decl_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                            && let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
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
}
