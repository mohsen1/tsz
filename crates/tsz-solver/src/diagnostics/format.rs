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
    /// The `file_id` of the file currently being checked.
    current_file_id: Option<u32>,
    /// Maximum depth for nested type printing
    max_depth: u32,
    /// Maximum number of union members to display before truncating
    max_union_members: usize,
    /// Current depth
    current_depth: u32,
    atom_cache: FxHashMap<Atom, Arc<str>>,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: None,
            def_store: None,
            module_specifiers: None,
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
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
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
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

    /// Set the `file_id` of the currently-checked file.
    pub const fn with_current_file_id(mut self, file_id: u32) -> Self {
        self.current_file_id = Some(file_id);
        self
    }

    /// Format a pair of types, disambiguating with import paths when names collide.
    pub fn format_disambiguated(&mut self, type_a: TypeId, type_b: TypeId) -> (String, String) {
        (
            self.format(type_a).into_owned(),
            self.format(type_b).into_owned(),
        )
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
                if matches!(def.kind, DefKind::Enum | DefKind::Namespace) {
                    return format!("typeof {name}").into();
                }
                return name.into();
            }
        }

        self.current_depth += 1;
        let result = self.format_key(&key);
        self.current_depth -= 1;
        result
    }

    fn format_key(&mut self, key: &TypeData) -> Cow<'static, str> {
        match key {
            TypeData::Intrinsic(kind) => Cow::Borrowed(self.format_intrinsic(*kind)),
            TypeData::Literal(lit) => self.format_literal(lit).into(),
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                self.format_object(shape.properties.as_slice()).into()
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
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
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("Ref({})", sym.0)
                };
                format!("typeof {name}").into()
            }
            TypeData::KeyOf(operand) => format!("keyof {}", self.format(*operand)).into(),
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
            TypeData::Enum(def_id, _member_type) => self.format_def_id(*def_id, "Enum").into(),
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
            LiteralValue::String(s) => format!("\"{}\"", self.atom(*s)),
            LiteralValue::Number(n) => format!("{}", n.0),
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
        if prop.is_method
            && let Some(TypeData::Function(f_id)) = self.interner.lookup(prop.type_id)
        {
            let shape = self.interner.function_shape(f_id);
            let type_params = self.format_type_params(&shape.type_params);
            let params = self.format_params(&shape.params, shape.this_type);
            let return_str = self.format(shape.return_type);
            return format!(
                "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                params = params.join(", ")
            );
        }

        let type_str = self.format(prop.type_id);
        // tsc displays optional properties with `| undefined` appended to the type
        // (when exactOptionalPropertyTypes is not enabled, which is the default).
        // Our type system stores the raw type without undefined for optional properties,
        // so we add it at display time to match tsc's output.
        if prop.optional && !self.type_contains_undefined(prop.type_id) {
            format!("{readonly}{name}{optional}: {type_str} | undefined")
        } else {
            format!("{readonly}{name}{optional}: {type_str}")
        }
    }

    /// Check if a type already contains `undefined` (either is `undefined` itself
    /// or is a union that includes `undefined` as a member).
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
            let type_str = self.format(p.type_id);
            // tsc displays optional params with `| undefined` appended when the stored type
            // doesn't already include undefined (e.g., interface member function types).
            if p.optional && !self.type_contains_undefined(p.type_id) {
                rendered.push(format!("{rest}{name}{optional}: {type_str} | undefined"));
            } else {
                rendered.push(format!("{rest}{name}{optional}: {type_str}"));
            }
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
                .unwrap_or_else(|| "index".to_owned());
            parts.push(format!(
                "[{key_name}: string]: {}",
                self.format(idx.value_type)
            ));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "index".to_owned());
            parts.push(format!(
                "[{key_name}: number]: {}",
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

    /// Format a union member, parenthesizing intersection types for clarity.
    /// TSC writes `(A & B) | (C & D)` even though `&` binds tighter than `|`.
    fn format_union_member(&mut self, id: TypeId) -> String {
        let formatted = self.format(id);
        if matches!(self.interner.lookup(id), Some(TypeData::Intersection(_))) {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn format_intersection(&mut self, members: &[TypeId]) -> String {
        // Re-sort members for display to approximate tsc's source-order display.
        // Intersection members are stored sorted by TypeId for identity/canonicalization,
        // but tsc preserves declaration order. For Lazy(DefId) types, DefId allocation
        // follows declaration order, so sorting by DefId approximates source order.
        let mut display_order: Vec<TypeId> = members.to_vec();
        display_order.sort_by(|&a, &b| {
            self.intersection_display_key(a)
                .cmp(&self.intersection_display_key(b))
        });
        let formatted: Vec<String> = display_order
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

    /// Compute a display sort key for an intersection member.
    /// Lazy(DefId) types sort by DefId (approximating declaration order).
    /// Union types sort by the minimum DefId of their Lazy members.
    /// Other types sort by TypeId (preserving canonical order).
    fn intersection_display_key(&self, id: TypeId) -> (u32, u32) {
        // Use (category, sub_key) tuple:
        // - Category 0 = non-Lazy types (sort by TypeId)
        // - Category 1 = Lazy/compound types (sort by DefId)
        match self.interner.lookup(id) {
            Some(TypeData::Lazy(def_id)) => (1, def_id.0),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                // For union/intersection members, use the minimum DefId of Lazy members
                // to approximate source order (e.g., `A | B` sorts by min(DefId(A), DefId(B)))
                let members = self.interner.type_list(list_id);
                let min_def = members
                    .iter()
                    .filter_map(|&m| {
                        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(m) {
                            Some(def_id.0)
                        } else {
                            None
                        }
                    })
                    .min();
                match min_def {
                    Some(def) => (1, def),
                    None => (0, id.0),
                }
            }
            _ => (0, id.0),
        }
    }

    fn format_tuple(&mut self, elements: &[TupleElement]) -> String {
        let formatted: Vec<String> = elements
            .iter()
            .map(|e| {
                let rest = if e.rest { "..." } else { "" };
                let optional = if e.optional { "?" } else { "" };
                let type_str = self.format(e.type_id);
                if let Some(name_atom) = e.name {
                    let name = self.atom(name_atom);
                    format!("{name}{optional}: {rest}{type_str}")
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
            parts.push(format!("[index: string]: {}", self.format(idx.value_type)));
        }
        if let Some(ref idx) = shape.number_index {
            parts.push(format!("[index: number]: {}", self.format(idx.value_type)));
        }
        let mut sorted_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        sorted_props.sort_by(|a, b| {
            self.interner
                .resolve_atom_ref(a.name)
                .cmp(&self.interner.resolve_atom_ref(b.name))
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
        format!(
            "{} extends {} ? {} : {}",
            self.format(cond.check_type),
            self.format(cond.extends_type),
            self.format(cond.true_type),
            self.format(cond.false_type)
        )
    }

    fn format_mapped(&mut self, mapped: &MappedType) -> String {
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
            "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {} }}",
            self.format(mapped.constraint),
            self.format(mapped.template)
        )
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
        format!("{}({})", fallback_prefix, def_id.0)
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

        // tsc uses the shortest name for diagnostics: it only qualifies
        // enum members with their parent enum name (e.g. `E.Member`).
        // Namespace and module parents are NOT prepended — tsc prints
        // `Foo`, not `Ns.Foo`. When disambiguation is needed (same name
        // in multiple scopes), tsc resolves that through source-level
        // qualification which is not available to the type formatter.
        use tsz_binder::symbol_flags;
        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                // Only qualify with parent if it is an enum (enum members need
                // `EnumName.MemberName`). Stop walking for all other parents
                // (namespaces, modules, source files, blocks).
                if parent_sym.has_any_flags(symbol_flags::ENUM) {
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
        if let Some(sym_id) = def.symbol_id
            && let Some(qualified_name) = self.format_symbol_name(SymbolId(sym_id))
        {
            return qualified_name;
        }

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
            type_id: TypeId::BOOLEAN_TRUE,
            write_type: TypeId::BOOLEAN_TRUE,
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
        assert_eq!(result, "{ \"data-prop\": true; }");
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
            result.contains("readonly [P in"),
            "Expected 'readonly [P in', got: {result}"
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
        let inter = db.intersection(vec![t, u]);
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
            result.contains("[index: string]: number"),
            "Expected string index signature, got: {result}"
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
            result.contains("[index: number]: string"),
            "Expected number index signature, got: {result}"
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
        assert_eq!(result, "{ [K in string]: number }");
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
}
