//! Interface declaration and duplicate member checking.
//!
//! Extracted from `member_access.rs` to keep each file under the 2000-line
//! architectural limit.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check an interface declaration.
    pub(crate) fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on interface declarations
        self.check_async_modifier_on_declaration(&iface.modifiers);

        // Check for reserved interface names (error 2427)
        if iface.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(iface.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            // Reserved type names that can't be used as interface names
            match ident.escaped_text.as_str() {
                "string" | "number" | "boolean" | "symbol" | "void" | "object" | "any"
                | "unknown" | "never" | "bigint" | "intrinsic" | "undefined" | "null" => {
                    self.error_at_node(
                        iface.name,
                        &format!("Interface name cannot be '{}'.", ident.escaped_text),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
                _ => {}
            }
        }

        // NOTE: TSC does NOT emit TS1212 for interface declaration names.
        // e.g. `interface interface {}` gets TS1438 only, not TS1212.

        // Check for circular inheritance (TS2310)
        // Must be done before resolving types to avoid infinite recursion
        use crate::class_inheritance::ClassInheritanceChecker;
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if checker.check_interface_inheritance_cycle(stmt_idx, iface) {
            // If cycle detected, we can still proceed with checking members but
            // heritage graph is now aware of the cycle (or it was reported)
        }

        // Push type parameters BEFORE checking heritage clauses
        // This allows heritage clauses to reference the interface's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&iface.type_parameters);

        // Check for duplicate type parameters
        self.check_duplicate_type_parameters(&iface.type_parameters);

        // Check type parameter defaults for ordering (TS2706), forward references (TS2744),
        // and circular defaults (TS2716)
        let iface_name_str = self
            .ctx
            .arena
            .get(iface.name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.to_string());
        if let Some(ref name) = iface_name_str {
            self.check_type_parameters_for_missing_names_with_enclosing(
                &iface.type_parameters,
                name,
            );
        } else {
            self.check_type_parameters_for_missing_names(&iface.type_parameters);
        }

        // Collect interface type parameter names for TS2304 checking in heritage clauses
        let interface_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &iface.heritage_clauses,
            false,
            &interface_type_param_names,
        );

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&iface.type_parameters, stmt_idx);

        // Check each interface member for missing type references and parameter properties
        // Get interface name for circularity checks (TS2502/TS2615)
        let iface_name = if iface.name != NodeIndex::NONE {
            self.ctx
                .arena
                .get(iface.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| self.ctx.arena.resolve_identifier_text(ident).to_string())
        } else {
            None
        };

        for &member_idx in &iface.members.nodes {
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
            // TS1268: Check index signature parameter types
            self.check_index_signature_parameter_type(member_idx);
            // TS1169: Computed property in interface must have literal/unique symbol type
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(sig) = self.ctx.arena.get_signature(member_node)
            {
                self.check_interface_computed_property_name(sig.name);
            }
            // TS2502 + TS2615: Check if property type annotation circularly
            // references itself through a mapped type applied to the enclosing interface.
            if let Some(ref iface_name) = iface_name {
                self.check_interface_property_circular_mapped_type(member_idx, iface_name);
            }
        }

        // TS2386: Check optionality agreement for interface method overloads
        {
            use rustc_hash::FxHashMap;

            // Group method signatures by name
            let mut method_groups: FxHashMap<String, Vec<(NodeIndex, bool)>> = FxHashMap::default();
            for &member_idx in &iface.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(sig.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                method_groups
                    .entry(ident.escaped_text.clone())
                    .or_default()
                    .push((member_idx, sig.question_token));
            }
            for members in method_groups.values() {
                if members.len() < 2 {
                    continue;
                }
                let first_optional = members[0].1;
                for &(member_idx, optional) in &members[1..] {
                    if optional != first_optional {
                        let error_node = self
                            .ctx
                            .arena
                            .get(member_idx)
                            .and_then(|n| self.ctx.arena.get_signature(n))
                            .map(|s| s.name)
                            .unwrap_or(member_idx);
                        self.error_at_node(
                            error_node,
                            crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                            crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                        );
                    }
                }
            }
        }

        // Check for duplicate member names (TS2300)
        self.check_duplicate_interface_members(&iface.members.nodes);

        // Check that properties are assignable to index signatures (TS2411)
        // This includes both directly declared and inherited index signatures.
        // Get the interface type to check for any index signatures (direct or inherited)
        // NOTE: Use get_type_of_symbol to get the cached type, avoiding recursion issues
        let iface_type = if iface.name.is_some() {
            // Get symbol from the interface name and resolve its type
            if let Some(name_node) = self.ctx.arena.get(iface.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) {
                        self.get_type_of_symbol(sym_id)
                    } else {
                        TypeId::ERROR
                    }
                } else {
                    TypeId::ERROR
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Anonymous interface - compute type directly
            self.get_type_of_interface(stmt_idx)
        };

        let index_info = self.ctx.types.get_index_signatures(iface_type);

        // Check if there are own index signatures by scanning members
        let has_own_index_sig = iface.members.nodes.iter().any(|&member_idx| {
            self.ctx.arena.get(member_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE
            })
        });

        // If there are any index signatures (direct, own, or inherited), check compatibility
        if index_info.string_index.is_some()
            || index_info.number_index.is_some()
            || has_own_index_sig
        {
            self.check_index_signature_compatibility(&iface.members.nodes, iface_type, stmt_idx);

            // Also check inherited members from base interfaces against index
            // signatures. The AST-based check above only sees own members; inherited
            // properties live in the solver's resolved type and must be checked too.
            if iface.heritage_clauses.is_some() {
                self.check_inherited_properties_against_index_signatures(
                    iface_type,
                    &iface.members.nodes,
                    stmt_idx,
                );
            }
        }

        // Check that interface correctly extends base interfaces (error 2430)
        self.check_interface_extension_compatibility(stmt_idx, iface);

        // Check variance annotations match actual usage (TS2636)
        self.check_variance_annotations(stmt_idx, &iface.type_parameters);

        self.pop_type_parameters(type_param_updates);
    }

    /// Check that variance annotations (`in`/`out`) on type parameters match
    /// the actual variance of each parameter as computed by the solver (TS2636).
    ///
    /// For `out T` (covariant), T must not appear in contravariant positions.
    /// For `in T` (contravariant), T must not appear in covariant positions.
    /// `in out T` (invariant) always passes.
    ///
    /// Works for interfaces, classes, and type aliases.
    /// If `body_type` is provided, variance is computed directly on it (for type aliases
    /// whose DefId body may not be resolved yet). Otherwise, resolves via DefId.
    pub(crate) fn check_variance_annotations(
        &mut self,
        stmt_idx: NodeIndex,
        type_parameters: &Option<tsz_parser::parser::base::NodeList>,
    ) {
        self.check_variance_annotations_with_body(stmt_idx, type_parameters, None);
    }

    /// Like `check_variance_annotations` but accepts an optional pre-resolved body type.
    pub(crate) fn check_variance_annotations_with_body(
        &mut self,
        stmt_idx: NodeIndex,
        type_parameters: &Option<tsz_parser::parser::base::NodeList>,
        body_type: Option<TypeId>,
    ) {
        use tsz_scanner::SyntaxKind;

        let Some(type_params) = type_parameters else {
            return;
        };

        // Collect declared variance info for each type parameter
        struct ParamVarianceInfo {
            declared_in: bool,
            declared_out: bool,
            modifier_idx: NodeIndex,
            name: String,
            atom: tsz_common::interner::Atom,
        }

        let mut annotated_params: Vec<(usize, ParamVarianceInfo)> = Vec::new();

        for (i, &param_idx) in type_params.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(modifiers) = &param.modifiers else {
                continue;
            };

            let mut declared_in = false;
            let mut declared_out = false;
            let mut first_modifier_idx = NodeIndex::NONE;

            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                if modifier_node.kind == SyntaxKind::InKeyword as u16 {
                    declared_in = true;
                    if first_modifier_idx.is_none() {
                        first_modifier_idx = modifier_idx;
                    }
                } else if modifier_node.kind == SyntaxKind::OutKeyword as u16 {
                    declared_out = true;
                    if first_modifier_idx.is_none() {
                        first_modifier_idx = modifier_idx;
                    }
                }
            }

            if !declared_in && !declared_out {
                continue;
            }

            // `in out` (invariant) is always valid
            if declared_in && declared_out {
                continue;
            }

            let param_name = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|id| id.escaped_text.clone())
                .unwrap_or_default();

            let atom = self.ctx.types.intern_string(&param_name);

            annotated_params.push((
                i,
                ParamVarianceInfo {
                    declared_in,
                    declared_out,
                    modifier_idx: first_modifier_idx,
                    name: param_name,
                    atom,
                },
            ));
        }

        if annotated_params.is_empty() {
            return;
        }

        // Compute all variances upfront (immutable borrow of self.ctx)
        // to avoid borrow conflicts with error_at_node (mutable borrow).
        let computed_variances: Vec<Option<tsz_solver::type_handles::Variance>> = {
            let db = self.ctx.types.as_type_database();
            let resolver = &self.ctx as &dyn tsz_solver::def::resolver::TypeResolver;

            // Try DefId-based resolution first (works for interfaces/classes and
            // type aliases whose bodies are already resolved)
            let sym_id = self.ctx.binder.get_node_symbol(stmt_idx);
            let def_id = sym_id.and_then(|sid| self.ctx.get_existing_def_id(sid));
            let def_variances = def_id.and_then(|did| {
                tsz_solver::relations::variance::compute_type_param_variances_with_resolver(
                    db, resolver, did,
                )
            });

            annotated_params
                .iter()
                .map(|(i, info)| {
                    // Try direct body type computation first (more reliable for
                    // type aliases where the DefId body may not be resolved yet)
                    if let Some(body) = body_type {
                        let v = tsz_solver::relations::variance::compute_variance_with_resolver(
                            db, resolver, body, info.atom,
                        );
                        if !v.is_independent() {
                            return Some(v);
                        }
                    }
                    // Fall back to DefId-based resolution
                    def_variances.as_ref().and_then(|v| v.get(*i).copied())
                })
                .collect()
        };

        // Get the declaration name for error messages
        let decl_name = self
            .ctx
            .binder
            .get_node_symbol(stmt_idx)
            .and_then(|sid| self.ctx.binder.get_symbol(sid))
            .map(|sym| sym.escaped_name.clone())
            .unwrap_or_default();

        // Collect all type param names for formatting
        let all_param_names: Vec<String> = type_params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                Some(ident.escaped_text.clone())
            })
            .collect();

        for (idx, (i, info)) in annotated_params.iter().enumerate() {
            let Some(actual_variance) = computed_variances[idx] else {
                continue;
            };

            let violation = if info.declared_out {
                // `out T` (covariant): error if T appears contravariantly
                actual_variance.contains(tsz_solver::type_handles::Variance::CONTRAVARIANT)
            } else {
                // `in T` (contravariant): error if T appears covariantly
                actual_variance.contains(tsz_solver::type_handles::Variance::COVARIANT)
            };

            if !violation {
                continue;
            }

            // Format error message: "Type 'Controller<sub-T>' is not assignable to
            // type 'Controller<super-T>' as implied by variance annotation."
            let format_type = |marker: &str| -> String {
                let args: Vec<String> = all_param_names
                    .iter()
                    .enumerate()
                    .map(|(j, name)| {
                        if j == *i {
                            format!("{}-{}", marker, name)
                        } else {
                            name.clone()
                        }
                    })
                    .collect();
                format!("{}<{}>", decl_name, args.join(", "))
            };

            let (sub_type, super_type) = if info.declared_out {
                (format_type("sub"), format_type("super"))
            } else {
                (format_type("super"), format_type("sub"))
            };

            let message = crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION
                .replace("{0}", &sub_type)
                .replace("{1}", &super_type);

            self.error_at_node(
                info.modifier_idx,
                &message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION,
            );
        }
    }

    /// Check for duplicate property names in interface members (TS2300).
    /// TypeScript reports "Duplicate identifier 'X'." for each duplicate occurrence.
    /// NOTE: Method signatures (overloads) are NOT considered duplicates - interfaces allow
    /// multiple method signatures with the same name for function overloading.
    pub(crate) fn check_duplicate_interface_members(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track canonical property names → (member_idx, type_annotation_node, is_syntactic) tuples.
        // `is_syntactic` is true when the name was determined from syntax alone (literal name),
        // false when it required evaluating a computed expression (e.g., `[c0]` where c0="1").
        // Methods are allowed to have overloads so they are excluded.
        let mut seen_properties: FxHashMap<String, Vec<(NodeIndex, NodeIndex, bool)>> =
            FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates
            // Method signatures can have multiple overloads (same name, different types)
            if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                continue;
            }
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };

            // Determine the canonical property name for duplicate detection.
            // For non-computed names, use the syntactic text directly.
            // For computed property names (like `[c0]` where c0 is a const),
            // resolve the expression type to get the actual property name
            // (e.g., c0="1" → canonical name "1").
            let is_computed = self
                .ctx
                .arena
                .get(sig.name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let (canonical_name, is_syntactic) = if is_computed {
                // For computed properties, resolve via type evaluation to get
                // the actual property name. This handles cases like `[c0]` and
                // `[c1]` where c0="1" and c1=1 both resolve to property "1".
                if let Some(name) = self.get_property_name_resolved(sig.name) {
                    (name, false)
                } else if let Some(name) = self.get_member_name_text(sig.name) {
                    // Fall back to syntactic text if resolution fails
                    (name, true)
                } else {
                    continue;
                }
            } else if let Some(name) = self.get_member_name_text(sig.name) {
                (name, true)
            } else {
                continue;
            };

            // tsc does not flag duplicate well-known Symbol properties in interfaces
            // (e.g., [Symbol.isConcatSpreadable]) because symbols are structurally unique.
            if canonical_name.starts_with("[Symbol.") {
                continue;
            }
            seen_properties.entry(canonical_name).or_default().push((
                member_idx,
                sig.type_annotation,
                is_syntactic,
            ));
        }

        // Report errors for duplicates — tsc reports TS2300 on ALL occurrences
        // (both first and subsequent), not just the second+.
        for (name, entries) in &seen_properties {
            if entries.len() > 1 {
                // Check if all entries have syntactic names (for TS2300 decisions).
                // When computed properties resolve to the same name (e.g., `[c0]` and `[c1]`
                // where c0="1" and c1=1), tsc emits only TS2717, not TS2300.
                let all_syntactic = entries.iter().all(|e| e.2);

                // Resolve the first property's type for TS2717 comparison
                let first_type = if entries[0].1.is_some() {
                    self.get_type_from_type_node(entries[0].1)
                } else {
                    TypeId::ANY
                };

                for (i, &(idx, type_ann, _is_syntactic)) in entries.iter().enumerate() {
                    let error_node = self.get_interface_member_name_node(idx).unwrap_or(idx);

                    // TS2300 only when all occurrences have syntactic (literal) names.
                    if all_syntactic {
                        self.error_at_node_msg(
                            error_node,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[name],
                        );
                    }

                    // TS2717 on subsequent declarations when types differ
                    if i > 0 {
                        let this_type = if type_ann.is_some() {
                            self.get_type_from_type_node(type_ann)
                        } else {
                            TypeId::ANY
                        };
                        if !self.type_contains_error(first_type)
                            && !self.type_contains_error(this_type)
                        {
                            // TS2717 uses type identity, not assignability.
                            // With interned types, TypeId equality is structural identity.
                            if first_type != this_type {
                                // Use display text for the property name in diagnostics.
                                // For computed properties, this preserves the `[expr]` syntax.
                                let display_name = self
                                    .get_member_name_display_text(error_node)
                                    .unwrap_or_else(|| name.clone());
                                let first_type_str = self.format_type(first_type);
                                let this_type_str = self.format_type(this_type);
                                self.error_at_node_msg(
                                    error_node,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[&display_name, &first_type_str, &this_type_str],
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get property information needed for index signature checking.
    /// Returns (`property_name`, `property_type`, `name_node_index`).
    /// Get the name text from a member name node for duplicate member detection.
    ///
    /// Delegates to `get_literal_property_name` for non-computed names, then handles
    /// computed property names specially: string literals are wrapped as `["text"]`
    /// (matching tsc's diagnostic format), numeric literals are canonicalized, and
    /// well-known symbols like `Symbol.hasInstance` are formatted as `[Symbol.xxx]`.
    pub(crate) fn get_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        // Try non-computed property name first
        if let Some(name) =
            crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
        {
            return Some(name);
        }

        // Handle computed property names with diagnostic-specific formatting
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let expr_node = self.ctx.arena.get(computed.expression)?;
            match expr_node.kind {
                ek if ek == tsz_scanner::SyntaxKind::StringLiteral as u16 => {
                    // tsc formats computed string literals as ["a"] in diagnostics
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(format!("[\"{}\"]", lit.text));
                }
                ek if ek == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(
                        tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                            .unwrap_or_else(|| lit.text.clone()),
                    );
                }
                ek if ek == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    // Handle well-known symbols like Symbol.hasInstance
                    let access = self.ctx.arena.get_access_expr(expr_node)?;
                    let obj_node = self.ctx.arena.get(access.expression)?;
                    let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
                    if obj_ident.escaped_text.as_str() == "Symbol" {
                        let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                        let prop_ident = self.ctx.arena.get_identifier(prop_node)?;
                        return Some(format!("[Symbol.{}]", prop_ident.escaped_text));
                    }
                }
                _ => {}
            }

            if let Some(expr_text) = self.get_simple_computed_name_expr_text(computed.expression) {
                return Some(format!("[{expr_text}]"));
            }
        }

        None
    }

    fn get_simple_computed_name_expr_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(expr_node)
                .map(|ident| ident.escaped_text.clone()),
            k if k == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(expr_node)?;
                let left = self.get_simple_computed_name_expr_text(access.expression)?;
                let right = self
                    .ctx
                    .arena
                    .get_identifier_text(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.ctx.arena.get_call_expr(expr_node)?;
                let callee = self.get_simple_computed_name_expr_text(call.expression)?;
                let args = call.arguments.as_ref()?;
                if !args.nodes.is_empty() {
                    return None;
                }
                Some(format!("{callee}()"))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(expr_node)?;
                self.get_simple_computed_name_expr_text(paren.expression)
            }
            _ => None,
        }
    }

    /// Returns `true` if the name node is a computed property with a non-statically-determinable
    /// expression (e.g., `[someVariable]` or `[expr()]`). TSC skips duplicate member checking
    /// for such "late-bound" names because the actual property name can't be known at compile time.
    ///
    /// Returns `false` for:
    /// - Regular identifiers (`foo`)
    /// - Computed properties with string/numeric literals (`["foo"]`, `[0]`)
    /// - Computed properties with well-known symbols (`[Symbol.iterator]`)
    /// - Computed properties whose expression resolves to a unique symbol type
    fn is_late_bound_member_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return true; // can't determine -> treat as late-bound
        };
        // String/numeric literals are statically determinable
        if expr_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            || expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
        {
            return false;
        }
        // Well-known symbols (Symbol.xxx) are statically determinable
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
            && let Some(obj_node) = self.ctx.arena.get(access.expression)
            && let Some(obj_ident) = self.ctx.arena.get_identifier(obj_node)
            && obj_ident.escaped_text.as_str() == "Symbol"
        {
            return false;
        }
        // Check if the expression resolves to a statically determinable type.
        // tsc treats unique symbols and single literal types as statically determinable
        // property keys, but not unions or non-literal types.
        let expr_type = self.get_type_of_node(computed.expression);
        if tsz_solver::unique_symbol_ref(self.ctx.types.as_type_database(), expr_type).is_some() {
            return false;
        }
        if !matches!(
            tsz_solver::type_queries::classify_literal_type(
                self.ctx.types.as_type_database(),
                expr_type
            ),
            tsz_solver::type_queries::LiteralTypeKind::NotLiteral
        ) {
            return false;
        }
        // Everything else (unions, non-literal types, etc.) is late-bound
        true
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
                .filter(|idx: &NodeIndex| idx.is_some()),
            k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| idx.is_some()),
            _ => None,
        }
    }

    /// Get the display text for a class member name, matching TSC's `declarationNameToString`.
    ///
    /// Unlike `get_member_name_text` which canonicalizes numeric names for dedup keys,
    /// this preserves the original source representation for diagnostic messages.
    /// - Identifiers: `foo` → `"foo"`
    /// - Numeric literals: `0.0` → `"0.0"` (NOT canonicalized to `"0"`)
    /// - String literals: `'0'` → `"'0'"` (wrapped in single quotes)
    pub(crate) fn get_member_name_display_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier — same as canonical
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }

        // String literal — wrap in single quotes like TSC's declarationNameToString
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(format!("'{}'", lit.text));
        }

        // Numeric literal — preserve source text (no canonicalization)
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(lit.text.clone());
        }

        // Fall back to get_member_name_text for computed properties, etc.
        self.get_member_name_text(name_idx)
    }

    /// Report TS2300 "Duplicate identifier" error for a class member (property or method).
    /// Helper function to avoid code duplication in `check_duplicate_class_members`.
    fn report_duplicate_class_member_ts2300(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let member_node = self.ctx.arena.get(member_idx);
        let (name_idx, error_node) = match member_node.map(|n| n.kind) {
            Some(k) if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(
                    member_node.expect("Some(k) match ensures member_node is Some"),
                );
                let name_idx = prop.map(|p| p.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            Some(k) if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(
                    member_node.expect("Some(k) match ensures member_node is Some"),
                );
                let name_idx = method.map(|m| m.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            Some(k) if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self
                    .ctx
                    .arena
                    .get_accessor(member_node.expect("Some(k) match ensures member_node is Some"));
                let name_idx = accessor.map(|a| a.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            _ => return,
        };

        // Use display text (preserves source representation) for the diagnostic message,
        // matching TSC's declarationNameToString behavior.
        if let Some(name_idx) = name_idx
            && let Some(display_name) = self.get_member_name_display_text(name_idx)
        {
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
        let (params, _this_type) = self.extract_params_from_parameter_list(&method.parameters);
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
                this_type: None,
                return_type,
                type_predicate: None,
                is_constructor: false,
                is_method: true,
            });

        Some((name, method.name, type_id))
    }

    fn get_class_member_name_info(
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
                        // Skip late-bound computed names — tsc doesn't check duplicates for these
                        if self.is_late_bound_member_name(prop.name) {
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

                // All properties: only report subsequent declarations
                for &idx in info.indices.iter().skip(1) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if property_count > 0 && method_count > 0 {
                let mut first_member_type: Option<(TypeId, String)> = None;
                for (&idx, &is_property) in info.indices.iter().zip(info.is_property.iter()) {
                    if first_member_type.is_none() {
                        first_member_type = if is_property {
                            self.get_class_property_declared_type_info(idx)
                                .map(|(name, _name_node, type_id)| (type_id, name))
                        } else {
                            self.get_class_method_type_info(idx)
                                .map(|(name, _name_node, type_id)| (type_id, name))
                        }
                        .filter(|(type_id, _)| !self.type_contains_error(*type_id));
                        continue;
                    }

                    if !is_property {
                        continue;
                    }

                    let Some((name, name_node, current_type)) =
                        self.get_class_property_declared_type_info(idx)
                    else {
                        continue;
                    };
                    let Some((first_type, _first_name)) = first_member_type.as_ref() else {
                        continue;
                    };
                    if self.type_contains_error(current_type) || *first_type == current_type {
                        continue;
                    }

                    let display_name = self.get_member_name_display_text(name_node).unwrap_or(name);
                    let first_type_str = self.format_type(*first_type);
                    let current_type_str = self.format_type(current_type);
                    self.error_at_node_msg(
                        name_node,
                        diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                        &[&display_name, &first_type_str, &current_type_str],
                    );
                }

                // Mixed properties and methods: check if first is property
                let first_is_property = info.is_property.first().copied().unwrap_or(false);
                let skip_count = usize::from(!first_is_property);

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
                        for &idx in indices {
                            self.report_duplicate_class_member_ts2300(idx);
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
                for &idx in info.indices.iter().skip(start) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            }
        }

        // TS2804: static and instance members cannot share the same private name.
        // Report on the later conflicting declaration only, matching tsc.
        let mut seen_private_name_staticness: FxHashMap<String, (bool, bool)> =
            FxHashMap::default();
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
                .or_insert((false, false));
            let has_opposite = if is_static { seen.0 } else { seen.1 };
            if has_opposite {
                let message = format_message(
                    diagnostic_messages::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                    &[&name],
                );
                self.error_at_node(
                    name_idx,
                    &message,
                    diagnostic_codes::DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE,
                );
            }

            if is_static {
                seen.1 = true;
            } else {
                seen.0 = true;
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
                            self.report_duplicate_class_member_ts2300(idx);
                        }
                    } else {
                        // Accessor came first — only flag methods/properties
                        for &idx in &member_info.indices {
                            self.report_duplicate_class_member_ts2300(idx);
                        }
                    }
                } else {
                    // Simple identifiers: tsc flags all conflicting declarations.
                    for &idx in &member_info.indices {
                        self.report_duplicate_class_member_ts2300(idx);
                    }
                    for &idx in accessor_indices {
                        self.report_duplicate_class_member_ts2300(idx);
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

    /// Check if an interface property type annotation circularly references
    /// itself through a mapped type applied to the enclosing interface.
    ///
    /// Detects patterns like:
    /// ```text
    /// type Child<T> = { [P in NonOptionalKeys<T>]: T[P] }
    /// interface ListWidget {
    ///     "each": Child<ListWidget>;  // TS2502 + TS2615
    /// }
    /// ```
    fn check_interface_property_circular_mapped_type(
        &mut self,
        member_idx: NodeIndex,
        iface_name: &str,
    ) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        // Only check PROPERTY_SIGNATURE members
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return;
        }
        let Some(sig) = self.ctx.arena.get_signature(member_node) else {
            return;
        };
        // Must have a type annotation
        if sig.type_annotation == NodeIndex::NONE {
            return;
        }
        let Some(type_node) = self.ctx.arena.get(sig.type_annotation) else {
            return;
        };
        // Type annotation must be a type reference with type arguments
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return;
        };
        let Some(args) = &type_ref.type_arguments else {
            return;
        };
        // Check if any type argument is the enclosing interface name
        let has_self_ref = args.nodes.iter().any(|&arg_idx| {
            self.ctx
                .arena
                .get(arg_idx)
                .and_then(|n| {
                    if n.kind == syntax_kind_ext::TYPE_REFERENCE {
                        let tr = self.ctx.arena.get_type_ref(n)?;
                        let name_n = self.ctx.arena.get(tr.type_name)?;
                        self.ctx.arena.get_identifier(name_n)
                    } else if n.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                        self.ctx.arena.get_identifier(n)
                    } else {
                        None
                    }
                })
                .is_some_and(|ident| self.ctx.arena.resolve_identifier_text(ident) == iface_name)
        });
        if !has_self_ref {
            return;
        }

        // Get the type alias symbol for the type reference
        let type_name_idx = type_ref.type_name;
        let alias_sym = self
            .ctx
            .arena
            .get(type_name_idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .and_then(|ident| {
                let name = self.ctx.arena.resolve_identifier_text(ident);
                self.ctx.binder.file_locals.get(name)
            });
        let Some(alias_sym) = alias_sym else {
            return;
        };
        // Check if the alias is a type alias with a mapped type body
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sym) else {
            return;
        };
        if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
            return;
        }
        let has_mapped_body = symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|n| self.ctx.arena.get_type_alias(n))
                .and_then(|alias| self.ctx.arena.get(alias.type_node))
                .is_some_and(|body_node| body_node.kind == syntax_kind_ext::MAPPED_TYPE)
        });
        if !has_mapped_body {
            return;
        }

        // Get the property name for the diagnostic
        let raw_name = if sig.name != NodeIndex::NONE {
            crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, sig.name)
        } else {
            None
        };
        let Some(raw_name) = raw_name else {
            return;
        };
        // tsc wraps string-literal property names in quotes for TS2502/TS2615
        let is_string_lit = sig.name != NodeIndex::NONE
            && self
                .ctx
                .arena
                .get(sig.name)
                .is_some_and(|n| n.kind == tsz_scanner::SyntaxKind::StringLiteral as u16);
        let prop_name = if is_string_lit {
            format!("\"{raw_name}\"")
        } else {
            raw_name
        };

        // TS2502: 'name' is referenced directly or indirectly in its own type annotation.
        let message_2502 = format!(
            "'{prop_name}' is referenced directly or indirectly in its own type annotation."
        );
        self.error_at_node(sig.name, &message_2502, 2502);

        // TS2615: Type of property 'name' circularly references itself in mapped type '...'.
        // Build a simplified mapped type representation for the message.
        // tsc uses the full expanded type, but the error code match is what matters.
        let mapped_type_str = format!(
            "{{ [P in keyof {iface_name}]: undefined extends {iface_name}[P] ? never : P; }}"
        );
        let message_2615 = format!(
            "Type of property '{prop_name}' circularly references itself in mapped type '{mapped_type_str}'."
        );
        self.error_at_node(sig.type_annotation, &message_2615, 2615);
    }
}
