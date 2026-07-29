//! Cross-arena resolution of the value type contributed by a module
//! augmentation's NEW exports (#14853).
//!
//! When `declare module "x" { export const c: T }` (or `function`/`class`/
//! `enum`) augments an ambient module declared in another file and *adds a new
//! export* (rather than merging into an existing one), the augmentation
//! declaration node lives in a foreign arena relative to the file currently
//! being checked. The merge previously typed such a new export `any`, dropping
//! every assignability error against it. This routes the declaration through a
//! delegate child checker over the owning arena/binder so the real declared
//! type is recovered, mirroring
//! `delegate_cross_arena_interface_member_simple_types`.

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::ModuleAugmentation;
use tsz_common::perf_counters::CheckerCreationReason;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::{PropertyInfo, TypeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleAugmentationRuntimeOrigin {
    DirectTarget,
    ReplayFallback,
}

pub(crate) struct NamedImportAugmentationRuntimeProvenance {
    pub(crate) module_specifier: String,
    pub(crate) import_name: String,
    pub(crate) binding_is_type_only: bool,
    pub(crate) origin: ModuleAugmentationRuntimeOrigin,
    runtime_module_specifier: String,
    runtime_import_name: String,
}

pub(crate) struct NamedImportAugmentationRuntimeBinding {
    pub(crate) module_specifier: String,
    pub(crate) import_name: String,
    pub(crate) binding_is_type_only: bool,
    pub(crate) type_id: TypeId,
    pub(crate) origin: ModuleAugmentationRuntimeOrigin,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn named_import_augmentation_runtime_provenance(
        &self,
        use_idx: NodeIndex,
        local_name: &str,
    ) -> Option<NamedImportAugmentationRuntimeProvenance> {
        let (module_specifier, import_name, binding_is_type_only) =
            self.resolve_named_import_for_local_name(use_idx, local_name)?;
        let (_, has_direct_value, has_runtime_value) = self
            .module_augmentation_runtime_declarations_with_direct_value(
                &module_specifier,
                &import_name,
            );
        let (runtime_module_specifier, runtime_import_name, origin) = if has_runtime_value {
            (
                module_specifier.clone(),
                import_name.clone(),
                if has_direct_value {
                    ModuleAugmentationRuntimeOrigin::DirectTarget
                } else {
                    ModuleAugmentationRuntimeOrigin::ReplayFallback
                },
            )
        } else {
            // A renamed `export type { HomeName as BarrelName }` deliberately
            // carries no runtime declaration under `BarrelName`. Follow the
            // binder-owned re-export chain to the original declaration owner,
            // then query that owner by stable file identity and original name.
            // Keep the public provenance on the barrel edge so TS1362 still
            // reflects the syntax the consumer imported through.
            let barrel_file_idx = self.ctx.resolve_import_target(&module_specifier)?;
            let (target_sym_id, target_file_idx) =
                self.resolve_reexport_chain_to_declaration(barrel_file_idx, &import_name)?;
            let target_name = self
                .ctx
                .get_binder_for_file(target_file_idx)?
                .get_symbol(target_sym_id)?
                .escaped_name
                .clone();
            let target_specifier = format!("file_idx:{target_file_idx}");
            let (_, _, target_has_runtime_value) = self
                .module_augmentation_runtime_declarations_with_direct_value(
                    &target_specifier,
                    &target_name,
                );
            if !target_has_runtime_value {
                return None;
            }
            (
                target_specifier,
                target_name,
                ModuleAugmentationRuntimeOrigin::ReplayFallback,
            )
        };
        Some(NamedImportAugmentationRuntimeProvenance {
            module_specifier,
            import_name,
            binding_is_type_only,
            origin,
            runtime_module_specifier,
            runtime_import_name,
        })
    }

    pub(crate) fn named_import_augmentation_runtime_binding(
        &mut self,
        use_idx: NodeIndex,
        local_name: &str,
    ) -> Option<NamedImportAugmentationRuntimeBinding> {
        let provenance = self.named_import_augmentation_runtime_provenance(use_idx, local_name)?;
        let (type_id, resolved_origin) = self.module_augmentation_runtime_export_type_with_origin(
            &provenance.runtime_module_specifier,
            &provenance.runtime_import_name,
        )?;
        let origin = if provenance.origin == ModuleAugmentationRuntimeOrigin::ReplayFallback {
            ModuleAugmentationRuntimeOrigin::ReplayFallback
        } else {
            resolved_origin
        };
        Some(NamedImportAugmentationRuntimeBinding {
            module_specifier: provenance.module_specifier,
            import_name: provenance.import_name,
            binding_is_type_only: provenance.binding_is_type_only,
            type_id,
            origin,
        })
    }

    /// Extract the runtime members contributed by one enum or namespace
    /// augmentation declaration.
    ///
    /// The declaration is interpreted in its owning `arena`: foreign
    /// declarations delegate through
    /// [`Self::augmentation_export_declaration_type`] so explicit variable
    /// annotations and declaration symbols are resolved by the owning binder.
    /// Type-only declarations (`interface`, `type`, and other non-value
    /// statements) are deliberately excluded. If a runtime declaration cannot
    /// be typed, it is omitted rather than widened to `any`.
    ///
    /// `declaration_order` is shared with the caller's other augmentation
    /// members and is advanced only for properties actually returned.
    pub(crate) fn module_augmentation_runtime_value_members(
        &mut self,
        declaration: NodeIndex,
        arena: &NodeArena,
        declaration_order: &mut u32,
    ) -> Vec<PropertyInfo> {
        use tsz_parser::parser::syntax_kind_ext::{ENUM_DECLARATION, MODULE_DECLARATION};

        let Some(node) = arena.get(declaration) else {
            return Vec::new();
        };

        match node.kind {
            ENUM_DECLARATION => {
                self.module_augmentation_enum_runtime_members(declaration, arena, declaration_order)
            }
            MODULE_DECLARATION => self.module_augmentation_namespace_runtime_members(
                declaration,
                arena,
                declaration_order,
            ),
            _ => Vec::new(),
        }
    }

    fn module_augmentation_enum_runtime_members(
        &mut self,
        declaration: NodeIndex,
        arena: &NodeArena,
        declaration_order: &mut u32,
    ) -> Vec<PropertyInfo> {
        let Some(enum_decl) = arena.get(declaration).and_then(|node| arena.get_enum(node)) else {
            return Vec::new();
        };
        let member_indices = enum_decl.members.nodes.clone();
        let mut properties = Vec::with_capacity(member_indices.len());

        for member_idx in member_indices {
            let Some(member) = arena
                .get(member_idx)
                .and_then(|node| arena.get_enum_member(node))
            else {
                continue;
            };
            let name_idx = member.name;
            let Some(property) = self.module_augmentation_runtime_property(
                member_idx,
                name_idx,
                arena,
                true,
                false,
                declaration_order,
            ) else {
                continue;
            };
            properties.push(property);
        }

        properties
    }

    fn module_augmentation_namespace_runtime_members(
        &mut self,
        declaration: NodeIndex,
        arena: &NodeArena,
        declaration_order: &mut u32,
    ) -> Vec<PropertyInfo> {
        let Some(namespace_type) = self.augmentation_export_declaration_type(declaration, arena)
        else {
            return Vec::new();
        };
        let Some(shape) = crate::query_boundaries::module_augmentation::object_shape(
            self.ctx.types,
            namespace_type,
        ) else {
            return Vec::new();
        };

        let mut properties_by_name: FxHashMap<_, _> = shape
            .properties
            .iter()
            .cloned()
            .map(|property| (property.name, property))
            .collect();
        let direct_value_declarations =
            self.module_augmentation_namespace_direct_value_declarations(declaration, arena);
        let mut seen = FxHashSet::default();
        let mut properties = Vec::new();
        for exported_name in
            self.module_augmentation_namespace_exported_value_names(declaration, arena)
        {
            let name = self.ctx.types.intern_string(&exported_name);
            if !seen.insert(name) {
                continue;
            }
            let mut property = if let Some(property) = properties_by_name.remove(&name) {
                property
            } else {
                // A bare named export in an ambient namespace disables the
                // binder's implicit export-table population. Recover only
                // direct value declarations here; named export aliases remain
                // shape-owned so type-only aliases cannot acquire a runtime
                // property through this fallback.
                let Some(&direct_declaration) = direct_value_declarations.get(&exported_name)
                else {
                    continue;
                };
                let Some(type_id) =
                    self.augmentation_export_declaration_type(direct_declaration, arena)
                else {
                    continue;
                };
                crate::query_boundaries::module_augmentation::augmentation_member_property(
                    name, type_id, false, false, false, 0,
                )
            };
            *declaration_order = declaration_order.saturating_add(1);
            property.declaration_order = *declaration_order;
            properties.push(property);
        }
        properties
    }

    /// Index direct runtime declarations by the value name they introduce.
    ///
    /// This intentionally excludes named export specifiers: their exported
    /// name can differ from the local declaration and may refer to a type-only
    /// symbol, so only the binder-built namespace shape may materialize them.
    fn module_augmentation_namespace_direct_value_declarations(
        &self,
        declaration: NodeIndex,
        arena: &NodeArena,
    ) -> FxHashMap<String, NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::{MODULE_BLOCK, MODULE_DECLARATION};

        let mut declarations = FxHashMap::default();
        let Some(module) = arena
            .get(declaration)
            .filter(|node| node.kind == MODULE_DECLARATION)
            .and_then(|node| arena.get_module(node))
        else {
            return declarations;
        };
        let Some(body) = arena.get(module.body) else {
            return declarations;
        };
        if body.kind == MODULE_DECLARATION {
            self.module_augmentation_direct_value_declarations(
                module.body,
                arena,
                &mut declarations,
            );
            return declarations;
        }
        let Some(block) = (body.kind == MODULE_BLOCK)
            .then(|| arena.get_module_block(body))
            .flatten()
        else {
            return declarations;
        };
        if let Some(statements) = block.statements.as_ref() {
            for &statement in &statements.nodes {
                self.module_augmentation_direct_value_declarations(
                    statement,
                    arena,
                    &mut declarations,
                );
            }
        }
        declarations
    }

    fn module_augmentation_direct_value_declarations(
        &self,
        declaration: NodeIndex,
        arena: &NodeArena,
        declarations: &mut FxHashMap<String, NodeIndex>,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, ENUM_DECLARATION, EXPORT_DECLARATION, FUNCTION_DECLARATION,
            IMPORT_EQUALS_DECLARATION, MODULE_DECLARATION, VARIABLE_DECLARATION_LIST,
            VARIABLE_STATEMENT,
        };

        let Some(node) = arena.get(declaration) else {
            return;
        };
        match node.kind {
            EXPORT_DECLARATION => {
                let Some(export) = arena.get_export_decl(node) else {
                    return;
                };
                if !export.is_type_only {
                    self.module_augmentation_direct_value_declarations(
                        export.export_clause,
                        arena,
                        declarations,
                    );
                }
            }
            VARIABLE_STATEMENT => {
                let Some(statement) = arena.get_variable(node) else {
                    return;
                };
                for &list in &statement.declarations.nodes {
                    let Some(list_node) = arena.get(list) else {
                        continue;
                    };
                    if list_node.kind != VARIABLE_DECLARATION_LIST {
                        continue;
                    }
                    let Some(list) = arena.get_variable(list_node) else {
                        continue;
                    };
                    for &variable_declaration in &list.declarations.nodes {
                        let Some(name) = arena
                            .get(variable_declaration)
                            .and_then(|node| arena.get_variable_declaration(node))
                            .map(|declaration| declaration.name)
                        else {
                            continue;
                        };
                        let mut names = Vec::new();
                        Self::module_augmentation_binding_names(arena, name, &mut names);
                        for name in names {
                            declarations.entry(name).or_insert(variable_declaration);
                        }
                    }
                }
            }
            MODULE_DECLARATION => {
                if self.module_augmentation_namespace_is_value_instantiated(declaration, arena)
                    && let Some(name) =
                        Self::module_augmentation_declaration_name(arena, declaration)
                {
                    declarations.entry(name).or_insert(declaration);
                }
            }
            FUNCTION_DECLARATION | CLASS_DECLARATION | ENUM_DECLARATION => {
                if let Some(name) = Self::module_augmentation_declaration_name(arena, declaration) {
                    declarations.entry(name).or_insert(declaration);
                }
            }
            IMPORT_EQUALS_DECLARATION => {
                if arena
                    .get_import_decl(node)
                    .is_some_and(|import| !import.is_type_only)
                    && let Some(name) =
                        Self::module_augmentation_declaration_name(arena, declaration)
                {
                    declarations.entry(name).or_insert(declaration);
                }
            }
            _ => {}
        }
    }

    /// Walk one namespace declaration's exported statements in source order.
    ///
    /// The namespace object type owns value typing and declaration merging, but
    /// its interned property storage is canonical rather than source ordered.
    /// Reading the syntax here also distinguishes a local alias source from the
    /// name it actually exports (`export { local as public }`). Direct value
    /// declarations in an ambient namespace are implicitly exported, while a
    /// non-ambient namespace still requires an explicit export.
    fn module_augmentation_namespace_exported_value_names(
        &self,
        declaration: NodeIndex,
        arena: &NodeArena,
    ) -> Vec<String> {
        use tsz_parser::parser::syntax_kind_ext::{MODULE_BLOCK, MODULE_DECLARATION};

        let Some(module) = arena
            .get(declaration)
            .filter(|node| node.kind == MODULE_DECLARATION)
            .and_then(|node| arena.get_module(node))
        else {
            return Vec::new();
        };
        let Some(body) = arena.get(module.body) else {
            return Vec::new();
        };
        if body.kind == MODULE_DECLARATION {
            return self
                .module_augmentation_namespace_is_value_instantiated(module.body, arena)
                .then(|| Self::module_augmentation_declaration_name(arena, module.body))
                .flatten()
                .into_iter()
                .collect();
        }
        let Some(block) = (body.kind == MODULE_BLOCK)
            .then(|| arena.get_module_block(body))
            .flatten()
        else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let implicitly_exports_members = arena.is_in_ambient_context(declaration);
        if let Some(statements) = block.statements.as_ref() {
            for &statement in &statements.nodes {
                if implicitly_exports_members {
                    self.module_augmentation_exported_declaration_names(
                        statement, arena, &mut names,
                    );
                } else {
                    self.module_augmentation_exported_statement_names(statement, arena, &mut names);
                }
            }
        }
        names
    }

    fn module_augmentation_exported_statement_names(
        &self,
        statement: NodeIndex,
        arena: &NodeArena,
        names: &mut Vec<String>,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{EXPORT_DECLARATION, NAMED_EXPORTS};

        let Some(node) = arena.get(statement) else {
            return;
        };
        if node.kind != EXPORT_DECLARATION {
            return;
        }
        let Some(export) = arena.get_export_decl(node) else {
            return;
        };
        if export.is_type_only {
            return;
        }
        if export.is_default_export {
            names.push("default".to_string());
            return;
        }
        let Some(clause) = arena.get(export.export_clause) else {
            return;
        };
        if clause.kind == NAMED_EXPORTS {
            let Some(named) = arena.get_named_imports(clause) else {
                return;
            };
            for &specifier in &named.elements.nodes {
                let Some(specifier) = arena
                    .get(specifier)
                    .and_then(|node| arena.get_specifier(node))
                else {
                    continue;
                };
                if !specifier.is_type_only
                    && let Some(name) = Self::module_augmentation_name_text(arena, specifier.name)
                {
                    names.push(name);
                }
            }
            return;
        }
        self.module_augmentation_exported_declaration_names(export.export_clause, arena, names);
    }

    fn module_augmentation_exported_declaration_names(
        &self,
        declaration: NodeIndex,
        arena: &NodeArena,
        names: &mut Vec<String>,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, ENUM_DECLARATION, EXPORT_DECLARATION, FUNCTION_DECLARATION,
            IMPORT_EQUALS_DECLARATION, MODULE_DECLARATION, VARIABLE_DECLARATION_LIST,
            VARIABLE_STATEMENT,
        };

        let Some(node) = arena.get(declaration) else {
            return;
        };
        match node.kind {
            EXPORT_DECLARATION => {
                self.module_augmentation_exported_statement_names(declaration, arena, names);
            }
            VARIABLE_STATEMENT => {
                let Some(statement) = arena.get_variable(node) else {
                    return;
                };
                for &list in &statement.declarations.nodes {
                    let Some(list_node) = arena.get(list) else {
                        continue;
                    };
                    if list_node.kind == VARIABLE_DECLARATION_LIST {
                        let Some(list) = arena.get_variable(list_node) else {
                            continue;
                        };
                        for &declaration in &list.declarations.nodes {
                            let Some(name) = arena
                                .get(declaration)
                                .and_then(|node| arena.get_variable_declaration(node))
                                .map(|declaration| declaration.name)
                            else {
                                continue;
                            };
                            Self::module_augmentation_binding_names(arena, name, names);
                        }
                    }
                }
            }
            MODULE_DECLARATION => {
                if self.module_augmentation_namespace_is_value_instantiated(declaration, arena)
                    && let Some(name) =
                        Self::module_augmentation_declaration_name(arena, declaration)
                {
                    names.push(name);
                }
            }
            FUNCTION_DECLARATION
            | CLASS_DECLARATION
            | ENUM_DECLARATION
            | IMPORT_EQUALS_DECLARATION => {
                if let Some(name) = Self::module_augmentation_declaration_name(arena, declaration) {
                    names.push(name);
                }
            }
            _ => {}
        }
    }

    fn module_augmentation_declaration_name(
        arena: &NodeArena,
        declaration: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, ENUM_DECLARATION, FUNCTION_DECLARATION, IMPORT_EQUALS_DECLARATION,
            MODULE_DECLARATION,
        };

        let node = arena.get(declaration)?;
        let name = match node.kind {
            FUNCTION_DECLARATION => arena.get_function(node)?.name,
            CLASS_DECLARATION => arena.get_class(node)?.name,
            ENUM_DECLARATION => arena.get_enum(node)?.name,
            MODULE_DECLARATION => arena.get_module(node)?.name,
            IMPORT_EQUALS_DECLARATION => arena.get_import_decl(node)?.import_clause,
            _ => return None,
        };
        Self::module_augmentation_name_text(arena, name)
    }

    fn module_augmentation_binding_names(
        arena: &NodeArena,
        name: NodeIndex,
        names: &mut Vec<String>,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{
            ARRAY_BINDING_PATTERN, BINDING_ELEMENT, OBJECT_BINDING_PATTERN,
        };

        let Some(node) = arena.get(name) else {
            return;
        };
        if let Some(name) = Self::module_augmentation_name_text(arena, name) {
            names.push(name);
            return;
        }
        if node.kind == BINDING_ELEMENT {
            if let Some(binding) = arena.get_binding_element(node) {
                Self::module_augmentation_binding_names(arena, binding.name, names);
            }
        } else if node.kind == OBJECT_BINDING_PATTERN || node.kind == ARRAY_BINDING_PATTERN {
            for child in arena.get_children(name) {
                Self::module_augmentation_binding_names(arena, child, names);
            }
        }
    }

    fn module_augmentation_name_text(arena: &NodeArena, name: NodeIndex) -> Option<String> {
        arena
            .get_identifier_text(name)
            .map(str::to_string)
            .or_else(|| {
                arena
                    .get(name)
                    .and_then(|node| arena.get_literal(node))
                    .map(|literal| literal.text.clone())
            })
    }

    pub(crate) fn normalize_module_augmentation_runtime_value_members(
        &mut self,
        properties: Vec<PropertyInfo>,
    ) -> Vec<PropertyInfo> {
        let mut positions = FxHashMap::default();
        let mut normalized: Vec<PropertyInfo> = Vec::with_capacity(properties.len());
        for property in properties {
            let Some(&position) = positions.get(&property.name) else {
                positions.insert(property.name, normalized.len());
                normalized.push(property);
                continue;
            };
            let previous = normalized[position].clone();
            let mut merged = self.merge_properties(
                std::slice::from_ref(&property),
                std::slice::from_ref(&previous),
                crate::interface_type::InterfaceMergeMode::Declaration,
            );
            if let Some(mut merged) = merged.pop() {
                merged.declaration_order =
                    previous.declaration_order.min(property.declaration_order);
                normalized[position] = merged;
            }
        }
        normalized
    }

    fn module_augmentation_runtime_property(
        &mut self,
        declaration: NodeIndex,
        name: NodeIndex,
        arena: &NodeArena,
        readonly: bool,
        is_method: bool,
        declaration_order: &mut u32,
    ) -> Option<PropertyInfo> {
        let name = self.augmentation_member_key_name(arena, name)?;
        let type_id = self.augmentation_export_declaration_type(declaration, arena)?;
        *declaration_order = declaration_order.saturating_add(1);
        Some(
            crate::query_boundaries::module_augmentation::augmentation_member_property(
                self.ctx.types.intern_string(&name),
                type_id,
                false,
                readonly,
                is_method,
                *declaration_order,
            ),
        )
    }

    pub(crate) fn module_augmentation_value_type(
        &mut self,
        augmentations: &[ModuleAugmentation],
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        if let Some(function_type) = self.module_augmentation_function_value_type(augmentations) {
            return Some(function_type);
        }

        // A concrete value declaration outranks a same-name namespace
        // companion regardless of source order. Resolve namespaces only after
        // every concrete candidate has failed so a discarded namespace cannot
        // publish symbol-instance/cache state during probing.
        for augmentation in augmentations {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(node) = arena.get(augmentation.node) else {
                continue;
            };
            if matches!(
                node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
            ) && let Some(type_id) =
                self.augmentation_export_declaration_type(augmentation.node, arena)
            {
                return Some(type_id);
            }
        }

        for augmentation in augmentations {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(node) = arena.get(augmentation.node) else {
                continue;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && self
                    .module_augmentation_namespace_is_value_instantiated(augmentation.node, arena)
                && let Some(type_id) =
                    self.augmentation_export_declaration_type(augmentation.node, arena)
            {
                return Some(type_id);
            }
        }

        None
    }

    /// Resolve only runtime function declaration groups from an authoritative
    /// augmentation declaration set.
    ///
    /// Same-name interfaces have an independent type-side callable surface and
    /// must never supply overloads for the JavaScript function value.
    pub(crate) fn module_augmentation_function_value_type(
        &mut self,
        augmentations: &[ModuleAugmentation],
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut merged_function_type = None;
        let mut seen_function_groups = FxHashSet::default();
        for augmentation in augmentations {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(node) = arena.get(augmentation.node) else {
                continue;
            };
            if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }

            let binder = self.ctx.get_binder_for_arena(arena);
            let owner_file_idx = self
                .ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx);
            let group_identity = binder.and_then(|binder| {
                binder
                    .get_node_symbol(augmentation.node)
                    .map(|symbol| (owner_file_idx, symbol))
            });
            if group_identity.is_some_and(|identity| !seen_function_groups.insert(identity)) {
                continue;
            }

            // Resolve against the augmentation's owner arena/binder. A
            // current-arena read of a foreign declaration can reuse the same
            // numeric `NodeIndex` for an unrelated node.
            if let Some(type_id) =
                self.augmentation_export_declaration_type(augmentation.node, arena)
            {
                merged_function_type = Some(merged_function_type.map_or(type_id, |previous| {
                    // Later declaration groups take overload precedence,
                    // matching `tsc` program order.
                    self.merge_interface_types_augmentation(type_id, previous)
                }));
            }
        }
        merged_function_type
    }

    /// Resolve the exact runtime export introduced by a module augmentation.
    ///
    /// A direct value declaration supplies the base (`function`, `class`,
    /// `enum`, or variable). An augmentation-only namespace value starts from
    /// the neutral empty declaration space and receives only its runtime
    /// members. Pure type-space declarations therefore return `None` instead of
    /// becoming an unsound `any` property on namespace imports.
    pub(crate) fn module_augmentation_runtime_export_type(
        &mut self,
        module_spec: &str,
        name: &str,
    ) -> Option<TypeId> {
        self.module_augmentation_runtime_export_type_with_origin(module_spec, name)
            .map(|(type_id, _)| type_id)
    }

    pub(crate) fn module_augmentation_runtime_export_type_with_origin(
        &mut self,
        module_spec: &str,
        name: &str,
    ) -> Option<(TypeId, ModuleAugmentationRuntimeOrigin)> {
        let (runtime_declarations, has_direct_value, has_runtime_value) =
            self.module_augmentation_runtime_declarations_with_direct_value(module_spec, name);
        if runtime_declarations.is_empty() || !has_runtime_value {
            return None;
        }
        let origin = if has_direct_value {
            ModuleAugmentationRuntimeOrigin::DirectTarget
        } else {
            ModuleAugmentationRuntimeOrigin::ReplayFallback
        };

        let publication_transaction = self.begin_module_augmentation_publication();
        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        let direct_value = self.module_augmentation_value_type(&runtime_declarations);
        let empty = crate::query_boundaries::module_augmentation::empty_declaration_space_type(
            self.ctx.types,
        );
        let base = direct_value.unwrap_or(empty);
        let augmented = self.apply_module_runtime_value_augmentations(
            module_spec,
            name,
            base,
            direct_value.is_some(),
            &runtime_declarations,
        );

        if Self::cross_arena_bailout_epoch() != bailout_epoch_before {
            self.rollback_module_augmentation_publication(publication_transaction);
            return Some((TypeId::ANY, origin));
        }

        self.commit_module_augmentation_publication(publication_transaction);
        (direct_value.is_some() || augmented != empty).then_some((augmented, origin))
    }

    /// Resolve the declared value type of an augmentation export declaration
    /// `node`, honoring the `arena` that owns it.
    ///
    /// Returns `None` when no concrete runtime type can be recovered; callers
    /// must omit that value surface. For a same-arena declaration this resolves
    /// directly; for a foreign-arena declaration it constructs a
    /// transient delegate child checker over the owning arena/binder so type
    /// references in the declaration resolve against the correct symbol table.
    pub(crate) fn augmentation_export_declaration_type(
        &mut self,
        node: NodeIndex,
        arena: &tsz_parser::parser::NodeArena,
    ) -> Option<TypeId> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.augmentation_node_value_type_local(node);
        }

        // O(1) via global_arena_index; falls back to None when the arena is not
        // part of the program overlay (e.g. lib arenas), in which case the
        // current binder is the best available interpreter.
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let delegate_binder_arc = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let delegate_binder = delegate_binder_arc.as_deref()?;

        let _cross_arena_guard = Self::enter_cross_arena_delegation()?;
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            return None;
        }

        let delegate_file_name = arena
            .source_files
            .first()
            .map_or_else(|| self.ctx.file_name.clone(), |sf| sf.file_name.clone());

        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = CheckerState::delegate_for_arena(
            arena,
            delegate_binder,
            delegate_file_name,
            self,
            CheckerCreationReason::ModuleAugmentationValue,
        );
        let preserve_symbol = delegate_binder
            .get_node_symbol(node)
            .unwrap_or(tsz_binder::SymbolId(u32::MAX));
        self.clear_delegated_symbol_cache_collisions(
            &mut checker,
            delegate_binder,
            preserve_symbol,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);

        let result = checker.augmentation_node_value_type_local(node);

        self.ctx.leave_recursion();

        result
    }

    /// Resolve an augmentation enum in type position through its exact owner
    /// arena. The terminal symbol owner is scoped inside the delegate so a
    /// same-number consumer alias cannot affect namespace qualification.
    pub(crate) fn augmentation_enum_declaration_type(
        &mut self,
        node: NodeIndex,
        arena: &tsz_parser::parser::NodeArena,
    ) -> Option<TypeId> {
        if arena
            .get(node)
            .is_none_or(|node| node.kind != tsz_parser::parser::syntax_kind_ext::ENUM_DECLARATION)
        {
            return None;
        }
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.augmentation_enum_declaration_type_local(node);
        }

        let delegate_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let delegate_binder = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned())?;
        let preserve_symbol = delegate_binder.get_node_symbol(node)?;
        let _cross_arena_guard = Self::enter_cross_arena_delegation()?;
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            return None;
        }

        let delegate_file_name = arena.source_files.first().map_or_else(
            || self.ctx.file_name.clone(),
            |source| source.file_name.clone(),
        );
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
        let mut checker = CheckerState::delegate_for_arena(
            arena,
            delegate_binder.as_ref(),
            delegate_file_name,
            self,
            CheckerCreationReason::ModuleAugmentationValue,
        );
        self.clear_delegated_symbol_cache_collisions(
            &mut checker,
            delegate_binder.as_ref(),
            preserve_symbol,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        let result = checker.augmentation_enum_declaration_type_local(node);

        self.ctx.leave_recursion();
        result
    }

    fn augmentation_enum_declaration_type_local(&mut self, node: NodeIndex) -> Option<TypeId> {
        let symbol_id = self.ctx.binder.get_node_symbol(node)?;
        let previous_owner = self.ctx.local_symbol_file_target_override(symbol_id);
        self.ctx
            .register_symbol_file_target(symbol_id, self.ctx.current_file_idx);
        let enum_type = self.get_type_of_symbol(symbol_id);
        self.ctx
            .restore_local_symbol_file_target_override(symbol_id, previous_owner);
        (enum_type != TypeId::ERROR && enum_type != TypeId::UNKNOWN).then_some(enum_type)
    }

    /// Resolve the value type of an augmentation export declaration `node`
    /// interpreted in the *current* checker's arena/binder.
    ///
    /// Prefers a variable declaration's explicit type annotation (matching the
    /// established same-file path), then falls back to the declared symbol's
    /// type, which uniformly covers `function`/`class`/`enum` declarations as
    /// the value they introduce.
    fn augmentation_node_value_type_local(&mut self, node: NodeIndex) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        // A type only "resolved" enough to use as the export's type when it is
        // neither the error nor the unevaluated sentinel.
        let concrete = |type_id: TypeId| {
            (type_id != TypeId::ERROR && type_id != TypeId::UNKNOWN).then_some(type_id)
        };

        let arena = self.ctx.arena;
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && arena
                .get(node)
                .is_some_and(|node| node.kind == syntax_kind_ext::ENUM_DECLARATION)
        {
            let enum_type = self.get_type_of_symbol(sym_id);
            return concrete(self.merge_namespace_exports_into_object(sym_id, enum_type));
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && arena
                .get(node)
                .is_some_and(|node| node.kind == syntax_kind_ext::MODULE_DECLARATION)
        {
            if !self.is_namespace_declaration_value_instantiated(node) {
                return None;
            }
            return concrete(
                self.build_namespace_object_type_without_symbol_instance_cache(sym_id),
            );
        }

        // A function's runtime value owns its complete declaration overload
        // group, but never the same-name interface's callable signatures.
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && arena
                .get(node)
                .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
            && let Some(type_id) = self.function_declaration_only_symbol_type(sym_id)
            && let Some(type_id) = concrete(type_id)
        {
            let type_id = if self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.has_any_flags(
                    tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE,
                )
            }) {
                self.merge_namespace_exports_into_function(sym_id, type_id)
                    .0
            } else {
                type_id
            };
            return Some(type_id);
        }

        // Resolve value declarations through the exact declaration before asking
        // for the merged symbol's canonical type. An interface and function can
        // share one symbol, while their callable type-side and runtime value-side
        // signatures remain distinct.
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && arena.get(node).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::VARIABLE_DECLARATION | syntax_kind_ext::CLASS_DECLARATION
                )
            })
            && let Some(type_id) = concrete(
                self.type_of_value_declaration_for_symbol_without_module_augmentations(
                    sym_id, node,
                ),
            )
        {
            let type_id = if arena
                .get(node)
                .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
                && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(
                        tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE,
                    )
                }) {
                self.merge_namespace_exports_into_constructor(sym_id, type_id)
            } else {
                type_id
            };
            return Some(type_id);
        }

        // Enum and namespace declarations need their symbol's runtime object
        // surface; their raw declaration-node type is not that value type.
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(node)
            && let Some(type_id) = concrete(self.get_type_of_symbol(sym_id))
        {
            return Some(type_id);
        }

        // Preserve the established declaration-node fallback for parser
        // recovery shapes that do not carry a direct binder symbol.
        concrete(self.get_type_of_node(node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerContext, CheckerOptions};
    use crate::module_resolution::build_module_resolution_maps;
    use crate::query_boundaries::state::type_resolution::string_literal_value;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    fn parse_and_bind(file_name: &str, source: &str) -> (Arc<NodeArena>, Arc<BinderState>) {
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        (Arc::new(parser.into_arena()), Arc::new(binder))
    }

    #[test]
    fn runtime_namespace_members_are_value_only_and_foreign_types_are_preserved() {
        let fixtures = [
            ("home.ts", "export class Surface {}"),
            (
                "augmentation.ts",
                r#"
import "./home";
declare module "./home" {
    namespace Surface {
        const payload: "payload";
        export const key: unique symbol;
        function invoke(value: number): string;
        export interface CallableExport {
            (value: "type-only"): "type-only";
            instanceOnly: true;
        }
        export function CallableExport(value: string): string;
        export function CallableExport(value: number): number;
        export function CallableExport(value: string | number): string | number {
            return value;
        }
        export namespace CallableExport {
            export const staticOnly: "callable-static";
        }
        export class Nested {
            static stamp: "stamp";
            member: string;
        }
        export namespace Nested {
            export const companion: "class-static";
        }
        export enum Choice { First = "first" }
        export namespace Inner { export const marker: true; }
        export namespace EmptyInner {}
        export namespace TypeOnlyInner { export interface Marker {} }
        export namespace TypeOnlyAliasInner {
            type Marker = string;
            export { Marker };
        }
        const implicitOnly: "implicit";
        const aliasTarget: "alias";
        export { aliasTarget as renamed };
        type TypeAliasTarget = string;
        export { TypeAliasTarget as TypeOnlyNamed };
        export interface TypeOnly {}
        export type AliasOnly = string;
    }
}
"#,
            ),
            ("consumer.ts", "export interface Decoy { value: false }"),
        ];

        let mut arenas = Vec::new();
        let mut binders = Vec::new();
        for (file_name, source) in fixtures {
            let (arena, binder) = parse_and_bind(file_name, source);
            arenas.push(arena);
            binders.push(binder);
        }
        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let file_names = vec![
            "home.ts".to_string(),
            "augmentation.ts".to_string(),
            "consumer.ts".to_string(),
        ];
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let types = TypeInterner::new();
        let mut checker = CheckerState {
            ctx: CheckerContext::new(
                all_arenas[2].as_ref(),
                all_binders[2].as_ref(),
                &types,
                file_names[2].clone(),
                CheckerOptions::default(),
            ),
        };
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(2);
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        let namespace_decl = all_binders[1]
            .module_augmentations
            .get("./home")
            .and_then(|entries| entries.iter().find(|entry| entry.name == "Surface"))
            .map(|entry| entry.node)
            .expect("Surface namespace augmentation");
        assert!(
            all_arenas[1].is_in_ambient_context(namespace_decl),
            "a namespace nested in a `declare module` must use ambient export semantics"
        );
        let nested_namespace = |name: &str| {
            all_binders[1]
                .get_symbols()
                .find_all_by_name(name)
                .iter()
                .filter_map(|&symbol_id| all_binders[1].get_symbol(symbol_id))
                .flat_map(|symbol| symbol.declarations.iter().copied())
                .find(|&declaration| {
                    all_arenas[1].get(declaration).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                    })
                })
                .unwrap_or_else(|| panic!("missing nested namespace {name}"))
        };
        assert!(checker.module_augmentation_namespace_is_value_instantiated(
            nested_namespace("Inner"),
            all_arenas[1].as_ref(),
        ));
        let type_only_alias_namespace = nested_namespace("TypeOnlyAliasInner");
        let namespace_module = all_arenas[1]
            .get(type_only_alias_namespace)
            .and_then(|node| all_arenas[1].get_module(node))
            .expect("type-only alias namespace module");
        let namespace_block = all_arenas[1]
            .get(namespace_module.body)
            .and_then(|node| all_arenas[1].get_module_block(node))
            .expect("type-only alias namespace block");
        let export_statement = namespace_block
            .statements
            .as_ref()
            .and_then(|statements| statements.nodes.last())
            .and_then(|&statement| all_arenas[1].get(statement))
            .and_then(|node| all_arenas[1].get_export_decl(node))
            .expect("type-only alias named export");
        let named_exports = all_arenas[1]
            .get(export_statement.export_clause)
            .and_then(|node| all_arenas[1].get_named_imports(node))
            .expect("named exports");
        let export_specifier = named_exports
            .elements
            .nodes
            .first()
            .and_then(|&specifier| all_arenas[1].get(specifier))
            .and_then(|node| all_arenas[1].get_specifier(node))
            .expect("export specifier");
        let exported_symbol_id = CheckerState::local_named_export_target_symbol_in_owner(
            all_arenas[1].as_ref(),
            all_binders[1].as_ref(),
            export_specifier.name,
        )
        .expect("exported local symbol");
        let exported_symbol = all_binders[1]
            .get_symbol(exported_symbol_id)
            .expect("exported local symbol metadata");
        assert!(
            exported_symbol.is_pure_type(),
            "a local named export of a type alias must resolve to its pure-type target, got {exported_symbol:?}"
        );
        assert!(
            !checker.module_augmentation_namespace_is_value_instantiated(
                type_only_alias_namespace,
                all_arenas[1].as_ref(),
            )
        );

        let mut declaration_order = 40;
        let properties = checker.module_augmentation_runtime_value_members(
            namespace_decl,
            all_arenas[1].as_ref(),
            &mut declaration_order,
        );
        let names: Vec<_> = properties
            .iter()
            .map(|property| types.resolve_atom(property.name))
            .collect();

        assert_eq!(
            names,
            [
                "payload",
                "key",
                "invoke",
                "CallableExport",
                "Nested",
                "Choice",
                "Inner",
                "implicitOnly",
                "aliasTarget",
                "renamed",
            ],
            "runtime extraction must include explicit and implicitly exported ambient \
             value declarations while excluding interface/type-alias declarations"
        );
        assert_eq!(
            properties
                .iter()
                .map(|property| property.declaration_order)
                .collect::<Vec<_>>(),
            [41, 42, 43, 44, 45, 46, 47, 48, 49, 50]
        );
        assert_eq!(declaration_order, 50);
        assert!(
            properties.iter().all(|property| !property.is_method),
            "namespace exports are runtime value properties, not interface methods"
        );
        assert!(
            properties
                .iter()
                .all(|property| property.type_id != TypeId::ERROR
                    && property.type_id != TypeId::UNKNOWN),
            "runtime declarations must retain concrete owner-arena types"
        );
        assert_eq!(
            string_literal_value(&types, properties[0].type_id)
                .map(|atom| types.resolve_atom(atom)),
            Some("payload".to_string()),
            "a foreign typed variable must retain its declared literal type"
        );
        assert!(
            crate::query_boundaries::type_predicates::is_unique_symbol_type(
                &types,
                properties[1].type_id,
            ),
            "a namespace `const` annotated `unique symbol` must retain its declaration identity"
        );

        let property_type = |name: &str| {
            properties
                .iter()
                .find(|property| types.resolve_atom(property.name) == name)
                .map(|property| property.type_id)
                .unwrap_or_else(|| panic!("missing runtime property {name}"))
        };
        assert_eq!(
            string_literal_value(&types, property_type("implicitOnly"))
                .map(|atom| types.resolve_atom(atom)),
            Some("implicit".to_string()),
            "an implicitly exported ambient value must retain its owner-arena type"
        );

        let function_type = property_type("invoke");
        assert!(
            crate::query_boundaries::module_augmentation::call_signatures(&types, function_type)
                .is_some_and(|signatures| !signatures.is_empty()),
            "a namespace function must retain its callable value type"
        );

        let merged_function_type = property_type("CallableExport");
        let merged_function_signatures =
            crate::query_boundaries::module_augmentation::call_signatures(
                &types,
                merged_function_type,
            )
            .expect("merged function value must stay callable");
        assert_eq!(
            merged_function_signatures.len(),
            2,
            "the complete runtime overload set must survive without the callable interface signature"
        );
        assert!(
            merged_function_signatures.iter().any(|signature| {
                signature.params.first().is_some_and(|param| {
                    crate::query_boundaries::property_access::is_string_type(&types, param.type_id)
                })
            }) && merged_function_signatures.iter().any(|signature| {
                signature.params.first().is_some_and(|param| {
                    crate::query_boundaries::property_access::is_number_type(&types, param.type_id)
                })
            }),
            "the runtime function must retain both its string and number overloads"
        );
        assert!(!matches!(
            crate::query_boundaries::property_access::resolve_property_access(
                &types,
                merged_function_type,
                types.intern_string("instanceOnly"),
            ),
            crate::query_boundaries::property_access::PropertyAccessResult::Success { .. }
        ));
        let callable_static_type =
            match crate::query_boundaries::property_access::resolve_property_access(
                &types,
                merged_function_type,
                types.intern_string("staticOnly"),
            ) {
                crate::query_boundaries::property_access::PropertyAccessResult::Success {
                    type_id,
                    ..
                } => type_id,
                result => {
                    panic!("CallableExport.staticOnly must survive the function merge: {result:?}")
                }
            };
        assert_eq!(
            string_literal_value(&types, callable_static_type).map(|atom| types.resolve_atom(atom)),
            Some("callable-static".to_string())
        );

        let class_type = property_type("Nested");
        assert!(
            crate::query_boundaries::checkers::constructor::has_construct_signatures(
                &types, class_type,
            ),
            "a nested class must contribute its constructor/static value, not its instance type"
        );
        let stamp_type = match crate::query_boundaries::property_access::resolve_property_access(
            &types,
            class_type,
            types.intern_string("stamp"),
        ) {
            crate::query_boundaries::property_access::PropertyAccessResult::Success {
                type_id,
                ..
            } => type_id,
            result => panic!("Nested.stamp must resolve on the class value: {result:?}"),
        };
        assert_eq!(
            string_literal_value(&types, stamp_type).map(|atom| types.resolve_atom(atom)),
            Some("stamp".to_string())
        );
        let companion_type = match crate::query_boundaries::property_access::resolve_property_access(
            &types,
            class_type,
            types.intern_string("companion"),
        ) {
            crate::query_boundaries::property_access::PropertyAccessResult::Success {
                type_id,
                ..
            } => type_id,
            result => panic!("Nested.companion must survive the class merge: {result:?}"),
        };
        assert_eq!(
            string_literal_value(&types, companion_type).map(|atom| types.resolve_atom(atom)),
            Some("class-static".to_string())
        );

        let enum_type = property_type("Choice");
        assert!(matches!(
            crate::query_boundaries::property_access::resolve_property_access(
                &types,
                enum_type,
                types.intern_string("First"),
            ),
            crate::query_boundaries::property_access::PropertyAccessResult::Success { .. }
        ));

        let namespace_type = property_type("Inner");
        assert!(matches!(
            crate::query_boundaries::property_access::resolve_property_access(
                &types,
                namespace_type,
                types.intern_string("marker"),
            ),
            crate::query_boundaries::property_access::PropertyAccessResult::Success { .. }
        ));

        assert_eq!(
            string_literal_value(&types, property_type("renamed"))
                .map(|atom| types.resolve_atom(atom)),
            Some("alias".to_string()),
            "a namespace export alias must expose the target value under its exported name"
        );
        assert_eq!(
            string_literal_value(&types, property_type("aliasTarget"))
                .map(|atom| types.resolve_atom(atom)),
            Some("alias".to_string()),
            "the ambient alias source remains implicitly exported under its declaration name"
        );
    }

    #[test]
    fn runtime_enum_members_are_readonly_and_keep_declared_member_types() {
        let (arena, binder) = parse_and_bind(
            "augmentation.ts",
            r#"
export {};
declare module "./home" {
    enum Registry {
        Alpha = "alpha",
        Beta = "beta",
    }
}
"#,
        );
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "augmentation.ts".to_string(),
            CheckerOptions::default(),
        );
        let enum_decl = binder
            .module_augmentations
            .get("./home")
            .and_then(|entries| entries.iter().find(|entry| entry.name == "Registry"))
            .map(|entry| entry.node)
            .expect("Registry enum augmentation");

        let mut declaration_order = 0;
        let properties = checker.module_augmentation_runtime_value_members(
            enum_decl,
            arena.as_ref(),
            &mut declaration_order,
        );
        let names: Vec<_> = properties
            .iter()
            .map(|property| types.resolve_atom(property.name))
            .collect();

        assert_eq!(names, ["Alpha", "Beta"]);
        assert!(properties.iter().all(|property| property.readonly));
        assert!(
            properties
                .iter()
                .all(|property| property.type_id != TypeId::ANY
                    && property.type_id != TypeId::ERROR
                    && property.type_id != TypeId::UNKNOWN),
            "enum value members must use their real member types"
        );
        assert_eq!(declaration_order, 2);
    }
}
