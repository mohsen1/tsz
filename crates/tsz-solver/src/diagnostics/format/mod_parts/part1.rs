









pub use property_names::format_excess_property_name;

pub(crate) use property_names::needs_property_name_quotes;

use crate::construction::TypeDatabase;

use crate::def::{DefId, DefinitionStore};

use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};

use crate::types::{MappedModifier, ObjectShape, TypeData, TypeId};

use rustc_hash::{FxHashMap, FxHashSet};

use std::borrow::Cow;

use std::mem::size_of;

use std::sync::Arc;

use tsz_common::interner::Atom;

/// Operation-local cache accounting for `TypeFormatter`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeFormatterCacheStatistics {
    /// Cached atom-to-string display entries.
    pub atom_cache_entries: usize,
    /// Approximate heap and struct residency owned by the formatter.
    pub estimated_size_bytes: usize,
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
    /// Maps `file_id` -> full project-relative stripped path for cross-module
    /// diagnostic disambiguation (e.g. `src/library-a/index`). When this is
    /// set it overrides `module_specifiers` for
    /// `import_qualified_name_for_type` so the `import("<path>")` qualifier
    /// distinguishes two files that share the same basename.
    module_path_specifiers: Option<&'a FxHashMap<u32, String>>,
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
    /// When true, format types using tsc's diagnostic display surface.
    diagnostic_mode: bool,
    /// When true, preserve the declared surface syntax of optional properties
    /// instead of appending synthetic `| undefined`.
    preserve_optional_property_surface_syntax: bool,
    /// When true, preserve the declared surface syntax of optional parameters
    /// instead of appending synthetic `| undefined`.
    preserve_optional_parameter_surface_syntax: bool,
    /// When true, use display properties (pre-widened literal types) for fresh
    /// object literals. This implements tsc's freshness model where error messages
    /// show literal types like `{ x: "hello" }` even when the type system uses
    /// widened types like `{ x: string }`.
    use_display_properties: bool,
    /// Set of Application `TypeIds` currently being formatted via `display_alias`.
    /// Prevents infinite recursion when a `display_alias` chain forms a cycle.
    display_alias_visiting: FxHashSet<TypeId>,
    /// Set of `TypeId`s currently on the formatter's recursion stack. Used to
    /// elide self-referential composite types with `...`, mirroring tsc's
    /// `canPossiblyExpandType` cycle detection.
    format_visiting: FxHashSet<TypeId>,
    /// When true, preserve `Array<T>` generic syntax instead of `T[]` shorthand.
    /// tsc preserves the declared form in type-parameter constraints.
    pub(crate) preserve_array_generic_form: bool,
    /// When true, skip using type alias names for aliases whose body is a generic
    /// Application (e.g., `type Foo = Id<{...}>`). In assignability error messages,
    /// tsc shows the Application form `Id<{...}>` rather than the outer alias `Foo`.
    skip_application_alias_names: bool,
    /// Internal guard used while formatting helper application arguments that
    /// should show structural inputs instead of chasing nested application
    /// display aliases.
    skip_application_display_alias_chase: bool,
    /// Internal guard used while formatting generic application arguments.
    /// In that context, tsc preserves indexed-access alias spelling such as
    /// `Partial<T>[keyof T]` instead of simplifying the nested access to
    /// `T[keyof T] | undefined`.
    preserve_application_arg_index_alias_surface: bool,
    /// Specific non-generic type aliases whose name should not be used for
    /// diagnostic display. This is used for `typeof` aliases in assignability
    /// messages where tsc prints the target's structural type rather than the
    /// alias name.
    skip_type_alias_def_ids: FxHashSet<DefId>,
    /// Type aliases currently being expanded through `skip_type_alias_def_ids`.
    /// This lets a recursive alias expand one structural layer before nested
    /// self-references elide as `...`.
    skipped_type_alias_expansion_visiting: FxHashSet<DefId>,
    /// Optional compiler-controlled display replacement for the lib-only
    /// `BuiltinIteratorReturn` alias.
    builtin_iterator_return_type: Option<TypeId>,
    /// When true, don't follow `display_alias` when it points to an Intersection
    /// type and the current type is an Object. Used for TS2741 messages where
    /// tsc shows the merged object form instead of the intersection form.
    skip_intersection_display_alias: bool,
    /// When true, don't follow `display_alias` when it points to an Application
    /// type and the current type is an Intersection. Used for TS2739 messages
    /// where tsc shows the structural `Number & { __brand: T }` form instead of
    /// the branded alias `Brand<T>`.
    skip_application_alias_for_intersections: bool,
    /// When true, format the primitive members of an intersection type using their
    /// apparent/boxed names: `Number` instead of `number`, `String` instead of
    /// `string`, `Boolean` instead of `boolean`. tsc always uses the capitalized
    /// forms for primitive members in intersection type display.
    capitalize_primitive_intersection_members: bool,
    /// When true, do not follow `display_alias` when the current type is an
    /// `Object` / `ObjectWithIndex`. Used for diagnostics like the JS
    /// prototype "property does not exist on type `{...}`" message where tsc
    /// shows the literal's structural shape regardless of any
    /// constructor-prototype symbol aliasing recorded by the type system.
    skip_object_display_alias: bool,
    /// When true, preserve a longer generic alias prefix while eliding nested
    /// structural object branches. Used for long property receiver diagnostics.
    long_property_receiver_display: bool,
    long_property_receiver_object_elision_end_depth: u32,
    /// When true, generic mapped type aliases that evaluate to scalar types are
    /// displayed as their evaluated result. Used for assignability diagnostics.
    expand_scalar_mapped_alias_applications: bool,
    /// When true, the canonical primitive key union (`string | number | symbol`,
    /// shared by `keyof any` and the lib.d.ts alias `PropertyKey`) is rendered
    /// in its structural form even in diagnostic mode. tsc strips the
    /// `aliasSymbol` from the constraint type before formatting TS2344 messages
    /// (`Type 'X' does not satisfy the constraint 'string | number | symbol'`)
    /// while still keeping `PropertyKey` in other diagnostics. The default is
    /// false to preserve the existing behavior across every other surface.
    expand_primitive_key_union: bool,
    /// When true, render union members in canonical interner order even when a
    /// source/display origin was recorded. This is used by narrow diagnostic
    /// surfaces where tsc does not preserve source-written union order.
    ignore_union_origins: bool,
}
