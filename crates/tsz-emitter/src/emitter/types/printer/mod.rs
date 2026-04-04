//! Type Printer - Convert `TypeId` to TypeScript syntax
//!
//! This module handles type reification: converting the Solver's internal `TypeId`
//! representation into printable TypeScript syntax for declaration emit (.d.ts files).

use tsz_binder::{Symbol, SymbolArena, SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::types::TypeId;
use tsz_solver::visitor;

use crate::type_cache_view::TypeCacheView;

/// Prints types as TypeScript syntax for declaration emit.
///
/// # Examples
///
/// ```ignore
/// # use tsz_solver::types::TypeId;
/// let printer = TypePrinter::new(&interner);
/// assert_eq!(printer.print_type(TypeId::STRING), "string");
/// assert_eq!(printer.print_type(TypeId::NUMBER), "number");
/// ```
#[derive(Clone)]
pub struct TypePrinter<'a> {
    interner: &'a TypeInterner,
    /// Symbol arena for checking symbol visibility
    symbol_arena: Option<&'a SymbolArena>,
    /// Type cache for resolving Lazy(DefId) types
    type_cache: Option<&'a TypeCacheView>,
    /// Current recursion depth (to prevent infinite loops)
    current_depth: u32,
    /// Maximum recursion depth
    max_depth: u32,
    /// Indentation level for multi-line type formatting (e.g., object types in .d.ts).
    /// `Some(n)` enables multi-line formatting at indent level `n`;
    /// `None` keeps flat single-line format.
    indent_level: Option<u32>,
    /// The enclosing symbol (namespace/class) whose qualified name prefix should
    /// be stripped from type references to produce context-relative names.
    enclosing_symbol: Option<SymbolId>,
    /// AST access for deciding whether a symbol is nameable from declaration output.
    node_arena: Option<&'a NodeArena>,
    /// Optional resolver for turning foreign symbols into import module specifiers.
    module_path_resolver: Option<&'a dyn Fn(SymbolId) -> Option<String>>,
    /// Optional resolver for reusing in-scope namespace import aliases.
    namespace_alias_resolver: Option<&'a dyn Fn(SymbolId) -> Option<String>>,
    /// Optional resolver for deciding whether a local import alias survives in emitted output.
    local_import_alias_name_resolver: Option<&'a dyn Fn(SymbolId) -> bool>,
    /// Optional resolver for checking whether a foreign symbol has a local import
    /// alias that will be emitted, so the symbol can be referenced by name.
    has_local_import_alias_resolver: Option<&'a dyn Fn(SymbolId) -> bool>,
    /// When false, standalone `null` and `undefined` widen to `any` and are
    /// filtered from union members (matching tsc's DTS behaviour).
    strict_null_checks: bool,
}

impl<'a> TypePrinter<'a> {
    pub const fn new(interner: &'a TypeInterner) -> Self {
        Self {
            interner,
            symbol_arena: None,
            type_cache: None,
            current_depth: 0,
            max_depth: 10,
            indent_level: None,
            enclosing_symbol: None,
            node_arena: None,
            module_path_resolver: None,
            namespace_alias_resolver: None,
            local_import_alias_name_resolver: None,
            has_local_import_alias_resolver: None,
            strict_null_checks: true,
        }
    }

    /// Set the symbol arena for visibility checking.
    pub const fn with_symbols(mut self, symbol_arena: &'a SymbolArena) -> Self {
        self.symbol_arena = Some(symbol_arena);
        self
    }

    /// Set the type cache for resolving Lazy(DefId) types.
    pub const fn with_type_cache(mut self, type_cache: &'a TypeCacheView) -> Self {
        self.type_cache = Some(type_cache);
        self
    }

    /// Set the maximum recursion depth for type inlining.
    pub const fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Enable multi-line type formatting at the given indentation level.
    /// Object types with members will be formatted across multiple lines
    /// using 4-space indentation. Without this, object types use flat format.
    pub const fn with_indent_level(mut self, indent_level: u32) -> Self {
        self.indent_level = Some(indent_level);
        self
    }

