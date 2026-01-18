//! Diagnostic generation for the solver.
//!
//! This module provides error message generation for type checking failures.
//! It produces human-readable diagnostics with source locations and context.
//!
//! ## Architecture: Lazy Diagnostics
//!
//! To avoid expensive string formatting during type checking (especially in tentative
//! contexts like overload resolution), this module uses a two-phase approach:
//!
//! 1. **Collection**: Store structured data in `PendingDiagnostic` with `DiagnosticArg` values
//! 2. **Rendering**: Format strings lazily only when displaying to the user
//!
//! This prevents calling `type_to_string()` thousands of times for errors that are
//! discarded during overload resolution.

use crate::binder::SymbolId;
use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Diagnostic severity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Suggestion,
    Message,
}

// =============================================================================
// Lazy Diagnostic Arguments
// =============================================================================

/// Argument for a diagnostic message template.
///
/// Instead of eagerly formatting types to strings, we store the raw data
/// (TypeId, SymbolId, etc.) and only format when rendering.
#[derive(Clone, Debug)]
pub enum DiagnosticArg {
    /// A type reference (will be formatted via TypeFormatter)
    Type(TypeId),
    /// A symbol reference (will be looked up by name)
    Symbol(SymbolId),
    /// An interned string
    Atom(Atom),
    /// A plain string
    String(Arc<str>),
    /// A number
    Number(usize),
}

impl From<TypeId> for DiagnosticArg {
    fn from(t: TypeId) -> Self {
        DiagnosticArg::Type(t)
    }
}

impl From<SymbolId> for DiagnosticArg {
    fn from(s: SymbolId) -> Self {
        DiagnosticArg::Symbol(s)
    }
}

impl From<Atom> for DiagnosticArg {
    fn from(a: Atom) -> Self {
        DiagnosticArg::Atom(a)
    }
}

impl From<&str> for DiagnosticArg {
    fn from(s: &str) -> Self {
        DiagnosticArg::String(s.into())
    }
}

impl From<String> for DiagnosticArg {
    fn from(s: String) -> Self {
        DiagnosticArg::String(s.into())
    }
}

impl From<usize> for DiagnosticArg {
    fn from(n: usize) -> Self {
        DiagnosticArg::Number(n)
    }
}

/// A pending diagnostic that hasn't been rendered yet.
///
/// This stores the structured data needed to generate an error message,
/// but defers the expensive string formatting until rendering time.
#[derive(Clone, Debug)]
pub struct PendingDiagnostic {
    /// Diagnostic code (e.g., 2322 for type not assignable)
    pub code: u32,
    /// Arguments for the message template
    pub args: Vec<DiagnosticArg>,
    /// Primary source location
    pub span: Option<SourceSpan>,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Related information (additional locations)
    pub related: Vec<PendingDiagnostic>,
}

impl PendingDiagnostic {
    /// Create a new pending error diagnostic.
    pub fn error(code: u32, args: Vec<DiagnosticArg>) -> Self {
        Self {
            code,
            args,
            span: None,
            severity: DiagnosticSeverity::Error,
            related: Vec::new(),
        }
    }

    /// Attach a source span to this diagnostic.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Add related information.
    pub fn with_related(mut self, related: PendingDiagnostic) -> Self {
        self.related.push(related);
        self
    }
}

/// A source location span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    /// Start position (byte offset)
    pub start: u32,
    /// Length in bytes
    pub length: u32,
    /// File path or name
    pub file: Arc<str>,
}

impl SourceSpan {
    pub fn new(file: impl Into<Arc<str>>, start: u32, length: u32) -> Self {
        SourceSpan {
            start,
            length,
            file: file.into(),
        }
    }
}

/// Related diagnostic information (e.g., "see declaration here").
#[derive(Clone, Debug)]
pub struct RelatedInformation {
    pub span: SourceSpan,
    pub message: String,
}

/// A type checking diagnostic.
#[derive(Clone, Debug)]
pub struct TypeDiagnostic {
    /// The main error message
    pub message: String,
    /// Diagnostic code (e.g., 2322 for "Type X is not assignable to type Y")
    pub code: u32,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Primary source location
    pub span: Option<SourceSpan>,
    /// Related information (additional locations)
    pub related: Vec<RelatedInformation>,
}

