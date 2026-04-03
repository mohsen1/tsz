//! Type formatting for the solver.
//! Centralizes logic for converting `TypeIds` and `TypeDatas` to human-readable strings.

mod compound;
#[cfg(test)]
mod tests;

use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};
use crate::types::{IntrinsicKind, StringIntrinsicKind, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

/// Returns `true` if a property name needs to be quoted in type display
/// (i.e. it is not a valid JS identifier or numeric literal).
fn needs_property_name_quotes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names wrapped in brackets (e.g. [Symbol.asyncIterator])
    // are displayed as-is without quotes, matching tsc behavior.
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Numeric property names don't need quotes
    if name.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' || first == '$' => {
            !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        }
        _ => true,
    }
}

/// Context for generating type strings.
pub struct TypeFormatter<'a> {
    interner: &'a dyn TypeDatabase,
    /// Symbol arena for looking up symbol names (optional)
    symbol_arena: Option<&'a tsz_binder::SymbolArena>,
    /// Definition store for looking up `DefId` names (optional)
    def_store: Option<&'a DefinitionStore>,
    /// Maps `file_id` -> module specifier for import-qualified type display.
    module_specifiers: Option<&'a FxHashMap<u32, String>>,
    /// Maps object `TypeId` -> module name for namespace types that were
    /// created as plain objects but should display as `typeof import("module")`.
    namespace_module_names: Option<&'a FxHashMap<TypeId, String>>,
    /// The `file_id` of the file currently being checked.
    current_file_id: Option<u32>,
    /// Maximum depth for nested type printing
    max_depth: u32,
    /// Maximum number of union members to display before truncating
    max_union_members: usize,
    /// Current depth
    current_depth: u32,
    atom_cache: FxHashMap<Atom, Arc<str>>,
    /// When true, skip adding synthetic `?: undefined` members to object unions.
    /// This should be set for error-message formatting (tsc doesn't optionalize
    /// union members in diagnostics, only in quickinfo/hover).
    skip_union_optionalize: bool,
    /// When true, preserve the declared surface syntax of optional properties
    /// instead of appending synthetic `| undefined`.
    preserve_optional_property_surface_syntax: bool,
    /// When true, use display properties (pre-widened literal types) for fresh
    /// object literals. This implements tsc's freshness model where error messages
    /// show literal types like `{ x: "hello" }` even when the type system uses
    /// widened types like `{ x: string }`.
    use_display_properties: bool,
    /// Set of Application `TypeIds` currently being formatted via `display_alias`.
    /// Prevents infinite recursion when a `display_alias` chain forms a cycle.
    display_alias_visiting: FxHashSet<TypeId>,
    /// When true, preserve `Array<T>` generic syntax instead of `T[]` shorthand.
    /// tsc preserves the declared form in type-parameter constraints.
    pub(crate) preserve_array_generic_form: bool,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: None,
            def_store: None,
            module_specifiers: None,
            namespace_module_names: None,
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
            skip_union_optionalize: false,
            preserve_optional_property_surface_syntax: false,
            use_display_properties: false,
            display_alias_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
        }
    }

    /// Create a formatter with access to symbol names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
    ) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: Some(symbol_arena),
            def_store: None,
            module_specifiers: None,
            namespace_module_names: None,
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
            skip_union_optionalize: false,
            preserve_optional_property_surface_syntax: false,
            use_display_properties: false,
            display_alias_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
        }
    }

    /// Add access to definition store for `DefId` name resolution.
    pub const fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.def_store = Some(def_store);
        self
    }

    /// Add module specifier map for import-qualified type display.
    pub const fn with_module_specifiers(
        mut self,
        module_specifiers: &'a FxHashMap<u32, String>,
    ) -> Self {
        self.module_specifiers = Some(module_specifiers);
        self
    }

    /// Add namespace module name mapping for displaying module namespace types
    /// as `typeof import("module")` instead of their object shape.
    pub const fn with_namespace_module_names(
        mut self,
        names: &'a FxHashMap<TypeId, String>,
    ) -> Self {
        self.namespace_module_names = Some(names);
        self
    }

    /// Set the `file_id` of the currently-checked file.
    pub const fn with_current_file_id(mut self, file_id: u32) -> Self {
        self.current_file_id = Some(file_id);
        self
    }

    /// Skip synthetic `?: undefined` member optionalization in union display.
    /// Should be set when formatting types for error messages (not hover/quickinfo).
    pub const fn with_diagnostic_mode(mut self) -> Self {
        self.skip_union_optionalize = true;
        self
    }

    /// Enable display properties for fresh object literal types.
    /// When enabled, the formatter uses pre-widened literal types from the
    /// freshness model side table for error messages.
    pub const fn with_display_properties(mut self) -> Self {
        self.use_display_properties = true;
        self
    }

    fn atom(&mut self, atom: Atom) -> Arc<str> {
        if let Some(value) = self.atom_cache.get(&atom) {
            return std::sync::Arc::clone(value);
        }
        let resolved = self.interner.resolve_atom_ref(atom);
        self.atom_cache
            .insert(atom, std::sync::Arc::clone(&resolved));
        resolved
    }

    /// Render a pending diagnostic to a complete diagnostic with formatted message.
    ///
    /// This is where the lazy evaluation happens - we format types to strings
    /// only when the diagnostic is actually going to be displayed.
    pub fn render(&mut self, pending: &PendingDiagnostic) -> TypeDiagnostic {
        let template = get_message_template(pending.code);
        let message = self.render_template(template, &pending.args);

        let mut diag = TypeDiagnostic {
            message,
            code: pending.code,
            severity: pending.severity,
            span: pending.span.clone(),
            related: Vec::new(),
        };

        // Render related diagnostics, falling back to the primary span.
        let fallback_span = pending
            .span
            .clone()
            .unwrap_or_else(|| SourceSpan::new("<unknown>", 0, 0));
        for related in &pending.related {
            let related_msg =
                self.render_template(get_message_template(related.code), &related.args);
            let span = related
                .span
                .clone()
                .unwrap_or_else(|| fallback_span.clone());
            diag.related.push(RelatedInformation {
                span,
                message: related_msg,
            });
        }

        diag
    }

    /// Render a message template with arguments.
    fn render_template(&mut self, template: &str, args: &[DiagnosticArg]) -> String {
        let mut result = template.to_string();

        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("{{{i}}}");
            if !template.contains(&placeholder) {
                continue;
            }
            let replacement: Cow<'_, str> = match arg {
                DiagnosticArg::Type(type_id) => self.format(*type_id),
                DiagnosticArg::Symbol(sym_id) => {
                    if let Some(name) = self.format_symbol_name(*sym_id) {
                        Cow::Owned(name)
                    } else {
                        Cow::Owned(format!("Symbol({})", sym_id.0))
                    }
                }
                DiagnosticArg::Atom(atom) => Cow::Owned(self.atom(*atom).to_string()),
                DiagnosticArg::String(s) => Cow::Owned(s.to_string()),
                DiagnosticArg::Number(n) => Cow::Owned(n.to_string()),
            };
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Format a type as a human-readable string.
    ///
    /// Returns `Cow::Borrowed` for static type names (e.g., `"never"`, `"any"`)
    /// and `Cow::Owned` for dynamically formatted types.
    pub fn format(&mut self, type_id: TypeId) -> Cow<'static, str> {
        if self.current_depth >= self.max_depth {
            return Cow::Borrowed("...");
        }

        // Handle intrinsic types
        match type_id {
            TypeId::NEVER => return Cow::Borrowed("never"),
            TypeId::UNKNOWN => return Cow::Borrowed("unknown"),
            TypeId::ANY => return Cow::Borrowed("any"),
            TypeId::VOID => return Cow::Borrowed("void"),
            TypeId::UNDEFINED => return Cow::Borrowed("undefined"),
            TypeId::NULL => return Cow::Borrowed("null"),
            TypeId::BOOLEAN => return Cow::Borrowed("boolean"),
            TypeId::NUMBER => return Cow::Borrowed("number"),
            TypeId::STRING => return Cow::Borrowed("string"),
            TypeId::BIGINT => return Cow::Borrowed("bigint"),
            TypeId::SYMBOL => return Cow::Borrowed("symbol"),
            TypeId::OBJECT => return Cow::Borrowed("object"),
            TypeId::FUNCTION => return Cow::Borrowed("Function"),
            TypeId::ERROR => return Cow::Borrowed("error"),
            _ => {}
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return format!("Type({})", type_id.0).into(),
        };

        // For composite types that might be named (interfaces, type aliases, classes),
        // check if this TypeId maps to a definition name. This handles:
        // - Type aliases: `type ExoticAnimal = CatDog | ManBearPig` displays as "ExoticAnimal"
        // - Interfaces: `interface Foo { a: string }` displays as "Foo"
        // - Cross-file scenarios where ObjectShape's symbol can't be resolved
        //
        // Restricted to composite shapes to avoid false positives where a primitive
        // or literal type coincidentally matches an alias body (e.g. `type U = 1`).
        if matches!(
            &key,
            TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Union(_)
                | TypeData::Intersection(_)
                | TypeData::Tuple(_)
                | TypeData::Callable(_)
                | TypeData::Function(_)
                | TypeData::Mapped(_)
        ) && let Some(def_store) = self.def_store
        {
            if let Some(def_id) = def_store.find_def_for_type(type_id)
                && let Some(def) = def_store.get(def_id)
            {
                let exact_name = self.format_def_name(&def);
                if exact_name.contains('<') {
                    return exact_name.into();
                }
            }
            if let Some(def_id) = def_store.find_type_alias_by_body(type_id)
                && let Some(def) = def_store.get(def_id)
            {
                return self.format_def_name(&def).into();
            }
            if let Some(def_id) = def_store.find_def_for_type(type_id)
                && let Some(def) = def_store.get(def_id)
            {
                let name = self.format_def_name(&def);
                // Enum and namespace value types are displayed as `typeof Name` by tsc.
                // Class instance types and interfaces use just the name.
                use crate::def::DefKind;
                if matches!(
                    def.kind,
                    DefKind::Enum | DefKind::Namespace | DefKind::ClassConstructor
                ) {
                    return format!("typeof {name}").into();
                }
                // For generic types, prefer the display_alias (which has the actual
                // instantiated type arguments like `A<number>`) over appending raw
                // type parameter names from the definition (like `A<T>`).
                // The display_alias is set when an Application type is evaluated,
                // and preserves the concrete type arguments from the instantiation.
                if !def.type_params.is_empty() {
                    if let Some(alias_origin) = self.interner.get_display_alias(type_id)
                        && self.display_alias_visiting.insert(alias_origin)
                    {
                        let result = self.format(alias_origin);
                        self.display_alias_visiting.remove(&alias_origin);
                        return result;
                    }
                    let params: Vec<String> = def
                        .type_params
                        .iter()
                        .map(|tp| self.atom(tp.name).to_string())
                        .collect();
                    return format!("{}<{}>", name, params.join(", ")).into();
                }
                return name.into();
            }
        }

        // Check if this type was produced by evaluating an Application (e.g.,
        // `Dictionary<string>` evaluated to `{ [index: string]: string }`).
        // If so, format the original Application type instead of the expanded form.
        // Guard against cycles: if we're already inside a display_alias Application's
        // args, skip further display_alias redirects to prevent `Wrap<Wrap<...>>`.
        if let Some(alias_origin) = self.interner.get_display_alias(type_id)
            && self.display_alias_visiting.insert(alias_origin)
        {
            let result = self.format(alias_origin);
            self.display_alias_visiting.remove(&alias_origin);
            return result;
            // Otherwise: cycle detected — fall through to format the expanded type directly
        }

        // Check if this type is a module namespace object that should display
        // as `typeof import("module")` instead of its expanded object shape.
        if matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_))
            && let Some(ns_names) = self.namespace_module_names
            && let Some(module_name) = ns_names.get(&type_id)
        {
            let display_name = module_name.strip_prefix("./").unwrap_or(module_name);
            return format!("typeof import(\"{display_name}\")").into();
        }

        self.current_depth += 1;
        let result = self.format_key(type_id, &key);
        self.current_depth -= 1;
        result
    }

    fn format_key(&mut self, type_id: TypeId, key: &TypeData) -> Cow<'static, str> {
        match key {
            TypeData::Intrinsic(kind) => Cow::Borrowed(self.format_intrinsic(*kind)),
            TypeData::Literal(lit) => self.format_literal(lit).into(),
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                // Use display properties (pre-widened literal types) when enabled.
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    return self.format_object(display_props.as_slice()).into();
                }
                self.format_object(shape.properties.as_slice()).into()
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    let mut display_shape = shape.as_ref().clone();
                    display_shape.properties = display_props.as_ref().clone();
                    return self.format_object_with_index(&display_shape).into();
                }
                self.format_object_with_index(shape.as_ref()).into()
            }
            TypeData::Union(members) => {
                let members = self.interner.type_list(*members);
                self.format_union(members.as_ref()).into()
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(*members);
                self.format_intersection(members.as_ref()).into()
            }
            TypeData::Array(elem) => {
                // tsc preserves `Array<T>` in type-parameter constraints
                if self.preserve_array_generic_form {
                    let ef = self.format(*elem);
                    return format!("Array<{ef}>").into();
                }
                let elem_formatted = self.format(*elem);
                let needs_parens = matches!(
                    self.interner.lookup(*elem),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::Function(_)
                            | TypeData::Callable(_)
                    )
                );
                if needs_parens {
                    format!("({elem_formatted})[]").into()
                } else {
                    format!("{elem_formatted}[]").into()
                }
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                self.format_tuple(elements.as_ref()).into()
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                self.format_function(shape.as_ref()).into()
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                self.format_callable(shape.as_ref()).into()
            }
            TypeData::TypeParameter(info) => Cow::Owned(self.atom(info.name).to_string()),
            TypeData::Lazy(def_id) => self.format_def_id_with_type_params(*def_id, "Lazy").into(),
            TypeData::Recursive(idx) => format!("Recursive({idx})").into(),
            TypeData::BoundParameter(idx) => format!("BoundParameter({idx})").into(),
            TypeData::Application(app) => {
                let app = self.interner.type_application(*app);
                let base_key = self.interner.lookup(app.base);

                trace!(
                    base_type_id = %app.base.0,
                    ?base_key,
                    args_count = app.args.len(),
                    "Formatting Application"
                );

                // Special handling for Application(Lazy(def_id), args)
                // Format as "TypeName<Args>" instead of "Lazy(def_id)<Args>"
                let base_str: Cow<'_, str> = if let Some(TypeData::Lazy(def_id)) = base_key {
                    let name = self.format_def_id(def_id, "Lazy");
                    trace!(
                        def_id = %def_id.0,
                        name = %name,
                        "Application base resolved from DefId"
                    );
                    Cow::Owned(name)
                } else if let Some(TypeData::TypeQuery(sym)) = base_key {
                    // For Application(TypeQuery(sym), args) — class instantiation
                    // like D<string>. Display as "D<string>" not "typeof D<string>",
                    // since typeof X<T> is not valid TS syntax and this represents
                    // the instantiated class type.
                    if let Some(name) = self.resolve_symbol_ref_name(sym) {
                        Cow::Owned(name)
                    } else {
                        Cow::Owned(format!("Ref({})", sym.0))
                    }
                } else {
                    let formatted = self.format(app.base);
                    trace!(
                        base_formatted = %formatted,
                        "Application base formatted (not Lazy)"
                    );
                    formatted
                };

                // TSC shorthand: Array<T> -> T[], ReadonlyArray<T> -> readonly T[]
                // and Readonly<T[]> -> readonly T[].
                // Skipped in constraint context (preserve_array_generic_form).
                if app.args.len() == 1 && !self.preserve_array_generic_form {
                    let single_arg = app.args[0];
                    if base_str == "Array" {
                        // Array<T> -> T[]
                        let elem_formatted = self.format(single_arg);
                        let needs_parens = matches!(
                            self.interner.lookup(single_arg),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("({elem_formatted})[]")
                        } else {
                            format!("{elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as array shorthand");
                        return result.into();
                    }
                    if base_str == "ReadonlyArray" {
                        // ReadonlyArray<T> -> readonly T[]
                        let elem_formatted = self.format(single_arg);
                        let needs_parens = matches!(
                            self.interner.lookup(single_arg),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as readonly array shorthand");
                        return result.into();
                    }
                    if base_str == "Readonly"
                        && let Some(TypeData::Array(elem)) = self.interner.lookup(single_arg)
                    {
                        // Readonly<T[]> -> readonly T[]
                        let elem_formatted = self.format(elem);
                        let needs_parens = matches!(
                            self.interner.lookup(elem),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as Readonly<T[]> shorthand");
                        return result.into();
                    }
                }

                let args: Vec<Cow<'static, str>> =
                    app.args.iter().map(|&arg| self.format(arg)).collect();
                let result = format!("{}<{}>", base_str, args.join(", "));
                trace!(result = %result, "Application formatted");
                result.into()
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                self.format_conditional(cond.as_ref()).into()
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                self.format_mapped(mapped.as_ref()).into()
            }
            TypeData::IndexAccess(obj, idx) => {
                format!("{}[{}]", self.format(*obj), self.format(*idx)).into()
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                self.format_template_literal(spans.as_ref()).into()
            }
            TypeData::TypeQuery(sym) => {
                // Check if the symbol is a namespace import (import * as X from "mod")
                // — tsc displays these as `typeof import("mod")` rather than `typeof X`.
                if let Some(arena) = self.symbol_arena
                    && let Some(symbol) = arena.get(SymbolId(sym.0))
                    && symbol.import_name.as_deref() == Some("*")
                    && let Some(ref module_specifier) = symbol.import_module
                {
                    let display_name = module_specifier
                        .strip_prefix("./")
                        .or_else(|| module_specifier.strip_prefix("../"))
                        .unwrap_or(module_specifier);
                    return format!("typeof import(\"{display_name}\")").into();
                }
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("Ref({})", sym.0)
                };
                format!("typeof {name}").into()
            }
            TypeData::KeyOf(operand) => {
                let operand_str = self.format(*operand);
                let needs_parens = matches!(
                    self.interner.lookup(*operand),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::Function(_)
                            | TypeData::Callable(_)
                            | TypeData::Conditional(_)
                    )
                );
                if needs_parens {
                    format!("keyof ({operand_str})").into()
                } else {
                    format!("keyof {operand_str}").into()
                }
            }
            TypeData::ReadonlyType(inner) => format!("readonly {}", self.format(*inner)).into(),
            TypeData::NoInfer(inner) => format!("NoInfer<{}>", self.format(*inner)).into(),
            TypeData::UniqueSymbol(_) => Cow::Borrowed("unique symbol"),
            TypeData::Infer(info) => format!("infer {}", self.atom(info.name)).into(),
            TypeData::ThisType => Cow::Borrowed("this"),
            TypeData::StringIntrinsic { kind, type_arg } => {
                let kind_name = match kind {
                    StringIntrinsicKind::Uppercase => "Uppercase",
                    StringIntrinsicKind::Lowercase => "Lowercase",
                    StringIntrinsicKind::Capitalize => "Capitalize",
                    StringIntrinsicKind::Uncapitalize => "Uncapitalize",
                };
                format!("{}<{}>", kind_name, self.format(*type_arg)).into()
            }
            TypeData::Enum(def_id, _member_type) => {
                // Enum members should be qualified with their parent enum name
                // (e.g., `Foo.A` not just `A`). Try the symbol arena first, which
                // walks the parent chain and qualifies enum members correctly.
                // Use the definition's stored symbol_id (not the raw def_id) to
                // find the correct binder symbol.
                if let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && let Some(sym_raw) = def.symbol_id
                    && let Some(name) = self.format_symbol_name(SymbolId(sym_raw))
                {
                    return name.into();
                }
                // Fallback: try raw def_id as symbol_id (legacy path)
                if let Some(name) = self.format_raw_def_id_symbol_fallback(*def_id) {
                    return Cow::Owned(name);
                }
                self.format_def_id(*def_id, "Enum").into()
            }
            TypeData::ModuleNamespace(sym) => {
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("module({})", sym.0)
                };
                format!("typeof import(\"{name}\")").into()
            }
            TypeData::Error => Cow::Borrowed("error"),
        }
    }

    const fn format_intrinsic(&self, kind: IntrinsicKind) -> &'static str {
        match kind {
            IntrinsicKind::Any => "any",
            IntrinsicKind::Unknown => "unknown",
            IntrinsicKind::Never => "never",
            IntrinsicKind::Void => "void",
            IntrinsicKind::Null => "null",
            IntrinsicKind::Undefined => "undefined",
            IntrinsicKind::Boolean => "boolean",
            IntrinsicKind::Number => "number",
            IntrinsicKind::String => "string",
            IntrinsicKind::Bigint => "bigint",
            IntrinsicKind::Symbol => "symbol",
            IntrinsicKind::Object => "object",
            IntrinsicKind::Function => "Function",
        }
    }
}
