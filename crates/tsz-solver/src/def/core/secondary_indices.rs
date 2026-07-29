//! Secondary definition indices and cross-checker publication channels.

use super::{
    DefId, DefinitionStore, augmentation_transaction, augmented_body_symbol_redirect_enabled,
    typeof_uri_selfloop_enabled,
};
use crate::construction::TypeDatabase;
use crate::types::{ObjectShape, SymbolRef, TypeData, TypeId, TypeParamInfo};
use std::sync::{Arc, atomic::Ordering};
use tsz_common::interner::Atom;

impl DefinitionStore {
    /// Number of definitions.
    pub fn len(&self) -> usize {
        if let Some(parent) = self.augmentation_parent() {
            parent.len()
                + self
                    .definitions
                    .iter()
                    .filter(|entry| !parent.contains(*entry.key()))
                    .count()
        } else {
            self.definitions.len()
        }
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
            && self
                .augmentation_parent()
                .is_none_or(|parent| parent.is_empty())
    }

    /// Clear all definitions (for testing).
    pub fn clear(&self) {
        self.definitions.clear();
        self.type_to_def.clear();
        self.body_dependency_defs.clear();
        self.augmentation_removed_body_dependencies.clear();
        self.type_param_for_decl_node.clear();
        self.symbol_def_index.clear();
        self.symbol_only_index.clear();
        self.decl_site_to_def.clear();
        self.invalidate_symbol_mappings_log();
        self.body_to_alias.clear();
        self.augmentation_removed_alias_bodies.clear();
        self.state_flags.clear_alias_bodies();
        self.shape_to_def.clear();
        self.file_to_defs.clear();
        self.class_to_constructor.clear();
        self.class_to_instance.clear();
        self.typeof_value_to_literal.clear();
        self.module_augmented_bodies.clear();
        self.augmented_base_body_def_for_symbol.clear();
        self.enum_member_to_parent.clear();
        self.name_to_defs.clear();
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
        self.bump_generation();
    }

    /// Register a mapping from a `TypeId` to its defining `DefId`.
    ///
    /// Called by the checker after computing class/interface instance types
    /// so the `TypeFormatter` can display named types (e.g., "A" instead of
    /// "{ a: string }") even across file boundaries.
    pub fn register_type_to_def(&self, type_id: TypeId, def_id: DefId) {
        // Intrinsic TypeIds (number, string, boolean, etc.) are universal and
        // must never be associated with a user-named def. Their canonical
        // display is the keyword (`number`, `string`, ...), provided by the
        // TypeFormatter's intrinsic short-circuit. If a checker path tries to
        // register an intrinsic type to a class/interface/alias def, that
        // mapping would later poison `find_def_for_type` lookups and cause
        // diagnostics like "Type 'FlatArray' is not assignable to type
        // 'Boolean'." for `let b: Boolean; b = 1;` (where the source is the
        // primitive `number`).  Drop the registration so the formatter falls
        // back to the intrinsic keyword.
        if type_id.is_intrinsic() {
            return;
        }
        if !self.type_to_def.contains_key(&type_id)
            && let Some(parent_def) = self
                .augmentation_parent()
                .and_then(|parent| parent.find_def_for_type(type_id))
        {
            self.type_to_def.insert(type_id, parent_def);
        }
        self.record_augmentation_publication(
            augmentation_transaction::AugmentationPublication::RegisterTypeToDef {
                type_id,
                def_id,
            },
        );
        use dashmap::mapref::entry::Entry;
        match self.type_to_def.entry(type_id) {
            Entry::Vacant(e) => {
                e.insert(def_id);
            }
            Entry::Occupied(mut e) => {
                let existing = *e.get();
                if existing == def_id {
                    return;
                }
                let existing_pos = self
                    .get(existing)
                    .and_then(|d| Some((d.file_id?, d.span?.0)));
                let new_pos = self.get(def_id).and_then(|d| Some((d.file_id?, d.span?.0)));
                match (existing_pos, new_pos) {
                    (Some((ef, ep)), Some((nf, np))) if (nf, np) < (ef, ep) => {
                        e.insert(def_id);
                    }
                    (None, Some(_)) => {
                        e.insert(def_id);
                    }
                    _ => {}
                }
            }
        }
        self.bump_generation();
    }