impl TypeDiagnostic {
    /// Create a new error diagnostic.
    pub fn error(message: impl Into<String>, code: u32) -> Self {
        TypeDiagnostic {
            message: message.into(),
            code,
            severity: DiagnosticSeverity::Error,
            span: None,
            related: Vec::new(),
        }
    }

    /// Add a source span to this diagnostic.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Add related information.
    pub fn with_related(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.related.push(RelatedInformation {
            span,
            message: message.into(),
        });
        self
    }
}

// =============================================================================
// Diagnostic Codes (matching TypeScript's)
// =============================================================================

/// TypeScript diagnostic codes for type errors.
pub mod codes {
    /// Type '{0}' is not assignable to type '{1}'.
    pub const TYPE_NOT_ASSIGNABLE: u32 = 2322;

    /// Argument of type '{0}' is not assignable to parameter of type '{1}'.
    pub const ARG_NOT_ASSIGNABLE: u32 = 2345;

    /// Property '{0}' is missing in type '{1}' but required in type '{2}'.
    pub const PROPERTY_MISSING: u32 = 2741;

    /// Property '{0}' does not exist on type '{1}'.
    pub const PROPERTY_NOT_EXIST: u32 = 2339;

    /// Type '{0}' has no properties in common with type '{1}'.
    pub const NO_COMMON_PROPERTIES: u32 = 2559;

    /// Cannot assign to '{0}' because it is a read-only property.
    pub const READONLY_PROPERTY: u32 = 2540;

    /// Type '{0}' is not assignable to type '{1}'.
    /// '{2}' is assignable to the constraint of type '{3}', but '{3}' could be instantiated with a different subtype.
    pub const CONSTRAINT_NOT_SATISFIED: u32 = 2344;

    /// Argument of type '{0}' is not assignable to parameter of type '{1}'.
    /// Types of property '{2}' are incompatible.
    pub const NESTED_TYPE_MISMATCH: u32 = 2322;

    /// The 'this' context of type '{0}' is not assignable to method's 'this' of type '{1}'.
    pub const THIS_CONTEXT_MISMATCH: u32 = 2684;

    /// Type 'never' is not a valid return type for an async function.
    pub const NEVER_ASYNC_RETURN: u32 = 1064;

    /// Cannot find name '{0}'.
    pub const CANNOT_FIND_NAME: u32 = 2304;

    /// This expression is not callable. Type '{0}' has no call signatures.
    pub const NOT_CALLABLE: u32 = 2349;

    /// Expected {0} arguments, but got {1}.
    pub const ARG_COUNT_MISMATCH: u32 = 2554;

    /// Object is possibly 'undefined'.
    pub const OBJECT_POSSIBLY_UNDEFINED: u32 = 2532;

    /// Object is possibly 'null'.
    pub const OBJECT_POSSIBLY_NULL: u32 = 2531;

    /// Object is of type 'unknown'.
    pub const OBJECT_IS_UNKNOWN: u32 = 2571;

    /// Object literal may only specify known properties, and '{0}' does not exist in type '{1}'.
    pub const EXCESS_PROPERTY: u32 = 2353;

    // =========================================================================
    // Implicit Any Errors (7xxx series)
    // =========================================================================

    /// Variable '{0}' implicitly has an '{1}' type.
    pub const IMPLICIT_ANY: u32 = 7005;

    /// Parameter '{0}' implicitly has an '{1}' type.
    pub const IMPLICIT_ANY_PARAMETER: u32 = 7006;

    /// Member '{0}' implicitly has an '{1}' type.
    pub const IMPLICIT_ANY_MEMBER: u32 = 7008;

    /// '{0}', which lacks return-type annotation, implicitly has an '{1}' return type.
    pub const IMPLICIT_ANY_RETURN: u32 = 7010;

    /// Function expression, which lacks return-type annotation, implicitly has an '{0}' return type.
    pub const IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION: u32 = 7011;

    // =========================================================================
    // Type Instantiation Errors (2xxx series)
    // =========================================================================

    /// Type instantiation is excessively deep and possibly infinite.
    pub const INSTANTIATION_TOO_DEEP: u32 = 2589;
}

// =============================================================================
// Message Templates
// =============================================================================