    /// Set the enclosing symbol (namespace/class) for context-relative name resolution.
    /// Qualified names that share a prefix with this symbol's path will have the
    /// shared prefix stripped (e.g., inside namespace `m1.m2`, type `m1.m2.c` becomes `c`).
    pub const fn with_enclosing_symbol(mut self, sym_id: SymbolId) -> Self {
        self.enclosing_symbol = Some(sym_id);
        self
    }

    /// Set the AST arena for declaration-reachability checks.
    pub const fn with_node_arena(mut self, node_arena: &'a NodeArena) -> Self {
        self.node_arena = Some(node_arena);
        self
    }

    /// Set a resolver for import-qualified foreign symbol references.
    pub fn with_module_path_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> Option<String>,
    ) -> Self {
        self.module_path_resolver = Some(resolver);
        self
    }

    /// Set a resolver for reusing namespace import aliases already in scope.
    pub fn with_namespace_alias_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> Option<String>,
    ) -> Self {
        self.namespace_alias_resolver = Some(resolver);
        self
    }

    /// Set a resolver for deciding whether local import aliases can be named directly.
    pub fn with_local_import_alias_name_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> bool,
    ) -> Self {
        self.local_import_alias_name_resolver = Some(resolver);
        self
    }

    /// Set a resolver for checking whether a foreign symbol has a local import alias.
    pub fn with_has_local_import_alias_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> bool,
    ) -> Self {
        self.has_local_import_alias_resolver = Some(resolver);
        self
    }

    /// Configure strictNullChecks mode. When false, standalone `null` and
    /// `undefined` widen to `any` and are stripped from union members.
    pub const fn with_strict_null_checks(mut self, strict: bool) -> Self {
        self.strict_null_checks = strict;
        self
    }
}

mod symbol_resolution;
mod type_printing;

/// Escape a cooked string value for embedding in a double-quoted string literal.
///
/// The solver stores "cooked" (unescaped) text for string literals. When
/// writing strings back into `.d.ts` output we must re-escape characters
/// that cannot appear raw inside double-quoted string literals.
fn escape_string_for_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

/// Quote a property name with the appropriate quote style.
/// tsc uses double quotes for numeric-like strings (e.g. "-1", "0")
/// and for other non-identifier names.
fn quote_property_name(name: &str) -> String {
    format!("\"{}\"", escape_string_for_double_quote(name))
}

/// Check if a property name needs quoting (contains spaces, hyphens, etc.)
/// Does NOT quote: valid identifiers, numeric literals, computed names `[...]`
fn needs_property_name_quoting(name: &str) -> bool {
    needs_property_name_quoting_with_flag(name, false)
}

/// Check if a property name needs quoting, with an `is_string_named` flag
/// for properties that were declared with a string key that looks numeric.
fn needs_property_name_quoting_with_flag(name: &str, is_string_named: bool) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names like [Symbol.dispose] are emitted as-is
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Pure numeric names: quote if originally a string key, else emit bare
    if name.chars().all(|ch| ch.is_ascii_digit()) {
        return is_string_named;
    }
    // `new` must be quoted because `new(...)` in a type literal is parsed
    // as a construct signature, not a method named "new".
    // tsc emits `"new"(x: number): number` in .d.ts output.
    if name == "new" {
        return true;
    }
    // In ES5+ and TypeScript, reserved keywords are valid property names
    // and do NOT need quoting. tsc emits them unquoted in .d.ts output.
    // e.g., `{ delete: boolean; class: string; }` — not `{ "delete": boolean; }`.
    let mut chars = name.chars();
    let first = chars
        .next()
        .expect("identifier name must be non-empty after keyword/numeric checks");
    if !(first == '_' || first == '$' || first.is_alphabetic()) {
        return true;
    }
    !chars.all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
}

#[cfg(test)]
#[path = "../../../../tests/type_printer.rs"]
mod tests;
