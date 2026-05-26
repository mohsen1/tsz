//! Constructors and conversion helpers for `DefinitionInfo`.

use super::{DefId, DefKind, DefinitionInfo, EnumMemberValue};
use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, TypeId, TypeParamInfo};
use std::sync::Arc;
use tsz_common::interner::Atom;

impl DefinitionInfo {
    /// Create a new type alias definition.
    pub const fn type_alias(name: Atom, type_params: Vec<TypeParamInfo>, body: TypeId) -> Self {
        Self {
            kind: DefKind::TypeAlias,
            name,
            type_params,
            body: Some(body),
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Returns `true` if this definition represents a class constructor (static side).
    pub const fn is_class_constructor(&self) -> bool {
        matches!(self.kind, DefKind::ClassConstructor)
    }

    /// Create a new interface definition.
    pub fn interface(
        name: Atom,
        type_params: Vec<TypeParamInfo>,
        properties: Vec<PropertyInfo>,
    ) -> Self {
        let shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        Self {
            kind: DefKind::Interface,
            name,
            type_params,
            body: None, // Body computed on demand
            instance_shape: Some(Arc::new(shape)),
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new class definition.
    pub fn class(
        name: Atom,
        type_params: Vec<TypeParamInfo>,
        instance_properties: Vec<PropertyInfo>,
        static_properties: Vec<PropertyInfo>,
    ) -> Self {
        let instance_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: instance_properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        let static_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: static_properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        Self {
            kind: DefKind::Class,
            name,
            type_params,
            body: None,
            instance_shape: Some(Arc::new(instance_shape)),
            static_shape: Some(Arc::new(static_shape)),
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new enum definition.
    pub const fn enumeration(name: Atom, members: Vec<(Atom, EnumMemberValue)>) -> Self {
        Self {
            kind: DefKind::Enum,
            name,
            type_params: Vec::new(),
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: members,
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new namespace definition.
    pub const fn namespace(name: Atom, exports: Vec<(Atom, DefId)>) -> Self {
        Self {
            kind: DefKind::Namespace,
            name,
            type_params: Vec::new(),
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports,
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Set the extends parent for a class.
    pub const fn with_extends(mut self, parent: DefId) -> Self {
        self.extends = Some(parent);
        self
    }

    /// Add an export to the namespace/module.
    pub fn add_export(&mut self, name: Atom, def_id: DefId) {
        self.exports.push((name, def_id));
    }

    /// Set file ID for debugging.
    pub const fn with_file_id(mut self, file_id: u32) -> Self {
        self.file_id = Some(file_id);
        self
    }

    /// Set source span.
    pub const fn with_span(mut self, start: u32, end: u32) -> Self {
        self.span = Some((start, end));
        self
    }

    /// Convert `SemanticDefKind` to `DefKind`.
    pub const fn kind_from_semantic(kind: tsz_binder::SemanticDefKind) -> DefKind {
        match kind {
            tsz_binder::SemanticDefKind::TypeAlias => DefKind::TypeAlias,
            tsz_binder::SemanticDefKind::Interface => DefKind::Interface,
            tsz_binder::SemanticDefKind::Class => DefKind::Class,
            tsz_binder::SemanticDefKind::Enum => DefKind::Enum,
            tsz_binder::SemanticDefKind::Namespace => DefKind::Namespace,
            tsz_binder::SemanticDefKind::Function => DefKind::Function,
            tsz_binder::SemanticDefKind::Variable => DefKind::Variable,
        }
    }

    /// Create a `DefinitionInfo` from a binder `SemanticDefEntry`.
    ///
    /// Centralizes the conversion used by both `DefinitionStore::from_semantic_def_entries`
    /// (merge pipeline) and `CheckerContext::populate_def_ids_from_semantic_defs`
    /// (per-file checker construction). Single conversion path prevents field
    /// divergence between the two code paths.
    pub fn from_semantic_def(
        entry: &tsz_binder::SemanticDefEntry,
        sym_id: u32,
        intern_string: &dyn Fn(&str) -> Atom,
    ) -> Self {
        let kind = Self::kind_from_semantic(entry.kind);
        let name = intern_string(&entry.name);

        let type_params = if entry.type_param_count > 0 {
            (0..entry.type_param_count)
                .map(|i| {
                    let param_name = entry
                        .type_param_names
                        .get(i as usize)
                        .map(|n| intern_string(n))
                        .unwrap_or(Atom(0));
                    crate::TypeParamInfo {
                        name: param_name,
                        constraint: None,
                        default: None,
                        is_const: false,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let enum_members: Vec<(Atom, EnumMemberValue)> = entry
            .enum_member_names
            .iter()
            .map(|n| (intern_string(n), EnumMemberValue::Computed))
            .collect();

        Self {
            kind,
            name,
            type_params,
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members,
            exports: Vec::new(),
            file_id: Some(entry.file_id),
            span: Some((entry.span_start, entry.span_start)),
            symbol_id: Some(sym_id),
            heritage_names: entry.heritage_names(),
            is_abstract: entry.is_abstract,
            is_const: entry.is_const,
            is_exported: entry.is_exported,
            is_global_augmentation: entry.is_global_augmentation,
            is_declare: entry.is_declare,
        }
    }

    /// Create a `ClassConstructor` companion from a class `SemanticDefEntry`.
    pub fn class_constructor_from_semantic_def(
        entry: &tsz_binder::SemanticDefEntry,
        sym_id: u32,
        intern_string: &dyn Fn(&str) -> Atom,
    ) -> Self {
        Self {
            kind: DefKind::ClassConstructor,
            name: intern_string(&entry.name),
            type_params: Vec::new(),
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: Some(entry.file_id),
            span: Some((entry.span_start, entry.span_start)),
            symbol_id: Some(sym_id),
            heritage_names: Vec::new(),
            is_abstract: entry.is_abstract,
            is_const: false,
            is_exported: entry.is_exported,
            is_global_augmentation: false,
            is_declare: entry.is_declare,
        }
    }
}