/// Get the message template for a diagnostic code.
///
/// Templates use {0}, {1}, etc. as placeholders for arguments.
pub fn get_message_template(code: u32) -> &'static str {
    match code {
        codes::TYPE_NOT_ASSIGNABLE => "Type '{0}' is not assignable to type '{1}'.",
        codes::ARG_NOT_ASSIGNABLE => {
            "Argument of type '{0}' is not assignable to parameter of type '{1}'."
        }
        codes::PROPERTY_MISSING => {
            "Property '{0}' is missing in type '{1}' but required in type '{2}'."
        }
        codes::PROPERTY_NOT_EXIST => "Property '{0}' does not exist on type '{1}'.",
        codes::NO_COMMON_PROPERTIES => "Type '{0}' has no properties in common with type '{1}'.",
        codes::READONLY_PROPERTY => "Cannot assign to '{0}' because it is a read-only property.",
        codes::CONSTRAINT_NOT_SATISFIED => {
            "Type '{0}' is not assignable to type '{1}'. '{2}' is assignable to the constraint of type '{3}', but '{3}' could be instantiated with a different subtype."
        }
        codes::THIS_CONTEXT_MISMATCH => {
            "The 'this' context of type '{0}' is not assignable to method's 'this' of type '{1}'."
        }
        codes::NEVER_ASYNC_RETURN => {
            "Type 'never' is not a valid return type for an async function."
        }
        codes::CANNOT_FIND_NAME => "Cannot find name '{0}'.",
        codes::NOT_CALLABLE => {
            "This expression is not callable. Type '{0}' has no call signatures."
        }
        codes::ARG_COUNT_MISMATCH => "Expected {0} arguments, but got {1}.",
        codes::OBJECT_POSSIBLY_UNDEFINED => "Object is possibly 'undefined'.",
        codes::OBJECT_POSSIBLY_NULL => "Object is possibly 'null'.",
        codes::OBJECT_IS_UNKNOWN => "Object is of type 'unknown'.",
        codes::EXCESS_PROPERTY => {
            "Object literal may only specify known properties, and '{0}' does not exist in type '{1}'."
        }
        // Implicit any errors (7xxx series)
        codes::IMPLICIT_ANY => "Variable '{0}' implicitly has an '{1}' type.",
        codes::IMPLICIT_ANY_PARAMETER => "Parameter '{0}' implicitly has an '{1}' type.",
        codes::IMPLICIT_ANY_MEMBER => "Member '{0}' implicitly has an '{1}' type.",
        codes::IMPLICIT_ANY_RETURN => {
            "'{0}', which lacks return-type annotation, implicitly has an '{1}' return type."
        }
        codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION => {
            "Function expression, which lacks return-type annotation, implicitly has an '{0}' return type."
        }
        codes::INSTANTIATION_TOO_DEEP => {
            "Type instantiation is excessively deep and possibly infinite."
        }
        _ => "Unknown diagnostic",
    }
}

// =============================================================================
// Type Formatting
// =============================================================================

