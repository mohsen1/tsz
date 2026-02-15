//! Type formatting for the solver.
//! Centralizes logic for converting TypeIds and TypeDatas to human-readable strings.

use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};
use crate::types::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

/// Context for generating type strings.
pub struct TypeFormatter<'a> {
    interner: &'a dyn TypeDatabase,
    /// Symbol arena for looking up symbol names (optional)
    symbol_arena: Option<&'a tsz_binder::SymbolArena>,
    /// Definition store for looking up DefId names (optional)
    def_store: Option<&'a DefinitionStore>,
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
            max_depth: 5,
            max_union_members: 5,
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
            max_depth: 5,
            max_union_members: 5,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
        }
    }

    /// Add access to definition store for DefId name resolution (Phase 4.2.1).
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.def_store = Some(def_store);
        self
    }

    pub fn with_limits(mut self, max_depth: u32, max_union_members: usize) -> Self {
        self.max_depth = max_depth;
        self.max_union_members = max_union_members;
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
            let placeholder = format!("{{{}}}", i);
            if !template.contains(&placeholder) {
                continue;
            }
            let replacement = match arg {
                DiagnosticArg::Type(type_id) => self.format(*type_id),
                DiagnosticArg::Symbol(sym_id) => {
                    if let Some(arena) = self.symbol_arena {
                        if let Some(sym) = arena.get(*sym_id) {
                            sym.escaped_name.to_string()
                        } else {
                            format!("Symbol({})", sym_id.0)
                        }
                    } else {
                        format!("Symbol({})", sym_id.0)
                    }
                }
                DiagnosticArg::Atom(atom) => self.atom(*atom).to_string(),
                DiagnosticArg::String(s) => s.to_string(),
                DiagnosticArg::Number(n) => n.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Format a type as a human-readable string.
    pub fn format(&mut self, type_id: TypeId) -> String {
        if self.current_depth >= self.max_depth {
            return "...".to_string();
        }

        // Handle intrinsic types
        match type_id {
            TypeId::NEVER => return "never".to_string(),
            TypeId::UNKNOWN => return "unknown".to_string(),
            TypeId::ANY => return "any".to_string(),
            TypeId::VOID => return "void".to_string(),
            TypeId::UNDEFINED => return "undefined".to_string(),
            TypeId::NULL => return "null".to_string(),
            TypeId::BOOLEAN => return "boolean".to_string(),
            TypeId::NUMBER => return "number".to_string(),
            TypeId::STRING => return "string".to_string(),
            TypeId::BIGINT => return "bigint".to_string(),
            TypeId::SYMBOL => return "symbol".to_string(),
            TypeId::OBJECT => return "object".to_string(),
            TypeId::FUNCTION => return "Function".to_string(),
            TypeId::ERROR => return "error".to_string(),
            _ => {}
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return format!("Type({})", type_id.0),
        };

        self.current_depth += 1;
        let result = self.format_key(&key);
        self.current_depth -= 1;
        result
    }

    fn format_key(&mut self, key: &TypeData) -> String {
        match key {
            TypeData::Intrinsic(kind) => self.format_intrinsic(*kind),
            TypeData::Literal(lit) => self.format_literal(lit),
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);

                // First, check if this is a class instance type with a symbol
                // Class instance types have their symbol set for nominal typing
                if let Some(sym_id) = shape.symbol
                    && let Some(arena) = self.symbol_arena
                    && let Some(sym) = arena.get(sym_id)
                {
                    // Use the class name instead of expanding all properties
                    return sym.escaped_name.to_string();
                }

                // If not a class or symbol not available, try definition store
                if let Some(def_store) = self.def_store
                    && let Some(def_id) = def_store.find_def_by_shape(&shape)
                    && let Some(def) = def_store.get(def_id)
                {
                    // Use the definition name if available
                    return self.atom(def.name).to_string();
                }
                self.format_object(shape.properties.as_slice())
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);

                // First, check if this is a class instance type with a symbol
                // Class instance types have their symbol set for nominal typing
                if let Some(sym_id) = shape.symbol
                    && let Some(arena) = self.symbol_arena
                    && let Some(sym) = arena.get(sym_id)
                {
                    // Use the class name instead of expanding all properties
                    return sym.escaped_name.to_string();
                }

                // If not a class or symbol not available, try definition store
                if let Some(def_store) = self.def_store
                    && let Some(def_id) = def_store.find_def_by_shape(&shape)
                    && let Some(def) = def_store.get(def_id)
                {
                    // Use the definition name if available
                    return self.atom(def.name).to_string();
                }
                self.format_object_with_index(shape.as_ref())
            }
            TypeData::Union(members) => {
                let members = self.interner.type_list(*members);
                self.format_union(members.as_ref())
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(*members);
                self.format_intersection(members.as_ref())
            }
            TypeData::Array(elem) => format!("{}[]", self.format(*elem)),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                self.format_tuple(elements.as_ref())
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                self.format_function(shape.as_ref())
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                self.format_callable(shape.as_ref())
            }
            TypeData::TypeParameter(info) => self.atom(info.name).to_string(),
            TypeData::Lazy(def_id) => {
                // Phase 4.2.1: Try to get the type name from the definition store
                if let Some(def_store) = self.def_store {
                    if let Some(def) = def_store.get(*def_id) {
                        // Use the definition name if available
                        self.atom(def.name).to_string()
                    } else {
                        format!("Lazy({})", def_id.0)
                    }
                } else {
                    format!("Lazy({})", def_id.0)
                }
            }
            TypeData::Recursive(idx) => {
                format!("Recursive({})", idx)
            }
            TypeData::BoundParameter(idx) => {
                format!("BoundParameter({})", idx)
            }
            TypeData::Application(app) => {
                let app = self.interner.type_application(*app);
                let base_key = self.interner.lookup(app.base);

                trace!(
                    base_type_id = %app.base.0,
                    ?base_key,
                    args_count = app.args.len(),
                    "Formatting Application"
                );

                // Phase 4.2.1: Special handling for Application(Lazy(def_id), args)
                // Format as "TypeName<Args>" instead of "Lazy(def_id)<Args>"
                let base_str = if let Some(TypeData::Lazy(def_id)) = base_key {
                    if let Some(def_store) = self.def_store {
                        if let Some(def) = def_store.get(def_id) {
                            let name = self.atom(def.name).to_string();
                            trace!(
                                def_id = %def_id.0,
                                name = %name,
                                kind = ?def.kind,
                                type_params_count = def.type_params.len(),
                                "Application base resolved from DefId"
                            );
                            name
                        } else {
                            trace!(def_id = %def_id.0, "DefId not found in store");
                            format!("Lazy({})", def_id.0)
                        }
                    } else {
                        trace!(def_id = %def_id.0, "No def_store available");
                        format!("Lazy({})", def_id.0)
                    }
                } else {
                    let formatted = self.format(app.base);
                    trace!(
                        base_formatted = %formatted,
                        "Application base formatted (not Lazy)"
                    );
                    formatted
                };

                let args: Vec<String> = app.args.iter().map(|&arg| self.format(arg)).collect();
                let result = format!("{}<{}>", base_str, args.join(", "));
                trace!(result = %result, "Application formatted");
                result
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                self.format_conditional(cond.as_ref())
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                self.format_mapped(mapped.as_ref())
            }
            TypeData::IndexAccess(obj, idx) => {
                format!("{}[{}]", self.format(*obj), self.format(*idx))
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                self.format_template_literal(spans.as_ref())
            }
            TypeData::TypeQuery(sym) => {
                let name = if let Some(arena) = self.symbol_arena {
                    if let Some(symbol) = arena.get(SymbolId(sym.0)) {
                        symbol.escaped_name.to_string()
                    } else {
                        format!("Ref({})", sym.0)
                    }
                } else {
                    format!("Ref({})", sym.0)
                };
                format!("typeof {}", name)
            }
            TypeData::KeyOf(operand) => format!("keyof {}", self.format(*operand)),
            TypeData::ReadonlyType(inner) => format!("readonly {}", self.format(*inner)),
            TypeData::NoInfer(inner) => format!("NoInfer<{}>", self.format(*inner)),
            TypeData::UniqueSymbol(sym) => {
                let name = if let Some(arena) = self.symbol_arena {
                    if let Some(symbol) = arena.get(SymbolId(sym.0)) {
                        symbol.escaped_name.to_string()
                    } else {
                        format!("symbol({})", sym.0)
                    }
                } else {
                    format!("symbol({})", sym.0)
                };
                format!("unique symbol {}", name)
            }
            TypeData::Infer(info) => format!("infer {}", self.atom(info.name)),
            TypeData::ThisType => "this".to_string(),
            TypeData::StringIntrinsic { kind, type_arg } => {
                let kind_name = match kind {
                    StringIntrinsicKind::Uppercase => "Uppercase",
                    StringIntrinsicKind::Lowercase => "Lowercase",
                    StringIntrinsicKind::Capitalize => "Capitalize",
                    StringIntrinsicKind::Uncapitalize => "Uncapitalize",
                };
                format!("{}<{}>", kind_name, self.format(*type_arg))
            }
            TypeData::Enum(def_id, _member_type) => {
                // Try to get the enum name from the definition store
                if let Some(def_store) = self.def_store {
                    if let Some(def) = def_store.get(*def_id) {
                        // Use the definition name if available
                        self.atom(def.name).to_string()
                    } else {
                        format!("Enum({})", def_id.0)
                    }
                } else {
                    format!("Enum({})", def_id.0)
                }
            }
            TypeData::ModuleNamespace(sym) => {
                let name = if let Some(arena) = self.symbol_arena {
                    if let Some(symbol) = arena.get(SymbolId(sym.0)) {
                        symbol.escaped_name.to_string()
                    } else {
                        format!("module({})", sym.0)
                    }
                } else {
                    format!("module({})", sym.0)
                };
                format!("typeof import(\"{}\")", name)
            }
            TypeData::Error => "error".to_string(),
        }
    }

    fn format_intrinsic(&self, kind: IntrinsicKind) -> String {
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
        .to_string()
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
        let mut sorted_props: Vec<&PropertyInfo> = props.iter().collect();
        sorted_props.sort_by(|a, b| {
            self.interner
                .resolve_atom_ref(a.name)
                .cmp(&self.interner.resolve_atom_ref(b.name))
        });
        if props.len() > 3 {
            let first_three: Vec<String> = sorted_props
                .iter()
                .take(3)
                .map(|p| self.format_property(p))
                .collect();
            return format!("{{ {}; ...; }}", first_three.join("; "));
        }
        let formatted: Vec<String> = sorted_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        format!("{{ {}; }}", formatted.join("; "))
    }

    fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let type_str = self.format(prop.type_id);
        let name = self.atom(prop.name);
        format!("{}{}{}: {}", readonly, name, optional, type_str)
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
            rendered.push(format!("{}{}{}: {}", rest, name, optional, type_str));
        }

        rendered
    }

    fn format_signature_arrow(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        this_type: Option<TypeId>,
        return_type: TypeId,
        is_construct: bool,
    ) -> String {
        let prefix = if is_construct { "new " } else { "" };
        let type_params = self.format_type_params(type_params);
        let params = self.format_params(params, this_type);
        let return_str = if is_construct && return_type == TypeId::UNKNOWN {
            "any".to_string()
        } else {
            self.format(return_type)
        };
        format!(
            "{}{}({}) => {}",
            prefix,
            type_params,
            params.join(", "),
            return_str
        )
    }

    fn format_object_with_index(&mut self, shape: &ObjectShape) -> String {
        let mut parts = Vec::new();

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

        format!("{{ {}; }}", parts.join("; "))
    }

    fn format_union(&mut self, members: &[TypeId]) -> String {
        if members.len() > self.max_union_members {
            let first: Vec<String> = members
                .iter()
                .take(self.max_union_members)
                .map(|&m| self.format(m))
                .collect();
            return format!("{} | ...", first.join(" | "));
        }
        let formatted: Vec<String> = members.iter().map(|&m| self.format(m)).collect();
        formatted.join(" | ")
    }

    fn format_intersection(&mut self, members: &[TypeId]) -> String {
        let formatted: Vec<String> = members.iter().map(|&m| self.format(m)).collect();
        formatted.join(" & ")
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
                    format!("{}{}: {}{}", name, optional, rest, type_str)
                } else {
                    format!("{}{}{}", rest, type_str, optional)
                }
            })
            .collect();
        format!("[{}]", formatted.join(", "))
    }

    fn format_function(&mut self, shape: &FunctionShape) -> String {
        self.format_signature_arrow(
            &shape.type_params,
            &shape.params,
            shape.this_type,
            shape.return_type,
            shape.is_constructor,
        )
    }

    fn format_callable(&mut self, shape: &CallableShape) -> String {
        if !shape.construct_signatures.is_empty()
            && let Some(sym_id) = shape.symbol
            && let Some(arena) = self.symbol_arena
            && let Some(sym) = arena.get(sym_id)
        {
            return format!("typeof {}", sym.escaped_name);
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();
        if !has_index && shape.properties.is_empty() {
            if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
                let sig = &shape.call_signatures[0];
                return self.format_signature_arrow(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    false,
                );
            }
            if shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty() {
                let sig = &shape.construct_signatures[0];
                return self.format_signature_arrow(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    true,
                );
            }
        }

        let mut parts = Vec::new();
        for sig in &shape.call_signatures {
            parts.push(self.format_call_signature(sig, false));
        }
        for sig in &shape.construct_signatures {
            parts.push(self.format_call_signature(sig, true));
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

    fn format_call_signature(&mut self, sig: &CallSignature, is_construct: bool) -> String {
        let prefix = if is_construct { "new " } else { "" };
        let type_params = self.format_type_params(&sig.type_params);
        let params = self.format_params(&sig.params, sig.this_type);
        let return_str = if is_construct && sig.return_type == TypeId::UNKNOWN {
            "any".to_string()
        } else {
            self.format(sig.return_type)
        };
        format!(
            "{}{}({}): {}",
            prefix,
            type_params,
            params.join(", "),
            return_str
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
        format!(
            "{{ [K in {}]: {} }}",
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
}
