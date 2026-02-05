//! Declaration & Statement Checking Module (Members)
//!
//! Extracted from state_checking.rs: Second half of CheckerState impl
//! containing interface checking, class member checking, type member
//! validation, and StatementCheckCallbacks implementation.

use crate::checker::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use crate::checker::statements::StatementCheckCallbacks;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::{ContextualTypeContext, TypeId};

impl<'a> CheckerState<'a> {
    /// Check an interface declaration.
    pub(crate) fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on interface declarations
        self.check_async_modifier_on_declaration(&iface.modifiers);

        // Check for reserved interface names (error 2427)
        if !iface.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(iface.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            // Reserved type names that can't be used as interface names
            match ident.escaped_text.as_str() {
                "string" | "number" | "boolean" | "symbol" | "void" | "object" => {
                    self.error_at_node(
                        iface.name,
                        &format!("Interface name cannot be '{}'.", ident.escaped_text),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
                _ => {}
            }
        }

        // Check heritage clauses for unresolved names (TS2304)
        self.check_heritage_clauses_for_unresolved_names(&iface.heritage_clauses, false);

        let (_type_params, type_param_updates) = self.push_type_parameters(&iface.type_parameters);

        // Check each interface member for missing type references and parameter properties
        for &member_idx in &iface.members.nodes {
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
        }

        // Check for duplicate member names (TS2300)
        self.check_duplicate_interface_members(&iface.members.nodes);

        // Check that interface correctly extends base interfaces (error 2430)
        self.check_interface_extension_compatibility(stmt_idx, iface);

        self.pop_type_parameters(type_param_updates);
    }

    /// Check for duplicate property names in interface members (TS2300).
    /// TypeScript reports "Duplicate identifier 'X'." for each duplicate occurrence.
    /// NOTE: Method signatures (overloads) are NOT considered duplicates - interfaces allow
    /// multiple method signatures with the same name for function overloading.
    pub(crate) fn check_duplicate_interface_members(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use std::collections::HashMap;

        // Track property names and their indices (methods are allowed to have overloads)
        let mut seen_properties: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates
            // Method signatures can have multiple overloads (same name, different types)
            let name = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                    .ctx
                    .arena
                    .get_signature(member_node)
                    .and_then(|sig| self.get_member_name_text(sig.name)),
                // Method signatures are allowed to have overloads - don't flag as duplicates
                k if k == syntax_kind_ext::METHOD_SIGNATURE => None,
                // Call, construct, and index signatures don't have names that can conflict
                _ => None,
            };

            if let Some(name) = name {
                seen_properties.entry(name).or_default().push(member_idx);
            }
        }