/// Context for generating type strings.
pub struct TypeFormatter<'a> {
    interner: &'a dyn TypeDatabase,
    /// Symbol arena for looking up symbol names (optional)
    symbol_arena: Option<&'a crate::binder::SymbolArena>,
    /// Maximum depth for nested type printing
    max_depth: u32,
    /// Current depth
    current_depth: u32,
    atom_cache: FxHashMap<Atom, Arc<str>>,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: None,
            max_depth: 5,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
        }
    }

    /// Create a formatter with access to symbol names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a crate::binder::SymbolArena,
    ) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: Some(symbol_arena),
            max_depth: 5,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
        }
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

    fn atom(&mut self, atom: Atom) -> Arc<str> {
        if let Some(value) = self.atom_cache.get(&atom) {
            return value.clone();
        }
        let resolved = self.interner.resolve_atom_ref(atom);
        self.atom_cache.insert(atom, resolved.clone());
        resolved
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

    fn format_key(&mut self, key: &TypeKey) -> String {
        match key {
            TypeKey::Intrinsic(kind) => self.format_intrinsic(*kind),
            TypeKey::Literal(lit) => self.format_literal(lit),
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                self.format_object(shape.properties.as_slice())
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                self.format_object_with_index(shape.as_ref())
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                self.format_union(members.as_ref())
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(*members);
                self.format_intersection(members.as_ref())
            }
            TypeKey::Array(elem) => format!("{}[]", self.format(*elem)),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                self.format_tuple(elements.as_ref())
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                self.format_function(shape.as_ref())
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                self.format_callable(shape.as_ref())
            }
            TypeKey::TypeParameter(info) => self.atom(info.name).to_string(),
            TypeKey::Ref(sym) => {
                // Try to look up the symbol name
                if let Some(arena) = self.symbol_arena {
                    if let Some(symbol) = arena.get(SymbolId(sym.0)) {
                        return symbol.escaped_name.to_string();
                    }
                }
                format!("Ref({})", sym.0)
            }
            TypeKey::Application(app) => {
                let app = self.interner.type_application(*app);
                let args: Vec<String> = app.args.iter().map(|&arg| self.format(arg)).collect();
                format!("{}<{}>", self.format(app.base), args.join(", "))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                self.format_conditional(cond.as_ref())
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                self.format_mapped(mapped.as_ref())
            }
            TypeKey::IndexAccess(obj, idx) => {
                format!("{}[{}]", self.format(*obj), self.format(*idx))
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                self.format_template_literal(spans.as_ref())
            }
            TypeKey::TypeQuery(sym) => {
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
            TypeKey::KeyOf(operand) => format!("keyof {}", self.format(*operand)),
            TypeKey::ReadonlyType(inner) => format!("readonly {}", self.format(*inner)),
            TypeKey::UniqueSymbol(sym) => {
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
            TypeKey::Infer(info) => format!("infer {}", self.atom(info.name)),
            TypeKey::ThisType => "this".to_string(),
            TypeKey::Error => "error".to_string(),
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
        if props.len() > 3 {
            let first_three: Vec<String> = props
                .iter()
                .take(3)
                .map(|p| self.format_property(p))
                .collect();
            return format!("{{ {}; ... }}", first_three.join("; "));
        }
        let formatted: Vec<String> = props.iter().map(|p| self.format_property(p)).collect();
        format!("{{ {} }}", formatted.join("; "))
    }

    fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let type_str = self.format(prop.type_id);
        let name = self.atom(prop.name);
        format!("{}{}{}: {}", readonly, name, optional, type_str)
    }

    fn format_object_with_index(&mut self, shape: &ObjectShape) -> String {
        let mut parts = Vec::new();

        if let Some(ref idx) = shape.string_index {
            parts.push(format!("[key: string]: {}", self.format(idx.value_type)));
        }
        if let Some(ref idx) = shape.number_index {
            parts.push(format!("[key: number]: {}", self.format(idx.value_type)));
        }
        for prop in &shape.properties {
            parts.push(self.format_property(prop));
        }

        format!("{{ {} }}", parts.join("; "))
    }

    fn format_union(&mut self, members: &[TypeId]) -> String {
        if members.len() > 5 {
            let first_five: Vec<String> = members.iter().take(5).map(|&m| self.format(m)).collect();
            return format!("{} | ...", first_five.join(" | "));
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
        let mut params: Vec<String> = Vec::new();
        if let Some(this_type) = shape.this_type {
            params.push(format!("this: {}", self.format(this_type)));
        }
        for p in &shape.params {
            let name = p
                .name
                .map(|atom| self.atom(atom))
                .unwrap_or_else(|| Arc::from("_"));
            let optional = if p.optional { "?" } else { "" };
            let rest = if p.rest { "..." } else { "" };
            let type_str = self.format(p.type_id);
            params.push(format!("{}{}{}: {}", rest, name, optional, type_str));
        }
        let arrow = if shape.is_constructor { "new " } else { "" };
        format!(
            "{}({}) => {}",
            arrow,
            params.join(", "),
            self.format(shape.return_type)
        )
    }

    fn format_callable(&mut self, shape: &CallableShape) -> String {
        let mut parts = Vec::new();
        for sig in &shape.call_signatures {
            parts.push(self.format_call_signature(sig, false));
        }
        for sig in &shape.construct_signatures {
            parts.push(self.format_call_signature(sig, true));
        }
        for prop in &shape.properties {
            parts.push(self.format_property(prop));
        }
        format!("{{ {} }}", parts.join("; "))
    }

    fn format_call_signature(&mut self, sig: &CallSignature, is_construct: bool) -> String {
        let mut params: Vec<String> = Vec::new();
        if let Some(this_type) = sig.this_type {
            params.push(format!("this: {}", self.format(this_type)));
        }
        for p in &sig.params {
            let name = p
                .name
                .map(|atom| self.atom(atom))
                .unwrap_or_else(|| Arc::from("_"));
            let type_str = self.format(p.type_id);
            params.push(format!("{}: {}", name, type_str));
        }
        let prefix = if is_construct { "new " } else { "" };
        format!(
            "{}({}): {}",
            prefix,
            params.join(", "),
            self.format(sig.return_type)
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

// =============================================================================
// Diagnostic Builder
// =============================================================================

/// Builder for creating type error diagnostics.
pub struct DiagnosticBuilder<'a> {
    formatter: TypeFormatter<'a>,
}

impl<'a> DiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        DiagnosticBuilder {
            formatter: TypeFormatter::new(interner),
        }
    }

    /// Create a "Type X is not assignable to type Y" diagnostic.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Type '{}' is not assignable to type '{}'.",
                source_str, target_str
            ),
            codes::TYPE_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Property X is missing in type Y" diagnostic.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
    ) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Property '{}' is missing in type '{}' but required in type '{}'.",
                prop_name, source_str, target_str
            ),
            codes::PROPERTY_MISSING,
        )
    }

    /// Create a "Property X does not exist on type Y" diagnostic.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!(
                "Property '{}' does not exist on type '{}'.",
                prop_name, type_str
            ),
            codes::PROPERTY_NOT_EXIST,
        )
    }

    /// Create an "Argument not assignable" diagnostic.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> TypeDiagnostic {
        let arg_str = self.formatter.format(arg_type);
        let param_str = self.formatter.format(param_type);
        TypeDiagnostic::error(
            format!(
                "Argument of type '{}' is not assignable to parameter of type '{}'.",
                arg_str, param_str
            ),
            codes::ARG_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Cannot find name" diagnostic.
    pub fn cannot_find_name(&mut self, name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Cannot find name '{}'.", name),
            codes::CANNOT_FIND_NAME,
        )
    }

    /// Create a "Type X is not callable" diagnostic.
    pub fn not_callable(&mut self, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!("Type '{}' has no call signatures.", type_str),
            codes::NOT_CALLABLE,
        )
    }

    /// Create an "Expected N arguments but got M" diagnostic.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Expected {} arguments, but got {}.", expected, got),
            codes::ARG_COUNT_MISMATCH,
        )
    }

    /// Create a "Cannot assign to readonly property" diagnostic.
    pub fn readonly_property(&mut self, prop_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!(
                "Cannot assign to '{}' because it is a read-only property.",
                prop_name
            ),
            codes::READONLY_PROPERTY,
        )
    }

    /// Create an "Excess property" diagnostic.
    pub fn excess_property(&mut self, prop_name: &str, target: TypeId) -> TypeDiagnostic {
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Object literal may only specify known properties, and '{}' does not exist in type '{}'.",
                prop_name, target_str
            ),
            codes::EXCESS_PROPERTY,
        )
    }

    // =========================================================================
    // Implicit Any Diagnostics (TS7006, TS7008, TS7010, TS7011)
    // =========================================================================

    /// Create a "Parameter implicitly has an 'any' type" diagnostic (TS7006).
    ///
    /// This is emitted when noImplicitAny is enabled and a function parameter
    /// has no type annotation and no contextual type.
    pub fn implicit_any_parameter(&mut self, param_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Parameter '{}' implicitly has an 'any' type.", param_name),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Parameter implicitly has a specific type" diagnostic (TS7006 variant).
    ///
    /// This is used when the implicit type is known to be something other than 'any',
    /// such as when a rest parameter implicitly has 'any[]'.
    pub fn implicit_any_parameter_with_type(
        &mut self,
        param_name: &str,
        implicit_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(implicit_type);
        TypeDiagnostic::error(
            format!(
                "Parameter '{}' implicitly has an '{}' type.",
                param_name, type_str
            ),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Member implicitly has an 'any' type" diagnostic (TS7008).
    ///
    /// This is emitted when noImplicitAny is enabled and a class/interface member
    /// has no type annotation.
    pub fn implicit_any_member(&mut self, member_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Member '{}' implicitly has an 'any' type.", member_name),
            codes::IMPLICIT_ANY_MEMBER,
        )
    }

    /// Create a "Variable implicitly has an 'any' type" diagnostic (TS7005).
    ///
    /// This is emitted when noImplicitAny is enabled and a variable declaration
    /// has no type annotation and the inferred type is 'any'.
    pub fn implicit_any_variable(&mut self, var_name: &str, var_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(var_type);
        TypeDiagnostic::error(
            format!("Variable '{}' implicitly has an '{}' type.", var_name, type_str),
            codes::IMPLICIT_ANY,
        )
    }

    /// Create an "implicitly has an 'any' return type" diagnostic (TS7010).
    ///
    /// This is emitted when noImplicitAny is enabled and a function declaration
    /// has no return type annotation and returns 'any'.
    pub fn implicit_any_return(&mut self, func_name: &str, return_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "'{}', which lacks return-type annotation, implicitly has an '{}' return type.",
                func_name, type_str
            ),
            codes::IMPLICIT_ANY_RETURN,
        )
    }

    /// Create a "Function expression implicitly has an 'any' return type" diagnostic (TS7011).
    ///
    /// This is emitted when noImplicitAny is enabled and a function expression
    /// has no return type annotation and returns 'any'.
    pub fn implicit_any_return_function_expression(
        &mut self,
        return_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "Function expression, which lacks return-type annotation, implicitly has an '{}' return type.",
                type_str
            ),
            codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
        )
    }
}

