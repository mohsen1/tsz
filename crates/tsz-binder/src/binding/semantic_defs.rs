//! Binder semantic definition recording helpers.

use crate::SymbolId;
use crate::binding::declaration::SemanticDefDetails;
use crate::state::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::{NodeIndex, NodeList};

impl BinderState {
    /// Collect type parameter names from a type parameter `NodeList`.
    ///
    /// Returns an empty `Vec` if `type_params` is `None` or contains no
    /// extractable names. Each entry is the escaped text of the type
    /// parameter identifier (e.g., `["T", "U"]` for `<T, U>`).
    pub(crate) fn collect_type_param_names(
        arena: &NodeArena,
        type_params: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(params) = type_params else {
            return Vec::new();
        };
        params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let node = arena.get(param_idx)?;
                let tp = arena.get_type_parameter(node)?;
                let name = Self::get_identifier_name(arena, tp.name)?;
                Some(name.to_string())
            })
            .collect()
    }

    pub(crate) fn record_semantic_def(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        type_param_count: u16,
        type_param_names: Vec<String>,
        is_exported: bool,
    ) {
        self.record_semantic_def_ext(
            sym_id,
            kind,
            name,
            declaration,
            SemanticDefDetails {
                type_param_count,
                type_param_names,
                is_exported,
                ..Default::default()
            },
        );
    }

    /// Like `record_semantic_def` but with explicit `is_declare` flag.
    pub(crate) fn record_semantic_def_with_declare(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        details: SemanticDefDetails,
    ) {
        self.record_semantic_def_ext(sym_id, kind, name, declaration, details);
    }

    /// Extended version of `record_semantic_def` that also captures enriched
    /// identity data: enum member names, const-enum flag, abstract-class flag,
    /// and split heritage names (extends vs implements).
    ///
    /// This captures stable identity information at bind time so the checker
    /// can pre-create solver `DefIds` during construction rather than inventing
    /// them on demand in hot paths.
    ///
    /// Only records entries for declarations at the source file scope (`ScopeId(0)`)
    /// to avoid noise from nested declarations that are less likely to be
    /// cross-file semantic references.
    pub(crate) fn record_semantic_def_ext(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        details: SemanticDefDetails,
    ) {
        let SemanticDefDetails {
            type_param_count,
            type_param_names,
            is_exported,
            enum_member_names,
            is_const,
            is_abstract,
            is_declare,
            extends_names,
            implements_names,
        } = details;
        // Only capture top-level declarations (source file scope or module scope)
        // and declarations inside `declare global { }` blocks.
        // Nested declarations (inside function bodies, class bodies, etc.) are not
        // recorded because they don't participate in cross-file identity.
        let is_top_level = self.current_scope_id == crate::ScopeId(0)
            || self
                .scopes
                .get(self.current_scope_id.0 as usize)
                .is_some_and(|scope| {
                    matches!(
                        scope.kind,
                        crate::ContainerKind::SourceFile | crate::ContainerKind::Module
                    )
                });
        // Declarations inside `declare global { }` blocks are semantically
        // top-level even if their scope chain doesn't directly match
        // SourceFile/Module (e.g., when the global block is nested inside
        // another module declaration). Capture them so the pre-population
        // pipeline creates stable DefIds for global augmentations.
        if !is_top_level && !self.in_global_augmentation {
            return;
        }
        // Declaration merging: keep the first declaration's core identity stable
        // (kind, name, span, file_id) but accumulate heritage and type_param_count
        // from later declarations.  This ensures the pre-populated DefinitionInfo
        // has complete heritage information (e.g., `interface A extends B {}` +
        // `interface A extends C {}` yields extends_names = ["B", "C"]).
        if let Some(existing) = std::sync::Arc::make_mut(&mut self.semantic_defs).get_mut(&sym_id) {
            // Type-side declaration merging into a value-side namespace must
            // promote the kind. A symbol like `namespace B {} interface B {}`
            // appears in TYPE positions as the interface; if the recorded kind
            // stayed `Namespace`, the type printer would emit `typeof B` instead
            // of `B`. tsc's resolver picks the type-side meaning in type
            // positions, so the binder mirrors that by upgrading the kind when
            // a Type-class declaration merges into a Namespace entry.
            if matches!(existing.kind, crate::state::SemanticDefKind::Namespace)
                && matches!(
                    kind,
                    crate::state::SemanticDefKind::Interface
                        | crate::state::SemanticDefKind::TypeAlias
                        | crate::state::SemanticDefKind::Class
                        | crate::state::SemanticDefKind::Enum
                )
            {
                existing.kind = kind;
            }
            // Accumulate new extends_names that aren't already present.
            for h in &extends_names {
                if !existing.extends_names.contains(h) {
                    existing.extends_names.push(h.clone());
                }
            }
            // Accumulate new implements_names that aren't already present.
            for h in &implements_names {
                if !existing.implements_names.contains(h) {
                    existing.implements_names.push(h.clone());
                }
            }
            // If the first declaration had no type params but this one does
            // (e.g., augmentation adds generics), update the arity and names.
            // However, do NOT merge function type params into/over a type-level
            // (interface/type alias/class) semantic def, and vice versa.
            // Function type params are function-scoped and don't represent
            // the type's generic arity.
            // E.g., `interface Mixin {}; function Mixin<T>(...) {...}` — the
            // interface has 0 type params, and the function's `T` is irrelevant.
            // Also handles the reverse: `function Mixin<T>(...); type Mixin = any;`
            // — the type alias has 0 type params and should override the function's.
            let is_type_kind = |k: &crate::state::SemanticDefKind| {
                matches!(
                    k,
                    crate::state::SemanticDefKind::Interface
                        | crate::state::SemanticDefKind::TypeAlias
                        | crate::state::SemanticDefKind::Class
                )
            };
            let is_function_kind = |k: &crate::state::SemanticDefKind| {
                matches!(k, crate::state::SemanticDefKind::Function)
            };
            let cross_function_type = (is_function_kind(&kind) && is_type_kind(&existing.kind))
                || (is_type_kind(&kind) && is_function_kind(&existing.kind));
            // When a type declaration (interface/type alias/class) merges with
            // a function, the semantic def's type_param_count should reflect the
            // TYPE declaration's params (which is the relevant arity for TS2314).
            if cross_function_type {
                // If a type declaration is merging in, update to its param count
                // (even if 0, since the type might have no params).
                if is_type_kind(&kind) {
                    existing.type_param_count = type_param_count;
                    existing.type_param_names = type_param_names;
                }
                // If function is merging into a type, don't update params (already handled)
            } else if existing.type_param_count == 0 && type_param_count > 0 {
                existing.type_param_count = type_param_count;
                existing.type_param_names = type_param_names;
            }
            // If the later declaration is exported, mark as exported.
            if is_exported {
                existing.is_exported = true;
            }
            // Accumulate enum members from later enum declarations.
            if !enum_member_names.is_empty() {
                for m in &enum_member_names {
                    if !existing.enum_member_names.contains(m) {
                        existing.enum_member_names.push(m.clone());
                    }
                }
            }
            // Promote global augmentation flag if any declaration is from declare global.
            if self.in_global_augmentation {
                existing.is_global_augmentation = true;
            }
            return;
        }
        // Determine containing namespace symbol, if any.
        // A declaration is namespace-parented when its scope is Module but not
        // the source-file root (ScopeId(0)).
        let parent_namespace = if self.current_scope_id != crate::ScopeId(0) {
            self.scopes
                .get(self.current_scope_id.0 as usize)
                .and_then(|scope| {
                    if scope.kind == crate::ContainerKind::Module {
                        // Look up the namespace symbol from the scope's container node.
                        self.get_node_symbol(scope.container_node)
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        std::sync::Arc::make_mut(&mut self.semantic_defs).insert(
            sym_id,
            crate::state::SemanticDefEntry {
                kind,
                name: name.to_string(),
                file_id: self
                    .symbols
                    .get(sym_id)
                    .map_or(u32::MAX, |s| s.decl_file_idx),
                span_start: declaration.0,
                type_param_count,
                type_param_names,
                is_exported,
                enum_member_names,
                is_const,
                is_abstract,
                extends_names,
                implements_names,
                parent_namespace,
                is_global_augmentation: self.in_global_augmentation,
                is_declare,
            },
        );
    }

    pub(crate) fn is_global_scope(&self) -> bool {
        // Global scope is ScopeId(0) in script files
        self.current_scope_id == crate::ScopeId(0)
    }

    /// Check whether a name in the current binder already resolves to a lib symbol.
    ///
    /// Lib symbols are merged into the local binder before user binding via
    /// `merge_lib_symbols`, so the symbol IDs in `current_scope`/`file_locals` for
    /// these names are tracked in `lib_symbol_ids`. This lets us detect "the user
    /// is declaring an interface whose name collides with a lib global" without
    /// hardcoding a static allow-list of lib types — covering DOM, `WebWorker`,
    /// `ScriptHost`, and any other ambient globals the project pulls in.
    pub(crate) fn name_collides_with_lib_symbol(&self, name: &str) -> bool {
        self.current_scope()
            .get(name)
            .or_else(|| self.file_locals.get(name))
            .is_some_and(|sym_id| self.lib_symbol_ids.contains(&sym_id))
    }

    /// Check if a type name is a built-in global type that can be augmented.
    ///
    /// These are types from lib.d.ts that TypeScript allows augmenting through
    /// top-level interface declarations in script files (without `declare global`).
    pub(crate) fn is_built_in_global_type(name: &str) -> bool {
        matches!(
            name,
            "Array"
                | "ReadonlyArray"
                | "Promise"
                | "PromiseLike"
                | "Map"
                | "ReadonlyMap"
                | "WeakMap"
                | "Set"
                | "ReadonlySet"
                | "WeakSet"
                | "ArrayConstructor"
                | "MapConstructor"
                | "SetConstructor"
                | "WeakMapConstructor"
                | "WeakSetConstructor"
                | "PromiseConstructor"
                | "ProxyHandler"
                | "ProxyConstructor"
                | "Reflect"
                | "Generator"
                | "GeneratorFunction"
                | "AsyncGenerator"
                | "AsyncGeneratorFunction"
                | "AsyncIterable"
                | "AsyncIterableIterator"
                | "AsyncIterator"
                | "Iterable"
                | "Iterator"
                | "IterableIterator"
                | "Symbol"
                | "SymbolConstructor"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Uint16Array"
                | "Uint32Array"
                | "Int8Array"
                | "Int16Array"
                | "Int32Array"
                | "Float32Array"
                | "Float64Array"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "RegExp"
                | "RegExpConstructor"
                | "Date"
                | "DateConstructor"
                | "Error"
                | "ErrorConstructor"
                | "EvalError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "TypeError"
                | "URIError"
                | "Boolean"
                | "Number"
                | "String"
                | "Object"
                | "ObjectConstructor"
                | "Function"
                | "IArguments"
                | "JSON"
                | "Math"
                | "Console"
        )
    }
}
