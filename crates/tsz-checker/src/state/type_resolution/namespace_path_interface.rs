//! Exact interface lookup for namespace-import type paths.
//!
//! A raw binder `SymbolId` is not enough to identify `ns.Container.Shared`:
//! another file or namespace can use the same numeric id and terminal name.
//! This query selects declarations by resolved module, namespace path, and
//! terminal declaration, then returns the structural interface surface without
//! publishing it through terminal-only augmentation caches.

use crate::query_boundaries::module_augmentation as module_augmentation_boundary;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{ModuleAugmentation, SymbolId};
use tsz_common::perf_counters::CheckerCreationReason;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(crate) fn exact_namespace_import_type_path(
        &self,
        type_name_idx: NodeIndex,
    ) -> Option<(String, Vec<String>)> {
        let mut members = Vec::new();
        let root_idx =
            Self::flatten_namespace_import_type_path(self.ctx.arena, type_name_idx, &mut members)?;
        if members.is_empty() {
            return None;
        }

        let root_name = self.ctx.arena.get_identifier_text(root_idx)?;
        let root_symbol_id = self
            .ctx
            .binder
            .get_node_symbol(root_idx)
            .or_else(|| self.ctx.binder.file_locals.get(root_name))?;
        let root_symbol = self.ctx.binder.get_symbol(root_symbol_id)?;
        if root_symbol.escaped_name != root_name || root_symbol.import_name() != Some("*") {
            return None;
        }

        Some((root_symbol.import_module()?.to_string(), members))
    }

    pub(crate) fn exact_namespace_import_interface_type(
        &mut self,
        type_reference_idx: NodeIndex,
        type_name_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if !self.ctx.program_has_module_augmentations() {
            return None;
        }
        let (module_specifier, members) = self.exact_namespace_import_type_path(type_name_idx)?;
        let (terminal, namespace_path) = members.split_last()?;

        let expected_prefix = (!namespace_path.is_empty()).then(|| namespace_path.join("."));
        let target_augmentation_declarations =
            self.exact_module_augmentation_declarations(&module_specifier, terminal);
        if target_augmentation_declarations.is_empty() {
            return None;
        }
        let augmentation_declarations: Vec<_> = target_augmentation_declarations
            .into_iter()
            .filter(|declaration| {
                let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
                let Some(node) = arena.get(declaration.node) else {
                    return false;
                };
                let Some(interface) = arena.get_interface(node) else {
                    return false;
                };
                arena
                    .get_identifier_text(interface.name)
                    .is_some_and(|name| name == terminal)
                    && Self::lib_interface_namespace_prefix(&[(declaration.node, arena)])
                        == expected_prefix
            })
            .collect();
        let mut declarations =
            self.exact_native_interface_declarations(&module_specifier, namespace_path, terminal);
        declarations.extend(augmentation_declarations);
        for declaration in &mut declarations {
            if declaration.arena.as_deref().is_some_and(|arena| {
                arena.shares_node_storage_with(self.ctx.arena)
                    || self.ctx.get_file_idx_for_arena(arena) == Some(self.ctx.current_file_idx)
            }) {
                declaration.arena = None;
            }
        }

        let mut seen = FxHashSet::default();
        declarations.retain(|declaration| {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let file_idx = self
                .ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx);
            seen.insert((file_idx, declaration.node))
        });
        declarations.sort_by_key(|declaration| {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let file_idx = self
                .ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx);
            let position = arena
                .get(declaration.node)
                .map_or(u32::MAX, |node| node.pos);
            (file_idx, position)
        });

        let mut exact_type_parameters: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        for declaration in &declarations {
            let declaration_parameters = self.exact_interface_type_parameters(declaration)?;
            let Some(parameters) = exact_type_parameters.as_mut() else {
                exact_type_parameters = Some(declaration_parameters);
                continue;
            };
            if parameters.len() != declaration_parameters.len() {
                return None;
            }
            for (parameter, declaration_parameter) in
                parameters.iter_mut().zip(declaration_parameters)
            {
                if parameter.default.is_none() {
                    parameter.default = declaration_parameter.default;
                }
                if parameter.constraint.is_none() {
                    parameter.constraint = declaration_parameter.constraint;
                }
            }
        }
        let exact_type_parameters = exact_type_parameters?;

        let construct_signatures = self
            .get_module_augmentation_construct_signatures_from_declarations(
                &declarations,
                terminal,
                None,
            );
        if !exact_type_parameters.is_empty() && !construct_signatures.is_empty() {
            return None;
        }

        let empty_type_arguments = NodeList::default();
        let explicit_type_arguments = type_arguments.unwrap_or(&empty_type_arguments);
        let lowered_type_arguments = if exact_type_parameters.is_empty() {
            if !explicit_type_arguments.nodes.is_empty() {
                return None;
            }
            None
        } else {
            if !self.is_inside_type_parameter_declaration(type_reference_idx) {
                let min_required = exact_type_parameters
                    .iter()
                    .filter(|parameter| parameter.default.is_none())
                    .count();
                let display_name = Self::format_generic_display_name_with_interner(
                    terminal,
                    &exact_type_parameters,
                    self.ctx.types,
                );
                if self.validate_type_reference_type_arguments_against_params(
                    &exact_type_parameters,
                    min_required,
                    explicit_type_arguments,
                    type_name_idx,
                    &display_name,
                ) {
                    return Some(TypeId::ERROR);
                }
            }
            let explicit: Vec<_> = explicit_type_arguments
                .nodes
                .iter()
                .map(|&argument| self.get_type_from_type_node(argument))
                .collect();
            Some(if explicit.is_empty() {
                crate::query_boundaries::common::resolve_default_type_args(
                    self.ctx.types,
                    &exact_type_parameters,
                )
            } else {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    self.ctx.types,
                    &explicit,
                    &exact_type_parameters,
                )?
            })
        };

        let exact_self_members = if exact_type_parameters.is_empty() {
            self.exact_interface_self_reference_members(&declarations, terminal)
        } else {
            FxHashSet::default()
        };
        let exact_definition = (!exact_self_members.is_empty())
            .then(|| self.exact_interface_path_definition(&declarations, terminal))
            .flatten();
        let surface = self.get_module_augmentation_members_inner(
            &module_specifier,
            terminal,
            lowered_type_arguments.as_deref(),
            Some(&declarations),
        );
        let mut properties = surface.properties;
        let call_signatures = surface.call_signatures;
        let string_index = surface.string_index;
        let number_index = surface.number_index;
        let symbol_index = surface.symbol_index;

        if let Some(definition) = exact_definition {
            let exact_self = module_augmentation_boundary::declaration_space_lazy_type(
                self.ctx.types,
                definition,
            );
            for property in &mut properties {
                if exact_self_members.contains(&property.name) {
                    property.type_id = exact_self;
                    property.write_type = exact_self;
                }
            }
        }

        let result = if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let callable = module_augmentation_boundary::augmented_callable_type(
                self.ctx.types,
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                None,
                false,
            );
            module_augmentation_boundary::with_augmentation_index_surface_raw(
                self.ctx.types,
                callable,
                None,
                None,
                symbol_index,
            )
        } else if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
            module_augmentation_boundary::exact_path_object_with_index_type(
                self.ctx.types,
                properties,
                string_index,
                number_index,
                symbol_index,
            )
        } else {
            module_augmentation_boundary::exact_path_object_type(self.ctx.types, properties)
        };
        if let Some(definition) = exact_definition {
            self.ctx
                .register_augmented_def_in_envs(definition, result, false);
            self.ctx.clear_type_evaluation_caches_for_def(definition);
        }
        Some(result)
    }

    fn exact_interface_type_parameters(
        &mut self,
        declaration: &ModuleAugmentation,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
        let type_parameters = arena
            .get(declaration.node)
            .and_then(|node| arena.get_interface(node))?
            .type_parameters
            .clone();
        if arena.shares_node_storage_with(self.ctx.arena) {
            let (parameters, updates) = self.push_type_parameters(&type_parameters);
            self.pop_type_parameters(updates);
            return Some(parameters);
        }

        let file_idx = self.ctx.get_file_idx_for_arena(arena)?;
        let binder = self.ctx.all_binders.as_ref()?.get(file_idx)?.clone();
        let cross_arena_guard = Self::enter_cross_arena_delegation()?;
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            drop(cross_arena_guard);
            return None;
        }
        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        let file_name = arena.source_files.first().map_or_else(
            || self.ctx.file_name.clone(),
            |source| source.file_name.clone(),
        );
        let mut checker = CheckerState::delegate_for_arena(
            arena,
            binder.as_ref(),
            file_name,
            self,
            CheckerCreationReason::DelegateCrossArenaOther,
        );
        let preserve_symbol = binder
            .get_node_symbol(declaration.node)
            .unwrap_or(SymbolId(u32::MAX));
        self.clear_delegated_symbol_cache_collisions(
            &mut checker,
            binder.as_ref(),
            preserve_symbol,
        );
        checker.ctx.current_file_idx = file_idx;
        let (parameters, updates) = checker.push_type_parameters(&type_parameters);
        checker.pop_type_parameters(updates);
        let resolved_under_bailout = Self::cross_arena_bailout_epoch() != bailout_epoch_before;

        drop(checker);
        self.ctx.leave_recursion();
        drop(cross_arena_guard);
        (!resolved_under_bailout).then_some(parameters)
    }

    fn exact_interface_self_reference_members(
        &mut self,
        declarations: &[ModuleAugmentation],
        interface_name: &str,
    ) -> FxHashSet<tsz_common::interner::Atom> {
        let mut names = FxHashSet::default();
        for declaration in declarations {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(interface) = arena
                .get(declaration.node)
                .and_then(|node| arena.get_interface(node))
            else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(signature) = arena.get_signature(member_node) else {
                    continue;
                };
                let Some(type_reference) = arena
                    .get(signature.type_annotation)
                    .and_then(|node| arena.get_type_ref(node))
                else {
                    continue;
                };
                if type_reference
                    .type_arguments
                    .as_ref()
                    .is_some_and(|arguments| !arguments.nodes.is_empty())
                    || arena
                        .get_identifier_text(type_reference.type_name)
                        .is_none_or(|name| name != interface_name)
                {
                    continue;
                }
                if let Some(member_name) = self.augmentation_member_key_name(arena, signature.name)
                {
                    names.insert(self.ctx.types.intern_string(&member_name));
                }
            }
        }
        names
    }

    fn exact_interface_path_definition(
        &self,
        declarations: &[ModuleAugmentation],
        interface_name: &str,
    ) -> Option<tsz_solver::DefId> {
        let declaration = declarations
            .iter()
            .find(|declaration| {
                let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
                Self::declaration_is_inside_external_module(arena, declaration.node)
            })
            .or_else(|| declarations.first())?;
        let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
        let node = arena.get(declaration.node)?;
        let file_idx = self
            .ctx
            .get_file_idx_for_arena(arena)
            .unwrap_or(self.ctx.current_file_idx) as u32;
        let name = self.ctx.types.intern_string(interface_name);

        if let Some(existing) = self
            .ctx
            .definition_store
            .defs_by_file(file_idx)
            .into_iter()
            .find(|&definition| {
                self.ctx
                    .definition_store
                    .get(definition)
                    .is_some_and(|info| {
                        info.symbol_id.is_none()
                            && info.name == name
                            && info.span == Some((node.pos, node.end))
                    })
            })
        {
            return Some(existing);
        }

        Some(
            self.ctx.definition_store.register(
                tsz_solver::def::DefinitionInfo::interface(name, Vec::new(), Vec::new())
                    .with_file_id(file_idx)
                    .with_span(node.pos, node.end),
            ),
        )
    }

    fn flatten_namespace_import_type_path(
        arena: &NodeArena,
        node_idx: NodeIndex,
        members: &mut Vec<String>,
    ) -> Option<NodeIndex> {
        let node = arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = arena.get_qualified_name(node)?;
            let root = Self::flatten_namespace_import_type_path(arena, qualified.left, members)?;
            members.push(arena.get_identifier_text(qualified.right)?.to_string());
            return Some(root);
        }
        arena.get_identifier(node)?;
        Some(node_idx)
    }

    fn exact_native_interface_declarations(
        &self,
        module_specifier: &str,
        namespace_path: &[String],
        terminal: &str,
    ) -> Vec<ModuleAugmentation> {
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return Vec::new();
        };
        let first_name = namespace_path.first().map_or(terminal, String::as_str);
        let mut visited = FxHashSet::default();
        let Some((mut symbol_id, owner_file_idx)) =
            self.resolve_export_in_file(target_file_idx, first_name, &mut visited)
        else {
            return Vec::new();
        };
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_file_idx) else {
            return Vec::new();
        };

        let remaining_names = namespace_path
            .iter()
            .skip(1)
            .map(String::as_str)
            .chain((!namespace_path.is_empty()).then_some(terminal));
        for member_name in remaining_names {
            let Some(symbol) = owner_binder.get_symbol(symbol_id) else {
                return Vec::new();
            };
            let Some(next_symbol_id) = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(member_name))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(member_name))
                })
            else {
                return Vec::new();
            };
            if owner_binder
                .get_symbol(next_symbol_id)
                .is_none_or(|member| member.escaped_name != member_name)
            {
                return Vec::new();
            }
            symbol_id = next_symbol_id;
        }

        self.native_interface_declarations_for_symbol(
            symbol_id,
            owner_file_idx,
            namespace_path,
            terminal,
        )
    }

    fn native_interface_declarations_for_symbol(
        &self,
        symbol_id: SymbolId,
        owner_file_idx: usize,
        namespace_path: &[String],
        terminal: &str,
    ) -> Vec<ModuleAugmentation> {
        let Some(symbol) = self
            .ctx
            .get_binder_for_file(owner_file_idx)
            .and_then(|binder| binder.get_symbol(symbol_id))
        else {
            return Vec::new();
        };
        let stable_declarations = symbol.stable_declarations.clone();
        let expected_prefix = (!namespace_path.is_empty()).then(|| namespace_path.join("."));
        let mut result = Vec::new();

        for location in stable_declarations {
            let Some((node_idx, arena)) = self.ctx.node_at_stable_location(location) else {
                continue;
            };
            let Some(node) = arena.get(node_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            if arena
                .get_identifier_text(interface.name)
                .is_none_or(|name| name != terminal)
                || Self::lib_interface_namespace_prefix(&[(node_idx, arena)]) != expected_prefix
                || Self::declaration_is_inside_external_module(arena, node_idx)
            {
                continue;
            }

            if arena.shares_node_storage_with(self.ctx.arena) {
                result.push(ModuleAugmentation::new(terminal.to_string(), node_idx));
                continue;
            }
            let Some(file_idx) = self.ctx.get_file_idx_for_arena(arena) else {
                continue;
            };
            let Some(owner_arena) = self
                .ctx
                .all_arenas
                .as_ref()
                .and_then(|arenas| arenas.get(file_idx))
            else {
                continue;
            };
            result.push(ModuleAugmentation::with_arena(
                terminal.to_string(),
                node_idx,
                owner_arena.clone(),
            ));
        }

        result
    }

    fn declaration_is_inside_external_module(arena: &NodeArena, node_idx: NodeIndex) -> bool {
        let mut parent = arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);
        while parent.is_some() {
            let Some(parent_node) = arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = arena.get_module(parent_node)
                && arena.get_identifier_at(module.name).is_none()
            {
                return true;
            }
            parent = arena.parent_of(parent).unwrap_or(NodeIndex::NONE);
        }
        false
    }
}