// =============================================================================
// Pending Diagnostic Builder (LAZY)
// =============================================================================

/// Builder for creating lazy pending diagnostics.
///
/// This builder creates PendingDiagnostic instances that defer expensive
/// string formatting until rendering time.
pub struct PendingDiagnosticBuilder;

// =============================================================================
// SubtypeFailureReason to PendingDiagnostic Conversion
// =============================================================================

use crate::solver::subtype::SubtypeFailureReason;

impl SubtypeFailureReason {
    /// Convert this failure reason to a PendingDiagnostic.
    ///
    /// This is the "explain slow" path - called only when we need to report
    /// an error and want a detailed message about why the type check failed.
    pub fn to_diagnostic(&self, source: TypeId, target: TypeId) -> PendingDiagnostic {
        match self {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::PROPERTY_MISSING,
                vec![
                    (*property_name).into(),
                    (*source_type).into(),
                    (*target_type).into(),
                ],
            ),

            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => {
                // Main error: Type not assignable
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );

                // Add elaboration: Types of property 'x' are incompatible
                let elaboration = PendingDiagnostic::error(
                    codes::NESTED_TYPE_MISMATCH,
                    vec![
                        (*property_name).into(),
                        (*source_property_type).into(),
                        (*target_property_type).into(),
                    ],
                );
                diag = diag.with_related(elaboration);

                // If there's a nested reason, add that too
                if let Some(nested) = nested_reason {
                    let nested_diag =
                        nested.to_diagnostic(*source_property_type, *target_property_type);
                    diag = diag.with_related(nested_diag);
                }

                diag
            }

            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                // This is a specific case of type not assignable
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PROPERTY_MISSING, // Close enough - property is "missing" because it's optional
                    vec![(*property_name).into(), source.into(), target.into()],
                ))
            }

            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::READONLY_PROPERTY,
                    vec![(*property_name).into()],
                ))
            }

            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => {
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );

                // Add: Type 'X' is not assignable to type 'Y' (for return types)
                let return_diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_return).into(), (*target_return).into()],
                );
                diag = diag.with_related(return_diag);

                if let Some(nested) = nested_reason {
                    let nested_diag = nested.to_diagnostic(*source_return, *target_return);
                    diag = diag.with_related(nested_diag);
                }

                diag
            }

            SubtypeFailureReason::ParameterTypeMismatch {
                param_index: _,
                source_param,
                target_param,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_param).into(), (*target_param).into()],
            )),

            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            } => PendingDiagnostic::error(
                codes::ARG_COUNT_MISMATCH,
                vec![(*target_count).into(), (*source_count).into()],
            ),

            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::ARG_COUNT_MISMATCH,
                vec![(*target_count).into(), (*source_count).into()],
            )),

            SubtypeFailureReason::TupleElementTypeMismatch {
                index: _,
                source_element,
                target_element,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_element).into(), (*target_element).into()],
            )),

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_element).into(), (*target_element).into()],
            )),

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: _,
                source_value_type,
                target_value_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_value_type).into(), (*target_value_type).into()],
            )),

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members,
            } => {
                const UNION_MEMBER_DIAGNOSTIC_LIMIT: usize = 3;
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_type).into(), target.into()],
                );
                for member in target_union_members
                    .iter()
                    .take(UNION_MEMBER_DIAGNOSTIC_LIMIT)
                {
                    diag.related.push(PendingDiagnostic::error(
                        codes::TYPE_NOT_ASSIGNABLE,
                        vec![(*source_type).into(), (*member).into()],
                    ));
                }
                diag
            }

            SubtypeFailureReason::NoCommonProperties {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::NO_COMMON_PROPERTIES,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::IntrinsicTypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::LiteralTypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::ErrorType {
                source_type,
                target_type,
            } => {
                // Error types indicate unresolved types that should trigger TS2322.
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_type).into(), (*target_type).into()],
                )
            }
        }
    }
}