        // Report errors for duplicates
        for (name, indices) in seen_properties {
            if indices.len() > 1 {
                // Report TS2300 for subsequent occurrences only (matching tsc behavior)
                // Skip the first declaration as it's valid
                for &idx in indices.iter().skip(1) {
                    let message =
                        format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
                    // Get the name node for precise error location
                    let error_node = self.get_interface_member_name_node(idx).unwrap_or(idx);
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    /// Get the name text from a member name node (identifier, string literal, or computed).
    fn get_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        use crate::scanner::SyntaxKind;

        if name_idx.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(name_idx)?;

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => self
                .ctx
                .arena
                .get_literal(name_node)
                .map(|lit| lit.text.to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .ctx
                .arena
                .get_literal(name_node)
                .map(|lit| lit.text.to_string()),
            // For computed property names, we can't easily check for duplicates
            // since the value is computed at runtime
            _ => None,
        }
    }

    /// Get the name node from an interface member for error reporting.
    fn get_interface_member_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.ctx.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| !idx.is_none()),
            k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| !idx.is_none()),
            _ => None,
        }
    }

    /// Report TS2300 "Duplicate identifier" error for a class member (property or method).
    /// Helper function to avoid code duplication in check_duplicate_class_members.
    fn report_duplicate_class_member_ts2300(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let member_node = self.ctx.arena.get(member_idx);
        let (name, error_node) = match member_node.map(|n| n.kind) {
            Some(k) if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node.unwrap());
                let name = prop.and_then(|p| self.get_member_name_text(p.name));
                let node = prop
                    .map(|p| p.name)
                    .filter(|idx| !idx.is_none())
                    .unwrap_or(member_idx);
                (name, node)
            }
            Some(k) if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node.unwrap());
                let name = method.and_then(|m| self.get_member_name_text(m.name));
                let node = method
                    .map(|m| m.name)
                    .filter(|idx| !idx.is_none())
                    .unwrap_or(member_idx);
                (name, node)
            }
            _ => return,
        };

        if let Some(name) = name {
            let message = format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
            self.error_at_node(error_node, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
        }
    }

    /// Check for duplicate property/method names in class members (TS2300, TS2393).
    /// TypeScript reports:
    /// - TS2300 "Duplicate identifier 'X'." for duplicate properties
    /// - TS2393 "Duplicate function implementation." for multiple method implementations
    ///
    /// NOTE: Method overloads (signatures + implementation) are allowed:
    ///   foo(x: number): void;    // overload signature
    ///   foo(x: string): void;    // overload signature  
    ///   foo(x: any) { }          // implementation - this is valid!
    pub(crate) fn check_duplicate_class_members(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use std::collections::HashMap;

        // Track member names with their info
        struct MemberInfo {
            indices: Vec<NodeIndex>,
            is_property: Vec<bool>, // true for PROPERTY_DECLARATION, false for METHOD_DECLARATION
            method_has_body: Vec<bool>, // only valid when is_property is false
            is_static: Vec<bool>,
        }

        let mut seen_names: HashMap<String, MemberInfo> = HashMap::new();

        // Track accessor occurrences for duplicate detection
        // Key: "get:name" or "set:name" (with "static:" prefix for static members)
        let mut seen_accessors: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Get the member name and type info
            let (name, is_property, method_has_body, is_static) = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|prop| {
                        let is_static = self.has_static_modifier(&prop.modifiers);
                        self.get_member_name_text(prop.name)
                            .map(|n| (n, true, false, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|method| {
                        let has_body = !method.body.is_none();
                        let is_static = self.has_static_modifier(&method.modifiers);
                        self.get_member_name_text(method.name)
                            .map(|n| (n, false, has_body, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Track accessors for duplicate detection (getter/setter pairs are allowed,
                    // but duplicate getters or duplicate setters are not)
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                        if let Some(name) = self.get_member_name_text(accessor.name) {
                            let is_static = self.has_static_modifier(&accessor.modifiers);
                            let kind = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                                "get"
                            } else {
                                "set"
                            };
                            let key = if is_static {
                                format!("static:{}:{}", kind, name)
                            } else {
                                format!("{}:{}", kind, name)
                            };
                            seen_accessors.entry(key).or_default().push(member_idx);
                        }
                    }
                    continue;
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    // Constructors have separate duplicate checking (TS2392)
                    continue;
                }
                _ => continue,
            };

            if name.is_empty() {
                continue;
            }

            // Create a key that considers static vs instance members separately
            let key = if is_static {
                format!("static:{}", name)
            } else {
                name.clone()
            };

            let info = seen_names.entry(key).or_insert(MemberInfo {
                indices: Vec::new(),
                is_property: Vec::new(),
                method_has_body: Vec::new(),
                is_static: Vec::new(),
            });
            info.indices.push(member_idx);
            info.is_property.push(is_property);
            info.method_has_body.push(method_has_body);
            info.is_static.push(is_static);
        }

        // Report errors for duplicates
        for (_key, info) in seen_names {
            if info.indices.len() <= 1 {
                continue;
            }

            // Count types of members
            let property_count = info.is_property.iter().filter(|&&p| p).count();
            let method_count = info.is_property.len() - property_count;
            let method_impl_count = info
                .is_property
                .iter()
                .zip(info.method_has_body.iter())
                .filter(|(is_prop, has_body)| !**is_prop && **has_body)
                .count();

            // Case 1: Multiple properties with same name (no methods) -> TS2300 for subsequent only
            // Case 2: Property mixed with methods:
            //   - If property comes first: TS2300 for ALL (both property and method)
            //   - If method comes first: TS2300 for subsequent (only property)
            // Case 3: Multiple method implementations -> TS2393 for implementations only
            // Case 4: Method overloads (signatures + 1 implementation) -> Valid, no error

            if property_count > 0 && method_count == 0 {
                // All properties: only report subsequent declarations
                for &idx in info.indices.iter().skip(1) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if property_count > 0 && method_count > 0 {
                // Mixed properties and methods: check if first is property
                let first_is_property = info.is_property.first().copied().unwrap_or(false);
                let skip_count = if first_is_property { 0 } else { 1 };

                for &idx in info.indices.iter().skip(skip_count) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if method_impl_count > 1 {
                // Multiple method implementations -> TS2393 for implementations only
                for ((&idx, &is_prop), &has_body) in info
                    .indices
                    .iter()
                    .zip(info.is_property.iter())
                    .zip(info.method_has_body.iter())
                {
                    if !is_prop && has_body {
                        let member_node = self.ctx.arena.get(idx);
                        let error_node = member_node
                            .and_then(|n| self.ctx.arena.get_method_decl(n))
                            .map(|m| m.name)
                            .filter(|idx| !idx.is_none())
                            .unwrap_or(idx);
                        self.error_at_node(
                            error_node,
                            "Duplicate function implementation.",
                            diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                        );
                    }
                }
            }
            // else: Only method signatures + at most 1 implementation = valid overloads
        }

        // Report TS2300 for duplicate accessors (e.g., two getters or two setters with same name)
        for (_key, indices) in seen_accessors {
            if indices.len() <= 1 {
                continue;
            }
            // Emit errors for ALL duplicate declarations (matching tsc behavior)
            for &idx in indices.iter() {
                let Some(member_node) = self.ctx.arena.get(idx) else {
                    continue;
                };
                if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                    if let Some(name) = self.get_member_name_text(accessor.name) {
                        let error_node = if accessor.name.is_none() {
                            idx
                        } else {
                            accessor.name
                        };
                        let message =
                            format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }
            }
        }
    }

    /// Check for invalid 'async' modifier on class, enum, interface, or module declarations.
    /// TS1042: 'async' modifier cannot be used here.
    ///
    /// In TypeScript, the `async` modifier is only valid on function declarations,
    /// method declarations, and arrow functions. When placed on class, enum, interface,
    /// or namespace/module declarations, TS1042 is reported.
    ///
    /// This matches tsc's checker behavior (checkGrammarModifiers) rather than
    /// emitting the error at parse time.
    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<crate::parser::NodeList>,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        if let Some(async_mod_idx) = self.find_async_modifier(modifiers) {
            self.error_at_node(
                async_mod_idx,
                "'async' modifier cannot be used here.",
                diagnostic_codes::ASYNC_MODIFIER_CANNOT_BE_USED_HERE,
            );
        }
    }

    pub(crate) fn lookup_member_access_in_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> MemberLookup {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return MemberLookup::NotFound;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return MemberLookup::NotFound;
        };

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) != is_static {
                        continue;
                    }
                    let Some(prop_name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    if prop_name == name {
                        let access_level = if self.is_private_identifier_name(prop.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&prop.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) != is_static {
                        continue;
                    }
                    let Some(method_name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if method_name == name {
                        let access_level = if self.is_private_identifier_name(method.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&method.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) != is_static {
                        continue;
                    }
                    let Some(accessor_name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    if accessor_name == name {
                        let access_level = if self.is_private_identifier_name(accessor.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&accessor.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    if is_static {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(param_name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        if param_name == name {
                            return match self.member_access_level_from_modifiers(&param.modifiers) {
                                Some(level) => MemberLookup::Restricted(level),
                                None => MemberLookup::Public,
                            };
                        }
                    }
                }
                _ => {}
            }
        }

        MemberLookup::NotFound
    }

    pub(crate) fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self.get_class_name_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let Some(base_idx) = self.get_base_class_idx(current) else {
                        return None;
                    };
                    current = base_idx;
                }
            }
        }

        None
    }

    /// Recursively check a type node for parameter properties in function types.
    /// Function types (like `(x: T) => R` or `new (x: T) => R`) cannot have parameter properties.
    /// Walk a type node and emit TS2304 for unresolved type names inside complex types.
    pub(crate) fn check_type_for_missing_names(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let _ = self.get_type_from_type_reference(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let _ = self.get_type_from_type_query(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let updates =
                        self.push_missing_name_type_parameters(&func_type.type_parameters);
                    self.check_type_parameters_for_missing_names(&func_type.type_parameters);
                    for &param_idx in &func_type.parameters.nodes {
                        self.check_parameter_type_for_missing_names(param_idx);
                    }
                    if !func_type.type_annotation.is_none() {
                        self.check_type_for_missing_names(func_type.type_annotation);
                    }
                    self.pop_type_parameters(updates);
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_for_missing_names(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_tuple_element_for_missing_names(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_for_missing_names(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Check check_type and extends_type first (infer type params not in scope yet)
                    self.check_type_for_missing_names(cond.check_type);
                    self.check_type_for_missing_names(cond.extends_type);

                    // Collect infer type parameters from extends_type and add them to scope for true_type
                    let infer_params = self.collect_infer_type_parameters(cond.extends_type);
                    let mut param_bindings = Vec::new();
                    for param_name in &infer_params {
                        let atom = self.ctx.types.intern_string(param_name);
                        let type_id = self.ctx.types.intern(crate::solver::TypeKey::TypeParameter(
                            crate::solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                                is_const: false,
                            },
                        ));
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(param_name.clone(), type_id);
                        param_bindings.push((param_name.clone(), previous));
                    }

                    // Check true_type with infer type parameters in scope
                    self.check_type_for_missing_names(cond.true_type);

                    // Remove infer type parameters from scope
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }

                    // Check false_type (infer type params not in scope)
                    self.check_type_for_missing_names(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.check_type_for_missing_names(op.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_for_missing_names(indexed.object_type);
                    self.check_type_for_missing_names(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    let mut param_binding: Option<(String, Option<TypeId>)> = None;
                    if let Some(param_node) = self.ctx.arena.get(mapped.type_parameter)
                        && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                        && let Some(name_node) = self.ctx.arena.get(param.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        let atom = self.ctx.types.intern_string(&name);
                        let type_id = self.ctx.types.intern(crate::solver::TypeKey::TypeParameter(
                            crate::solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                                is_const: false,
                            },
                        ));
                        let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                        param_binding = Some((name, previous));
                    }
                    if !mapped.name_type.is_none() {
                        self.check_type_for_missing_names(mapped.name_type);
                    }
                    if !mapped.type_node.is_none() {
                        self.check_type_for_missing_names(mapped.type_node);
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    if let Some((name, previous)) = param_binding {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node)
                    && !pred.type_node.is_none()
                {
                    self.check_type_for_missing_names(pred.type_node);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_for_missing_names(span.expression);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn push_missing_name_type_parameters(
        &mut self,
        type_parameters: &Option<crate::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>)> {
        use crate::solver::{TypeKey, TypeParamInfo};

        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let mut updates = Vec::new();
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = ident.escaped_text.clone();
            let atom = self.ctx.types.intern_string(&name);
            let type_id = self.ctx.types.intern(TypeKey::TypeParameter(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            }));
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
        }

        updates
    }

    pub(crate) fn check_type_member_for_missing_names(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            let updates = self.push_missing_name_type_parameters(&sig.type_parameters);
            self.check_type_parameters_for_missing_names(&sig.type_parameters);
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
            if !sig.type_annotation.is_none() {
                self.check_type_for_missing_names(sig.type_annotation);
            }
            self.pop_type_parameters(updates);
            return;
        }

        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            for &param_idx in &index_sig.parameters.nodes {
                self.check_parameter_type_for_missing_names(param_idx);
            }
            if !index_sig.type_annotation.is_none() {
                self.check_type_for_missing_names(index_sig.type_annotation);
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    pub(crate) fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Check call signatures and construct signatures for parameter properties
        if node.kind == syntax_kind_ext::CALL_SIGNATURE
            || node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
        {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(sig.type_annotation);
            }
        }
        // Check method signatures in type literals
        else if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                self.check_type_for_parameter_properties(sig.type_annotation);
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(name) = self.property_name_for_error(sig.name)
                {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let message =
                        format_message(diagnostic_messages::IMPLICIT_ANY_RETURN, &[&name, "any"]);
                    self.error_at_node(sig.name, &message, diagnostic_codes::IMPLICIT_ANY_RETURN);
                }
            }
        }
        // Check property signatures for implicit any (error 7008)
        else if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if !sig.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(sig.type_annotation);
                }
                // Property signature without type annotation implicitly has 'any' type
                // Only emit TS7008 when noImplicitAny is enabled
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(member_name) = self.get_property_name(sig.name)
                {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let message = format_message(
                        diagnostic_messages::MEMBER_IMPLICIT_ANY,
                        &[&member_name, "any"],
                    );
                    self.error_at_node(sig.name, &message, diagnostic_codes::IMPLICIT_ANY_MEMBER);
                }
            }
        }
        // Check accessors in type literals/interfaces - cannot have body (error 1183)
        else if (node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.ctx.arena.get_accessor(node)
        {
            // Accessors in type literals and interfaces cannot have implementations
            if !accessor.body.is_none() {
                use crate::checker::types::diagnostics::diagnostic_codes;
                // Report error on the body
                self.error_at_node(
                    accessor.body,
                    "An implementation cannot be declared in ambient contexts.",
                    diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
                );
            }
        }
    }

    /// Check that all method/constructor overload signatures have implementations.
    /// Reports errors 2389, 2390, 2391, 1042.
    pub(crate) fn check_class_member_implementations(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                i += 1;
                continue;
            };

            match node.kind {
                // TS1042: 'async' modifier cannot be used on getters/setters
                syntax_kind_ext::GET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::ASYNC_MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::ASYNC_MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::CONSTRUCTOR => {
                    if let Some(ctor) = self.ctx.arena.get_constructor(node)
                        && ctor.body.is_none()
                    {
                        // Constructor overload signature - check for implementation
                        let has_impl = self.find_constructor_impl(members, i + 1);
                        if !has_impl {
                            self.error_at_node(
                                member_idx,
                                "Constructor implementation is missing.",
                                diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_MISSING,
                            );
                        }
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        // Abstract methods don't need implementations (they're meant for derived classes)
                        let is_abstract = self.has_abstract_modifier(&method.modifiers);
                        if method.body.is_none() && !is_abstract {
                            // Method overload signature - check for implementation
                            let method_name = self.get_method_name_from_node(member_idx);
                            if let Some(name) = method_name {
                                let (has_impl, impl_name) =
                                    self.find_method_impl(members, i + 1, &name);
                                if !has_impl {
                                    self.error_at_node(
                                        member_idx,
                                        "Function implementation is missing or not immediately following the declaration.",
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_MISSING
                                    );
                                } else if let Some(actual_name) = impl_name
                                    && actual_name != name
                                {
                                    // Implementation has wrong name
                                    self.error_at_node(
                                        members[i + 1],
                                        &format!(
                                            "Function implementation name must be '{}'.",
                                            name
                                        ),
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    pub(crate) fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &crate::parser::node::ParameterData,
        has_contextual_type: bool,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // Skip parameters that have explicit type annotations
        if !param.type_annotation.is_none() {
            return;
        }
        // Check if parameter has an initializer
        if !param.initializer.is_none() {
            // TypeScript infers type from initializer, EXCEPT for null and undefined
            // Parameters initialized with null/undefined still trigger TS7006
            use crate::scanner::SyntaxKind;
            let initializer_is_null_or_undefined =
                if let Some(init_node) = self.ctx.arena.get(param.initializer) {
                    init_node.kind == SyntaxKind::NullKeyword as u16
                        || init_node.kind == SyntaxKind::UndefinedKeyword as u16
                } else {
                    false
                };

            // Skip only if initializer is NOT null or undefined
            if !initializer_is_null_or_undefined {
                return;
            }
            // Otherwise continue to emit TS7006 for null/undefined initializers
        }
        if self.is_this_parameter_name(param.name) {
            return;
        }

        // Enhanced destructuring parameter detection
        // Check if the parameter name is a destructuring pattern (object/array binding)
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let kind = name_node.kind;

            // Direct destructuring patterns
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // For destructuring parameters, recursively check nested binding elements
                self.emit_implicit_any_parameter_for_pattern(param.name, param.dot_dot_dot_token);
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Rest parameters implicitly have 'any[]' type, regular parameters have 'any'
        let implicit_type = if param.dot_dot_dot_token {
            "any[]"
        } else {
            "any"
        };
        let message = format_message(
            diagnostic_messages::PARAMETER_IMPLICIT_ANY,
            &[&param_name, implicit_type],
        );
        self.error_at_node(
            param.name,
            &message,
            diagnostic_codes::IMPLICIT_ANY_PARAMETER,
        );
    }

    /// Emit TS7006 errors for nested binding elements in destructuring parameters.
    /// TypeScript reports implicit 'any' for individual bindings in patterns like:
    ///   function foo({ x, y }: any) {}  // no error on x, y with type annotation
    ///   function bar({ x, y }) {}        // errors on x and y
    pub(crate) fn emit_implicit_any_parameter_for_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        // Handle object binding patterns: { x, y, z }
        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    if let Some(element_node) = self.ctx.arena.get(element_idx) {
                        // Skip omitted expressions
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            continue;
                        }

                        if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                        {
                            // Check if this binding element has an initializer
                            let has_initializer = !binding_elem.initializer.is_none();

                            // If no initializer, report error for implicit any
                            if !has_initializer {
                                // Get the property name (could be identifier or string literal)
                                let binding_name = if !binding_elem.property_name.is_none() {
                                    self.parameter_name_for_error(binding_elem.property_name)
                                } else {
                                    self.parameter_name_for_error(binding_elem.name)
                                };

                                let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                                let message = format_message(
                                    diagnostic_messages::PARAMETER_IMPLICIT_ANY,
                                    &[&binding_name, implicit_type],
                                );
                                self.error_at_node(
                                    binding_elem.name,
                                    &message,
                                    diagnostic_codes::IMPLICIT_ANY_PARAMETER,
                                );
                            }

                            // Recursively check nested patterns
                            if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                                let name_kind = name_node.kind;
                                if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                {
                                    self.emit_implicit_any_parameter_for_pattern(
                                        binding_elem.name,
                                        is_rest_parameter,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // Handle array binding patterns: [ x, y, z ]
        else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    let element_kind = element_node.kind;

                    // Skip omitted expressions (holes in array patterns)
                    if element_kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    // Check if this element is a binding element with initializer
                    if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) {
                        let has_initializer = !binding_elem.initializer.is_none();

                        if !has_initializer {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);

                            let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                            let message = format_message(
                                diagnostic_messages::PARAMETER_IMPLICIT_ANY,
                                &[&binding_name, implicit_type],
                            );
                            self.error_at_node(
                                binding_elem.name,
                                &message,
                                diagnostic_codes::IMPLICIT_ANY_PARAMETER,
                            );
                        }

                        // Recursively check nested patterns
                        if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                            let name_kind = name_node.kind;
                            if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            {
                                self.emit_implicit_any_parameter_for_pattern(
                                    binding_elem.name,
                                    is_rest_parameter,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Report an error at a specific node.

    /// Check an expression node for TS1359: await outside async function.
    /// Recursively checks the expression tree for await expressions.
    /// Report an error with context about a related symbol.

    /// Check a class member (property, method, constructor, accessor).
    pub(crate) fn check_class_member(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let mut pushed_this = false;
        if let Some(this_type) = self.class_member_this_type(member_idx) {
            self.ctx.this_type_stack.push(this_type);
            pushed_this = true;
        }

        self.check_class_member_name(member_idx);

        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.check_property_declaration(member_idx);
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.check_method_declaration(member_idx);
            }
            syntax_kind_ext::CONSTRUCTOR => {
                self.check_constructor_declaration(member_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.check_accessor_declaration(member_idx);
            }
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                // Static blocks contain statements that must be type-checked
                if let Some(block) = self.ctx.arena.get_block(node) {
                    // Check for unreachable code in the static block
                    self.check_unreachable_code_in_block(&block.statements.nodes);

                    // Check each statement in the block
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement(stmt_idx);
                    }
                }
            }
            _ => {
                // Other class member types (index signatures, etc.)
                self.get_type_of_node(member_idx);
            }
        }

        if pushed_this {
            self.ctx.this_type_stack.pop();
        }
    }

    /// Check a property declaration.
    pub(crate) fn check_property_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(prop) = self.ctx.arena.get_property_decl(node) else {
            return;
        };

        // Track static property initializer context for TS17011
        let is_static = self.has_static_modifier(&prop.modifiers);
        let prev_static_prop_init = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_static_property_initializer)
            .unwrap_or(false);
        if is_static && !prop.initializer.is_none() {
            if let Some(ref mut class_info) = self.ctx.enclosing_class {
                class_info.in_static_property_initializer = true;
            }
        }

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&prop.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::CONST_MODIFIER_CANNOT_APPEAR_ON_A_CLASS_ELEMENT,
            );
        }

        // If property has type annotation and initializer, check type compatibility
        if !prop.type_annotation.is_none() && !prop.initializer.is_none() {
            let declared_type = self.get_type_from_type_node(prop.type_annotation);
            let prev_context = self.ctx.contextual_type;
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                self.ctx.contextual_type = Some(declared_type);
            }
            let init_type = self.get_type_of_node(prop.initializer);
            self.ctx.contextual_type = prev_context;

            if declared_type != TypeId::ANY
                && !self.type_contains_error(declared_type)
                && !self.is_assignable_to(init_type, declared_type)
            {
                self.error_type_not_assignable_with_reason_at(
                    init_type,
                    declared_type,
                    prop.initializer,
                );
            }
        } else if !prop.initializer.is_none() {
            // Just check the initializer to catch errors within it
            self.get_type_of_node(prop.initializer);
        }

        // Error 2729: Property is used before its initialization
        // Check if initializer references properties declared after this one
        if !prop.initializer.is_none() && !self.has_static_modifier(&prop.modifiers) {
            self.check_property_initialization_order(member_idx, prop.initializer);
        }

        // TS7008: Member implicitly has an 'any' type
        // Report this error when noImplicitAny is enabled and the property has no type annotation
        // AND no initializer (if there's an initializer, TypeScript can infer the type)
        if self.ctx.no_implicit_any()
            && prop.type_annotation.is_none()
            && prop.initializer.is_none()
            && let Some(member_name) = self.get_property_name(prop.name)
        {
            use crate::checker::types::diagnostics::{
                diagnostic_codes, diagnostic_messages, format_message,
            };
            let message = format_message(
                diagnostic_messages::MEMBER_IMPLICIT_ANY,
                &[&member_name, "any"],
            );
            self.error_at_node(prop.name, &message, diagnostic_codes::IMPLICIT_ANY_MEMBER);
        }

        // Cache the inferred type for the property node so DeclarationEmitter can use it
        // Get type: either from annotation or inferred from initializer
        let prop_type = if !prop.type_annotation.is_none() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if !prop.initializer.is_none() {
            self.get_type_of_node(prop.initializer)
        } else {
            TypeId::ANY
        };

        self.ctx.node_types.insert(member_idx.0, prop_type);

        // Restore static property initializer context
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_static_property_initializer = prev_static_prop_init;
        }
    }

    /// Check a method declaration.
    pub(crate) fn check_method_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return;
        };

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&method.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::CONST_MODIFIER_CANNOT_APPEAR_ON_A_CLASS_ELEMENT,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the method has a body
        if !method.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
            );
        }

        // Push type parameters (like <U> in `fn<U>(id: U)`) before checking types
        let (_type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);

        // Extract parameter types from contextual type (for object literal methods)
        // This enables shorthand method parameter type inference
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        if let Some(ctx_type) = self.ctx.contextual_type {
            let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, ctx_type);

            for (i, &param_idx) in method.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let type_id = if !param.type_annotation.is_none() {
                        // Use explicit type annotation if present
                        Some(self.get_type_from_type_node(param.type_annotation))
                    } else {
                        // Infer from contextual type
                        ctx_helper.get_parameter_type(i)
                    };
                    param_types.push(type_id);
                }
            }
        }

        let has_type_annotation = !method.type_annotation.is_none();
        let mut return_type = if has_type_annotation {
            self.get_type_from_type_node(method.type_annotation)
        } else {
            TypeId::ANY
        };

        // Cache parameter types for use in method body
        // If we have contextual types, use them; otherwise fall back to type annotations or UNKNOWN
        if param_types.is_empty() {
            self.cache_parameter_types(&method.parameters.nodes, None);
        } else {
            self.cache_parameter_types(&method.parameters.nodes, Some(&param_types));
        }

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&method.parameters);

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&method.parameters);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&method.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in methods
        self.check_parameter_properties(&method.parameters.nodes);

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &method.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if !param.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                self.maybe_report_implicit_any_parameter(param, false);
            }
        }

        // Check return type annotation for parameter properties in function types
        if !method.type_annotation.is_none() {
            self.check_type_for_parameter_properties(method.type_annotation);
        }

        // Check for async modifier (needed for both abstract and concrete methods)
        let is_async = self.has_async_modifier(&method.modifiers);
        let is_generator = method.asterisk_token;

        // Check method body
        if !method.body.is_none() {
            if !has_type_annotation {
                return_type = self.infer_return_type_from_body(method.body, None);
            }

            // TS2697: Check if async method has access to Promise type
            // DISABLED: Causes too many false positives
            // TODO: Investigate lib loading for Promise detection
            // if is_async && !is_generator && !self.is_promise_global_available() {
            //     use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
            //     self.error_at_node(
            //         method.name,
            //         diagnostic_messages::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //         diagnostic_codes::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //     );
            // }

            // TS7011 (implicit any return) is only emitted for ambient methods,
            // matching TypeScript's behavior
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7011
            let is_ambient_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .map(|c| c.is_declared)
                .unwrap_or(false);
            let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");

            if (is_ambient_class || is_ambient_file) && !is_async {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }

            self.push_return_type(return_type);

            // Enter async context for await expression checking
            if is_async {
                self.ctx.enter_async_context();
            }

            self.check_statement(method.body);

            // Exit async context
            if is_async {
                self.ctx.exit_async_context();
            }

            let check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(method.body);
            let falls_through = self.function_body_falls_through(method.body);

            // TS2355: Skip for async methods - they implicitly return Promise<void>
            if has_type_annotation && requires_return && falls_through && !is_async {
                if !has_return {
                    self.error_at_node(
                        method.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                    );
                } else {
                    use crate::checker::types::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        method.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                    );
                }
            } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::checker::types::diagnostics::diagnostic_messages;
                let error_node = if !method.name.is_none() {
                    method.name
                } else {
                    method.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                );
            }

            self.pop_return_type();
        } else {
            // Abstract method or method overload signature
            // Report TS7010 for abstract methods without return type annotation
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if !is_async {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }
        }

        // Check overload compatibility for method implementations
        if !method.body.is_none() {
            self.check_overload_compatibility(member_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    /// Check a constructor declaration.
    pub(crate) fn check_constructor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(ctor) = self.ctx.arena.get_constructor(node) else {
            return;
        };

        // Error 1242: 'abstract' modifier can only appear on a class, method, or property declaration.
        // Constructors cannot be abstract.
        if self.has_abstract_modifier(&ctor.modifiers) {
            self.error_at_node(
                member_idx,
                "'abstract' modifier can only appear on a class, method, or property declaration.",
                diagnostic_codes::ABSTRACT_MODIFIER_ONLY_ON_CLASS_METHOD_OR_PROPERTY,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the constructor has a body
        if !ctor.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
            );
        }

        // Check for parameter properties in constructor overload signatures (error 2369)
        // Parameter properties are only allowed in constructor implementations (with body)
        if ctor.body.is_none() {
            self.check_parameter_properties(&ctor.parameters.nodes);
        }

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if !param.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                self.maybe_report_implicit_any_parameter(param, false);
            }
        }

        // Constructors don't have explicit return types, but they implicitly return the class instance type
        // Get the class instance type to validate constructor return expressions (TS2322)

        self.cache_parameter_types(&ctor.parameters.nodes, None);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&ctor.parameters);

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&ctor.parameters);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&ctor.parameters.nodes);

        // Set in_constructor flag for abstract property checks (error 2715)
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = true;
        }

        // Check constructor body
        if !ctor.body.is_none() {
            // Get class instance type for constructor return expression validation
            let instance_type = if let Some(ref class_info) = self.ctx.enclosing_class {
                let class_node = self.ctx.arena.get(class_info.class_idx);
                if let Some(class) = class_node.and_then(|n| self.ctx.arena.get_class(n)) {
                    self.get_class_instance_type(class_info.class_idx, class)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };

            // Set expected return type to class instance type
            self.push_return_type(instance_type);
            self.check_statement(ctor.body);
            self.pop_return_type();
        }

        // Reset in_constructor flag
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = false;
        }

        // Check overload compatibility for constructor implementations
        if !ctor.body.is_none() {
            self.check_overload_compatibility(member_idx);
        }
    }

    /// Check an accessor declaration (getter/setter).
    pub(crate) fn check_accessor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(accessor) = self.ctx.arena.get_accessor(node) else {
            return;
        };

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the accessor has a body
        if !accessor.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
            );
        }

        // Error 1318: An abstract accessor cannot have an implementation
        // Abstract accessors must not have a body
        if !accessor.body.is_none() && self.has_abstract_modifier(&accessor.modifiers) {
            self.error_at_node(
                member_idx,
                "An abstract accessor cannot have an implementation.",
                diagnostic_codes::ABSTRACT_ACCESSOR_CANNOT_HAVE_IMPLEMENTATION,
            );
        }

        let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;
        let has_type_annotation = is_getter && !accessor.type_annotation.is_none();
        let mut return_type = if is_getter {
            if has_type_annotation {
                self.get_type_from_type_node(accessor.type_annotation)
            } else {
                TypeId::VOID // Default to void for getters without type annotation
            }
        } else {
            TypeId::VOID
        };

        self.cache_parameter_types(&accessor.parameters.nodes, None);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&accessor.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // Check getter parameters for TS7006 here.
        // Setter parameters are checked in check_setter_parameter() below, which also
        // validates other setter constraints (no initializer, no rest parameter).
        if is_getter {
            for &param_idx in &accessor.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    self.maybe_report_implicit_any_parameter(param, false);
                }
            }
        }

        // For setters, check parameter constraints (1052, 1053)
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.check_setter_parameter(&accessor.parameters.nodes);
        }

        // Check accessor body
        if !accessor.body.is_none() {
            if is_getter && !has_type_annotation {
                return_type = self.infer_getter_return_type(accessor.body);
            }

            // TS7010 (implicit any return) is only emitted for ambient accessors,
            // matching TypeScript's behavior
            // Async getters infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if is_getter {
                let is_ambient_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .map(|c| c.is_declared)
                    .unwrap_or(false);
                let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");
                let is_async = self.has_async_modifier(&accessor.modifiers);

                if (is_ambient_class || is_ambient_file) && !is_async {
                    let accessor_name = self.get_property_name(accessor.name);
                    self.maybe_report_implicit_any_return(
                        accessor_name,
                        Some(accessor.name),
                        return_type,
                        has_type_annotation,
                        false,
                        member_idx,
                    );
                }
            }

            self.push_return_type(return_type);
            self.check_statement(accessor.body);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                // For async getters, extract the inner type from Promise<T>
                let check_return_type = self.return_type_for_implicit_return_check(
                    return_type,
                    is_async,
                    false, // getters cannot be generators
                );
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);

                // TS2378: A 'get' accessor must return a value (regardless of type annotation)
                // Get accessors ALWAYS require a return value, even without type annotation
                if !has_return && falls_through {
                    // Use TS2378 for getters without return statements
                    self.error_at_node(
                        accessor.name,
                        "A 'get' accessor must return a value.",
                        diagnostic_codes::GET_ACCESSOR_MUST_RETURN_VALUE,
                    );
                } else if has_type_annotation && requires_return && falls_through {
                    // TS2355: For getters with type annotation that requires return, but have
                    // some return statements but also fall through
                    use crate::checker::types::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        accessor.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                    );
                } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::checker::types::diagnostics::diagnostic_messages;
                    let error_node = if !accessor.name.is_none() {
                        accessor.name
                    } else {
                        accessor.body
                    };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                    );
                }
            }
            self.pop_return_type();
        }
    }

    /// Check setter parameter constraints (1052, 1053).
    /// - A 'set' accessor parameter cannot have an initializer
    /// - A 'set' accessor cannot have rest parameter
    ///
    /// Promise/async type checking methods moved to promise_checker.rs
    /// The lower_type_with_bindings helper remains here as it requires
    /// access to private resolver methods.

    /// Lower a type node with type parameter bindings.
    ///
    /// This is used to substitute type parameters with concrete types
    /// when extracting type arguments from generic Promise types.
    /// Made pub(crate) so it can be called from promise_checker.rs.
    pub(crate) fn lower_type_with_bindings(
        &self,
        type_node: NodeIndex,
        bindings: Vec<(crate::interner::Atom, TypeId)>,
    ) -> TypeId {
        use crate::solver::TypeLowering;

        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(bindings);
        lowering.lower_type(type_node)
    }

    // Note: type_contains_any, implicit_any_return_display, should_report_implicit_any_return are in type_checking.rs

    pub(crate) fn maybe_report_implicit_any_return(
        &mut self,
        name: Option<String>,
        name_node: Option<NodeIndex>,
        return_type: TypeId,
        has_type_annotation: bool,
        has_contextual_return: bool,
        fallback_node: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.no_implicit_any() || has_type_annotation || has_contextual_return {
            return;
        }
        if !self.should_report_implicit_any_return(return_type) {
            return;
        }

        let return_text = self.implicit_any_return_display(return_type);
        if let Some(name) = name {
            let message = format_message(
                diagnostic_messages::IMPLICIT_ANY_RETURN,
                &[&name, &return_text],
            );
            self.error_at_node(
                name_node.unwrap_or(fallback_node),
                &message,
                diagnostic_codes::IMPLICIT_ANY_RETURN,
            );
        } else {
            let message = format_message(
                diagnostic_messages::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
                &[&return_text],
            );
            self.error_at_node(
                fallback_node,
                &message,
                diagnostic_codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
            );
        }
    }

    /// Check overload compatibility: implementation must be assignable to all overload signatures.
    ///
    /// Reports TS2394 when an implementation signature is not compatible with its overload signatures.
    /// This check ensures that the implementation can handle all valid calls that match the overloads.
    ///
    /// Per TypeScript's variance rules:
    /// - Implementation parameters must be supertypes of overload parameters (contravariant)
    /// - Implementation return type must be subtype of overload return type (covariant)
    /// - Effectively: Implementation <: Overload (implementation is assignable to overload)
    ///
    /// This handles:
    /// - Function declarations
    /// - Method declarations (class methods)
    /// - Constructor declarations
    pub(crate) fn check_overload_compatibility(&mut self, impl_node_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // 1. Get the implementation's symbol
        let Some(impl_sym_id) = self.ctx.binder.get_node_symbol(impl_node_idx) else {
            return;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(impl_sym_id) else {
            return;
        };

        // 2. Create TypeLowering instance for manual signature lowering
        // This unblocks overload validation for methods/constructors where get_type_of_node returns ERROR
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let lowering = crate::solver::TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        // 3. Get the implementation's type using manual lowering
        let impl_type = lowering.lower_signature_from_declaration(impl_node_idx, None);
        if impl_type == crate::solver::TypeId::ERROR {
            // Fall back to get_type_of_node for cases where manual lowering fails
            let impl_type = self.get_type_of_node(impl_node_idx);
            if impl_type == crate::solver::TypeId::ERROR {
                return;
            }
        }

        // 4. Check each overload declaration
        for &decl_idx in &symbol.declarations {
            // Skip the implementation itself
            if decl_idx == impl_node_idx {
                continue;
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // 5. Check if this declaration is an overload (has no body)
            // We must handle Functions, Methods, and Constructors
            let is_overload = match decl_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .ctx
                    .arena
                    .get_function(decl_node)
                    .map(|f| f.body.is_none())
                    .unwrap_or(false),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(decl_node)
                    .map(|m| m.body.is_none())
                    .unwrap_or(false),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(decl_node)
                    .map(|c| c.body.is_none())
                    .unwrap_or(false),
                _ => false, // Not a callable declaration we care about
            };

            if !is_overload {
                continue;
            }

            // 6. Get the overload's type using manual lowering
            let overload_type = lowering.lower_signature_from_declaration(decl_idx, None);
            if overload_type == crate::solver::TypeId::ERROR {
                // Fall back to get_type_of_node for cases where manual lowering fails
                let overload_type = self.get_type_of_node(decl_idx);
                if overload_type == crate::solver::TypeId::ERROR {
                    continue;
                }
            }

            // 7. Check assignability: Impl <: Overload
            if !self.is_assignable_to(impl_type, overload_type) {
                self.error_at_node(
                    decl_idx,
                    diagnostic_messages::OVERLOAD_NOT_COMPATIBLE_WITH_IMPLEMENTATION,
                    diagnostic_codes::OVERLOAD_SIGNATURE_NOT_COMPATIBLE,
                );
            }
        }
    }

    // Note: is_derived_property_redeclaration, find_containing_class are in type_checking.rs
}

