//! Type formatting for the solver.
//! Centralizes logic for converting `TypeIds` and `TypeDatas` to human-readable strings.

use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IntrinsicKind, LiteralValue,
    MappedModifier, MappedType, ObjectShape, ParamInfo, PropertyInfo, StringIntrinsicKind,
    SymbolRef, TemplateSpan, TupleElement, TypeData, TypeId, TypeParamInfo,
};
use rustc_hash::FxHashMap;
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
        ) && let Some(def_store) = self.def_store
        {
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
                // For generic types, append type parameter names from the definition.
                if !def.type_params.is_empty() {
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
                // and Readonly<T[]> -> readonly T[]
                if app.args.len() == 1 {
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
                    return name.into();
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

    fn format_literal(&mut self, lit: &LiteralValue) -> String {
        match lit {
            LiteralValue::String(s) => {
                let raw = self.atom(*s);
                let escaped = raw
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("\"{escaped}\"")
            }
            LiteralValue::Number(n) => {
                let v = n.0;
                if v.is_infinite() {
                    if v.is_sign_positive() {
                        "Infinity".to_string()
                    } else {
                        "-Infinity".to_string()
                    }
                } else if v.is_nan() {
                    "NaN".to_string()
                } else {
                    format!("{v}")
                }
            }
            LiteralValue::BigInt(b) => format!("{}n", self.atom(*b)),
            LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }

    fn format_object(&mut self, props: &[PropertyInfo]) -> String {
        if props.is_empty() {
            return "{}".to_string();
        }
        let mut display_props: Vec<&PropertyInfo> = props.iter().collect();
        // Sort properties for display. Use declaration_order as primary key when
        // available, with tsc-compatible tiebreaking: numeric keys in numeric order,
        // then string keys in existing order (stable sort preserves Atom ID order).
        // Properties are stored sorted by Atom ID for identity/hashing, so display
        // order must be restored here.
        display_props.sort_by(|a, b| {
            // Primary: declaration_order (0 means unset, treated as equal)
            let ord = a.declaration_order.cmp(&b.declaration_order);
            if ord != std::cmp::Ordering::Equal
                && a.declaration_order > 0
                && b.declaration_order > 0
            {
                return ord;
            }
            // Tiebreak for properties with same declaration_order:
            // numeric keys get sorted numerically (tsc puts them first),
            // but string keys preserve their existing order via stable sort.
            let a_name = self.interner.resolve_atom_ref(a.name);
            let b_name = self.interner.resolve_atom_ref(b.name);
            let a_num = a_name.parse::<u64>();
            let b_num = b_name.parse::<u64>();
            match (a_num, b_num) {
                (Ok(an), Ok(bn)) => an.cmp(&bn),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                // For non-numeric keys with same decl_order, preserve existing
                // order (stable sort) — Atom ID order often matches source order
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        // tsc does not truncate object properties in error messages — it uses
        // NoTruncation for diagnostics.  Only truncate when displaying extremely
        // large objects (>= 10 props) to prevent pathological output.
        if display_props.len() >= 10 {
            let first: Vec<String> = display_props
                .iter()
                .take(8)
                .map(|p| self.format_property(p))
                .collect();
            return format!("{{ {}; ...; }}", first.join("; "));
        }
        let formatted: Vec<String> = display_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        format!("{{ {}; }}", formatted.join("; "))
    }

    fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let raw_name = self.atom(prop.name);
        let name = if needs_property_name_quotes(&raw_name) {
            format!("\"{raw_name}\"")
        } else {
            raw_name.to_string()
        };

        // Method shorthand: `name(params): return_type` instead of `name: (params) => return_type`
        if prop.is_method {
            match self.interner.lookup(prop.type_id) {
                Some(TypeData::Function(f_id)) => {
                    let shape = self.interner.function_shape(f_id);
                    let type_params = self.format_type_params(&shape.type_params);
                    let params = self.format_params(&shape.params, shape.this_type);
                    let return_str = self.format(shape.return_type);
                    return format!(
                        "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                        params = params.join(", ")
                    );
                }
                Some(TypeData::Callable(callable_id)) => {
                    let shape = self.interner.callable_shape(callable_id);
                    if let Some(sig) = shape.call_signatures.first() {
                        let type_params = self.format_type_params(&sig.type_params);
                        let params = self.format_params(&sig.params, sig.this_type);
                        let return_str = self.format(sig.return_type);
                        return format!(
                            "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                            params = params.join(", ")
                        );
                    }
                }
                _ => {}
            }
        }

        // tsc displays optional object properties WITH `| undefined`:
        // `n?: number | undefined`. If the stored type doesn't already contain
        // undefined, we append it. For function params, tsc strips `| undefined`
        // (handled in format_params).
        let type_str: String = if prop.optional {
            let formatted = self.format(prop.type_id).into_owned();
            if self.preserve_optional_property_surface_syntax {
                formatted
            } else if prop.type_id == TypeId::NEVER {
                // `never | undefined` simplifies to `undefined`; tsc displays just `undefined`
                "undefined".to_string()
            } else if !self.type_contains_undefined(prop.type_id) {
                format!("{formatted} | undefined")
            } else {
                formatted
            }
        } else {
            self.format(prop.type_id).into_owned()
        };
        format!("{readonly}{name}{optional}: {type_str}")
    }

    /// Check if a type already contains `undefined` (as a union member or is undefined itself).
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members.contains(&TypeId::UNDEFINED);
        }
        false
    }

    /// Format a type while stripping `undefined` from it.
    /// Used for optional function parameters where the `?` already implies optionality,
    /// so displaying `| undefined` is redundant.
    fn format_stripping_undefined(&mut self, type_id: TypeId) -> String {
        if type_id == TypeId::UNDEFINED {
            // Edge case: type is just `undefined` — display it as-is since
            // there's nothing else to show.
            return self.format(type_id).into_owned();
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            let filtered: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&m| m != TypeId::UNDEFINED)
                .collect();
            if filtered.len() < members.len() {
                // We stripped some undefined members
                return match filtered.len() {
                    0 => self.format(TypeId::NEVER).into_owned(),
                    1 => self.format(filtered[0]).into_owned(),
                    _ => self.format_union(&filtered),
                };
            }
        }
        self.format(type_id).into_owned()
    }

    fn format_type_params(&mut self, type_params: &[TypeParamInfo]) -> String {
        if type_params.is_empty() {
            return String::new();
        }

        let mut parts = Vec::with_capacity(type_params.len());
        for tp in type_params {
            let mut part = String::new();
            if tp.is_const {
                part.push_str("const ");
            }
            part.push_str(self.atom(tp.name).as_ref());
            if let Some(constraint) = tp.constraint {
                part.push_str(" extends ");
                part.push_str(&self.format(constraint));
            }
            if let Some(default) = tp.default {
                part.push_str(" = ");
                part.push_str(&self.format(default));
            }
            parts.push(part);
        }

        format!("<{}>", parts.join(", "))
    }

    fn format_params(&mut self, params: &[ParamInfo], this_type: Option<TypeId>) -> Vec<String> {
        let mut rendered = Vec::with_capacity(params.len() + usize::from(this_type.is_some()));

        if let Some(this_ty) = this_type {
            rendered.push(format!("this: {}", self.format(this_ty)));
        }

        for p in params {
            let name = p
                .name
                .map_or_else(|| "_".to_string(), |atom| self.atom(atom).to_string());
            let optional = if p.optional { "?" } else { "" };
            let rest = if p.rest { "..." } else { "" };
            let type_str: String = self.format(p.type_id).into_owned();
            rendered.push(format!("{rest}{name}{optional}: {type_str}"));
        }

        rendered
    }

    /// Format a signature with the given separator between params and return type.
    fn format_signature(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        this_type: Option<TypeId>,
        return_type: TypeId,
        is_construct: bool,
        is_abstract: bool,
        separator: &str,
    ) -> String {
        let prefix = if is_construct && is_abstract {
            "abstract new "
        } else if is_construct {
            "new "
        } else {
            ""
        };
        let type_params = self.format_type_params(type_params);
        let params = self.format_params(params, this_type);
        let return_str: Cow<'static, str> = if is_construct && return_type == TypeId::UNKNOWN {
            Cow::Borrowed("any")
        } else {
            self.format(return_type)
        };
        format!(
            "{}{}({}){} {}",
            prefix,
            type_params,
            params.join(", "),
            separator,
            return_str
        )
    }

    fn format_object_with_index(&mut self, shape: &ObjectShape) -> String {
        let mut parts = Vec::new();

        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: string]: {}",
                self.format(idx.value_type)
            ));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: number]: {}",
                self.format(idx.value_type)
            ));
        }
        // Sort properties by declaration_order for display (preserves source order)
        let mut display_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        let has_decl_order = display_props.iter().any(|p| p.declaration_order > 0);
        if has_decl_order {
            display_props.sort_by_key(|p| p.declaration_order);
        }
        for prop in display_props {
            parts.push(self.format_property(prop));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        format!("{{ {}; }}", parts.join("; "))
    }

    fn format_union(&mut self, members: &[TypeId]) -> String {
        // tsc displays union members with null/undefined at the end.
        // Reorder so non-nullish members come first, then null, then undefined.
        let mut ordered: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut has_null = false;
        let mut has_undefined = false;
        for &m in members {
            if m == TypeId::NULL {
                has_null = true;
            } else if m == TypeId::UNDEFINED {
                has_undefined = true;
            } else {
                ordered.push(m);
            }
        }
        if has_null {
            ordered.push(TypeId::NULL);
        }
        if has_undefined {
            ordered.push(TypeId::UNDEFINED);
        }

        if !self.skip_union_optionalize
            && let Some(normalized) = self.optionalize_object_union_members_for_display(&ordered)
        {
            ordered = normalized;
        }

        if let Some(collapsed) = self.collapse_same_enum_members_for_display(&ordered) {
            return collapsed;
        }

        if ordered.len() > self.max_union_members {
            let first: Vec<String> = ordered
                .iter()
                .take(self.max_union_members)
                .map(|&m| self.format_union_member(m))
                .collect();
            return format!("{} | ...", first.join(" | "));
        }
        let formatted: Vec<String> = ordered
            .iter()
            .map(|&m| self.format_union_member(m))
            .collect();
        formatted.join(" | ")
    }

    fn collapse_same_enum_members_for_display(&mut self, members: &[TypeId]) -> Option<String> {
        if members.len() < 2 {
            return None;
        }

        let mut rendered = Vec::with_capacity(members.len());
        let mut shared_enum_name: Option<String> = None;
        let mut saw_enum_member = false;
        let mut enum_member_count = 0usize;

        for &member in members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                rendered.push(self.format_union_member(member));
                continue;
            }

            let Some(enum_name) = self.enum_member_parent_name_for_display(member) else {
                rendered.push(self.format_union_member(member));
                continue;
            };

            saw_enum_member = true;
            enum_member_count += 1;
            match shared_enum_name.as_ref() {
                Some(existing) if existing == &enum_name => {}
                Some(_) => return None,
                None => {
                    shared_enum_name = Some(enum_name.clone());
                    rendered.push(enum_name);
                }
            }
        }

        (saw_enum_member && enum_member_count > 1).then_some(rendered.join(" | "))
    }

    fn enum_member_parent_name_for_display(&mut self, type_id: TypeId) -> Option<String> {
        let def_id = crate::type_queries::get_enum_def_id(self.interner, type_id)?;
        let def_store = self.def_store?;
        let sym_id = def_store.get(def_id)?.symbol_id?;
        let arena = self.symbol_arena?;
        let symbol = arena.get(SymbolId(sym_id))?;
        use tsz_binder::symbol_flags;
        if !symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }
        let parent = arena.get(symbol.parent)?;
        Some(parent.escaped_name.to_string())
    }

    fn optionalize_object_union_members_for_display(
        &self,
        members: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let mut object_members = Vec::new();
        let mut suffix = Vec::new();

        for &member in members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                suffix.push(member);
                continue;
            }
            let shape_id = match self.interner.lookup(member) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
                _ => return None,
            };
            let shape = self.interner.object_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            object_members.push((member, shape_id, shape.as_ref().clone()));
        }

        if object_members.len() < 2 {
            return None;
        }

        let mut all_props: Vec<PropertyInfo> = Vec::new();
        for (_, _, shape) in &object_members {
            for prop in &shape.properties {
                if !all_props.iter().any(|existing| existing.name == prop.name) {
                    all_props.push(prop.clone());
                }
            }
        }

        let mut changed = false;
        let mut normalized = Vec::with_capacity(members.len());
        for (_, _, mut shape) in object_members {
            let next_order = shape
                .properties
                .iter()
                .map(|p| p.declaration_order)
                .max()
                .unwrap_or(0)
                + 1;
            let mut append_order = next_order;

            for prop in &all_props {
                if shape
                    .properties
                    .iter()
                    .any(|existing| existing.name == prop.name)
                {
                    continue;
                }
                changed = true;
                let mut synthetic = prop.clone();
                synthetic.type_id = TypeId::UNDEFINED;
                synthetic.write_type = TypeId::UNDEFINED;
                synthetic.optional = true;
                synthetic.readonly = false;
                synthetic.is_method = false;
                synthetic.declaration_order = append_order;
                append_order += 1;
                shape.properties.push(synthetic);
            }

            normalized.push(self.interner.object(shape.properties));
        }

        if !changed {
            return None;
        }

        normalized.extend(suffix);
        Some(normalized)
    }

    /// Format a union member, parenthesizing types that need disambiguation.
    /// TSC parenthesizes intersection types `(A & B) | (C & D)`, function types
    /// `(() => string) | (() => number)`, and constructor types in union positions.
    fn format_union_member(&mut self, id: TypeId) -> String {
        if let Some(enum_name) = self.short_enum_name_for_union_display(id) {
            return enum_name;
        }

        let formatted = self.format(id);
        let needs_parens = matches!(
            self.interner.lookup(id),
            Some(TypeData::Intersection(_) | TypeData::Function(_) | TypeData::Callable(_))
        );
        if needs_parens {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn short_enum_name_for_union_display(&mut self, type_id: TypeId) -> Option<String> {
        let def_id = crate::type_queries::get_enum_def_id(self.interner, type_id)?;
        let def_store = self.def_store?;
        let sym_id = def_store.get(def_id)?.symbol_id?;
        let arena = self.symbol_arena?;
        let symbol = arena.get(SymbolId(sym_id))?;
        use tsz_binder::symbol_flags;

        if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            let parent = arena.get(symbol.parent)?;
            return Some(format!("{}.{}", parent.escaped_name, symbol.escaped_name));
        }

        if symbol.has_any_flags(symbol_flags::ENUM) {
            return Some(symbol.escaped_name.to_string());
        }

        None
    }

    fn format_intersection(&mut self, members: &[TypeId]) -> String {
        // Preserve the member order as stored in the TypeListId.
        // For intersections containing Lazy types (type parameters, type aliases),
        // normalize_intersection skips sorting and preserves source/declaration order.
        // tsc also preserves the original declaration order, so displaying members
        // in their stored order matches tsc's behavior.
        let formatted: Vec<String> = members
            .iter()
            .map(|&m| self.format_intersection_member(m))
            .collect();
        formatted.join(" & ")
    }

    /// Format an intersection member, parenthesizing union types.
    /// `(A | B) & (C | D)` is semantically different from `A | B & C | D`.
    fn format_intersection_member(&mut self, id: TypeId) -> String {
        let formatted = self.format(id);
        if matches!(self.interner.lookup(id), Some(TypeData::Union(_))) {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn format_tuple(&mut self, elements: &[TupleElement]) -> String {
        let formatted: Vec<String> = elements
            .iter()
            .map(|e| {
                let rest = if e.rest { "..." } else { "" };
                // Rest elements are never printed with `?` in tsc
                let optional = if e.optional && !e.rest { "?" } else { "" };
                let type_str: String = if e.optional && !e.rest {
                    self.format_stripping_undefined(e.type_id)
                } else {
                    self.format(e.type_id).into_owned()
                };
                if let Some(name_atom) = e.name {
                    let name = self.atom(name_atom);
                    format!("{rest}{name}{optional}: {type_str}")
                } else {
                    format!("{rest}{type_str}{optional}")
                }
            })
            .collect();
        format!("[{}]", formatted.join(", "))
    }

    fn format_function(&mut self, shape: &FunctionShape) -> String {
        self.format_signature(
            &shape.type_params,
            &shape.params,
            shape.this_type,
            shape.return_type,
            shape.is_constructor,
            false,
            " =>",
        )
    }

    fn format_callable(&mut self, shape: &CallableShape) -> String {
        if !shape.construct_signatures.is_empty()
            && let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            return format!("typeof {name}");
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();
        if !has_index && shape.properties.is_empty() {
            if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
                let sig = &shape.call_signatures[0];
                return self.format_signature(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    false,
                    false,
                    " =>",
                );
            }
            if shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty() {
                let sig = &shape.construct_signatures[0];
                return self.format_signature(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    true,
                    shape.is_abstract,
                    " =>",
                );
            }
        }

        let mut parts = Vec::new();
        for sig in &shape.call_signatures {
            parts.push(self.format_call_signature(sig, false, false));
        }
        for sig in &shape.construct_signatures {
            parts.push(self.format_call_signature(sig, true, shape.is_abstract));
        }
        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: string]: {}",
                self.format(idx.value_type)
            ));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: number]: {}",
                self.format(idx.value_type)
            ));
        }
        let mut sorted_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        // Sort by declaration_order (same logic as format_object)
        sorted_props.sort_by(|a, b| {
            let ord = a.declaration_order.cmp(&b.declaration_order);
            if ord != std::cmp::Ordering::Equal
                && a.declaration_order > 0
                && b.declaration_order > 0
            {
                return ord;
            }
            let a_name = self.interner.resolve_atom_ref(a.name);
            let b_name = self.interner.resolve_atom_ref(b.name);
            let a_num = a_name.parse::<u64>();
            let b_num = b_name.parse::<u64>();
            match (a_num, b_num) {
                (Ok(an), Ok(bn)) => an.cmp(&bn),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        for prop in sorted_props {
            parts.push(self.format_property(prop));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        format!("{{ {}; }}", parts.join("; "))
    }

    fn format_call_signature(
        &mut self,
        sig: &CallSignature,
        is_construct: bool,
        is_abstract: bool,
    ) -> String {
        self.format_signature(
            &sig.type_params,
            &sig.params,
            sig.this_type,
            sig.return_type,
            is_construct,
            is_abstract,
            ":",
        )
    }

    fn format_conditional(&mut self, cond: &ConditionalType) -> String {
        let prev = self.preserve_optional_property_surface_syntax;
        self.preserve_optional_property_surface_syntax = true;
        let extends_type = self.format(cond.extends_type).into_owned();
        self.preserve_optional_property_surface_syntax = prev;
        format!(
            "{} extends {} ? {} : {}",
            self.format(cond.check_type),
            extends_type,
            self.format(cond.true_type),
            self.format(cond.false_type)
        )
    }

    fn format_mapped(&mut self, mapped: &MappedType) -> String {
        if let Some(index_signature) = self.try_format_mapped_as_index_signature(mapped) {
            return index_signature;
        }
        let param_name = self.atom(mapped.type_param.name);
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        let optional_suffix = match mapped.optional_modifier {
            Some(MappedModifier::Add) => "?",
            Some(MappedModifier::Remove) => "-?",
            None => "",
        };
        format!(
            "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {}; }}",
            self.format(mapped.constraint),
            self.format(mapped.template)
        )
    }

    fn try_format_mapped_as_index_signature(&mut self, mapped: &MappedType) -> Option<String> {
        if mapped.name_type.is_some() || mapped.optional_modifier.is_some() {
            return None;
        }
        let key_kind = match mapped.constraint {
            TypeId::STRING => "string",
            TypeId::NUMBER => "number",
            _ => return None,
        };
        if crate::contains_type_parameter_named(
            self.interner,
            mapped.template,
            mapped.type_param.name,
        ) {
            return None;
        }
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        Some(format!(
            "{{ {readonly_prefix}[x: {key_kind}]: {}; }}",
            self.format(mapped.template)
        ))
    }

    fn format_template_literal(&mut self, spans: &[TemplateSpan]) -> String {
        let mut result = String::from("`");
        for span in spans {
            match span {
                TemplateSpan::Text(text) => {
                    let text = self.atom(*text);
                    result.push_str(text.as_ref());
                }
                TemplateSpan::Type(type_id) => {
                    result.push_str("${");
                    result.push_str(&self.format(*type_id));
                    result.push('}');
                }
            }
        }
        result.push('`');
        result
    }

    /// Resolve a `DefId` to a human-readable name via the definition store,
    /// falling back to `"<prefix>(<raw_id>)"` if unavailable.
    fn format_def_id(&mut self, def_id: crate::def::DefId, fallback_prefix: &str) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            return self.format_def_name(&def);
        }
        if let Some(name) = self.format_raw_def_id_symbol_fallback(def_id) {
            return name;
        }
        format!("{}({})", fallback_prefix, def_id.0)
    }

    /// Format a `DefId` with type parameters appended when the definition is generic.
    ///
    /// tsc displays uninstantiated generic types with their type parameter names:
    /// e.g., `B<T>` instead of just `B`. This matches that behavior for
    /// `TypeData::Lazy(DefId)` nodes that represent generic types without
    /// an `Application` wrapper.
    fn format_def_id_with_type_params(
        &mut self,
        def_id: crate::def::DefId,
        fallback_prefix: &str,
    ) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            let name = self.format_def_name(&def);
            if def.type_params.is_empty() {
                return name;
            }
            let params: Vec<String> = def
                .type_params
                .iter()
                .map(|tp| self.atom(tp.name).to_string())
                .collect();
            return format!("{}<{}>", name, params.join(", "));
        }
        if let Some(name) = self.format_raw_def_id_symbol_fallback(def_id) {
            return name;
        }
        format!("{}({})", fallback_prefix, def_id.0)
    }

    /// Some checker paths still materialize fallback `Lazy(DefId(symbol_id))` nodes
    /// without registering the `DefId` in the definition store. When that happens,
    /// use the raw id as a `SymbolId` if it resolves in the active symbol arena.
    fn format_raw_def_id_symbol_fallback(&mut self, def_id: crate::def::DefId) -> Option<String> {
        let sym_id = SymbolId(def_id.0);
        self.format_symbol_name(sym_id)
    }

    /// Try to resolve a human-readable name for an object shape via symbol or def store lookup.
    fn resolve_object_shape_name(&mut self, shape: &ObjectShape) -> Option<String> {
        if let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            // Namespace/module/enum value types are displayed as `typeof Name` by tsc.
            if let Some(arena) = self.symbol_arena
                && let Some(sym) = arena.get(sym_id)
            {
                use tsz_binder::symbol_flags;
                let is_namespace =
                    sym.has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE);
                let is_enum = sym.has_any_flags(symbol_flags::ENUM);
                let is_class = sym.has_flags(symbol_flags::CLASS);
                let is_interface = sym.has_any_flags(symbol_flags::INTERFACE);
                // When a symbol is both an interface and a namespace (declaration
                // merging), the type-space name wins — tsc displays `B`, not
                // `typeof B`.  Similarly, classes take priority over namespaces.
                if (is_namespace || is_enum) && !is_class && !is_interface {
                    return Some(format!("typeof {name}"));
                }
            }
            return Some(name);
        }
        // Fall back to def-store structural lookup for type aliases and lib interfaces.
        // User-defined interfaces preserve their symbol through merge_interface_types, so they
        // are found via path 1 above. Anonymous types (symbol=None) cannot accidentally match
        // named interfaces (symbol=Some(...)) via find_def_by_shape because PartialEq includes symbol.
        // This path handles: (a) type aliases (always symbol=None), and (b) lib interfaces
        // (built without symbol stamps, e.g. String) whose unique structural content prevents
        // false matches.
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_shape(shape)
            && let Some(def) = def_store.get(def_id)
        {
            return Some(self.format_def_name(&def));
        }
        None
    }

    fn format_symbol_name(&mut self, sym_id: SymbolId) -> Option<String> {
        let arena = self.symbol_arena?;
        let sym = arena.get(sym_id)?;
        let mut qualified_name = sym.escaped_name.to_string();
        let mut current_parent = sym.parent;

        use tsz_binder::symbol_flags;

        // Walk up the parent chain, qualifying with enum parents only.
        // tsc qualifies type names with their containing enum (e.g., `Choice.Yes`)
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`).
        // Skip file-level module symbols (synthetic names like __test1__, "file.ts", etc.)
        // as those represent file modules, not declared namespaces.
        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                let is_qualifying_parent = parent_sym.has_any_flags(symbol_flags::ENUM);
                let name = &parent_sym.escaped_name;
                let is_file_module = name.starts_with('"')
                    || name.starts_with("__")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.is_empty();
                if is_qualifying_parent && !is_file_module {
                    qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                    current_parent = parent_sym.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some(qualified_name)
    }

    /// Resolve a `SymbolRef` (from `TypeQuery` / `ModuleNamespace`) to a display name.
    /// Tries the symbol arena first, then falls back to the definition store's
    /// `find_def_by_symbol` lookup.
    fn resolve_symbol_ref_name(&mut self, sym: SymbolRef) -> Option<String> {
        if let Some(name) = self.format_symbol_name(SymbolId(sym.0)) {
            return Some(name);
        }
        // Fallback: try the definition store by symbol id
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_symbol(sym.0)
            && let Some(def) = def_store.get(def_id)
        {
            return Some(self.format_def_name(&def));
        }
        None
    }

    fn format_def_name(&mut self, def: &crate::def::DefinitionInfo) -> String {
        // Always use the short (unqualified) definition name.
        // Enum member qualification (e.g., `Choice.Yes`) is handled by
        // `format_symbol_name` through the `resolve_symbol_ref_name` path.
        // Using `format_symbol_name` here causes cross-binder SymbolId
        // collisions where the def's symbol_id maps to a namespace-qualified
        // symbol in the current binder (e.g., `A.B` instead of just `B`).
        self.atom(def.name).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;

    #[test]
    fn union_null_at_end() {
        let db = TypeInterner::new();
        // Create union: null | string  (null first in storage order)
        // union_preserve_members keeps the input order in storage
        let union_id = db.union_preserve_members(vec![TypeId::NULL, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // null should appear at end, not beginning
        assert_eq!(result, "string | null");
    }

    #[test]
    fn union_undefined_at_end() {
        let db = TypeInterner::new();
        let union_id = db.union_preserve_members(vec![TypeId::UNDEFINED, TypeId::NUMBER]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        assert_eq!(result, "number | undefined");
    }

    #[test]
    fn union_null_and_undefined_at_end() {
        let db = TypeInterner::new();
        let union_id =
            db.union_preserve_members(vec![TypeId::NULL, TypeId::UNDEFINED, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // Non-nullish first, then null, then undefined
        assert_eq!(result, "string | null | undefined");
    }

    #[test]
    fn union_no_nullish_unchanged() {
        let db = TypeInterner::new();
        let union_id = db.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // Union members are sorted by tsc's type creation order (string=8, number=9)
        assert_eq!(result, "string | number");
    }

    #[test]
    fn needs_property_name_quotes_basic() {
        // Valid identifiers: no quotes needed
        assert!(!super::needs_property_name_quotes("foo"));
        assert!(!super::needs_property_name_quotes("_private"));
        assert!(!super::needs_property_name_quotes("$jquery"));
        assert!(!super::needs_property_name_quotes("camelCase"));
        assert!(!super::needs_property_name_quotes("PascalCase"));
        assert!(!super::needs_property_name_quotes("x"));

        // Numeric: no quotes needed
        assert!(!super::needs_property_name_quotes("0"));
        assert!(!super::needs_property_name_quotes("42"));

        // Names with hyphens/spaces/etc: quotes needed
        assert!(super::needs_property_name_quotes("data-prop"));
        assert!(super::needs_property_name_quotes("aria-label"));
        assert!(super::needs_property_name_quotes("my name"));
        assert!(super::needs_property_name_quotes(""));
    }

    #[test]
    fn tuple_type_alias_preserved_in_format() {
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Create a tuple type: [number, string, boolean]
        let tuple_id = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // Register a type alias T1 = [number, string, boolean]
        let name = db.intern_string("T1");
        let info = crate::def::DefinitionInfo::type_alias(name, vec![], tuple_id);
        let _def_id = def_store.register(info);

        // Without def_store: should show structural form
        let mut fmt = TypeFormatter::new(&db);
        let without_alias = fmt.format(tuple_id);
        assert_eq!(without_alias, "[number, string, boolean]");

        // With def_store: should show alias name
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let with_alias = fmt.format(tuple_id);
        assert_eq!(with_alias, "T1");
    }

    #[test]
    fn object_type_with_hyphenated_property_quoted() {
        let db = TypeInterner::new();
        let name = db.intern_string("data-prop");
        let prop = PropertyInfo {
            name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        };
        let obj = db.object(vec![prop]);
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(obj);
        assert_eq!(result, "{ \"data-prop\": boolean; }");
    }

    #[test]
    fn mapped_type_preserves_param_name() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: db.keyof(TypeId::STRING),
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("[P in "),
            "Expected [P in ...], got: {result}"
        );
    }

    #[test]
    fn mapped_type_shows_optional_modifier() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("]?:"),
            "Expected ]?: in mapped type, got: {result}"
        );
    }

    #[test]
    fn mapped_type_shows_readonly_modifier() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: Some(MappedModifier::Add),
            optional_modifier: None,
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("readonly [x: string]: number"),
            "Expected readonly index-signature display, got: {result}"
        );
    }

    // =================================================================
    // Primitive type formatting
    // =================================================================

    #[test]
    fn format_all_primitive_type_ids() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(TypeId::NEVER), "never");
        assert_eq!(fmt.format(TypeId::UNKNOWN), "unknown");
        assert_eq!(fmt.format(TypeId::ANY), "any");
        assert_eq!(fmt.format(TypeId::VOID), "void");
        assert_eq!(fmt.format(TypeId::UNDEFINED), "undefined");
        assert_eq!(fmt.format(TypeId::NULL), "null");
        assert_eq!(fmt.format(TypeId::BOOLEAN), "boolean");
        assert_eq!(fmt.format(TypeId::NUMBER), "number");
        assert_eq!(fmt.format(TypeId::STRING), "string");
        assert_eq!(fmt.format(TypeId::BIGINT), "bigint");
        assert_eq!(fmt.format(TypeId::SYMBOL), "symbol");
        assert_eq!(fmt.format(TypeId::OBJECT), "object");
        assert_eq!(fmt.format(TypeId::FUNCTION), "Function");
        assert_eq!(fmt.format(TypeId::ERROR), "error");
    }

    // =================================================================
    // Literal formatting
    // =================================================================

    #[test]
    fn format_string_literal_with_special_chars() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let empty = db.literal_string("");
        assert_eq!(fmt.format(empty), "\"\"");

        let spaces = db.literal_string("hello world");
        assert_eq!(fmt.format(spaces), "\"hello world\"");
    }

    #[test]
    fn format_number_literals() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.literal_number(0.0)), "0");
        assert_eq!(fmt.format(db.literal_number(-1.0)), "-1");
        assert_eq!(fmt.format(db.literal_number(3.15)), "3.15");
        assert_eq!(fmt.format(db.literal_number(1e10)), "10000000000");
        assert_eq!(fmt.format(db.literal_number(f64::INFINITY)), "Infinity");
        assert_eq!(
            fmt.format(db.literal_number(f64::NEG_INFINITY)),
            "-Infinity"
        );
        assert_eq!(fmt.format(db.literal_number(f64::NAN)), "NaN");
    }

    #[test]
    fn format_boolean_literals() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(TypeId::BOOLEAN_TRUE), "true");
        assert_eq!(fmt.format(TypeId::BOOLEAN_FALSE), "false");
    }

    #[test]
    fn format_bigint_literal() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let big = db.literal_bigint("123");
        assert_eq!(fmt.format(big), "123n");
    }

    // =================================================================
    // Union formatting
    // =================================================================

    #[test]
    fn format_union_two_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let union = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let result = fmt.format(union);
        assert!(result.contains("string"));
        assert!(result.contains("number"));
        assert!(result.contains(" | "));
    }

    #[test]
    fn format_union_three_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
        let result = fmt.format(union);
        assert!(result.contains("string"));
        assert!(result.contains("number"));
        assert!(result.contains("boolean"));
        // Should have exactly 2 "|" separators
        assert_eq!(result.matches(" | ").count(), 2);
    }

    #[test]
    fn format_union_with_literal_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let s1 = db.literal_string("a");
        let s2 = db.literal_string("b");
        let union = db.union(vec![s1, s2]);
        let result = fmt.format(union);
        assert!(result.contains("\"a\""));
        assert!(result.contains("\"b\""));
        assert!(result.contains(" | "));
    }

    #[test]
    fn format_large_union_truncation() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create a union with more members than max_union_members (default: 10)
        let members: Vec<TypeId> = (0..15).map(|i| db.literal_number(i as f64)).collect();
        let union = db.union_preserve_members(members);
        let result = fmt.format(union);
        // Should truncate with "..."
        assert!(
            result.contains("..."),
            "Large union should be truncated, got: {result}"
        );
    }

    // =================================================================
    // Intersection formatting
    // =================================================================

    #[test]
    fn format_intersection_two_type_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let u = db.type_param(TypeParamInfo {
            name: db.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let inter = db.intersection2(t, u);
        let result = fmt.format(inter);
        assert!(result.contains("T"));
        assert!(result.contains("U"));
        assert!(result.contains(" & "));
    }

    // =================================================================
    // Object type formatting
    // =================================================================

    #[test]
    fn format_empty_object() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![]);
        assert_eq!(fmt.format(obj), "{}");
    }

    #[test]
    fn format_object_single_property() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo::new(
            db.intern_string("x"),
            TypeId::NUMBER,
        )]);
        assert_eq!(fmt.format(obj), "{ x: number; }");
    }

    #[test]
    fn format_object_multiple_properties() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![
            PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER),
            PropertyInfo::new(db.intern_string("y"), TypeId::STRING),
        ]);
        let result = fmt.format(obj);
        assert!(result.contains("x: number"));
        assert!(result.contains("y: string"));
    }

    #[test]
    fn format_object_readonly_property() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mut prop = PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER);
        prop.readonly = true;
        let obj = db.object(vec![prop]);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly x: number"),
            "Expected 'readonly x: number', got: {result}"
        );
    }

    #[test]
    fn format_object_many_properties_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // 10+ properties triggers truncation
        let props: Vec<PropertyInfo> = (0..12)
            .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
            .collect();
        let obj = db.object(props);
        let result = fmt.format(obj);
        assert!(
            result.contains("..."),
            "Object with >=10 properties should truncate, got: {result}"
        );
    }

    #[test]
    fn format_object_with_string_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: Some(crate::types::IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("[x: string]: number"),
            "Expected string index signature with default param name 'x', got: {result}"
        );
    }

    #[test]
    fn format_object_with_number_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("[x: number]: string"),
            "Expected number index signature with default param name 'x', got: {result}"
        );
    }

    #[test]
    fn format_object_with_readonly_number_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: true,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly [x: number]: string"),
            "Expected readonly number index signature, got: {result}"
        );
    }

    #[test]
    fn format_object_with_readonly_string_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: Some(crate::types::IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: true,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly [x: string]: number"),
            "Expected readonly string index signature, got: {result}"
        );
    }

    // =================================================================
    // Function type formatting
    // =================================================================

    #[test]
    fn format_function_no_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(result, "() => void");
    }

    #[test]
    fn format_function_two_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(db.intern_string("a")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(db.intern_string("b")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(result, "(a: string, b: number) => boolean");
    }

    #[test]
    fn format_function_rest_param() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let arr = db.array(TypeId::STRING);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("args")),
                type_id: arr,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("...args"),
            "Expected rest param, got: {result}"
        );
    }

    #[test]
    fn format_function_with_type_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(result.contains("<T>"), "Expected type param, got: {result}");
        assert!(result.contains("x: T"));
        assert!(result.contains("=> T"));
    }

    #[test]
    fn format_function_type_param_with_constraint() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("T extends string"),
            "Expected 'T extends string', got: {result}"
        );
    }

    #[test]
    fn format_function_type_param_with_default() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: Some(TypeId::STRING),
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: Some(TypeId::STRING),
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("T = string"),
            "Expected 'T = string', got: {result}"
        );
    }

    #[test]
    fn format_constructor_function() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("new "),
            "Constructor should start with 'new', got: {result}"
        );
    }

    // =================================================================
    // Array/tuple formatting
    // =================================================================

    #[test]
    fn format_array_primitive() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.array(TypeId::STRING)), "string[]");
        assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
        assert_eq!(fmt.format(db.array(TypeId::BOOLEAN)), "boolean[]");
    }

    #[test]
    fn format_array_of_function_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let arr = db.array(func);
        let result = fmt.format(arr);
        assert!(
            result.starts_with('(') && result.ends_with(")[]"),
            "Array of function should be parenthesized, got: {result}"
        );
    }

    #[test]
    fn format_tuple_empty() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![]);
        assert_eq!(fmt.format(tuple), "[]");
    }

    #[test]
    fn format_tuple_single_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert_eq!(fmt.format(tuple), "[string]");
    }

    #[test]
    fn format_tuple_two_elements() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert_eq!(fmt.format(tuple), "[string, number]");
    }

    #[test]
    fn format_tuple_named_elements() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: Some(db.intern_string("name")),
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(db.intern_string("age")),
                optional: false,
                rest: false,
            },
        ]);
        assert_eq!(fmt.format(tuple), "[name: string, age: number]");
    }

    #[test]
    fn format_tuple_optional_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: true,
                rest: false,
            },
        ]);
        let result = fmt.format(tuple);
        assert_eq!(result, "[string, number?]");
    }

    #[test]
    fn format_tuple_rest_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_arr = db.array(TypeId::STRING);
        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: str_arr,
                name: None,
                optional: false,
                rest: true,
            },
        ]);
        let result = fmt.format(tuple);
        assert_eq!(result, "[number, ...string[]]");
    }

    // =================================================================
    // Conditional type formatting
    // =================================================================

    #[test]
    fn format_conditional_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cond = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::BOOLEAN,
            false_type: TypeId::NEVER,
            is_distributive: false,
        });
        let result = fmt.format(cond);
        assert_eq!(result, "string extends number ? boolean : never");
    }

    #[test]
    fn format_conditional_type_nested() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // T extends string ? (T extends "a" ? 1 : 2) : 3
        let inner = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: db.literal_string("a"),
            true_type: db.literal_number(1.0),
            false_type: db.literal_number(2.0),
            is_distributive: false,
        });
        let outer = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: inner,
            false_type: db.literal_number(3.0),
            is_distributive: false,
        });
        let result = fmt.format(outer);
        assert!(result.contains("extends"));
        assert!(result.contains("?"));
        assert!(result.contains(":"));
    }

    // =================================================================
    // Mapped type formatting
    // =================================================================

    #[test]
    fn format_mapped_type_basic() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let result = fmt.format(mapped);
        assert_eq!(result, "{ [x: string]: number; }");
    }

    #[test]
    fn format_mapped_type_with_remove_optional() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Remove),
        });
        let result = fmt.format(mapped);
        assert!(
            result.contains("]-?:"),
            "Expected remove optional modifier '-?', got: {result}"
        );
    }

    #[test]
    fn format_mapped_type_with_remove_readonly() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: Some(MappedModifier::Remove),
            optional_modifier: None,
        });
        let result = fmt.format(mapped);
        assert!(
            result.contains("-readonly"),
            "Expected remove readonly modifier, got: {result}"
        );
    }

    #[test]
    fn format_mapped_string_index_signature_like() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(fmt.format(mapped), "{ [x: string]: number; }");
    }

    #[test]
    fn format_mapped_preserves_key_dependent_template() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let key_name = db.intern_string("P");
        let key_param = db.type_param(TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: key_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: key_param,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(fmt.format(mapped), "{ [P in string]: P; }");
    }

    // =================================================================
    // Template literal formatting
    // =================================================================

    #[test]
    fn format_template_literal_text_only() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![TemplateSpan::Text(db.intern_string("hello"))]);
        // Text-only template literals may be simplified by the interner
        // but if they survive, they should format with backticks
        let result = fmt.format(tl);
        assert!(
            result.contains("hello"),
            "Expected 'hello' in template literal, got: {result}"
        );
    }

    #[test]
    fn format_template_literal_with_type_interpolation() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![
            TemplateSpan::Text(db.intern_string("hello ")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = fmt.format(tl);
        assert!(
            result.contains("hello "),
            "Expected 'hello ' prefix, got: {result}"
        );
        assert!(
            result.contains("${string}"),
            "Expected '${{string}}' interpolation, got: {result}"
        );
    }

    #[test]
    fn format_template_literal_complex() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![
            TemplateSpan::Text(db.intern_string("key_")),
            TemplateSpan::Type(TypeId::NUMBER),
            TemplateSpan::Text(db.intern_string("_suffix")),
        ]);
        let result = fmt.format(tl);
        assert!(result.contains("key_"), "Expected 'key_', got: {result}");
        assert!(
            result.contains("${number}"),
            "Expected '${{number}}', got: {result}"
        );
        assert!(
            result.contains("_suffix"),
            "Expected '_suffix', got: {result}"
        );
    }

    // =================================================================
    // String intrinsic formatting
    // =================================================================

    #[test]
    fn format_string_intrinsic_uppercase() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let upper = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
        assert_eq!(fmt.format(upper), "Uppercase<string>");
    }

    #[test]
    fn format_string_intrinsic_lowercase() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let lower = db.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);
        assert_eq!(fmt.format(lower), "Lowercase<string>");
    }

    #[test]
    fn format_string_intrinsic_capitalize() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cap = db.string_intrinsic(StringIntrinsicKind::Capitalize, TypeId::STRING);
        assert_eq!(fmt.format(cap), "Capitalize<string>");
    }

    #[test]
    fn format_string_intrinsic_uncapitalize() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let uncap = db.string_intrinsic(StringIntrinsicKind::Uncapitalize, TypeId::STRING);
        assert_eq!(fmt.format(uncap), "Uncapitalize<string>");
    }

    // =================================================================
    // Error type formatting
    // =================================================================

    #[test]
    fn format_error_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(TypeId::ERROR), "error");
    }

    // =================================================================
    // Depth limiting (deeply nested types)
    // =================================================================

    #[test]
    fn format_deeply_nested_array_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create deeply nested arrays: string[][][][][][]...
        let mut current = TypeId::STRING;
        for _ in 0..10 {
            current = db.array(current);
        }
        let result = fmt.format(current);
        // At some depth, the formatter should produce "..." due to max_depth
        assert!(
            result.contains("..."),
            "Deeply nested type should hit depth limit and show '...', got: {result}"
        );
    }

    #[test]
    fn format_deeply_nested_union_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create nested unions: wrap in array at each level to increase depth
        let mut current = TypeId::STRING;
        for _ in 0..10 {
            let inner_union = db.union(vec![current, TypeId::NUMBER]);
            current = db.array(inner_union);
        }
        let result = fmt.format(current);
        // Should hit depth limit
        assert!(
            result.contains("..."),
            "Deeply nested type should truncate, got: {result}"
        );
    }

    // =================================================================
    // Special types
    // =================================================================

    #[test]
    fn format_type_parameter() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tp = db.type_param(TypeParamInfo {
            name: db.intern_string("MyType"),
            constraint: None,
            default: None,
            is_const: false,
        });
        assert_eq!(fmt.format(tp), "MyType");
    }

    #[test]
    fn format_keyof_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let keyof = db.keyof(TypeId::STRING);
        assert_eq!(fmt.format(keyof), "keyof string");
    }

    #[test]
    fn format_keyof_intersection_operand_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let u = db.type_param(TypeParamInfo {
            name: db.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let intersection = db.intersection2(t, u);
        let keyof = db.keyof(intersection);

        assert_eq!(fmt.format(keyof), "keyof (T & U)");
    }

    #[test]
    fn format_readonly_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let ro = db.readonly_type(TypeId::NUMBER);
        assert_eq!(fmt.format(ro), "readonly number");
    }

    #[test]
    fn format_index_access_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let idx = db.index_access(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(fmt.format(idx), "string[number]");
    }

    #[test]
    fn format_this_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let this = db.this_type();
        assert_eq!(fmt.format(this), "this");
    }

    #[test]
    fn format_infer_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("R"),
            constraint: None,
            default: None,
            is_const: false,
        });
        assert_eq!(fmt.format(infer), "infer R");
    }

    #[test]
    fn format_unique_symbol() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let sym = db.unique_symbol(crate::types::SymbolRef(999));
        assert_eq!(fmt.format(sym), "unique symbol");
    }

    #[test]
    fn format_no_infer_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let no_infer = db.no_infer(TypeId::STRING);
        assert_eq!(fmt.format(no_infer), "NoInfer<string>");
    }

    // =================================================================
    // Generic application formatting
    // =================================================================

    #[test]
    fn format_application_single_arg() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let base = db.lazy(crate::def::DefId(100));
        let app = db.application(base, vec![TypeId::NUMBER]);
        let result = fmt.format(app);
        // Without def store, base resolves to "Lazy(100)"
        assert!(
            result.contains("Lazy(100)"),
            "Expected 'Lazy(100)', got: {result}"
        );
        assert!(
            result.contains("<number>"),
            "Expected '<number>', got: {result}"
        );
    }

    #[test]
    fn format_application_two_args() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let base = db.lazy(crate::def::DefId(200));
        let app = db.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
        let result = fmt.format(app);
        assert!(
            result.contains("<string, number>"),
            "Expected '<string, number>', got: {result}"
        );
    }

    // =================================================================
    // Callable type formatting
    // =================================================================

    #[test]
    fn format_callable_single_call_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let callable = db.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let result = fmt.format(callable);
        // Single call sig with no props/index = arrow-style
        assert!(result.contains("x: number"));
        assert!(result.contains("=> string"));
    }

    #[test]
    fn format_callable_multiple_call_signatures() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let callable = db.callable(CallableShape {
            call_signatures: vec![
                CallSignature {
                    type_params: vec![],
                    params: vec![ParamInfo {
                        name: Some(db.intern_string("x")),
                        type_id: TypeId::STRING,
                        optional: false,
                        rest: false,
                    }],
                    this_type: None,
                    return_type: TypeId::NUMBER,
                    type_predicate: None,
                    is_method: false,
                },
                CallSignature {
                    type_params: vec![],
                    params: vec![ParamInfo {
                        name: Some(db.intern_string("x")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    }],
                    this_type: None,
                    return_type: TypeId::STRING,
                    type_predicate: None,
                    is_method: false,
                },
            ],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let result = fmt.format(callable);
        // Multiple signatures => object-like format with { sig1; sig2 }
        assert!(
            result.contains("{") && result.contains("}"),
            "Multiple sigs should use object format, got: {result}"
        );
    }

    // =================================================================
    // Recursive / BoundParameter formatting
    // =================================================================

    #[test]
    fn format_recursive_index() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let rec = db.recursive(0);
        assert_eq!(fmt.format(rec), "Recursive(0)");

        let rec2 = db.recursive(3);
        assert_eq!(fmt.format(rec2), "Recursive(3)");
    }

    #[test]
    fn format_bound_parameter() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let bp = db.bound_parameter(0);
        assert_eq!(fmt.format(bp), "BoundParameter(0)");

        let bp1 = db.bound_parameter(1);
        assert_eq!(fmt.format(bp1), "BoundParameter(1)");
    }

    // =================================================================
    // Property name quoting edge cases
    // =================================================================

    #[test]
    fn needs_property_name_quotes_edge_cases() {
        // Leading digit is not a valid identifier start
        assert!(super::needs_property_name_quotes("1abc"));
        // Underscore-only is valid
        assert!(!super::needs_property_name_quotes("_"));
        assert!(!super::needs_property_name_quotes("__proto__"));
        // Dollar-only
        assert!(!super::needs_property_name_quotes("$"));
        assert!(!super::needs_property_name_quotes("$0"));
        // Special characters
        assert!(super::needs_property_name_quotes("."));
        assert!(super::needs_property_name_quotes("@"));
        assert!(super::needs_property_name_quotes("#private"));
    }

    #[test]
    fn needs_property_name_quotes_bracket_wrapped() {
        // Computed symbol property names wrapped in brackets should not be quoted
        assert!(!super::needs_property_name_quotes("[Symbol.asyncIterator]"));
        assert!(!super::needs_property_name_quotes("[Symbol.iterator]"));
        assert!(!super::needs_property_name_quotes("[Symbol.hasInstance]"));
        assert!(!super::needs_property_name_quotes("[Symbol.toPrimitive]"));
        // Single bracket only (not a computed property) should still need quotes
        assert!(super::needs_property_name_quotes("["));
        assert!(super::needs_property_name_quotes("]"));
        // Bracket at start but not end (not computed property syntax)
        assert!(super::needs_property_name_quotes("[foo"));
        assert!(super::needs_property_name_quotes("foo]"));
    }

    // =================================================================
    // Method shorthand formatting
    // =================================================================

    #[test]
    fn format_object_method_shorthand() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let method_type = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let mut method_prop = PropertyInfo::new(db.intern_string("greet"), method_type);
        method_prop.is_method = true;

        let obj = db.object(vec![method_prop]);
        let result = fmt.format(obj);
        // Method shorthand: greet(x: number): string
        assert!(
            result.contains("greet(") && result.contains("): string"),
            "Expected method shorthand, got: {result}"
        );
        // Should NOT use arrow notation
        assert!(
            !result.contains("=>"),
            "Method shorthand should use ':' not '=>', got: {result}"
        );
    }

    // =================================================================
    // Const type parameter
    // =================================================================

    #[test]
    fn format_const_type_param() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: true,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: None,
                is_const: true,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("const T"),
            "Expected 'const T' in type params, got: {result}"
        );
    }

    #[test]
    fn generic_class_type_shows_type_params() {
        // When a generic class (e.g., `class B<T>`) has its instance type formatted,
        // the formatter should show `B<T>` not just `B`.
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Create an empty object type as the class instance body
        let instance_type = db.object(vec![]);

        // Register a class definition with one type parameter T
        let name = db.intern_string("B");
        let t_name = db.intern_string("T");
        let info = crate::def::DefinitionInfo {
            kind: crate::def::DefKind::Class,
            name,
            type_params: vec![TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            }],
            body: Some(instance_type),
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            span: None,
            file_id: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };
        let def_id = def_store.register(info);

        // Register the instance type -> def mapping
        def_store.register_type_to_def(instance_type, def_id);

        // Without def_store: should show structural form
        let mut fmt = TypeFormatter::new(&db);
        let without = fmt.format(instance_type);
        assert_eq!(without, "{}");

        // With def_store: should show `B<T>` with type parameter name
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let with = fmt.format(instance_type);
        assert_eq!(with, "B<T>", "Generic class should show type params");
    }

    #[test]
    fn application_lazy_shows_type_args() {
        // Application(Lazy(def_id), [string, number]) should format as `Name<string, number>`
        use crate::caches::db::QueryDatabase;
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Register a definition
        let name = db.intern_string("MyClass");
        let info = crate::def::DefinitionInfo {
            kind: crate::def::DefKind::Class,
            name,
            type_params: vec![
                TypeParamInfo {
                    name: db.intern_string("T"),
                    constraint: None,
                    default: None,
                    is_const: false,
                },
                TypeParamInfo {
                    name: db.intern_string("U"),
                    constraint: None,
                    default: None,
                    is_const: false,
                },
            ],
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            span: None,
            file_id: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };
        let def_id = def_store.register(info);

        // Create Application(Lazy(def_id), [string, number])
        let factory = db.factory();
        let lazy = factory.lazy(def_id);
        let app = factory.application(lazy, vec![TypeId::STRING, TypeId::NUMBER]);

        // With def_store: should show `MyClass<string, number>`
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let result = fmt.format(app);
        assert_eq!(
            result, "MyClass<string, number>",
            "Application should show formatted type args"
        );
    }

    #[test]
    fn lazy_raw_def_id_falls_back_to_symbol_name() {
        let db = TypeInterner::new();
        let mut symbols = tsz_binder::SymbolArena::new();
        let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "Num".to_string());
        let lazy = db.lazy(crate::def::DefId(sym_id.0));

        let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
        assert_eq!(fmt.format(lazy), "Num");
    }

    // =================================================================
    // Optional parameter/property display (no redundant `| undefined`)
    // =================================================================

    #[test]
    fn optional_param_shows_undefined() {
        // tsc displays optional params WITHOUT `| undefined` in diagnostic error messages
        // The `?` suffix already implies optionality.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a?: string) => any",
            "Optional param omits '| undefined' — ? already implies optionality"
        );
    }

    #[test]
    fn optional_param_with_union_undefined_keeps_it() {
        // When the type is internally `string | undefined`, the formatter strips
        // `undefined` for optional params since `?` already implies optionality.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: str_or_undef,
                optional: true,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a?: string | undefined) => any",
            "Optional param preserves '| undefined' — matches tsc display"
        );
    }

    #[test]
    fn optional_property_shows_undefined() {
        // tsc: `{ x?: string | undefined; }` — object properties show | undefined
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: string | undefined; }",
            "tsc shows '| undefined' for optional object properties"
        );
    }

    #[test]
    fn optional_property_never_shows_as_undefined() {
        // When the property type is `never` and it's optional, tsc displays just `undefined`
        // since `never | undefined = undefined`.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: undefined; }",
            "Optional never property displays as undefined, not 'never | undefined'"
        );
    }

    #[test]
    fn optional_property_with_union_undefined_keeps_it() {
        // When the type already has `string | undefined`, display as-is (no duplicate)
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: str_or_undef,
            write_type: str_or_undef,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: string | undefined; }",
            "Optional property with string | undefined should keep it as-is"
        );
    }

    #[test]
    fn empty_object_shape_formats_without_spurious_separator() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.object(Vec::new())), "{}");
    }

    #[test]
    fn non_optional_param_keeps_undefined_in_union() {
        // Non-optional params should still show `| undefined` if it's in the type
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: str_or_undef,
                optional: false,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a: string | undefined) => any",
            "Non-optional param should keep '| undefined' in union"
        );
    }
}