impl PendingDiagnosticBuilder {
    /// Create a "Type X is not assignable to type Y" pending diagnostic.
    pub fn type_not_assignable(source: TypeId, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::TYPE_NOT_ASSIGNABLE,
            vec![source.into(), target.into()],
        )
    }

    /// Create a "Property X is missing" pending diagnostic.
    pub fn property_missing(prop_name: &str, source: TypeId, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::PROPERTY_MISSING,
            vec![prop_name.into(), source.into(), target.into()],
        )
    }

    /// Create a "Property X does not exist" pending diagnostic.
    pub fn property_not_exist(prop_name: &str, type_id: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::PROPERTY_NOT_EXIST,
            vec![prop_name.into(), type_id.into()],
        )
    }

    /// Create an "Argument not assignable" pending diagnostic.
    pub fn argument_not_assignable(arg_type: TypeId, param_type: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::ARG_NOT_ASSIGNABLE,
            vec![arg_type.into(), param_type.into()],
        )
    }

    /// Create a "Cannot find name" pending diagnostic.
    pub fn cannot_find_name(name: &str) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::CANNOT_FIND_NAME, vec![name.into()])
    }

    /// Create a "Type is not callable" pending diagnostic.
    pub fn not_callable(type_id: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::NOT_CALLABLE, vec![type_id.into()])
    }

    /// Create an "Expected N arguments but got M" pending diagnostic.
    pub fn argument_count_mismatch(expected: usize, got: usize) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::ARG_COUNT_MISMATCH, vec![expected.into(), got.into()])
    }

    /// Create a "Cannot assign to readonly property" pending diagnostic.
    pub fn readonly_property(prop_name: &str) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::READONLY_PROPERTY, vec![prop_name.into()])
    }

    /// Create an "Excess property" pending diagnostic.
    pub fn excess_property(prop_name: &str, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::EXCESS_PROPERTY,
            vec![prop_name.into(), target.into()],
        )
    }
}

