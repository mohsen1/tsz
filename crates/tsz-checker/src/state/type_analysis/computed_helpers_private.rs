//! Private property access type checking.
//!
//! Handles type resolution and diagnostics for private identifier (`#field`)
//! property accesses on class instances and static sides.

use crate::query_boundaries::common::lazy_def_id;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn static_private_member_access_compatible(
        &mut self,
        object_type: TypeId,
        declaring_type: TypeId,
        private_name: &str,
    ) -> bool {
        if object_type == declaring_type {
            return true;
        }
        if self.types_have_same_private_brand(object_type, declaring_type) {
            return true;
        }

        match (
            self.get_class_decl_for_display_type(object_type),
            self.get_class_decl_for_display_type(declaring_type),
        ) {
            (Some((object_class, object_is_static)), Some((declaring_class, _))) => {
                // Instance types (is_static=false) must NOT access static privates,
                // even if they're from the same class. Only the constructor/static side
                // can access static private members.
                object_is_static && object_class == declaring_class
            }
            // When we can't resolve the class declaration for the object type
            // (e.g. property typed as `typeof A` or function returning the class),
            // check cached constructor identity, then assignability, then whether
            // the object type structurally has the same private member.
            (None, Some((declaring_class, _))) => {
                let cached_ctor = self
                    .ctx
                    .class_constructor_type_cache
                    .get(&declaring_class)
                    .copied();
                if let Some(ctor_type) = cached_ctor
                    && object_type == ctor_type
                {
                    return true;
                }
                if self.is_assignable_to(object_type, declaring_type) {
                    return true;
                }
                // Last resort: check if the object type has the private property
                // in its shape. Private names are lexically scoped and unique per
                // class, so if the object type has `#field`, it must be from the
                // same class declaration.
                self.ctx
                    .types
                    .property_access_type(object_type, private_name)
                    .is_success()
            }
            _ => self.is_assignable_to(object_type, declaring_type),
        }
    }

    fn private_member_declared_on_type(
        &self,
        object_type: TypeId,
        member_name: &str,
    ) -> Option<(NodeIndex, bool)> {
        let (class_idx, want_static) = self.get_class_decl_for_display_type(object_type)?;
        let mut current = class_idx;
        let mut visited = rustc_hash::FxHashSet::default();

        while visited.insert(current) {
            if let Some(is_static) =
                self.class_directly_declares_private_member(current, member_name)
                && is_static == want_static
            {
                return Some((current, is_static));
            }

            match self.get_base_class_idx(current) {
                Some(base) => current = base,
                None => break,
            }
        }

        None
    }

    fn class_decl_hint_from_expression(&mut self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(expr_idx)?;
        self.ctx.arena.get_identifier(node)?;

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        self.get_class_declaration_from_symbol(sym_id).or_else(|| {
            let type_id = self.get_type_of_symbol(sym_id);
            self.get_class_decl_for_display_type(type_id)
                .map(|(class_idx, _)| class_idx)
        })
    }

    fn private_member_name_matches(&self, candidate: &str, requested: &str) -> bool {
        candidate == requested
            || candidate.trim_start_matches('#') == requested.trim_start_matches('#')
    }

    fn class_directly_declares_private_member(
        &self,
        class_idx: NodeIndex,
        member_name: &str,
    ) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        for &member_idx in &class.members.nodes {
            let member_node = self.ctx.arena.get(member_idx)?;
            let name_idx = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.ctx.arena.get_property_decl(member_node)?.name
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.ctx.arena.get_method_decl(member_node)?.name
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    self.ctx.arena.get_accessor(member_node)?.name
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx.arena.get_accessor(member_node)?.name
                }
                _ => continue,
            };

            if self
                .get_property_name(name_idx)
                .is_some_and(|candidate| self.private_member_name_matches(&candidate, member_name))
            {
                return Some(self.class_member_is_static(member_idx));
            }
        }

        None
    }

    fn private_accessor_presence_in_class(
        &self,
        class_idx: NodeIndex,
        member_name: &str,
        is_static: bool,
    ) -> (bool, bool) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return (false, false);
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return (false, false);
        };

        let mut has_getter = false;
        let mut has_setter = false;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if self.class_member_is_static(member_idx) != is_static {
                continue;
            }

            let is_getter = member_node.kind == syntax_kind_ext::GET_ACCESSOR;
            let is_setter = member_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if !is_getter && !is_setter {
                continue;
            }

            let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                continue;
            };
            if !self
                .get_property_name(accessor.name)
                .is_some_and(|candidate| self.private_member_name_matches(&candidate, member_name))
            {
                continue;
            }

            has_getter |= is_getter;
            has_setter |= is_setter;
        }

        (has_getter, has_setter)
    }

    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &tsz_parser::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
        is_write_context: bool,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        use crate::query_boundaries::common::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        // Mark the private identifier symbol as referenced for unused-variable tracking.
        for &sym_id in &symbols {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }

        if !is_write_context && let Some(class_info) = self.ctx.enclosing_class.as_ref() {
            let is_static_context = self.find_enclosing_static_block(idx).is_some()
                || self.is_this_in_static_class_member(idx);
            let (has_getter, has_setter) = self.private_accessor_presence_in_class(
                class_info.class_idx,
                &property_name,
                is_static_context,
            );
            if has_setter && !has_getter {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                    diagnostic_codes::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                );
                return TypeId::ERROR;
            }
        }

        // NOTE: Do NOT emit TS18016 here — property access position is grammatically valid.
        // Semantic errors (TS18013) are handled below based on the object's type.

        // Evaluate for type checking but preserve original for error messages
        // This preserves nominal identity (e.g., D<string>) in error messages
        let is_original_unknown = object_type == TypeId::UNKNOWN;
        let original_object_type = object_type;
        let object_type = self.evaluate_application_type(object_type);
        let emit_unknown_on_expression = if is_original_unknown && saw_class_scope {
            self.error_is_of_type_unknown(access.expression)
        } else {
            false
        };

        // NOTE: Do NOT resolve Lazy class types to constructor type here.
        // Static private member access (e.g., `C.#method()`) is handled later at the
        // member_is_static check below, which correctly only converts to constructor
        // type when the accessed member is actually static.

        if emit_unknown_on_expression && !symbols.is_empty() {
            return TypeId::ERROR;
        }

        // Property access on `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for property access on `never` — the result is
        // simply `never`, which allows exhaustive narrowing patterns to work correctly.
        if object_type == TypeId::NEVER {
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            return TypeId::NEVER;
        }

        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // If `symbols.is_empty()`, the private identifier was not declared in any enclosing lexical class scope.
        // Therefore, this access is invalid, regardless of whether the object type actually has the property.
        if symbols.is_empty() {
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);
            let private_member_on_object = self
                .private_member_declared_on_type(original_object_type, &property_name)
                .or_else(|| {
                    self.private_member_declared_on_type(object_type_for_check, &property_name)
                })
                .or_else(|| self.private_member_declared_on_type(resolved_type, &property_name));
            let is_any_like = resolved_type == TypeId::ANY
                || resolved_type == TypeId::UNKNOWN
                || resolved_type == TypeId::ERROR;

            if is_any_like {
                if private_member_on_object.is_some() {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                        access.expression,
                    );
                    return TypeId::ERROR;
                }
                if emit_unknown_on_expression {
                    // TSC can still emit TS2339 for undeclared private names even when
                    // `unknown` diagnostics are emitted (e.g., `x.#bar` where x: unknown).
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!("Property '{property_name}' does not exist on type 'any'."),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    if resolved_type == TypeId::ERROR {
                        return TypeId::ANY;
                    }
                    return TypeId::ERROR;
                }
                // TSC special case: for any-like types, private names can't be looked up
                // dynamically. If we're outside any class body, emit TS18016. If inside a class
                // body (but the private name isn't declared there), emit TS2339.
                if !saw_class_scope {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        "Private identifiers are not allowed outside class bodies.",
                        diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                    );
                } else if self.is_js_file() && !self.ctx.should_resolve_jsdoc() {
                    // In unchecked JS files (allowJs without checkJs/@ts-check), tsc emits
                    // TS1111 for undeclared private names inside a class (via
                    // checkGrammarPrivateIdentifierExpression). TS2339 would be filtered
                    // out by the JS error filter, so we emit the grammar error instead.
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!(
                            "Private field '{property_name}' must be declared in an enclosing class.",
                        ),
                        diagnostic_codes::PRIVATE_FIELD_MUST_BE_DECLARED_IN_AN_ENCLOSING_CLASS,
                    );
                } else {
                    // For private identifiers on any-like types inside a class, tsc emits
                    // TS2339 directly (unlike regular properties which are suppressed on `any`).
                    // Private names are nominally scoped, so `any` doesn't satisfy them.
                    let type_str = if resolved_type == TypeId::ANY {
                        "any"
                    } else if resolved_type == TypeId::UNKNOWN {
                        "unknown"
                    } else {
                        "error"
                    };
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!(
                            "Property '{property_name}' does not exist on type '{type_str}'.",
                        ),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
            } else if let Some((declaring_class_idx, is_static_member)) = private_member_on_object {
                let object_class_idx = self
                    .get_class_decl_for_display_type(object_type_for_check)
                    .map(|(class_idx, _)| class_idx)
                    .or_else(|| self.class_decl_hint_from_expression(access.expression));
                if is_static_member && object_class_idx != Some(declaring_class_idx) {
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                        access.expression,
                    );
                }
            } else {
                // For concrete types, check if the property actually exists on the type.
                // If found: TS18013 (property exists but not accessible from outside its class).
                // If not found: TS2339 (property does not exist on type).
                let mut found = false;

                use crate::query_boundaries::common::PropertyAccessResult;
                match self
                    .ctx
                    .types
                    .property_access_type(resolved_type, &property_name)
                {
                    PropertyAccessResult::Success { .. } => {
                        found = true;
                    }
                    _ => {
                        if let Some(shape) =
                            crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                                self.ctx.types,
                                resolved_type,
                            )
                        {
                            let prop_atom = self.ctx.types.intern_string(&property_name);
                            for prop in &shape.properties {
                                if prop.name == prop_atom {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if found {
                    // Property exists, but we are not in the declaring scope (TS18013)
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                        access.expression,
                    );
                } else if saw_class_scope && self.is_js_file() && !self.ctx.should_resolve_jsdoc() {
                    // In unchecked JS files (allowJs without checkJs/@ts-check), tsc emits
                    // TS1111 for undeclared private names inside a class (via
                    // checkGrammarPrivateIdentifierExpression). TS2339 would be filtered
                    // out by the JS error filter, so we emit the grammar error instead.
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!(
                            "Private field '{property_name}' must be declared in an enclosing class.",
                        ),
                        diagnostic_codes::PRIVATE_FIELD_MUST_BE_DECLARED_IN_AN_ENCLOSING_CLASS,
                    );
                } else if saw_class_scope {
                    // In TS files, tsc emits TS2339 (property does not exist) when accessing
                    // an undeclared private name inside a class.
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    // TS2339: Property does not exist (outside any class context)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                }
            }
            return TypeId::ERROR;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                        access.expression,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(access.expression) {
                return TypeId::ERROR;
            }
            return TypeId::ANY;
        }

        // Resolve Lazy class references to their constructor types for STATIC private members.
        // Only for static members; instance members correctly resolve via Lazy.
        let member_is_static = self.ctx.binder.get_symbol(symbols[0]).is_some_and(|sym| {
            sym.declarations
                .iter()
                .any(|&decl_idx| self.class_member_is_static(decl_idx))
        });
        let object_type_for_check_pre_resolution = object_type_for_check;
        let object_type_for_check = if member_is_static
            && let Some(def_id) = lazy_def_id(self.ctx.types, object_type_for_check)
            && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & tsz_binder::symbol_flags::CLASS != 0
        {
            // Resolve to the constructor type. Use the class_constructor_type_cache to
            // avoid triggering further recursion if the constructor is already built.
            let class_decl = self.get_class_declaration_from_symbol(sym_id);
            if let Some(class_idx) = class_decl
                && let Some(&ctor_type) = self.ctx.class_constructor_type_cache.get(&class_idx)
            {
                ctor_type
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            object_type_for_check
        };

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        //
        // For static private members, detect when the Lazy resolution above falsely
        // promoted an instance type to a constructor type. This happens when the
        // pre-resolution type was a Lazy class reference (instance side) that got
        // resolved to the constructor type. In that case, the access should fail.
        let lazy_promoted_instance = member_is_static
            && object_type_for_check != object_type_for_check_pre_resolution
            && lazy_def_id(self.ctx.types, object_type_for_check_pre_resolution)
                .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|sym| sym.flags & tsz_binder::symbol_flags::CLASS != 0);
        let types_compatible = if member_is_static && lazy_promoted_instance {
            // The object type was a Lazy instance ref that got promoted to constructor.
            // Check if the expression actually refers to the class (e.g., `A.#field`),
            // which is the only case where this promotion is valid.

            self.resolve_identifier_symbol_without_tracking(access.expression)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|sym| sym.flags & tsz_binder::symbol_flags::CLASS != 0)
        } else if member_is_static {
            self.static_private_member_access_compatible(
                object_type_for_check,
                declaring_type,
                &property_name,
            )
        } else if self.types_have_same_private_brand(object_type_for_check, declaring_type)
            || self.is_assignable_to(object_type_for_check, declaring_type)
        {
            true
        } else {
            // Fallback for partial/intermediate instance types: during class instance
            // type building, resolve_self_referencing_constructor may return a partial
            // type from symbol_instance_types that lacks the private brand but contains
            // the declared field. When there is exactly one private symbol in scope
            // (no shadowing), the object type has no brand (partial type), and the
            // property exists on the object, treat the access as compatible. This
            // prevents false TS2339 on patterns like `this.getInstance().#field`.
            //
            // The single-symbol guard ensures this does not suppress legitimate
            // TS18014 shadowing errors when multiple private identifiers with the
            // same name exist in nested class scopes.
            symbols.len() == 1
                && self.get_private_brand(object_type_for_check).is_none()
                && matches!(
                    self.ctx
                        .types
                        .property_access_type(object_type_for_check, &property_name),
                    PropertyAccessResult::Success { .. }
                )
        };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .is_some_and(|ty| {
                        if member_is_static {
                            self.static_private_member_access_compatible(
                                object_type_for_check,
                                ty,
                                &property_name,
                            )
                        } else if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
            });
            if shadowed {
                self.report_private_identifier_shadowed(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
                return TypeId::ERROR;
            }

            // Check if the object's own class directly declares a private member with
            // the same name AND the same static-ness as the found symbol. If so, the
            // member exists but is not accessible from this class scope (TS18013),
            // not "does not exist" (TS2339). This handles cases like `B.#foo` accessed
            // from class A's constructor, where both A and B have their own `static #foo`.
            //
            // Skip this check when the declaring class of the found symbol IS the same
            // as the object's class. In that case, the incompatibility is a
            // static/instance mismatch within the same class (e.g., `x.#foo` where
            // `#foo` is static), which should be TS2339 "does not exist".
            let declaring_class_idx = self
                .private_member_declaring_type(symbols[0])
                .and_then(|dt| self.get_class_decl_for_display_type(dt))
                .map(|(ci, _)| ci);
            let object_class_info = self
                .get_class_decl_for_display_type(original_object_type)
                .or_else(|| self.get_class_decl_for_display_type(object_type_for_check));
            let same_class = declaring_class_idx.is_some()
                && object_class_info.is_some()
                && declaring_class_idx == object_class_info.map(|(ci, _)| ci);
            let object_class_directly_has_member = if same_class {
                // Same class, static/instance mismatch -> TS2339
                None
            } else {
                object_class_info.and_then(|(class_idx, _)| {
                    let is_static =
                        self.class_directly_declares_private_member(class_idx, &property_name)?;
                    // The declared member's static-ness must match the found symbol's.
                    if is_static == member_is_static {
                        Some(is_static)
                    } else {
                        None
                    }
                })
            };
            if object_class_directly_has_member.is_some() {
                self.report_private_identifier_outside_class(
                    name_idx,
                    &property_name,
                    original_object_type,
                    access.expression,
                );
            } else {
                // Use original_object_type to preserve nominal identity (e.g., D<string>)
                self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            }
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                write_type,
                from_index_signature,
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                // In write context, use the setter parameter type instead of the read type.
                if is_write_context {
                    write_type.unwrap_or(type_id)
                } else if type_id == TypeId::UNDEFINED && write_type.is_some() {
                    // TS2806: Reading from a private setter-only accessor.
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                        diagnostic_codes::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                    );
                    return TypeId::ERROR;
                } else {
                    type_id
                }
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                if let Some(shape) =
                    crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                        self.ctx.types,
                        declaring_type,
                    )
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                factory.union2(prop.type_id, TypeId::UNDEFINED)
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS18046: 'x' is of type 'unknown'.
                // Report on the expression, not the property name.
                // Without strictNullChecks, unknown is treated like any.
                if self.error_is_of_type_unknown(name_idx) {
                    TypeId::ERROR
                } else {
                    TypeId::ANY
                }
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = factory.union2(result_type, TypeId::UNDEFINED);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }
}
