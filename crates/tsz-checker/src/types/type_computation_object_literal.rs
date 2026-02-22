//! Object literal type computation.
//!
//! Handles typing of object literal expressions including property assignments,
//! shorthand properties, method shorthands, getters/setters, spread properties,
//! duplicate property detection, and contextual type inference.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};

impl<'a> CheckerState<'a> {
    /// Get the type of an object literal expression.
    ///
    /// Computes the type of object literals like `{ x: 1, y: 2 }` or `{ foo, bar }`.
    /// Handles:
    /// - Property assignments: `{ x: value }`
    /// - Shorthand properties: `{ x }`
    /// - Method shorthands: `{ foo() {} }`
    /// - Getters/setters: `{ get foo() {}, set foo(v) {} }`
    /// - Spread properties: `{ ...obj }`
    /// - Duplicate property detection
    /// - Contextual type inference
    /// - Implicit any reporting (TS7008)
    pub(crate) fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;
        use tsz_common::interner::Atom;
        use tsz_solver::PropertyInfo;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut string_index_types: Vec<TypeId> = Vec::new();
        let mut number_index_types: Vec<TypeId> = Vec::new();
        let mut has_spread = false;
        // Track getter/setter names to allow getter+setter pairs with the same name
        let mut getter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut setter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut explicit_property_names: rustc_hash::FxHashSet<Atom> =
            rustc_hash::FxHashSet::default();
        // Track which named properties came from explicit assignments (not spreads)
        // so we can emit TS2783 when a later spread overwrites them.
        // Maps property name atom -> (node_idx for error, property display name)
        let mut named_property_nodes: FxHashMap<Atom, (NodeIndex, String)> = FxHashMap::default();

        // Skip duplicate property checks for destructuring assignment targets.
        // `({ x, y: y1, "y": y1 } = obj)` is valid - same property extracted twice.
        let skip_duplicate_check = self.ctx.in_destructuring_target;

        // Check for ThisType<T> marker in contextual type (Vue 2 / Options API pattern)
        // We need to extract this BEFORE the for loop so it's available for the pop at the end
        let marker_this_type: Option<TypeId> = if let Some(ctx_type) = self.ctx.contextual_type {
            use tsz_solver::ContextualTypeContext;
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            ctx_helper.get_this_type_from_marker()
        } else {
            None
        };

        // Push this type onto stack if found (methods will pick it up)
        if let Some(this_type) = marker_this_type {
            self.ctx.this_type_stack.push(this_type);
        }