    /// Look up the `DefId` that produced the given `TypeId`.
    ///
    /// Returns `Some(def_id)` if a class/interface was registered for this type.
    pub fn find_def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        self.type_to_def.get(&type_id).map(|r| *r).or_else(|| {
            self.augmentation_parent()
                .and_then(|parent| parent.find_def_for_type(type_id))
        })
    }

    /// Look up the canonical `TypeId` for a type-parameter declaration
    /// identified by its owning file and name-node index (declarations
    /// without a `DefId` registration).
    ///
    /// `file` is the interned file-name `Atom` of the arena owning
    /// `name_node`; together they form a globally unambiguous declaration
    /// identity that is stable across parent and child checkers.
    pub fn find_type_param_for_decl_node(
        &self,
        file: Atom,
        name_node: u32,
        info: &TypeParamInfo,
    ) -> Option<TypeId> {
        self.type_param_for_decl_node
            .get(&(file, name_node, *info))
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.find_type_param_for_decl_node(file, name_node, info))
            })
    }

    /// #14351 measurement-only: number of distinct decl-site canonical type-param
    /// entries `(file, name_node, info)`. Comparing this to the count of distinct
    /// interned `TypeParameter` ids decides whether the divergent ids are
    /// fresh-mints that BYPASS the decl-site map (map-len << distinct-ids =>
    /// tractable re-mint convergence) or genuinely-distinct decl-sites
    /// (map-len ~= distinct-ids => XL relation-level).
    pub fn type_param_decl_node_count(&self) -> usize {
        if let Some(parent) = self.augmentation_parent() {
            parent.type_param_decl_node_count()
                + self
                    .type_param_for_decl_node
                    .iter()
                    .filter(|entry| {
                        parent
                            .find_type_param_for_decl_node(
                                entry.key().0,
                                entry.key().1,
                                &entry.key().2,
                            )
                            .is_none()
                    })
                    .count()
        } else {
            self.type_param_for_decl_node.len()
        }
    }

    /// Register the canonical `TypeId` for a type-parameter declaration
    /// identified by `(file, name_node, info)`.
    ///
    /// First writer wins: the returned `TypeId` is the canonical one, which
    /// may differ from `type_id` when another checker registered the same
    /// declaration first (parallel file checking or cross-arena
    /// delegation). Callers must adopt the returned id.
    pub fn register_type_param_for_decl_node(
        &self,
        file: Atom,
        name_node: u32,
        info: TypeParamInfo,
        type_id: TypeId,
    ) -> TypeId {
        if let Some(parent) = self.augmentation_parent() {
            return parent.register_type_param_for_decl_node(file, name_node, info, type_id);
        }
        if let Some(existing) = self.find_type_param_for_decl_node(file, name_node, &info) {
            return existing;
        }
        *self
            .type_param_for_decl_node
            .entry((file, name_node, info))
            .or_insert(type_id)
    }

    /// Register a mapping from a `Class` `DefId` to its `ClassConstructor` companion `DefId`.
    ///
    /// Called during pre-population to establish constructor identity at merge time
    /// rather than on-demand during type checking. The checker can then look up the
    /// companion with `get_constructor_def` and reuse the stable identity.
    pub fn register_constructor_companion(&self, class_def: DefId, ctor_def: DefId) {
        if let Some(parent) = self.augmentation_parent() {
            parent.register_constructor_companion(class_def, ctor_def);
            return;
        }
        self.class_to_constructor.insert(class_def, ctor_def);
        self.bump_generation();
    }

    /// Look up the pre-populated `ClassConstructor` `DefId` for a class.
    ///
    /// Returns `Some(ctor_def_id)` if a constructor companion was registered
    /// during pre-population. Returns `None` for classes without a pre-populated
    /// companion (e.g., anonymous classes or those created on-demand).
    pub fn get_constructor_def(&self, class_def: DefId) -> Option<DefId> {
        self.class_to_constructor
            .get(&class_def)
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_constructor_def(class_def))
            })
    }

    /// Publish the resolved instance `TypeId` for a class `DefId` into the
    /// shared cross-file cache (see `class_to_instance` field doc).
    ///
    /// Producer checkers call this whenever they finalize a class's instance
    /// type. Consumer checkers in other files read it through
    /// `get_class_instance_type` when their per-checker
    /// `TypeEnvironment::class_instance_types` is cold.
    pub fn register_class_instance_type(&self, class_def: DefId, instance_type: TypeId) {
        self.record_augmentation_publication(
            augmentation_transaction::AugmentationPublication::RegisterClassInstanceType {
                class_def,
                instance_type,
            },
        );
        self.class_to_instance.insert(class_def, instance_type);
        self.bump_generation();
    }

    /// Look up the shared instance `TypeId` for a class `DefId`.
    ///
    /// Returns `Some(instance_type)` if some checker has already finalized
    /// the class's instance type and published it via
    /// `register_class_instance_type`. Returns `None` when no checker has
    /// finished building the class yet.
    pub fn get_class_instance_type(&self, class_def: DefId) -> Option<TypeId> {
        self.class_to_instance
            .get(&class_def)
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_class_instance_type(class_def))
            })
    }

    /// Publish a merged value+type symbol's value-space `typeof` literal into
    /// the program-wide slot (see the `typeof_value_to_literal` field doc).
    ///
    /// Write-through target for `TypeEnvironment::insert_typeof_value_type`. The
    /// producing arena's local `typeof_value_types` map only carries entries for
    /// symbols whose `typeof` query was syntactically reached while checking that
    /// file; this shared copy lets a consuming arena's `resolve_type_query`
    /// resolve a cross-arena self-looping `typeof X` to the concrete literal.
    pub fn register_typeof_value_literal(&self, symbol_id: u32, literal: TypeId) {
        self.record_augmentation_publication(
            augmentation_transaction::AugmentationPublication::RegisterTypeofValueLiteral {
                symbol_id,
                literal,
            },
        );
        if self
            .typeof_value_to_literal
            .insert(symbol_id, literal)
            .is_none_or(|prev| prev != literal)
        {
            self.bump_generation();
        }
    }

    /// Look up the program-wide value-space `typeof` literal for a merged
    /// value+type symbol, if some checker has published one.
    pub fn get_typeof_value_literal(&self, symbol_id: u32) -> Option<TypeId> {
        self.typeof_value_to_literal
            .get(&symbol_id)
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_typeof_value_literal(symbol_id))
            })
    }

    /// Record the redirect from a HOME interface `SymbolId` (raw u32) to the
    /// HOME `DefId` whose `get_body` holds its fully-merged augmented body
    /// (see the `augmented_base_body_def_for_symbol` field doc).
    ///
    /// First-wins (the home def's identity is stable across re-checks), so a
    /// later re-publication of the same edge is a no-op and does not perturb the
    /// store generation. Write-through target for the checker's augmentation
    /// merge site.
    pub fn register_augmented_base_body_def(&self, symbol_id: u32, def_id: DefId) {
        if self.augmented_base_body_def_for_symbol(symbol_id).is_some() {
            return;
        }
        self.record_augmentation_publication(
            augmentation_transaction::AugmentationPublication::RegisterAugmentedBaseBodyDef {
                symbol_id,
                def_id,
            },
        );
        use dashmap::mapref::entry::Entry;
        if let Entry::Vacant(vacant) = self.augmented_base_body_def_for_symbol.entry(symbol_id) {
            vacant.insert(def_id);
            self.bump_generation();
        }
    }

    /// Look up the HOME `DefId` whose merged augmented body should answer an
    /// index access against a frozen empty pre-merge snapshot carrying this home
    /// `SymbolId`, if some checker has published the edge.
    pub fn augmented_base_body_def_for_symbol(&self, symbol_id: u32) -> Option<DefId> {
        self.augmented_base_body_def_for_symbol
            .get(&symbol_id)
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.augmented_base_body_def_for_symbol(symbol_id))
            })
    }

    /// Flag-gated write-through of a merged value+type symbol's value literal
    /// into the program-wide slot (issue #14345). Called from
    /// `TypeEnvironment::insert_typeof_value_type`; the producer only passes a
    /// concrete value type (unknown/error/any are rejected upstream in the
    /// checker), so the shared slot stays sound. Gated on
    /// `typeof_uri_selfloop_enabled()` so flag-OFF leaves the shared store (and
    /// its generation) untouched — fully byte-parity.
    pub fn register_typeof_value_literal_if_enabled(&self, symbol_id: u32, literal: TypeId) {
        if typeof_uri_selfloop_enabled() {
            self.register_typeof_value_literal(symbol_id, literal);
        }
    }

    /// Flag-gated write-through of the home-symbol → home-`DefId` redirect edge
    /// (issue #14344 / #14345). Called from the checker's augmentation merge
    /// site once the merged augmented body is published under the home `DefId`.
    /// Gated on `augmented_body_symbol_redirect_enabled()` so flag-OFF leaves
    /// the shared store (and its generation) untouched — fully byte-parity.
    pub fn register_augmented_base_body_def_if_enabled(&self, symbol_id: u32, def_id: DefId) {
        if augmented_body_symbol_redirect_enabled() {
            self.register_augmented_base_body_def(symbol_id, def_id);
        }
    }

    /// Update the instance shape for a type definition.
    ///
    /// This is used by checker code when a concrete object-like shape is computed
    /// for an interface/class definition and should be recorded for diagnostics.
    pub fn set_instance_shape(&self, id: DefId, shape: Arc<ObjectShape>) {
        self.ensure_augmentation_definition(id);
        self.record_augmentation_publication_with(|| {
            augmentation_transaction::AugmentationPublication::SetInstanceShape {
                id,
                shape: Arc::clone(&shape),
            }
        });
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            let hash = Self::hash_shape(&shape);
            if self.find_def_by_shape(&shape).is_none() {
                self.shape_to_def.entry(hash).or_insert(id);
            }
            entry.instance_shape = Some(shape);
            self.bump_generation();
        }
    }

    /// Update the static shape for a type definition.
    pub(super) fn set_static_shape(&self, id: DefId, shape: Arc<ObjectShape>) {
        self.ensure_augmentation_definition(id);
        self.record_augmentation_publication_with(|| {
            augmentation_transaction::AugmentationPublication::SetStaticShape {
                id,
                shape: Arc::clone(&shape),
            }
        });
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.static_shape = Some(shape);
            self.bump_generation();
        }
    }

    /// Resolve a self-looping `typeof X` query to its program-wide value literal
    /// (issue #14345). A merged value+type symbol whose type-space body is a
    /// self-referential `typeof X` (the fp-ts `const URI = "..."; type URI =
    /// typeof URI` tag idiom) self-loops in `resolve_type_query` — `candidate`
    /// re-yields `TypeQuery(symbol)`, so `URItoKind[URI]` never reduces. When the
    /// consuming arena never registered the value literal locally (its `typeof X`
    /// site lives in another arena), substitute the program-wide literal
    /// published by the producing arena. Returns `None` (leaving `candidate`
    /// unchanged) unless the flag is on, `candidate` is exactly the self-loop
    /// `TypeQuery(symbol)`, and a concrete literal was published — so an abstract
    /// URI / literal-less `typeof` stays deferred. Flag-gated; OFF is byte-parity.
    pub fn typeof_self_loop_literal(
        &self,
        symbol: SymbolRef,
        candidate: Option<TypeId>,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        if !typeof_uri_selfloop_enabled() {
            return None;
        }
        let is_self_loop = candidate.is_some_and(
            |ty| matches!(interner.lookup(ty), Some(TypeData::TypeQuery(s)) if s == symbol),
        );
        if is_self_loop {
            self.get_typeof_value_literal(symbol.0)
        } else {
            None
        }
    }

    /// Publish an enum member `DefId` -> parent enum `DefId` edge into the
    /// shared, program-wide map (see the `enum_member_to_parent` field doc).
    ///
    /// Write-through target for `TypeEnvironment::register_enum_parent`. The
    /// producing file's local `enum_parents` map is wiped on its file-session
    /// reset; this shared copy persists so a consuming file's flow-analyzer env
    /// can still resolve the member's parent during cross-file enum
    /// discriminant narrowing.
    pub fn register_enum_parent(&self, member_def: DefId, parent_def: DefId) {
        if let Some(parent) = self.augmentation_parent() {
            parent.register_enum_parent(member_def, parent_def);
            return;
        }
        if self
            .enum_member_to_parent
            .insert(member_def, parent_def)
            .is_none()
        {
            self.bump_generation();
        }
    }

    /// Look up the parent enum `DefId` for an enum member `DefId` in the shared
    /// program-wide map. Returns `None` when no checker has registered the edge
    /// (e.g. a non-enum-member `DefId`).
    pub fn get_enum_parent(&self, member_def: DefId) -> Option<DefId> {
        self.enum_member_to_parent
            .get(&member_def)
            .map(|r| *r)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_enum_parent(member_def))
            })
    }
}
