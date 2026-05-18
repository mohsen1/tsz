//! Type formatting and globalThis helpers.
//!
//! Extracted from `core.rs` to keep module size manageable.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn error_property_not_exist_on_global_this(
        &mut self,
        name: &str,
        error_node: NodeIndex,
        base_display: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        self.error_at_node(
            error_node,
            &format_message(
                diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                &[name, base_display],
            ),
            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
        );
    }

    /// Format a type as a human-readable string for error messages and diagnostics.
    ///
    /// This is the main entry point for converting `TypeId` representations into
    /// human-readable type strings. Used throughout the type checker for error
    /// messages, quick info, and IDE features.
    ///
    /// ## Formatting Strategy:
    /// - Delegates to the solver's `TypeFormatter`
    /// - Provides symbol table for resolving symbol names
    /// - Handles all type constructs (primitives, generics, unions, etc.)
    ///
    /// ## Type Formatting Rules:
    /// - Primitives: Display as intrinsic names (string, number, etc.)
    /// - Literals: Display as literal values ("hello", 42, true)
    /// - Arrays: Display as T[] or Array<T>
    /// - Tuples: Display as [T, U, V]
    /// - Unions: Display as T | U | V (with parentheses when needed)
    /// - Intersections: Display as T & U & V (with parentheses when needed)
    /// - Functions: Display as (args) => return
    /// - Objects: Display as { prop: Type; ... }
    /// - Type Parameters: Display as T, U, V (short names)
    /// - Type References: Display as `RefName`<Args>
    ///
    /// ## Use Cases:
    /// - Error messages: "Type X is not assignable to Y"
    /// - Quick info (hover): Type information for IDE
    /// - Completion: Type hints in autocomplete
    /// - Diagnostics: All type-related error messages
    ///
    /// ## TypeScript Examples (Formatted Output):
    /// ```typescript
    /// // Primitives
    /// let x: string;           // format_type → "string"
    /// let y: number;           // format_type → "number"
    ///
    /// // Literals
    /// let a: "hello";          // format_type → "\"hello\""
    /// let b: 42;               // format_type → "42"
    ///
    /// // Composed types
    /// type Pair = [string, number];
    /// // format_type(Pair) → "[string, number]"
    ///
    /// type Union = string | number | boolean;
    /// // format_type(Union) → "string | number | boolean"
    ///
    /// // Generics
    /// type Map<K, V> = Record<K, V>;
    /// // format_type(Map<string, number>) → "Record<string, number>"
    ///
    /// // Functions
    /// type Handler = (data: string) => void;
    /// // format_type(Handler) → "(data: string) => void"
    ///
    /// // Objects
    /// type User = { name: string; age: number };
    /// // format_type(User) → "{ name: string; age: number }"
    ///
    /// // Complex
    /// type Complex = Array<{ id: number } | null>;
    /// // format_type(Complex) → "Array<{ id: number } | null>"
    /// ```
    pub fn format_type(&self, type_id: TypeId) -> String {
        // Use full formatter with DefId context for proper type name display
        let mut formatter = self.ctx.create_type_formatter();
        formatter.format(type_id).into_owned()
    }

    /// Format a type without following `display_alias` for `Object` /
    /// `ObjectWithIndex` types. Used by diagnostic paths (e.g. JS prototype
    /// `Foo.prototype.X = ...` writes) that must show the literal's
    /// structural shape regardless of any constructor-prototype symbol
    /// aliasing recorded by the type system.
    pub fn format_type_skip_object_display_alias(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_type_formatter()
            .with_skip_object_display_alias();
        formatter.format(type_id).into_owned()
    }

    /// Format a type for use in diagnostic error messages.
    /// Unlike `format_type`, this skips union optionalization (synthetic `?: undefined`)
    /// that tsc only uses in hover/quickinfo, not in error messages.
    /// Enables display properties to preserve original literal types from the
    /// freshness model (e.g., `"frizzlebizzle"` not `string`) matching tsc.
    pub fn format_type_diagnostic(&self, type_id: TypeId) -> String {
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
            && let Some(parent) = self.ctx.binder.get_symbol(symbol.parent)
        {
            return format!("{}.{}", parent.escaped_name, symbol.escaped_name);
        }
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties();
        formatter.format(type_id).into_owned()
    }

    /// Format a type-parameter constraint for TS2344 messages. tsc strips the
    /// `aliasSymbol` from the constraint type before formatting, so the
    /// canonical primitive key union (`string | number | symbol`) is rendered
    /// structurally even though `PropertyKey` is its registered alias. Other
    /// diagnostic surfaces keep the alias (notably TS2322 against
    /// `Object.groupBy<K extends PropertyKey, T>`), so this is a narrow opt-in
    /// for the constraint-not-satisfied emitter only.
    pub fn format_type_diagnostic_constraint(&self, type_id: TypeId) -> String {
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
            && let Some(parent) = self.ctx.binder.get_symbol(symbol.parent)
        {
            return format!("{}.{}", parent.escaped_name, symbol.escaped_name);
        }
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_expanded_primitive_key_union();
        formatter.format(type_id).into_owned()
    }

    fn evaluate_call_signature_for_instantiation_display(
        &mut self,
        sig: &tsz_solver::CallSignature,
    ) -> tsz_solver::CallSignature {
        let mut sig = sig.clone();
        for param in &mut sig.params {
            param.type_id = self.evaluate_type_for_instantiation_display(param.type_id);
        }
        sig.return_type = self.evaluate_type_for_instantiation_display(sig.return_type);
        sig
    }

    fn evaluate_function_shape_for_instantiation_display(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> tsz_solver::FunctionShape {
        let mut shape = shape.clone();
        for param in &mut shape.params {
            param.type_id = self.evaluate_type_for_instantiation_display(param.type_id);
        }
        shape.return_type = self.evaluate_type_for_instantiation_display(shape.return_type);
        shape
    }

    fn evaluate_type_for_instantiation_display(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        let evaluated = self.evaluate_type_with_env(type_id);
        let indexed =
            crate::query_boundaries::common::get_indexed_access_type(self.ctx.types, evaluated)
                .or_else(|| {
                    crate::query_boundaries::common::get_indexed_access_type(
                        self.ctx.types,
                        type_id,
                    )
                });
        let Some(indexed) = indexed else {
            return evaluated;
        };
        let Some(prop_atom) = crate::query_boundaries::common::string_literal_value(
            self.ctx.types,
            indexed.index_type,
        ) else {
            return evaluated;
        };
        let prop_name = self.ctx.types.resolve_atom_ref(prop_atom).to_string();
        let object_type = self
            .instantiate_application_alias_body_for_instantiation_display(indexed.object_type)
            .unwrap_or_else(|| self.evaluate_type_with_env(indexed.object_type));
        match self.resolve_property_access_with_env(object_type, &prop_name) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => {
                let evaluated = self.evaluate_type_with_env(type_id);
                self.reduce_alias_applications_for_instantiation_display(evaluated, 0)
            }
            _ => evaluated,
        }
    }

    fn reduce_alias_applications_for_instantiation_display(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> TypeId {
        if depth >= 8 {
            return type_id;
        }
        if let Some(reduced) =
            self.instantiate_application_alias_body_for_instantiation_display(type_id)
            && reduced != type_id
        {
            return self.reduce_alias_applications_for_instantiation_display(reduced, depth + 1);
        }
        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, type_id)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let template = self
                .reduce_alias_applications_for_instantiation_display(mapped.template, depth + 1);
            if template != mapped.template {
                let mut mapped = *mapped;
                mapped.template = template;
                return self.ctx.types.factory().mapped(mapped);
            }
        }
        type_id
    }

    fn instantiate_application_alias_body_for_instantiation_display(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let (base, args) =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        let args: Vec<_> = args
            .into_iter()
            .map(|arg| {
                if self.is_failed_typeof_instantiation_arg(arg) {
                    TypeId::ANY
                } else {
                    arg
                }
            })
            .collect();
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
        Some(self.evaluate_type_with_env(instantiated))
    }

    fn evaluate_callable_shape_for_instantiation_display(
        &mut self,
        shape: &tsz_solver::CallableShape,
    ) -> tsz_solver::CallableShape {
        let mut anonymous = shape.clone();
        anonymous.symbol = None;
        anonymous.properties = Vec::new();
        anonymous.call_signatures = anonymous
            .call_signatures
            .iter()
            .map(|sig| self.evaluate_call_signature_for_instantiation_display(sig))
            .collect();
        anonymous.construct_signatures = anonymous
            .construct_signatures
            .iter()
            .map(|sig| self.evaluate_call_signature_for_instantiation_display(sig))
            .collect();
        anonymous
    }

    pub(crate) fn format_type_diagnostic_for_instantiation_expression(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let named_callable = if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
            // Exclude synthetic (`__…`) and quoted-property (`"foo-bar"`) names.
            && !symbol.escaped_name.is_empty()
            && !symbol.escaped_name.starts_with('"')
            && !symbol.escaped_name.starts_with("__")
        {
            let raw = symbol.escaped_name.as_str();
            let is_class_constructor = symbol.has_flags(tsz_binder::symbol_flags::CLASS)
                && !shape.construct_signatures.is_empty();
            Some((
                raw.to_owned(),
                is_class_constructor,
                !shape.construct_signatures.is_empty(),
            ))
        } else {
            None
        };
        let mut formatter =
            tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                )
                .with_display_properties()
                .with_namespace_module_names(&self.ctx.namespace_module_names)
                .with_module_specifiers(&self.ctx.module_specifiers)
                .with_module_path_specifiers(&self.ctx.module_path_specifiers)
                .with_current_file_id(self.ctx.current_file_idx as u32)
                .with_def_store(&self.ctx.definition_store);
        let display = formatter.format(type_id).into_owned();
        let direct_application =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id).is_some();
        let has_display_alias = self.ctx.types.get_display_alias(type_id).is_some();
        let application_base = if direct_application {
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
                .map(|(base, _)| base)
        } else {
            self.ctx.types.get_display_alias(type_id).and_then(|alias| {
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
                    .map(|(base, _)| base)
            })
        };
        // Named non-application callables (lib interfaces like `ArrayConstructor`) must show their
        // symbol name in TS2635 messages; failed-instantiation display aliases set by
        // `typeof Ctor<A, B>` would otherwise steer the formatter into the structural branch.
        // Real generic callable applications still use the normal formatter so `Box<number>` keeps
        // its type arguments.
        if !direct_application
            && let Some((raw, is_class_constructor, has_construct_signatures)) = &named_callable
        {
            if *is_class_constructor {
                return format!("typeof {raw}");
            }
            if *has_construct_signatures
                && (application_base.is_some()
                    || display == *raw
                    || display.starts_with(&format!("{raw}<")))
            {
                return raw.clone();
            }
        }
        let display_is_simple_identifier = display
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
        if display.contains('<')
            || application_base.is_some()
            || has_display_alias
            || display_is_simple_identifier
        {
            if let Some(name) = display.split('<').next()
                && let Some(overloads) =
                    self.format_function_overloads_for_instantiation_expression(name)
            {
                return overloads;
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            {
                let anonymous = self.evaluate_function_shape_for_instantiation_display(&shape);
                let anonymous_type = self.ctx.types.factory().function(anonymous);
                let mut structural_formatter = tsz_solver::TypeFormatter::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                )
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                )
                .with_display_properties();
                return structural_formatter.format(anonymous_type).into_owned();
            }
            let shape_base = application_base
                .map(|base| {
                    crate::query_boundaries::common::type_query_symbol(self.ctx.types, base)
                        .and_then(|sym| {
                            self.ctx
                                .symbol_types
                                .get(&tsz_binder::SymbolId(sym.0))
                                .copied()
                        })
                        .unwrap_or(base)
                })
                .or_else(|| {
                    let name = display.split('<').next()?;
                    if name
                        .chars()
                        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
                    {
                        self.ctx
                            .binder
                            .file_locals
                            .get(name)
                            .and_then(|sym| self.ctx.symbol_types.get(&sym))
                            .copied()
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .symbols
                                    .find_all_by_name(name)
                                    .iter()
                                    .find_map(|sym| self.ctx.symbol_types.get(sym).copied())
                            })
                    } else {
                        None
                    }
                });
            if let Some(shape) = shape_base.and_then(|base| {
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, base)
            }) && !shape.call_signatures.is_empty()
                && shape.construct_signatures.is_empty()
            {
                let anonymous = self.evaluate_callable_shape_for_instantiation_display(&shape);
                let anonymous_type = self.ctx.types.factory().callable(anonymous);
                let mut structural_formatter = tsz_solver::TypeFormatter::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                )
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                )
                .with_display_properties();
                return structural_formatter.format(anonymous_type).into_owned();
            }
            if let Some(sigs) =
                crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
                && !sigs.is_empty()
            {
                let call_signatures: Vec<_> = sigs
                    .iter()
                    .map(|sig| self.evaluate_call_signature_for_instantiation_display(sig))
                    .collect();
                let anonymous_type = self
                    .ctx
                    .types
                    .factory()
                    .callable(tsz_solver::CallableShape {
                        call_signatures,
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                        symbol: None,
                        is_abstract: false,
                    });
                let mut structural_formatter = tsz_solver::TypeFormatter::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                )
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                )
                .with_display_properties();
                return structural_formatter.format(anonymous_type).into_owned();
            }
        }
        display
    }

    fn format_function_overloads_for_instantiation_expression(&self, name: &str) -> Option<String> {
        if !name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        fn clean_signature_part(text: &str) -> &str {
            text.trim()
                .trim_start_matches(',')
                .trim()
                .trim_end_matches(['>', ')', ',', ';'])
                .trim()
        }
        let source = &self.ctx.arena.source_files.first()?.text;
        let mut signatures = Vec::new();
        for sym_id in self.ctx.binder.symbols.find_all_by_name(name) {
            let Some(symbol) = self.ctx.binder.get_symbol(*sym_id) else {
                continue;
            };
            for &decl in &symbol.declarations {
                let Some(node) = self.ctx.arena.get(decl) else {
                    continue;
                };
                let Some(func) = self.ctx.arena.get_function(node) else {
                    continue;
                };
                let type_params = func
                    .type_parameters
                    .as_ref()
                    .filter(|params| !params.nodes.is_empty())
                    .map(|params| {
                        let params = params
                            .nodes
                            .iter()
                            .filter_map(|&param| self.ctx.arena.get(param))
                            .map(|node| {
                                clean_signature_part(&source[node.pos as usize..node.end as usize])
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("<{params}>")
                    })
                    .unwrap_or_default();
                let params = func
                    .parameters
                    .nodes
                    .iter()
                    .filter_map(|&param| self.ctx.arena.get(param))
                    .map(|node| clean_signature_part(&source[node.pos as usize..node.end as usize]))
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type = if func.type_annotation.is_some() {
                    self.ctx
                        .arena
                        .get(func.type_annotation)
                        .map(|node| {
                            format!(
                                ": {}",
                                clean_signature_part(&source[node.pos as usize..node.end as usize])
                            )
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                signatures.push(format!("{type_params}({params}){return_type}"));
            }
        }
        if signatures.len() > 1 {
            Some(format!("{{ {}; }}", signatures.join("; ")))
        } else {
            None
        }
    }

    /// Format a type for assignability error messages WITHOUT display properties.
    /// tsc shows widened property types in assignability messages:
    /// `{ two: number }` not `{ two: 1 }`.
    pub fn format_type_diagnostic_widened(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_type_formatter()
            .with_diagnostic_mode()
            .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks);
        formatter.format(type_id).into_owned()
    }

    /// Format a type for TS2741 messages, showing the merged object form
    /// instead of following `display_alias` to intersection types.
    pub fn format_type_diagnostic_flattened(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_intersection_display_alias();
        formatter.format(type_id).into_owned()
    }

    /// Format a type for diagnostics while suppressing function-type parameter
    /// binders in the displayed surface.
    ///
    /// This matches tsc's iterator-protocol diagnostics, which commonly print
    /// `() => T` instead of `<T>() => T`.
    pub fn format_type_diagnostic_without_function_type_params(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let type_id = self.evaluate_type_with_env(type_id);
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && !shape.type_params.is_empty()
        {
            let display_type = self
                .ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: Vec::new(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                });
            return self.format_type_diagnostic(display_type);
        }

        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            && shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.construct_signatures.is_empty()
            && shape.call_signatures.len() == 1
        {
            let sig = &shape.call_signatures[0];
            if !sig.type_params.is_empty() {
                let display_type = self
                    .ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: Vec::new(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: sig.is_method,
                    });
                return self.format_type_diagnostic(display_type);
            }
        }

        self.format_type_diagnostic(type_id)
    }

    /// Format a type for diagnostics with display properties enabled.
    /// Uses pre-widened literal types from the freshness model side table.
    pub fn format_type_diagnostic_with_display(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties();
        formatter.format(type_id).into_owned()
    }

    /// Format a pair of types for diagnostics that display two types side by side.
    /// Applies cross-type name disambiguation (namespace / `import("<specifier>")`)
    /// when the two types format to the same short name.
    pub fn format_type_pair(&self, type_a: TypeId, type_b: TypeId) -> (String, String) {
        let mut formatter = self.ctx.create_type_formatter();
        formatter.format_pair_disambiguated(type_a, type_b)
    }

    /// Format a type for TS2367 comparison overlap error messages.
    /// tsc shows unique symbols as `typeof varName` in comparison contexts
    /// (distinct from index-type errors where it shows `unique symbol`).
    pub(crate) fn format_type_for_ts2367_display(&self, type_id: TypeId) -> String {
        use crate::query_boundaries::common::unique_symbol_ref;
        if let Some(sym_ref) = unique_symbol_ref(self.ctx.types, type_id) {
            let mut formatter = self.ctx.create_type_formatter();
            if let Some(name) = formatter.resolve_unique_symbol_name(sym_ref) {
                return format!("typeof {name}");
            }
        }
        self.format_type(type_id)
    }

    /// Format a pair of types for diagnostic messages (skips union optionalization).
    /// When the two types format to the same short name, the formatter re-qualifies
    /// them — first via namespace prefix, then `import("<specifier>").Name` — so
    /// the reader can distinguish them.
    pub fn format_type_pair_diagnostic(&self, type_a: TypeId, type_b: TypeId) -> (String, String) {
        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        formatter.format_pair_disambiguated(type_a, type_b)
    }

    /// Restore boolean literal types from display properties onto an
    /// already-widened object type.
    ///
    /// tsc preserves boolean literals (`true`/`false`) in error messages while
    /// widening other literal types (`""` → `string`, `42` → `number`). When
    /// the internal object type has already been widened (properties are
    /// `boolean` not `true`), we look up the display-property side table to
    /// find the original boolean literal and rebuild the object with it.
    ///
    /// Falls back to the `evaluated_type` if no display properties exist or
    /// no boolean restoration is needed.
    pub fn restore_boolean_display_properties(
        &self,
        evaluated_type: TypeId,
        original_type: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common;

        // Try display properties on both the evaluated and original type IDs.
        let display_props = self
            .ctx
            .types
            .get_display_properties(evaluated_type)
            .or_else(|| self.ctx.types.get_display_properties(original_type));

        let display_props = match display_props {
            Some(props) => props,
            None => return evaluated_type,
        };

        // Check if the evaluated type is an object with properties we can patch.
        let shape = match common::object_shape_for_type(self.ctx.types, evaluated_type) {
            Some(shape) => shape,
            None => return evaluated_type,
        };

        // Build a map of boolean literal display properties keyed by property name.
        let mut bool_overrides = rustc_hash::FxHashMap::default();
        for dp in display_props.iter() {
            if dp.type_id == TypeId::BOOLEAN_TRUE || dp.type_id == TypeId::BOOLEAN_FALSE {
                bool_overrides.insert(dp.name, dp.type_id);
            }
        }

        if bool_overrides.is_empty() {
            return evaluated_type;
        }

        // Rebuild properties with boolean literals restored.
        let mut new_props: Vec<tsz_solver::PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| {
                if let Some(&bool_type) = bool_overrides.get(&prop.name) {
                    let mut p = prop.clone();
                    p.type_id = bool_type;
                    p
                } else {
                    prop.clone()
                }
            })
            .collect();
        new_props.sort_by_key(|p| p.name);
        self.ctx.types.factory().object(new_props)
    }
}