        // Pre-scan: collect getter property names so setter TS7006 checks can
        // detect paired getters regardless of declaration order.
        let obj_getter_names: rustc_hash::FxHashSet<String> = obj
            .elements
            .nodes
            .iter()
            .filter_map(|&elem_idx| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                if elem_node.kind != syntax_kind_ext::GET_ACCESSOR {
                    return None;
                }
                let accessor = self.ctx.arena.get_accessor(elem_node)?;
                self.get_property_name(accessor.name).or_else(|| {
                    let prop_name_node = self.ctx.arena.get(accessor.name)?;
                    if prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    {
                        let computed = self.ctx.arena.get_computed_property(prop_name_node)?;
                        let prop_name_type = self.get_type_of_node(computed.expression);
                        crate::query_boundaries::type_computation_access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    } else {
                        None
                    }
                })
            })
            .collect();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(prop_name_node) = self.ctx.arena.get(prop.name)
                    && prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    // Always run TS2464 validation for computed property names, even when
                    // the name can be resolved to a literal atom.
                    self.check_computed_property_name(prop.name);
                }

                let name_opt = self.get_property_name(prop.name).or_else(|| {
                    let prop_name_node = self.ctx.arena.get(prop.name)?;
                    if prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    {
                        let computed = self.ctx.arena.get_computed_property(prop_name_node)?;
                        let prop_name_type = self.get_type_of_node(computed.expression);
                        crate::query_boundaries::type_computation_access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    } else {
                        None
                    }
                });
                if let Some(name) = name_opt.clone() {
                    // Get contextual type for this property.
                    // For mapped/conditional/application types that contain Lazy references
                    // (e.g. { [K in keyof Props]: Props[K] } after generic inference),
                    // evaluate them with the full resolver first so the solver can
                    // extract property types from the resulting concrete object type.
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

                    // When the parser can't parse a value expression (e.g. `{ a: return; }`),
                    // it uses the property NAME node as the fallback initializer for error
                    // recovery (prop.initializer == prop.name). Skip type-checking in that
                    // case to prevent a spurious TS2304 for the property name identifier.
                    let value_type = if prop.initializer == prop.name {
                        TypeId::ANY
                    } else {
                        self.get_type_of_node(prop.initializer)
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // Apply bidirectional type inference - use contextual type to narrow the value type
                    let value_type = tsz_solver::apply_contextual_type(
                        self.ctx.types,
                        value_type,
                        property_context_type,
                    );

                    // Widen literal types for object literal properties (tsc behavior).
                    // Object literal properties are mutable by default, so `{ x: "a" }`
                    // produces `{ x: string }`.  Only preserve literals when:
                    // - A const assertion is active (`as const`)
                    // - A contextual type narrows the property to a literal
                    let value_type =
                        if !self.ctx.in_const_assertion && property_context_type.is_none() {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                    // Note: TS7008 is NOT emitted for object literal properties.
                    // tsc only emits TS7008 for class properties, property signatures,
                    // auto-accessors, and binary expressions.

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    // Track this named property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (prop.name, name.clone()));

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else {
                    // Computed property name that can't be statically resolved (e.g., { [expr]: value })
                    // Still type-check the computed expression and the value to catch errors like TS2304.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(prop.name);

                    let mut prop_name_type = TypeId::ANY;
                    if let Some(prop_name_node) = self.ctx.arena.get(prop.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        if let Some(atom) =
                            crate::query_boundaries::type_computation_access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !self.ctx.has_parse_errors
                            {
                                let name = self.ctx.types.resolve_atom(atom).to_string();
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                                self.error_at_node(
                                            prop.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }
                    let index_ctx_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        // Use a synthetic name that won't match any named property,
                        // causing contextual_property_type to fall back to the index signature.
                        self.ctx
                            .types
                            .contextual_property_type(ctx_type, "__@computed")
                    } else {
                        None
                    };
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = index_ctx_type;
                    let value_type = self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(value_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(value_type);
                    }
                }
            }
            // Shorthand property: { x } - identifier is both name and value
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                    && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.clone();
                    let shorthand_name_idx = shorthand.name;

                    // Get contextual type for this property
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for shorthand property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

                    let value_type = if self.resolve_identifier_symbol(shorthand_name_idx).is_none()
                    {
                        // Don't emit TS18004 for strict reserved words that require `:` syntax.
                        // Example: `{ class }` — parser already emits TS1005 "':' expected".
                        // Checker should not also emit TS18004 (cascading error).
                        //
                        // Only suppress for ECMAScript reserved words that ALWAYS require `:`
                        // in object literals. Be conservative — when in doubt, emit TS18004.
                        let is_strict_reserved = matches!(
                            name.as_str(),
                            "break"
                                | "case"
                                | "catch"
                                | "class"
                                | "const"
                                | "continue"
                                | "debugger"
                                | "default"
                                | "delete"
                                | "do"
                                | "else"
                                | "enum"
                                | "export"
                                | "extends"
                                | "finally"
                                | "for"
                                | "function"
                                | "if"
                                | "import"
                                | "in"
                                | "instanceof"
                                | "new"
                                | "return"
                                | "super"
                                | "switch"
                                | "throw"
                                | "try"
                                | "var"
                                | "void"
                                | "while"
                                | "with"
                        );

                        // Also suppress TS18004 for obviously invalid names that
                        // are parser-recovery artifacts (single punctuation characters
                        // like `:`, `,`, `;` that became shorthand properties during
                        // error recovery).
                        let is_obviously_invalid_name = name.len() == 1
                            && name
                                .chars()
                                .next()
                                .is_some_and(|c| !c.is_alphanumeric() && c != '_' && c != '$');

                        if !is_strict_reserved && !is_obviously_invalid_name {
                            // TS18004: Missing value binding for shorthand property name
                            // Example: `({ arguments })` inside arrow function where `arguments`
                            // is not in scope as a value.
                            let message = format_message(
                                diagnostic_messages::NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID,
                                &[&name],
                            );
                            self.error_at_node(
                                elem_idx,
                                &message,
                                diagnostic_codes::NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID,
                            );
                        }

                        // In destructuring assignment targets, unresolved shorthand names
                        // are already invalid (TS18004). Don't synthesize a required
                        // object property from this invalid entry; doing so can produce
                        // follow-on missing-property errors (e.g. TS2741) that tsc omits.
                        if self.ctx.in_destructuring_target {
                            continue;
                        }
                        TypeId::ANY
                    } else {
                        // Use shorthand_name_idx (the identifier) so that get_type_of_identifier
                        // is invoked, which calls check_flow_usage and can emit TS2454
                        // if the variable is used before assignment.
                        // Using elem_idx (SHORTHAND_PROPERTY_ASSIGNMENT) would return TypeId::ERROR
                        // since that node kind has no dispatch handler, silently suppressing TS2454.
                        self.get_type_of_node(shorthand_name_idx)
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // Apply bidirectional type inference - use contextual type to narrow the value type
                    let value_type = tsz_solver::apply_contextual_type(
                        self.ctx.types,
                        value_type,
                        property_context_type,
                    );

                    // Widen literal types for shorthand properties (same as named properties)
                    let value_type =
                        if !self.ctx.in_const_assertion && property_context_type.is_none() {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                    // Note: TS7008 is NOT emitted for object literal properties.
                    // tsc only emits TS7008 for class properties, property signatures,
                    // auto-accessors, and binary expressions.

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    // Track this shorthand property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (elem_idx, name.clone()));

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                    self.check_computed_property_name(shorthand.name);
                }
            }
            // Method shorthand: { foo() {} }
            else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                let name_opt = self.get_property_name(method.name).or_else(|| {
                    let prop_name_node = self.ctx.arena.get(method.name)?;
                    if prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    {
                        let computed = self.ctx.arena.get_computed_property(prop_name_node)?;
                        let prop_name_type = self.get_type_of_node(computed.expression);
                        crate::query_boundaries::type_computation_access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    } else {
                        None
                    }
                });
                if let Some(name) = name_opt.clone() {
                    // Set contextual type for method
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    // If no explicit ThisType marker exists, use the object literal's
                    // contextual type as `this` inside method bodies.
                    let mut pushed_contextual_this = false;
                    if marker_this_type.is_none()
                        && self.current_this_type().is_none()
                        && let Some(ctx_type) = prev_context
                    {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.this_type_stack.push(ctx_type);
                        pushed_contextual_this = true;
                    }

                    let method_type = self.get_type_of_function(elem_idx);

                    if pushed_contextual_this {
                        self.ctx.this_type_stack.pop();
                    }

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            method.name,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: true, // Object literal methods should be bivariant
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else {
                    // Computed method name - still type-check the expression and function body.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(method.name);

                    let mut prop_name_type = TypeId::ANY;
                    if let Some(prop_name_node) = self.ctx.arena.get(method.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        if let Some(atom) =
                            crate::query_boundaries::type_computation_access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !self.ctx.has_parse_errors
                            {
                                let name = self.ctx.types.resolve_atom(atom).to_string();
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                                self.error_at_node(
                                            method.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.contextual_type = self
                            .ctx
                            .types
                            .contextual_property_type(ctx_type, "__@computed");
                    }
                    let method_type = self.get_type_of_function(elem_idx);
                    self.ctx.contextual_type = prev_context;

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(method_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(method_type);
                    }
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                // Check for missing body - error 1005 at end of accessor
                if accessor.body.is_none() {
                    use crate::diagnostics::diagnostic_codes;
                    // Report at accessor.end - 1 (pointing to the closing paren)
                    let end_pos = elem_node.end.saturating_sub(1);
                    self.error_at_position(end_pos, 1, "'{' expected.", diagnostic_codes::EXPECTED);
                }

                // For setters, check implicit any on parameters (error 7006) and on
                // the property name itself (error 7032).
                // When a paired getter exists, the setter parameter type is inferred
                // from the getter return type (contextually typed, suppress TS7006/7032).
                if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
                    let name_opt = self.get_property_name(accessor.name).or_else(|| {
                        let prop_name_type = self.get_type_of_node(accessor.name);
                        crate::query_boundaries::type_computation_access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    });
                    let has_paired_getter = name_opt
                        .as_ref()
                        .is_some_and(|name| obj_getter_names.contains(name));
                    // Check if accessor JSDoc has @param type annotations
                    let accessor_jsdoc = self.get_jsdoc_for_function(elem_idx);
                    let mut first_param_lacks_annotation = false;
                    for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            let has_jsdoc = has_paired_getter
                                || self.param_has_inline_jsdoc_type(param_idx)
                                || if let Some(ref jsdoc) = accessor_jsdoc {
                                    let pname = self.parameter_name_for_error(param.name);
                                    Self::jsdoc_has_param_type(jsdoc, &pname)
                                } else {
                                    false
                                };
                            if param.type_annotation.is_none() && !has_jsdoc {
                                first_param_lacks_annotation = true;
                            }
                            self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                        }
                    }
                    // TS7032: emit on property name when the setter has no parameter type
                    // annotation and no paired getter (TSC checks this at accessor symbol
                    // resolution time; we emit it here during object literal checking).
                    if first_param_lacks_annotation
                        && !has_paired_getter
                        && self.ctx.no_implicit_any()
                        && let Some(prop_name) = name_opt.as_deref()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            accessor.name,
                            diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                            &[prop_name],
                        );
                    }
                }

                let name_opt = self.get_property_name(accessor.name).or_else(|| {
                    let prop_name_node = self.ctx.arena.get(accessor.name)?;
                    if prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    {
                        let computed = self.ctx.arena.get_computed_property(prop_name_node)?;
                        let prop_name_type = self.get_type_of_node(computed.expression);
                        crate::query_boundaries::type_computation_access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    } else {
                        None
                    }
                });
                if let Some(name) = name_opt.clone() {
                    // For non-contextual object literals, TypeScript treats `this` inside
                    // accessors as the object literal under construction. Provide a
                    // lightweight synthetic receiver so property access checks (TS2339)
                    // run during accessor body checking.
                    let mut pushed_synthetic_this = false;
                    if marker_this_type.is_none() {
                        let mut this_props: Vec<PropertyInfo> =
                            properties.values().cloned().collect();
                        let name_atom = self.ctx.types.intern_string(&name);
                        if !this_props.iter().any(|p| p.name == name_atom) {
                            this_props.push(PropertyInfo {
                                name: name_atom,
                                type_id: TypeId::ANY,
                                write_type: TypeId::ANY,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            });
                        }
                        self.ctx
                            .this_type_stack
                            .push(self.ctx.types.factory().object(this_props));
                        pushed_synthetic_this = true;
                    }

                    // For getter, infer return type; for setter, use the parameter type
                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        // Check getter body/parameters via function checking, but object
                        // property read type is the getter's return type (not a function type).
                        self.get_type_of_function(elem_idx);
                        if accessor.type_annotation.is_none() {
                            self.infer_getter_return_type(accessor.body)
                        } else {
                            self.get_type_from_type_node(accessor.type_annotation)
                        }
                    } else {
                        // Setter: type-check the function body to track variable usage
                        // (especially for noUnusedParameters/noUnusedLocals checking),
                        // but use the parameter type annotation for the property type
                        self.get_type_of_function(elem_idx);

                        // Extract setter write type from first parameter.
                        // When no type annotation, fall back to the paired getter's
                        // return type (mirroring tsc's inference behavior).
                        accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| {
                                let param = self.ctx.arena.get_parameter_at(param_idx)?;
                                if param.type_annotation.is_none() {
                                    None
                                } else {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                }
                            })
                            .or_else(|| {
                                // No annotation — infer from paired getter's type
                                let setter_name = name_opt.clone()?;
                                let name_atom = self.ctx.types.intern_string(&setter_name);
                                properties.get(&name_atom).map(|p| p.type_id)
                            })
                            .unwrap_or(TypeId::ANY)
                    };

                    if pushed_synthetic_this {
                        self.ctx.this_type_stack.pop();
                    }

                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        if accessor.type_annotation.is_none() {
                            use crate::diagnostics::diagnostic_codes;
                            let self_refs =
                                self.collect_property_name_references(accessor.body, &name);
                            if !self_refs.is_empty() {
                                self.error_at_node_msg(
                                    accessor.name,
                                    diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                                    &[&name],
                                );
                            }
                        }

                        self.maybe_report_implicit_any_return(
                            Some(name.clone()),
                            Some(accessor.name),
                            accessor_type,
                            accessor.type_annotation.is_some(),
                            false,
                            elem_idx,
                        );
                    }

                    // TS2378: A 'get' accessor must return a value.
                    // Check if the getter has a body but no return statement with a value.
                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR && accessor.body.is_some() {
                        let has_return = self.body_has_return_with_value(accessor.body);
                        let falls_through = self.function_body_falls_through(accessor.body);

                        if !has_return && falls_through {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                accessor.name,
                                "A 'get' accessor must return a value.",
                                diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                            );
                        }
                    }
                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property - but allow getter+setter pairs
                    // A getter and setter with the same name is valid, not a duplicate
                    let is_getter = elem_node.kind == syntax_kind_ext::GET_ACCESSOR;
                    let is_complementary_pair = if is_getter {
                        setter_names.contains(&name_atom) && !getter_names.contains(&name_atom)
                    } else {
                        getter_names.contains(&name_atom) && !setter_names.contains(&name_atom)
                    };
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !is_complementary_pair
                        && !self.ctx.has_parse_errors
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            accessor.name,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    if is_getter {
                        getter_names.insert(name_atom);
                    } else {
                        setter_names.insert(name_atom);
                    }

                    // Merge getter/setter into a single property with separate
                    // read (type_id) and write (write_type) types.
                    if let Some(existing) = properties.get(&name_atom) {
                        let (read_type, write_type) = if is_getter {
                            // Getter arriving after setter
                            (accessor_type, existing.write_type)
                        } else {
                            // Setter arriving after getter
                            (existing.type_id, accessor_type)
                        };
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: read_type,
                                write_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            },
                        );
                    } else {
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: accessor_type,
                                write_type: accessor_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            },
                        );
                    }
                } else {
                    // Computed accessor name - still type-check the expression and body
                    self.check_computed_property_name(accessor.name);

                    let mut prop_name_type = TypeId::ANY;
                    if let Some(prop_name_node) = self.ctx.arena.get(accessor.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        if let Some(atom) =
                            crate::query_boundaries::type_computation_access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            let is_getter =
                                elem_node.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                            let is_complementary_pair = if is_getter {
                                setter_names.contains(&atom) && !getter_names.contains(&atom)
                            } else {
                                getter_names.contains(&atom) && !setter_names.contains(&atom)
                            };
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !is_complementary_pair
                                && !self.ctx.has_parse_errors
                            {
                                let name = self.ctx.types.resolve_atom(atom).to_string();
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                                self.error_at_node(
                                            accessor.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }

                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        self.get_type_of_function(elem_idx);

                        // TS2378: A 'get' accessor must return a value.
                        if accessor.body.is_some() {
                            let has_return = self.body_has_return_with_value(accessor.body);
                            let falls_through = self.function_body_falls_through(accessor.body);
                            if !has_return && falls_through {
                                use crate::diagnostics::diagnostic_codes;
                                self.error_at_node(
                                    accessor.name,
                                    "A 'get' accessor must return a value.",
                                    diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                                );
                            }
                        }

                        if accessor.type_annotation.is_none() {
                            self.infer_getter_return_type(accessor.body)
                        } else {
                            self.get_type_from_type_node(accessor.type_annotation)
                        }
                    } else {
                        self.get_type_of_function(elem_idx);
                        accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| {
                                let param = self.ctx.arena.get_parameter_at(param_idx)?;
                                if param.type_annotation.is_none() {
                                    None
                                } else {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                }
                            })
                            .unwrap_or(TypeId::ANY)
                    };

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(accessor_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(accessor_type);
                    }
                }
            }
            // Spread assignment: { ...obj }
            else if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                has_spread = true;
                let spread_expr = self
                    .ctx
                    .arena
                    .get_spread(elem_node)
                    .map(|spread| spread.expression)
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_unary_expr_ex(elem_node)
                            .map(|unary| unary.expression)
                    });
                if let Some(spread_expr) = spread_expr {
                    let spread_type = self.get_type_of_node(spread_expr);
                    // TS2698: Spread types may only be created from object types
                    let resolved_spread = self.resolve_type_for_property_access(spread_type);
                    let resolved_spread = self.resolve_lazy_type(resolved_spread);
                    if !crate::query_boundaries::type_computation_access::is_valid_spread_type(
                        self.ctx.types,
                        resolved_spread,
                    ) {
                        self.report_spread_not_object_type(elem_idx);
                    }
                    let spread_props = self.collect_object_spread_properties(spread_type);

                    // TS2783: Check if any earlier named properties will be
                    // overwritten by required properties from this spread.
                    // Only when strict null checks are enabled.
                    if self.ctx.strict_null_checks() {
                        for sp in &spread_props {
                            if !sp.optional
                                && let Some((prop_node, prop_name)) =
                                    named_property_nodes.get(&sp.name)
                            {
                                let message = format_message(
                                        diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                        &[prop_name],
                                    );
                                self.error_at_node(
                                        *prop_node,
                                        &message,
                                        diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    );
                            }
                        }
                    }

                    // After TS2783 check, clear the named-property tracking
                    // for properties that the spread overwrites (so only the
                    // first occurrence can trigger the diagnostic, not later
                    // spreads which are spread-vs-spread and exempt).
                    for prop in &spread_props {
                        named_property_nodes.remove(&prop.name);
                    }

                    for prop in spread_props {
                        properties.insert(prop.name, prop);
                    }
                }
            }
            // Other element types (e.g., unknown AST node kinds) are silently skipped
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        // Object literals with spreads are not fresh (no excess property checking)

        let object_type = if string_index_types.is_empty() && number_index_types.is_empty() {
            if has_spread {
                self.ctx.types.factory().object(properties)
            } else {
                self.ctx.types.factory().object_fresh(properties)
            }
        } else {
            use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape};

            let string_index = if !string_index_types.is_empty() {
                Some(IndexSignature {
                    key_type: TypeId::STRING,
                    value_type: self.ctx.types.factory().union(string_index_types),
                    readonly: false,
                })
            } else {
                None
            };

            let number_index = if !number_index_types.is_empty() {
                Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type: self.ctx.types.factory().union(number_index_types),
                    readonly: false,
                })
            } else {
                None
            };

            let flags = if has_spread {
                ObjectFlags::empty()
            } else {
                ObjectFlags::FRESH_LITERAL
            };

            self.ctx.types.factory().object_with_index(ObjectShape {
                flags,
                properties,
                string_index,
                number_index,
                symbol: None,
            })
        };

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // This fixes the "Zombie Freshness" bug by distinguishing fresh vs
        // non-fresh object types at interning time.

        // Pop this type from stack if we pushed it earlier
        if marker_this_type.is_some() {
            self.ctx.this_type_stack.pop();
        }

        object_type
    }

    /// Collect properties from a spread expression in an object literal.
    ///
    /// Given the type of the spread expression, extracts all properties that would
    /// be spread into the object literal.
    pub(crate) fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<tsz_solver::PropertyInfo> {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.resolve_lazy_type(resolved);
        self.ctx.types.collect_object_spread_properties(resolved)
    }
}