// =============================================================================
// Spanned Diagnostic Builder
// =============================================================================

/// A diagnostic builder that automatically attaches source spans.
///
/// This builder wraps `DiagnosticBuilder` and requires a file name and
/// position information for each diagnostic.
pub struct SpannedDiagnosticBuilder<'a> {
    builder: DiagnosticBuilder<'a>,
    file: Arc<str>,
}

impl<'a> SpannedDiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        SpannedDiagnosticBuilder {
            builder: DiagnosticBuilder::new(interner),
            file: file.into(),
        }
    }

    /// Create a span for this file.
    pub fn span(&self, start: u32, length: u32) -> SourceSpan {
        SourceSpan::new(self.file.clone(), start, length)
    }

    /// Create a "Type X is not assignable to type Y" diagnostic with span.
    pub fn type_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .type_not_assignable(source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X is missing" diagnostic with span.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_missing(prop_name, source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X does not exist" diagnostic with span.
    pub fn property_not_exist(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_not_exist(prop_name, type_id)
            .with_span(self.span(start, length))
    }

    /// Create an "Argument not assignable" diagnostic with span.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot find name" diagnostic with span.
    pub fn cannot_find_name(&mut self, name: &str, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .cannot_find_name(name)
            .with_span(self.span(start, length))
    }

    /// Create an "Expected N arguments" diagnostic with span.
    pub fn argument_count_mismatch(
        &mut self,
        expected: usize,
        got: usize,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_count_mismatch(expected, got)
            .with_span(self.span(start, length))
    }

    /// Create a "Type is not callable" diagnostic with span.
    pub fn not_callable(&mut self, type_id: TypeId, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .not_callable(type_id)
            .with_span(self.span(start, length))
    }

    /// Create an "Excess property" diagnostic with span.
    pub fn excess_property(
        &mut self,
        prop_name: &str,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .excess_property(prop_name, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot assign to readonly property" diagnostic with span.
    pub fn readonly_property(
        &mut self,
        prop_name: &str,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .readonly_property(prop_name)
            .with_span(self.span(start, length))
    }

    /// Add a related location to an existing diagnostic.
    pub fn add_related(
        &self,
        diag: TypeDiagnostic,
        message: impl Into<String>,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        diag.with_related(self.span(start, length), message)
    }
}

// =============================================================================
// Diagnostic Conversion
// =============================================================================

/// Convert a solver TypeDiagnostic to a checker Diagnostic.
///
/// This allows the solver's diagnostic infrastructure to integrate
/// with the existing checker diagnostic system.
impl TypeDiagnostic {
    /// Convert to a checker::Diagnostic.
    ///
    /// Uses the provided file_name if no span is present.
    pub fn to_checker_diagnostic(
        &self,
        default_file: &str,
    ) -> crate::checker::types::diagnostics::Diagnostic {
        use crate::checker::types::diagnostics::{
            Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
        };

        let (file, start, length) = if let Some(ref span) = self.span {
            (span.file.to_string(), span.start, span.length)
        } else {
            (default_file.to_string(), 0, 0)
        };

        let category = match self.severity {
            DiagnosticSeverity::Error => DiagnosticCategory::Error,
            DiagnosticSeverity::Warning => DiagnosticCategory::Warning,
            DiagnosticSeverity::Suggestion => DiagnosticCategory::Suggestion,
            DiagnosticSeverity::Message => DiagnosticCategory::Message,
        };

        let related_information: Vec<DiagnosticRelatedInformation> = self
            .related
            .iter()
            .map(|rel| DiagnosticRelatedInformation {
                file: rel.span.file.to_string(),
                start: rel.span.start,
                length: rel.span.length,
                message_text: rel.message.clone(),
                category: DiagnosticCategory::Message,
                code: 0,
            })
            .collect();

        Diagnostic {
            file,
            start,
            length,
            message_text: self.message.clone(),
            category,
            code: self.code,
            related_information,
        }
    }
}

// =============================================================================
// Source Location Tracker
// =============================================================================

/// Tracks source locations for AST nodes during type checking.
///
/// This struct provides a convenient way to associate type checking
/// operations with their source locations for diagnostic generation.
#[derive(Clone)]
pub struct SourceLocation {
    /// File name
    pub file: Arc<str>,
    /// Start position (byte offset)
    pub start: u32,
    /// End position (byte offset)
    pub end: u32,
}

impl SourceLocation {
    pub fn new(file: impl Into<Arc<str>>, start: u32, end: u32) -> Self {
        SourceLocation {
            file: file.into(),
            start,
            end,
        }
    }

    /// Get the length of this location.
    pub fn length(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Convert to a SourceSpan.
    pub fn to_span(&self) -> SourceSpan {
        SourceSpan::new(self.file.clone(), self.start, self.length())
    }
}

/// A diagnostic collector that accumulates diagnostics with source tracking.
pub struct DiagnosticCollector<'a> {
    interner: &'a dyn TypeDatabase,
    file: Arc<str>,
    diagnostics: Vec<TypeDiagnostic>,
}

impl<'a> DiagnosticCollector<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        DiagnosticCollector {
            interner,
            file: file.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Get the collected diagnostics.
    pub fn diagnostics(&self) -> &[TypeDiagnostic] {
        &self.diagnostics
    }

    /// Take the collected diagnostics.
    pub fn take_diagnostics(&mut self) -> Vec<TypeDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Report a type not assignable error.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .type_not_assignable(source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property missing error.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_missing(prop_name, source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property not exist error.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_not_exist(prop_name, type_id)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument not assignable error.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a cannot find name error.
    pub fn cannot_find_name(&mut self, name: &str, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder.cannot_find_name(name).with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument count mismatch error.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_count_mismatch(expected, got)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Convert all collected diagnostics to checker diagnostics.
    pub fn to_checker_diagnostics(&self) -> Vec<crate::checker::types::diagnostics::Diagnostic> {
        self.diagnostics
            .iter()
            .map(|d| d.to_checker_diagnostic(&self.file))
            .collect()
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
