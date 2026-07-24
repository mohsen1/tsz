//! Duplicate class-member checking (TS2300 / TS2393 / TS2717).
//!
//! Extracted from `interface_checks.rs` to keep each file under the
//! 2000-line architectural limit. These helpers validate class member
//! declarations (properties, methods, accessors) for duplicate identifiers
//! and conflicting subsequent declarations.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// The property/method/accessor name node of a class member, if any.
    fn class_member_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.ctx.arena.get(member_idx)?;
        let name_idx = match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(member_node)
                .map(|p| p.name),
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.ctx.arena.get_method_decl(member_node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(member_node).map(|a| a.name)
            }
            _ => None,
        }?;
        name_idx.is_some().then_some(name_idx)
    }

    /// Report TS2300 "Duplicate identifier" error for a class member (property or method).
    /// Helper function to avoid code duplication in `check_duplicate_class_members`.
    ///
    /// The error is anchored at `member_idx`, but the rendered name is taken from
    /// `name_source_idx` — the *first* declaration in the duplicate group. tsc's
    /// `declarationNameToString` renders the first declaration's verbatim source
    /// spelling and reuses it at every occurrence, so `{ 0; 0.0 }` reports `'0'`
    /// (not `0.0`) at both, and `{ "1"; 1 }` reports `'"1"'`.
    fn report_duplicate_class_member_ts2300(
        &mut self,
        member_idx: NodeIndex,
        name_source_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let error_node = self
            .class_member_name_node(member_idx)
            .unwrap_or(member_idx);
        let Some(name_source_name) = self.class_member_name_node(name_source_idx) else {
            return;
        };
        if let Some(display_name) = self.declaration_name_to_string(name_source_name) {
            self.error_at_node_msg(
                error_node,
                diagnostic_codes::DUPLICATE_IDENTIFIER,
                &[&display_name],
            );
        }
    }

    /// Extract explicit type annotation info for a class property declaration.
    fn get_class_property_declared_type_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, TypeId)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }

        let prop = self.ctx.arena.get_property_decl(member_node)?;
        let name = self.get_member_name_text(prop.name)?;

        let type_id = if let Some(declared_type) =
            self.effective_class_property_declared_type(member_idx, prop)
        {
            declared_type
        } else if prop.initializer.is_some() {
            // Infer type from initializer when no explicit annotation
            self.get_type_of_node(prop.initializer)
        } else {
            return None;
        };
        Some((name, prop.name, type_id))
    }

    fn get_class_method_type_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, TypeId)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return None;
        }

        let method = self.ctx.arena.get_method_decl(member_node)?;
        let name = self.get_member_name_text(method.name)?;
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let return_type = if method.type_annotation.is_some() {
            self.get_type_from_type_node(method.type_annotation)
        } else if method.body.is_some() {
            self.infer_return_type_from_body(member_idx, method.body, None)
        } else {
            TypeId::ANY
        };
        self.pop_type_parameters(type_param_updates);

        let type_id = self
            .ctx
            .types
            .factory()
            .function(tsz_solver::FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate: None,
                is_constructor: false,
                is_method: true,
            });

        Some((name, method.name, type_id))
    }

    pub(super) fn get_class_member_name_info(
        &self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, bool)> {
        let member_node = self.ctx.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node)?;
                Some((
                    self.get_member_name_text(prop.name)?,
                    prop.name,
                    self.has_static_modifier(&prop.modifiers),
                ))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node)?;
                Some((
                    self.get_member_name_text(method.name)?,
                    method.name,
                    self.has_static_modifier(&method.modifiers),
                ))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                Some((
                    self.get_member_name_text(accessor.name)?,
                    accessor.name,
                    self.has_static_modifier(&accessor.modifiers),
                ))
            }
            _ => None,
        }
    }

    /// Extract type info for a class accessor declaration.
    /// For getters, use explicit return annotation if present, otherwise infer from body.
    /// For setters, use the first parameter type annotation (or `any` if omitted).
    fn get_class_accessor_type_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, TypeId, bool)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        if member_node.kind != syntax_kind_ext::GET_ACCESSOR
            && member_node.kind != syntax_kind_ext::SET_ACCESSOR
        {
            return None;
        }

        let accessor = self.ctx.arena.get_accessor(member_node)?;
        let name = self.get_member_name_text(accessor.name)?;
        let is_static = self.has_static_modifier(&accessor.modifiers);

        let type_id = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                self.get_type_from_type_node(accessor.type_annotation)
            } else if accessor.body.is_some() {
                self.infer_getter_return_type(accessor.body)
            } else {
                TypeId::ANY
            }
        } else if let Some(&first_param_idx) = accessor.parameters.nodes.first() {
            if let Some(param) = self.ctx.arena.get_parameter_at(first_param_idx) {
                if param.type_annotation.is_some() {
                    self.get_type_from_type_node(param.type_annotation)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::ANY
        };

        Some((name, accessor.name, type_id, is_static))
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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::{FxHashMap, FxHashSet};

        // Track member names with their info
        struct MemberInfo {
            indices: Vec<NodeIndex>,
            is_property: Vec<bool>, // true for PROPERTY_DECLARATION, false for METHOD_DECLARATION
            method_has_body: Vec<bool>, // only valid when is_property is false
            is_static: Vec<bool>,
        }

        struct AccessorInfo {
            indices: Vec<NodeIndex>,
            is_private: bool,
        }

        let mut seen_names: FxHashMap<String, MemberInfo> = FxHashMap::default();
        let mut constructor_declarations: Vec<NodeIndex> = Vec::new();
        let mut constructor_implementations: Vec<NodeIndex> = Vec::new();

        // Track accessor occurrences for duplicate detection
        // Key: "get:name" or "set:name" (with "static:" prefix for static members)
        let mut seen_accessors: FxHashMap<String, AccessorInfo> = FxHashMap::default();

        // Track accessor plain names (without get/set prefix) for cross-checking
        // against properties/methods. Key: "name" or "static:name"
        let mut accessor_plain_names: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

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
                        // Skip late-bound computed names unless the key is a
                        // const symbol identifier (e.g. `[sym]` where `const sym = Symbol()`).
                        if self.is_late_bound_member_name(prop.name)
                            && !self.should_check_late_bound_class_property_name(prop.name)
                        {
                            return None;
                        }
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
                        // Skip late-bound computed names — tsc doesn't check duplicates for these
                        if self.is_late_bound_member_name(method.name) {
                            return None;
                        }
                        let has_body = method.body.is_some();
                        let is_static = self.has_static_modifier(&method.modifiers);
                        self.get_member_name_text(method.name)
                            .map(|n| (n, false, has_body, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Track accessors for duplicate detection (getter/setter pairs are allowed,
                    // but duplicate getters or duplicate setters are not)
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                        && !self.is_late_bound_member_name(accessor.name)
                        && let Some(name) = self.get_member_name_text(accessor.name)
                    {
                        let is_static = self.has_static_modifier(&accessor.modifiers);
                        let is_private = self.is_private_identifier_name(accessor.name);
                        let kind = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                            "get"
                        } else {
                            "set"
                        };
                        let key = if is_static {
                            format!("static:{kind}:{name}")
                        } else {
                            format!("{kind}:{name}")
                        };
                        let info = seen_accessors.entry(key).or_insert(AccessorInfo {
                            indices: Vec::new(),
                            is_private,
                        });
                        info.indices.push(member_idx);
                        info.is_private |= is_private;

                        // Also track plain name for cross-checking with properties/methods
                        let plain_key = if is_static {
                            format!("static:{name}")
                        } else {
                            name.clone()
                        };
                        accessor_plain_names
                            .entry(plain_key)
                            .or_default()
                            .push(member_idx);
                    }
                    continue;
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    constructor_declarations.push(member_idx);
                    if let Some(constructor) = self.ctx.arena.get_constructor(member_node)
                        && constructor.body.is_some()
                    {
                        constructor_implementations.push(member_idx);
                    }
                    continue;
                }
                _ => continue,
            };

            if name.is_empty() {
                continue;
            }

            // Create a key that considers static vs instance members separately
            let key = if is_static {
                format!("static:{name}")
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
        for info in seen_names.values() {
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
                // TS2717: Duplicate class property declarations with incompatible explicit types.
                // Keep this narrow to explicit type annotations to avoid inference cascades.
                let first_declared = info
                    .indices
                    .first()
                    .and_then(|&idx| self.get_class_property_declared_type_info(idx));

                if let Some((_first_name, _first_name_node, first_type)) = &first_declared
                    && !self.type_contains_error(*first_type)
                {
                    let first_type_str = self.format_type(*first_type);
                    for &idx in info.indices.iter().skip(1) {
                        let Some((_name, name_node, current_type)) =
                            self.get_class_property_declared_type_info(idx)
                        else {
                            continue;
                        };
                        if self.type_contains_error(current_type) {
                            continue;
                        }
                        // TS2717 uses type identity, not assignability.
                        if *first_type != current_type {
                            // Use display text for the message to match TSC's declarationNameToString
                            let display_name = self
                                .get_member_name_display_text(name_node)
                                .unwrap_or_else(|| _name.clone());
                            let current_type_str = self.format_type(current_type);
                            self.error_at_node_msg(
                                    name_node,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[&display_name, &first_type_str, &current_type_str],
                                );
                        }
                    }
                }

                // All properties: tsc's binder (declareSymbol) reports
                // TS2300 at EVERY declaration site, the first included.
                let name_source = info.indices[0];
                for &idx in info.indices.iter() {
                    self.report_duplicate_class_member_ts2300(idx, name_source);
                }
            } else if property_count > 0 && method_count > 0 {
                // Mixed property/method duplicates: tsc 7.0.2 reports TS2300
                // at EVERY declaration (either order) and no TS2717 — the
                // subsequent-declaration type rule only applies between
                // PROPERTY declarations (oracle: `class { m(){} m: number }`
                // gets 2x TS2300 and nothing else).
                let name_source = info.indices[0];
                for &idx in info.indices.iter() {
                    self.report_duplicate_class_member_ts2300(idx, name_source);
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
                            .filter(|idx| idx.is_some())
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

        // TS2392: multiple constructor implementations are not allowed.
        // Constructor overload signatures are valid; only declarations with bodies count.
        if constructor_implementations.len() > 1 {
            for &idx in &constructor_declarations {
                self.error_at_node(
                    idx,
                    "Multiple constructor implementations are not allowed.",
                    diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                );
            }
        }

        // Report TS2300 for duplicate accessors (e.g., two getters or two setters with same name).
        //
        // tsc behaviour:
        // - When there are duplicate accessors of one kind (e.g., 2 setters) AND a paired
        //   accessor of the other kind (getter) exists, ALL accessor declarations for that
        //   name are flagged (the entire accessor group is invalid).
        // - When there are only duplicates of one kind with NO paired accessor, only the
        //   subsequent (non-first) duplicate declarations are flagged.
        // - Private names always report on all same-kind declarations.
        {
            // Collect plain names that have both a duplicate accessor AND a paired accessor
            // of the other kind (indicating the entire accessor group is broken).
            let mut names_with_paired_dup_accessors: FxHashSet<String> = FxHashSet::default();
            for (key, info) in &seen_accessors {
                if info.indices.len() <= 1 {
                    continue;
                }
                let static_prefix = key.starts_with("static:");
                let rest = key.strip_prefix("static:").unwrap_or(key);
                let (kind, plain) = if let Some(p) = rest.strip_prefix("get:") {
                    ("get", p)
                } else if let Some(p) = rest.strip_prefix("set:") {
                    ("set", p)
                } else {
                    continue;
                };
                // Check if the other kind exists
                let other_kind = if kind == "get" { "set" } else { "get" };
                let other_key = if static_prefix {
                    format!("static:{other_kind}:{plain}")
                } else {
                    format!("{other_kind}:{plain}")
                };
                if seen_accessors.contains_key(&other_key) {
                    let plain_key = if static_prefix {
                        format!("static:{plain}")
                    } else {
                        plain.to_string()
                    };
                    names_with_paired_dup_accessors.insert(plain_key);
                }
            }

            // For names with paired duplicate accessors, report on ALL accessor declarations
            if !names_with_paired_dup_accessors.is_empty() {
                for (plain_key, indices) in &accessor_plain_names {
                    if names_with_paired_dup_accessors.contains(plain_key) {
                        let name_source = indices[0];
                        for &idx in indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                    }
                }
            }

            // For remaining duplicate accessor keys (no paired accessor of other kind),
            // use the original single-kind duplicate logic: report only subsequent declarations.
            for (key, info) in &seen_accessors {
                if info.indices.len() <= 1 {
                    continue;
                }
                let static_prefix = key.starts_with("static:");
                let rest = key.strip_prefix("static:").unwrap_or(key);
                let plain = rest
                    .strip_prefix("get:")
                    .or_else(|| rest.strip_prefix("set:"))
                    .unwrap_or(rest);
                let plain_key = if static_prefix {
                    format!("static:{plain}")
                } else {
                    plain.to_string()
                };
                if names_with_paired_dup_accessors.contains(&plain_key) {
                    // Already handled above via accessor_plain_names
                    continue;
                }
                let start = if info.is_private { 0 } else { 1 };
                let name_source = info.indices[0];
                for &idx in info.indices.iter().skip(start) {
                    self.report_duplicate_class_member_ts2300(idx, name_source);
                }
            }
        }

        // TS2804: static and instance members cannot share the same private name.
        // tsc reports on BOTH colliding declarations (the earlier and the current),
        // not just the later one. Track the first instance and first static
        // declaration's name node per private name, plus whether the pair has
        // already been reported (so a 3rd colliding declaration does not
        // re-report the earlier one).
        let mut seen_private_name_staticness: FxHashMap<
            String,
            (Option<NodeIndex>, Option<NodeIndex>, bool),
        > = FxHashMap::default();
        for &member_idx in members {
            let Some((name, name_idx, is_static)) = self.get_class_member_name_info(member_idx)
            else {
                continue;
            };
            if !self.is_private_identifier_name(name_idx) {
                continue;
            }

            let seen = seen_private_name_staticness
                .entry(name.clone())
                .or_insert((None, None, false));
            // The opposite-staticness declaration seen earlier, if any.
            let opposite_idx = if is_static { seen.0 } else { seen.1 };
            let already_reported = seen.2;
            // First-wins per staticness.
            if is_static {
                seen.1.get_or_insert(name_idx);
            } else {
                seen.0.get_or_insert(name_idx);
            }
            if opposite_idx.is_some() {
                seen.2 = true;
            }

            if let Some(earlier_idx) = opposite_idx {
                self.error_at_node(
                    name_idx,
                    &format_message(
                        diagnostic_messages::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                        &[&name],
                    ),
                    diagnostic_codes::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                );
                if !already_reported {
                    self.error_at_node(
                        earlier_idx,
                        &format_message(
                            diagnostic_messages::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                            &[&name],
                        ),
                        diagnostic_codes::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                    );
                }
            }
        }

        // Cross-check accessors against properties/methods for TS2300.
        // Getter/setter pairs are allowed on their own, so conflicts with fields/methods
        // are reported only on declarations that appear after the opposing kind first
        // established the member name.
        //
        // tsc behaviour depends on whether the name is a computed property name
        // (e.g. `[Symbol.toPrimitive]`, `[sym]`) or a simple identifier (`m`, `x`):
        // - Simple identifiers: tsc flags ALL conflicting declarations (both property/method
        //   and accessor).
        // - Computed names: tsc flags only the LATER declarations — the first declaration
        //   that established the name is not flagged.
        for (key, accessor_indices) in &accessor_plain_names {
            if let Some(member_info) = seen_names.get(key) {
                // Strip "static:" prefix to check the bare name.
                let bare_key = key.strip_prefix("static:").unwrap_or(key);
                let is_computed = bare_key.starts_with('[');

                // tsc renders the duplicate name from the group's FIRST declaration
                // (by source position, across both the property/method and accessor
                // members), reusing that spelling at every reported occurrence.
                let name_source = member_info
                    .indices
                    .iter()
                    .chain(accessor_indices.iter())
                    .copied()
                    .min_by_key(|&idx| self.ctx.arena.pos_at(idx).unwrap_or(u32::MAX))
                    .unwrap_or(NodeIndex::NONE);

                if is_computed {
                    // Computed names: only report on later declarations.
                    let first_member_pos = member_info
                        .indices
                        .first()
                        .and_then(|&idx| self.ctx.arena.get(idx))
                        .map(|n| n.pos)
                        .unwrap_or(u32::MAX);
                    let first_accessor_pos = accessor_indices
                        .first()
                        .and_then(|&idx| self.ctx.arena.get(idx))
                        .map(|n| n.pos)
                        .unwrap_or(u32::MAX);

                    if first_member_pos < first_accessor_pos {
                        // Method/property came first — only flag accessors
                        for &idx in accessor_indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                    } else {
                        // Accessor came first — only flag methods/properties
                        for &idx in &member_info.indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                    }
                } else {
                    // Simple identifiers: tsc's rule is order-sensitive.
                    //
                    // Two exceptions to the default "flag all" rule:
                    //
                    // 1. Private names: when an accessor appears BEFORE a field
                    //    (property) with the same private name, tsc flags only the
                    //    field — the accessor is treated as the established
                    //    declaration, and the subsequent field is the "real" conflict.
                    //
                    // 2. Public names with a valid get+set pair: when a getter and
                    //    setter (one of each kind, no same-kind duplicates) are
                    //    declared BEFORE a conflicting property/method, tsc treats
                    //    the accessor pair as establishing the member — only the
                    //    later property/method is flagged.
                    //
                    // Otherwise (property came first, or accessors don't form a
                    // valid pair), flag all conflicting declarations.
                    let is_private = bare_key.starts_with('#');
                    let first_accessor_pos = accessor_indices
                        .first()
                        .and_then(|&idx| self.ctx.arena.get(idx))
                        .map(|n| n.pos);
                    let first_field_pos = member_info
                        .indices
                        .iter()
                        .zip(member_info.is_property.iter())
                        .filter(|(_, is_prop)| **is_prop)
                        .filter_map(|(&idx, _)| self.ctx.arena.pos_at(idx))
                        .min();
                    let field_strictly_after_accessor = matches!(
                        (first_field_pos, first_accessor_pos),
                        (Some(fp), Some(ap)) if fp > ap
                    );

                    let static_prefix = key.starts_with("static:");
                    let plain = key.strip_prefix("static:").unwrap_or(key);
                    let get_key = if static_prefix {
                        format!("static:get:{plain}")
                    } else {
                        format!("get:{plain}")
                    };
                    let set_key = if static_prefix {
                        format!("static:set:{plain}")
                    } else {
                        format!("set:{plain}")
                    };
                    let get_info = seen_accessors.get(&get_key);
                    let set_info = seen_accessors.get(&set_key);
                    let has_valid_pair = matches!(
                        (get_info, set_info),
                        (Some(g), Some(s)) if g.indices.len() == 1 && s.indices.len() == 1
                    );

                    let first_member_pos = member_info
                        .indices
                        .first()
                        .and_then(|&idx| self.ctx.arena.get(idx))
                        .map(|n| n.pos)
                        .unwrap_or(u32::MAX);
                    let last_accessor_pos = accessor_indices
                        .iter()
                        .filter_map(|&idx| self.ctx.arena.pos_at(idx))
                        .max()
                        .unwrap_or(0);

                    let public_pair_before_member =
                        has_valid_pair && last_accessor_pos < first_member_pos;

                    if (is_private && field_strictly_after_accessor) || public_pair_before_member {
                        // Accessor(s) established the member first — flag only
                        // the later property/method declarations.
                        for &idx in &member_info.indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                    } else {
                        // Property/method came first, or accessors don't form a
                        // qualifying set: flag all conflicting declarations.
                        for &idx in &member_info.indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                        for &idx in accessor_indices {
                            self.report_duplicate_class_member_ts2300(idx, name_source);
                        }
                    }
                }
            }
        }

        // TS2717: If a property declaration comes after accessors with the same name,
        // report incompatible types (e.g., get/set infer `number`, later field is `any`).
        let mut seen_accessor_type_by_key: FxHashMap<String, TypeId> = FxHashMap::default();
        for &member_idx in members {
            if let Some((name, _name_node, accessor_type, is_static)) =
                self.get_class_accessor_type_info(member_idx)
            {
                if self.type_contains_error(accessor_type) {
                    continue;
                }
                let key = if is_static {
                    format!("static:{name}")
                } else {
                    name
                };
                seen_accessor_type_by_key
                    .entry(key)
                    .or_insert(accessor_type);
                continue;
            }

            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            let Some(name) = self.get_member_name_text(prop.name) else {
                continue;
            };
            let is_static = self.has_static_modifier(&prop.modifiers);
            let key = if is_static {
                format!("static:{}", name.clone())
            } else {
                name.clone()
            };
            let Some(&first_type) = seen_accessor_type_by_key.get(&key) else {
                continue;
            };
            if self.type_contains_error(first_type) {
                continue;
            }
            let current_type = if let Some(declared_type) =
                self.effective_class_property_declared_type(member_idx, prop)
            {
                declared_type
            } else if prop.initializer.is_some() {
                self.get_type_of_node(prop.initializer)
            } else {
                TypeId::ANY
            };
            if self.type_contains_error(current_type) {
                continue;
            }
            let is_incompatible = if first_type == TypeId::ANY || current_type == TypeId::ANY {
                first_type != current_type
            } else {
                !self.are_mutually_assignable(first_type, current_type)
            };
            if is_incompatible {
                let first_type_str = self.format_type(first_type);
                let current_type_str = self.format_type(current_type);
                self.error_at_node_msg(
                    prop.name,
                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                    &[&name, &first_type_str, &current_type_str],
                );
            }
        }
    }
}