/// Implementation of StatementCheckCallbacks for CheckerState.
///
/// This provides the actual implementation of statement checking operations
/// that StatementChecker delegates to. Each callback method calls the
/// corresponding method on CheckerState.
impl<'a> StatementCheckCallbacks for CheckerState<'a> {
    fn arena(&self) -> &crate::parser::node::NodeArena {
        self.ctx.arena
    }

    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        CheckerState::get_type_of_node(self, idx)
    }

    fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_variable_statement(self, stmt_idx)
    }

    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        CheckerState::check_variable_declaration_list(self, list_idx)
    }

    fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        CheckerState::check_variable_declaration(self, decl_idx)
    }

    fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_return_statement(self, stmt_idx)
    }

    fn check_unreachable_code_in_block(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_unreachable_code_in_block(self, stmts)
    }

    fn check_function_implementations(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_function_implementations(self, stmts)
    }

    fn check_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        // Delegate to DeclarationChecker for function declaration-specific checks
        // (only for actual function declarations, not expressions/arrows)
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            let mut checker = crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_function_declaration(func_idx);
        }

        // Re-get node after DeclarationChecker borrows ctx
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        // Check for missing Promise global type when function is async (TS2318)
        // TSC emits this at the start of the file when Promise is not available
        // Only check for non-generator async functions (async generators use AsyncGenerator, not Promise)
        if func.is_async && !func.asterisk_token {
            self.check_global_promise_available();
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors
        self.check_parameter_properties(&func.parameters.nodes);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&func.parameters);

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&func.parameters);

        // Check return type annotation for parameter properties in function types
        if !func.type_annotation.is_none() {
            self.check_type_for_parameter_properties(func.type_annotation);
        }

        // Check parameter type annotations for parameter properties
        for &param_idx in &func.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                && !param.type_annotation.is_none()
            {
                self.check_type_for_parameter_properties(param.type_annotation);
            }
        }

        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            self.maybe_report_implicit_any_parameter(param, false);
        }

        // Check function body if present
        let has_type_annotation = !func.type_annotation.is_none();
        if !func.body.is_none() {
            let mut return_type = if has_type_annotation {
                self.get_type_of_node(func.type_annotation)
            } else {
                // Use UNKNOWN to enforce strict checking
                TypeId::UNKNOWN
            };

            // Cache parameter types from annotations (so for-of binding uses correct types)
            // and then infer for any remaining unknown parameters using contextual information.
            self.cache_parameter_types(&func.parameters.nodes, None);
            self.infer_parameter_types_from_context(&func.parameters.nodes);

            // Check that parameter default values are assignable to declared types (TS2322)
            self.check_parameter_initializers(&func.parameters.nodes);

            if !has_type_annotation {
                return_type = self.infer_return_type_from_body(func.body, None);
            }

            // TS7010 (implicit any return) is emitted for functions without
            // return type annotations when noImplicitAny is enabled and the return
            // type cannot be inferred (e.g., is 'any' or only returns undefined)
            // Async functions infer Promise<void>, not 'any', so they should NOT trigger TS7010
            // maybe_report_implicit_any_return handles the noImplicitAny check internally
            if !func.is_async {
                let func_name = self.get_function_name_from_node(func_idx);
                let name_node = if !func.name.is_none() {
                    Some(func.name)
                } else {
                    None
                };
                self.maybe_report_implicit_any_return(
                    func_name,
                    name_node,
                    return_type,
                    has_type_annotation,
                    false,
                    func_idx,
                );
            }

            // TS2705: Async function must return Promise
            // Only check if there's an explicit return type annotation that is NOT Promise
            // Skip this check if the return type is ERROR or the annotation looks like Promise
            // Note: Async generators (async function*) return AsyncGenerator, not Promise
            if func.is_async && !func.asterisk_token && has_type_annotation {
                let should_emit_ts2705 = !self.is_promise_type(return_type)
                    && return_type != TypeId::ERROR
                    && !self.return_type_annotation_looks_like_promise(func.type_annotation);

                if should_emit_ts2705 {
                    use crate::checker::context::ScriptTarget;
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };

                    // For ES5/ES3 targets, emit TS1055 instead of TS2705
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );

                    if is_es5_or_lower {
                        let type_name = self.format_type(return_type);
                        self.error_at_node(
                            func.type_annotation,
                            &format_message(
                                diagnostic_messages::TYPE_NOT_VALID_ASYNC_RETURN_TYPE_ES5,
                                &[&type_name],
                            ),
                            diagnostic_codes::TYPE_NOT_VALID_ASYNC_RETURN_TYPE_ES5,
                        );
                    } else {
                        self.error_at_node(
                            func.type_annotation,
                            diagnostic_messages::ASYNC_FUNCTION_RETURNS_PROMISE,
                            diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                        );
                    }
                }
            }

            // Extract this type from explicit `this` parameter
            let mut pushed_this_type = false;
            if let Some(&first_param) = func.parameters.nodes.first() {
                if let Some(param_node) = self.ctx.arena.get(first_param)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let is_this = if let Some(name_node) = self.ctx.arena.get(param.name) {
                        if name_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
                            self.ctx
                                .arena
                                .get_identifier(name_node)
                                .is_some_and(|id| id.escaped_text == "this")
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if is_this && !param.type_annotation.is_none() {
                        let this_type = self.get_type_from_type_node(param.type_annotation);
                        self.ctx.this_type_stack.push(this_type);
                        pushed_this_type = true;
                    }
                }
            }

            // Enter async context for await expression checking
            if func.is_async {
                self.ctx.enter_async_context();
            }

            // For generator functions with explicit return type (Generator<Y, R, N> or AsyncGenerator<Y, R, N>),
            // return statements should be checked against TReturn (R), not the full Generator type.
            // This matches TypeScript's behavior where `return x` in a generator checks `x` against TReturn.
            let is_generator = func.asterisk_token;
            let body_return_type = if is_generator && has_type_annotation {
                self.get_generator_return_type_argument(return_type)
                    .unwrap_or(return_type)
            } else {
                return_type
            };

            self.push_return_type(body_return_type);
            self.check_statement(func.body);

            // Check for error 2355: function with return type must return a value
            // Only check if there's an explicit return type annotation
            let is_async = func.is_async;
            let is_generator = func.asterisk_token;
            let check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(func.body);
            let falls_through = self.function_body_falls_through(func.body);

            // TS2355: Skip for async functions - they implicitly return Promise<void>
            if has_type_annotation && requires_return && falls_through && !is_async {
                if !has_return {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        func.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                    );
                } else {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        func.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                    );
                }
            } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                let error_node = if !func.name.is_none() {
                    func.name
                } else {
                    func.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                );
            }

            self.pop_return_type();

            // Exit async context
            if func.is_async {
                self.ctx.exit_async_context();
            }

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }
        } else if self.ctx.no_implicit_any() && !has_type_annotation {
            let is_ambient =
                self.has_declare_modifier(&func.modifiers) || self.ctx.file_name.ends_with(".d.ts");
            if is_ambient && let Some(func_name) = self.get_function_name_from_node(func_idx) {
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::IMPLICIT_ANY_RETURN,
                    &[&func_name, "any"],
                );
                let name_node = if !func.name.is_none() {
                    Some(func.name)
                } else {
                    None
                };
                self.error_at_node(
                    name_node.unwrap_or(func_idx),
                    &message,
                    diagnostic_codes::IMPLICIT_ANY_RETURN,
                );
            }
        }

        // Check overload compatibility: implementation must be assignable to all overloads
        // This is the function implementation validation (TS2394)
        if !func.body.is_none() {
            // Only check for implementations (functions with bodies)
            self.check_overload_compatibility(func_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        // Note: DeclarationChecker::check_class_declaration handles TS2564 (property
        // initialization) but CheckerState::check_class_declaration also handles it
        // more comprehensively (with parameter properties, derived classes, etc.).
        // We skip the DeclarationChecker delegation for classes to avoid duplicate
        // TS2564 emissions. DeclarationChecker::check_class_declaration is tested
        // independently via its own test suite.
        CheckerState::check_class_declaration(self, class_idx)
    }

    fn check_interface_declaration(&mut self, iface_idx: NodeIndex) {
        // Delegate to DeclarationChecker first
        let mut checker = crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_interface_declaration(iface_idx);

        // Continue with comprehensive interface checking in CheckerState
        CheckerState::check_interface_declaration(self, iface_idx)
    }

    fn check_import_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_declaration(self, import_idx)
    }

    fn check_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_equals_declaration(self, import_idx)
    }

    fn check_export_declaration(&mut self, export_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(export_idx) {
            if let Some(export_decl) = self.ctx.arena.get_export_decl(node) {
                // Check module specifier for unresolved modules (TS2792)
                if !export_decl.module_specifier.is_none() {
                    self.check_export_module_specifier(export_idx);
                }
                // Check the wrapped declaration
                if !export_decl.export_clause.is_none() {
                    self.check_statement(export_decl.export_clause);
                }
            }
        }
    }

    fn check_type_alias_declaration(&mut self, type_alias_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(type_alias_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_type_alias_declaration(type_alias_idx);

            // Continue with comprehensive type alias checking
            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                let (_params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                self.check_type_for_missing_names(type_alias.type_node);
                self.check_type_for_parameter_properties(type_alias.type_node);
                self.pop_type_parameters(updates);
            }
        }
    }
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        // TS1042: async modifier cannot be used on enum declarations
        if let Some(node) = self.ctx.arena.get(enum_idx)
            && let Some(enum_data) = self.ctx.arena.get_enum(node)
        {
            self.check_async_modifier_on_declaration(&enum_data.modifiers);
        }

        // Delegate to DeclarationChecker first
        let mut checker = crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_enum_declaration(enum_idx);

        // Continue with enum duplicate members checking
        CheckerState::check_enum_duplicate_members(self, enum_idx)
    }

    fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(module_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_module_declaration(module_idx);

            // Check module body and modifiers
            if let Some(module) = self.ctx.arena.get_module(node) {
                // TS1042: async modifier cannot be used on module/namespace declarations
                self.check_async_modifier_on_declaration(&module.modifiers);

                let is_ambient = self.has_declare_modifier(&module.modifiers);
                if !module.body.is_none() && !is_ambient {
                    self.check_module_body(module.body);
                }
            }
        }
    }

    fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        CheckerState::check_await_expression(self, expr_idx)
    }

    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
    ) {
        CheckerState::assign_for_in_of_initializer_types(self, decl_list_idx, loop_var_type)
    }

    fn for_of_element_type(&mut self, expr_type: TypeId) -> TypeId {
        CheckerState::for_of_element_type(self, expr_type)
    }

    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    ) {
        CheckerState::check_for_of_iterability(self, expr_type, expr_idx, await_modifier);
    }

    fn check_statement(&mut self, stmt_idx: NodeIndex) {
        // This calls back to the main check_statement which will delegate to StatementChecker
        CheckerState::check_statement(self, stmt_idx)
    }

    fn check_switch_exhaustiveness(
        &mut self,
        _stmt_idx: NodeIndex,
        expression: NodeIndex,
        case_block: NodeIndex,
        has_default: bool,
    ) {
        // If there's a default clause, the switch is syntactically exhaustive
        if has_default {
            return;
        }

        // Get the discriminant type
        let discriminant_type = self.get_type_of_node(expression);

        // Create a FlowAnalyzer to check exhaustiveness
        let analyzer = crate::checker::control_flow::FlowAnalyzer::new(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
        );

        // Create a narrowing context
        let narrowing = crate::solver::NarrowingContext::new(self.ctx.types);

        // Calculate the "no-match" type (what type the discriminant would have
        // if none of the case clauses match)
        let no_match_type = analyzer.narrow_by_default_switch_clause(
            discriminant_type,
            expression,
            case_block,
            expression, // target is the discriminant itself
            &narrowing,
        );

        // If the no-match type is not `never`, the switch is not exhaustive
        if no_match_type != crate::solver::TypeId::NEVER {
            // TODO: Emit diagnostic (TS2366 or custom error)
        }
    }
}
