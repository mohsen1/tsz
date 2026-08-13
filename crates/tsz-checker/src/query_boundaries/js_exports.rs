//! Unified JS/CommonJS export surface synthesis.
//!
//! This module provides a single authority for computing the export shape of a
//! CommonJS/JS module. Instead of each consumer re-deriving the export surface
//! from scratch, they call `resolve_js_export_surface` which synthesizes a
//! `JsExportSurface` combining:
//!
//! - `module.exports = X` (direct module export assignment)
//! - `exports.foo = Y` / `module.exports.foo = Y` (property assignments)
//! - `Object.defineProperty(exports, "foo", desc)` (defineProperty exports)
//! - Prototype property assignments (`Ctor.prototype.method = fn`)
//! - Constructor function -> callable+constructable type upgrade
//!
//! The result is cached per target file index to avoid redundant computation.

use crate::{context::is_js_file_name, state::CheckerState};
use rustc_hash::FxHashMap;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, FunctionShape, ObjectShape, ParamInfo, PropertyInfo, TypeId,
    Visibility,
};

pub(crate) use crate::query_boundaries::js_exports_json::{
    commonjs_json_namespace_type, json_esm_namespace_type, json_module_value_type,
};

pub(crate) fn commonjs_direct_export_supports_named_props(
    types: &dyn tsz_solver::construction::TypeDatabase,
    direct_export_type: TypeId,
) -> bool {
    if matches!(
        direct_export_type,
        TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER | TypeId::OBJECT
    ) {
        return true;
    }

    if matches!(
        direct_export_type,
        TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BOOLEAN
            | TypeId::BIGINT
            | TypeId::SYMBOL
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::VOID
    ) {
        return false;
    }

    tsz_solver::visitor::is_object_like_type(types, direct_export_type)
        || crate::query_boundaries::common::callable_shape_for_type(types, direct_export_type)
            .is_some()
        || tsz_solver::type_queries::get_function_shape(types, direct_export_type).is_some()
}

pub(super) fn public_export_property(
    db: &dyn TypeDatabase,
    name: &str,
    type_id: TypeId,
    optional: bool,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name: db.intern_string(name),
        type_id,
        write_type: type_id,
        optional,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn commonjs_namespace_any_property(
    db: &dyn TypeDatabase,
    name: &str,
    declaration_order: u32,
) -> PropertyInfo {
    public_export_property(db, name, TypeId::ANY, false, declaration_order)
}

pub(crate) fn commonjs_namespace_export_property(
    db: &dyn TypeDatabase,
    name: &str,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    public_export_property(db, name, type_id, false, declaration_order)
}

fn commonjs_export_property_with_write(
    db: &dyn TypeDatabase,
    name: &str,
    type_id: TypeId,
    write_type: TypeId,
    readonly: bool,
    is_method: bool,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name: db.intern_string(name),
        type_id,
        write_type,
        optional: false,
        readonly,
        is_method,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

fn widen_commonjs_descriptor_type(db: &dyn TypeDatabase, ty: TypeId) -> TypeId {
    crate::query_boundaries::common::widen_type(
        db,
        crate::query_boundaries::common::widen_freshness(db, ty),
    )
}

pub(crate) fn commonjs_define_property_setter_contextual_function_type(
    db: &dyn TypeDatabase,
    getter_type: Option<TypeId>,
) -> Option<TypeId> {
    getter_type.map(|ty| {
        db.function(FunctionShape::new(
            vec![ParamInfo::unnamed(ty)],
            TypeId::VOID,
        ))
    })
}

pub(crate) struct CommonJsDefinePropertyDescriptorFacts {
    pub(crate) value_type: Option<TypeId>,
    pub(crate) getter_type: Option<TypeId>,
    pub(crate) setter_type: Option<TypeId>,
    pub(crate) has_value: bool,
    pub(crate) has_setter: bool,
    pub(crate) writable_true: bool,
}

pub(crate) fn commonjs_define_property_descriptor_property(
    db: &dyn TypeDatabase,
    name: &str,
    facts: CommonJsDefinePropertyDescriptorFacts,
    declaration_order: u32,
) -> PropertyInfo {
    let has_getter = facts.getter_type.is_some();
    let has_accessor_descriptor = has_getter || facts.has_setter;
    let has_data_descriptor = facts.has_value || facts.writable_true;

    if (!has_accessor_descriptor && !has_data_descriptor)
        || (has_accessor_descriptor && has_data_descriptor)
        || (facts.writable_true && !facts.has_value && !has_accessor_descriptor)
    {
        return commonjs_export_property_with_write(
            db,
            name,
            TypeId::ANY,
            TypeId::ANY,
            true,
            false,
            declaration_order,
        );
    }

    let value_type = facts
        .value_type
        .map(|ty| widen_commonjs_descriptor_type(db, ty));
    let getter_type = facts
        .getter_type
        .map(|ty| widen_commonjs_descriptor_type(db, ty));
    let mut setter_type = facts
        .setter_type
        .map(|ty| widen_commonjs_descriptor_type(db, ty));

    if facts.has_setter && setter_type == Some(TypeId::ANY) && getter_type.is_some() {
        setter_type = getter_type;
    }

    let writable = facts.has_setter || (facts.has_value && facts.writable_true);
    let precise_setter_type = setter_type.filter(|&ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN);
    let read_type = value_type
        .or(getter_type)
        .or(setter_type)
        .unwrap_or(TypeId::ANY);
    let write_type = if writable {
        precise_setter_type
            .or(getter_type)
            .or(value_type)
            .unwrap_or(read_type)
    } else {
        read_type
    };

    commonjs_export_property_with_write(
        db,
        name,
        read_type,
        write_type,
        !writable,
        false,
        declaration_order,
    )
}

pub(crate) fn commonjs_type_with_define_property_members(
    db: &dyn tsz_solver::construction::QueryDatabase,
    base_type: TypeId,
    props: Vec<PropertyInfo>,
) -> TypeId {
    if props.is_empty() {
        return base_type;
    }

    let base_shape = crate::query_boundaries::common::object_shape_for_type(db, base_type)
        .map(|shape| shape.as_ref().clone())
        .or_else(|| {
            let widened = crate::query_boundaries::common::widen_freshness(db, base_type);
            crate::query_boundaries::common::object_shape_for_type(db, widened)
                .map(|shape| shape.as_ref().clone())
        });

    if let Some(shape) = base_shape {
        let mut merged_props = shape.properties.clone();
        for prop in props {
            if let Some(existing) = merged_props
                .iter_mut()
                .find(|existing| existing.name == prop.name)
            {
                *existing = prop;
            } else {
                merged_props.push(prop);
            }
        }

        return db
            .factory()
            .object_with_shape_metadata(merged_props, &shape);
    }

    let define_property_type = db.object(props);
    if base_type.is_unknown_or_error() {
        define_property_type
    } else {
        db.intersection2(base_type, define_property_type)
    }
}

pub(crate) struct CommonJsExpandoMember {
    pub(crate) name: String,
    pub(crate) type_id: TypeId,
}

fn commonjs_expando_property(
    db: &dyn TypeDatabase,
    member: &CommonJsExpandoMember,
    declaration_order: u32,
) -> PropertyInfo {
    let type_id = crate::query_boundaries::common::widen_literal_type(db, member.type_id);
    commonjs_namespace_export_property(db, &member.name, type_id, declaration_order)
}

pub(crate) fn commonjs_export_type_with_expando_members(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    members: &[CommonJsExpandoMember],
) -> TypeId {
    if members.is_empty() {
        return base_type;
    }

    let object_type = commonjs_export_object_type_with_expando_members(db, base_type, members);
    commonjs_export_callable_type_with_expando_members(db, object_type, members)
}

fn commonjs_export_object_type_with_expando_members(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    members: &[CommonJsExpandoMember],
) -> TypeId {
    let Some(shape) = crate::query_boundaries::common::object_shape_for_type(db, base_type) else {
        return base_type;
    };

    let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> = shape
        .properties
        .iter()
        .map(|prop| (prop.name, prop.clone()))
        .collect();
    let mut changed = false;

    for member in members {
        let prop_atom = db.intern_string(&member.name);
        if properties.contains_key(&prop_atom) {
            continue;
        }

        properties.insert(
            prop_atom,
            commonjs_expando_property(db, member, properties.len() as u32),
        );
        changed = true;
    }

    if !changed {
        return base_type;
    }

    db.object_with_index(ObjectShape {
        flags: shape.flags,
        properties: properties.into_values().collect(),
        string_index: shape.string_index,
        number_index: shape.number_index,
        symbol_index: shape.symbol_index,
        symbol: shape.symbol,
    })
}

fn commonjs_export_callable_type_with_expando_members(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    members: &[CommonJsExpandoMember],
) -> TypeId {
    let (mut callable_shape, mut property_count) = if let Some(shape) =
        crate::query_boundaries::common::callable_shape_for_type(db, base_type)
    {
        ((*shape).clone(), shape.properties.len())
    } else if let Some(function_shape) =
        crate::query_boundaries::common::function_shape_for_type(db, base_type)
    {
        let signature = CallSignature {
            type_params: function_shape.type_params.clone(),
            params: function_shape.params.clone(),
            this_type: function_shape.this_type,
            return_type: function_shape.return_type,
            type_predicate: function_shape.type_predicate,
            is_method: function_shape.is_method,
        };
        (
            CallableShape {
                call_signatures: if function_shape.is_constructor {
                    Vec::new()
                } else {
                    vec![signature.clone()]
                },
                construct_signatures: if function_shape.is_constructor {
                    vec![signature]
                } else {
                    Vec::new()
                },
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            },
            0,
        )
    } else {
        return base_type;
    };

    let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> = callable_shape
        .properties
        .iter()
        .map(|prop| (prop.name, prop.clone()))
        .collect();
    let mut changed = false;

    for member in members {
        let prop_type = crate::query_boundaries::common::widen_literal_type(db, member.type_id);
        let prop_atom = db.intern_string(&member.name);
        if let Some(existing) = properties.get_mut(&prop_atom) {
            let existing_is_placeholder = existing.type_id.is_any_unknown_or_error();
            if existing_is_placeholder && !matches!(prop_type, TypeId::ANY | TypeId::UNKNOWN) {
                existing.type_id = prop_type;
                existing.write_type = prop_type;
                changed = true;
            }
            continue;
        }

        properties.insert(
            prop_atom,
            commonjs_expando_property(db, member, property_count as u32),
        );
        property_count += 1;
        changed = true;
    }

    if !changed {
        return base_type;
    }

    callable_shape.properties = properties.into_values().collect();
    db.callable(callable_shape)
}

pub(crate) fn commonjs_empty_namespace_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn commonjs_export_surface_can_merge_named_exports(
    db: &dyn TypeDatabase,
    surface: &JsExportSurface,
) -> bool {
    surface.direct_export_type.is_none_or(|direct_export_type| {
        commonjs_direct_export_supports_named_props(db, direct_export_type)
    })
}

pub(crate) fn current_file_commonjs_namespace_type(
    checker: &mut CheckerState<'_>,
    surface: JsExportSurface,
    late_export_names: impl IntoIterator<Item = String>,
    display_name: String,
) -> TypeId {
    let can_merge_named_exports =
        commonjs_export_surface_can_merge_named_exports(checker.ctx.types, &surface);

    let mut props = if can_merge_named_exports {
        surface.named_exports
    } else {
        Vec::new()
    };

    if can_merge_named_exports {
        for name in late_export_names {
            let name_atom = checker.ctx.types.intern_string(&name);
            if props.iter().any(|p| p.name == name_atom) {
                continue;
            }
            props.push(commonjs_namespace_any_property(
                checker.ctx.types,
                &name,
                props.len() as u32,
            ));
        }
    }

    let has_named_props = !props.is_empty();
    JsExportSurface {
        direct_export_type: surface.direct_export_type,
        named_exports: props,
        prototype_members: surface.prototype_members,
        has_commonjs_exports: surface.has_commonjs_exports || has_named_props,
        has_augmented_named_exports: surface.has_augmented_named_exports || has_named_props,
        direct_export_reads_exports: surface.direct_export_reads_exports,
    }
    .to_type_id_with_display_name(checker, Some(display_name.clone()))
    .unwrap_or_else(|| {
        let empty_namespace = commonjs_empty_namespace_type(checker.ctx.types);
        checker
            .ctx
            .namespace_module_names
            .insert(empty_namespace, display_name);
        empty_namespace
    })
}

pub(crate) fn commonjs_export_surface_type_with_display_name(
    checker: &mut CheckerState<'_>,
    surface: &JsExportSurface,
    display_name: String,
) -> Option<TypeId> {
    surface.to_type_id_with_display_name(checker, Some(display_name))
}

pub(crate) fn commonjs_imported_module_value_type(
    checker: &mut CheckerState<'_>,
    mut props: Vec<PropertyInfo>,
    export_equals_type: Option<TypeId>,
    module_is_non_module_entity: bool,
    display_module_name: Option<String>,
) -> Option<TypeId> {
    let namespace_type = (!props.is_empty()).then(|| {
        JsExportSurface::normalize_property_declaration_order(&mut props);
        let namespace_type = checker.ctx.types.object(props);
        if let Some(display_module_name) = display_module_name.as_ref() {
            checker
                .ctx
                .namespace_module_names
                .insert(namespace_type, display_module_name.clone());
        }
        namespace_type
    });

    if let Some(export_equals_type) = export_equals_type {
        let result = if module_is_non_module_entity {
            if checker.ctx.allow_synthetic_default_imports() {
                namespace_type.unwrap_or(export_equals_type)
            } else {
                export_equals_type
            }
        } else {
            namespace_type
                .map(|namespace_type| {
                    checker
                        .ctx
                        .types
                        .intersection2(export_equals_type, namespace_type)
                })
                .unwrap_or(export_equals_type)
        };
        if let Some(display_module_name) = display_module_name {
            checker
                .ctx
                .namespace_module_names
                .entry(result)
                .or_insert(display_module_name);
        }
        return Some(result);
    }

    namespace_type
}

/// Represents the synthesized export surface of a JS/CommonJS module.
#[derive(Debug, Clone)]
pub struct JsExportSurface {
    /// The direct `module.exports = X` type, if any.
    /// This is the "base" type that gets intersected with namespace properties.
    pub direct_export_type: Option<TypeId>,

    /// Named property exports from `exports.foo = ...`, `module.exports.foo = ...`,
    /// and `Object.defineProperty(exports, ...)`.
    pub named_exports: Vec<PropertyInfo>,

    /// Prototype method bindings collected from `Ctor.prototype.method = fn` patterns.
    /// These get merged into the constructor's instance type.
    pub prototype_members: Vec<PropertyInfo>,

    /// Whether the module has any CommonJS export patterns at all.
    pub has_commonjs_exports: bool,

    /// Whether `named_exports` includes properties from genuine augmentation
    /// (`exports.foo = ...`, `module.exports.foo = ...`,
    /// `Object.defineProperty(exports, "foo", …)`) — as opposed to only the
    /// implicit seed extracted from a single `module.exports = { … }` object
    /// literal. Used to decide whether the synthesized type should be tagged
    /// with `namespace_module_names` (display as `typeof import("mod")`)
    /// or left as the bare literal shape (`{ a: number; }`), matching tsc.
    pub has_augmented_named_exports: bool,

    /// Whether the bare `module.exports = X` reads from the `exports`/
    /// `module.exports` object itself (e.g. `module.exports = exports.default`).
    /// Such self-references are circular in tsc, which resolves the module to
    /// `any` (TS7022) and emits no member errors, so TS7 merge suppression must
    /// not apply.
    pub direct_export_reads_exports: bool,
}

impl JsExportSurface {
    fn normalize_property_declaration_order(props: &mut [PropertyInfo]) {
        props.sort_by(
            |a, b| match (a.declaration_order > 0, b.declaration_order > 0) {
                (true, true) => a
                    .declaration_order
                    .cmp(&b.declaration_order)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.type_id.0.cmp(&b.type_id.0)),
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (false, false) => a
                    .name
                    .cmp(&b.name)
                    .then_with(|| a.type_id.0.cmp(&b.type_id.0)),
            },
        );

        for (idx, prop) in props.iter_mut().enumerate() {
            prop.declaration_order = idx as u32 + 1;
        }
    }

    fn merged_declaration_order(existing: u32, overlay: u32) -> u32 {
        match (existing > 0, overlay > 0) {
            (true, true) => existing.min(overlay),
            (true, false) => existing,
            (false, true) => overlay,
            (false, false) => 0,
        }
    }

    fn merge_property_info(
        checker: &mut CheckerState<'_>,
        existing: &PropertyInfo,
        overlay: &PropertyInfo,
    ) -> PropertyInfo {
        let factory = checker.ctx.types.factory();
        PropertyInfo {
            name: existing.name,
            type_id: if existing.type_id == overlay.type_id {
                existing.type_id
            } else {
                factory.union2(existing.type_id, overlay.type_id)
            },
            write_type: if existing.write_type == overlay.write_type {
                existing.write_type
            } else {
                factory.union2(existing.write_type, overlay.write_type)
            },
            optional: existing.optional && overlay.optional,
            readonly: existing.readonly && overlay.readonly,
            is_method: existing.is_method && overlay.is_method,
            is_class_prototype: existing.is_class_prototype || overlay.is_class_prototype,
            visibility: existing.visibility,
            parent_id: existing.parent_id.or(overlay.parent_id),
            declaration_order: Self::merged_declaration_order(
                existing.declaration_order,
                overlay.declaration_order,
            ),
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }
    }

    fn merge_named_exports_into_direct_export_type(
        &self,
        checker: &mut CheckerState<'_>,
        direct_export_type: TypeId,
    ) -> Option<TypeId> {
        if self.named_exports.is_empty()
            || !commonjs_direct_export_supports_named_props(checker.ctx.types, direct_export_type)
        {
            return Some(direct_export_type);
        }

        let mut overlay_by_name: FxHashMap<_, _> = FxHashMap::default();
        for prop in &self.named_exports {
            overlay_by_name.insert(prop.name, prop.clone());
        }

        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type_extended(
            checker.ctx.types,
            direct_export_type,
        ) {
            let mut merged_shape: CallableShape = shape.as_ref().clone();
            let mut merged_props = Vec::new();
            for existing in &shape.properties {
                if let Some(overlay) = overlay_by_name.remove(&existing.name) {
                    merged_props.push(Self::merge_property_info(checker, existing, &overlay));
                } else {
                    merged_props.push(existing.clone());
                }
            }
            merged_props.extend(overlay_by_name.into_values());
            Self::normalize_property_declaration_order(&mut merged_props);
            merged_shape.properties = merged_props;
            return Some(checker.ctx.types.factory().callable(merged_shape));
        }

        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
            checker.ctx.types,
            direct_export_type,
        ) {
            let mut merged_props = Vec::new();
            for existing in &shape.properties {
                if let Some(overlay) = overlay_by_name.remove(&existing.name) {
                    merged_props.push(Self::merge_property_info(checker, existing, &overlay));
                } else {
                    merged_props.push(existing.clone());
                }
            }
            merged_props.extend(overlay_by_name.into_values());
            Self::normalize_property_declaration_order(&mut merged_props);

            return Some(
                checker
                    .ctx
                    .types
                    .factory()
                    .object_with_shape_metadata(merged_props, &shape),
            );
        }

        None
    }

    pub const fn empty() -> Self {
        Self {
            direct_export_type: None,
            named_exports: Vec::new(),
            prototype_members: Vec::new(),
            has_commonjs_exports: false,
            has_augmented_named_exports: false,
            direct_export_reads_exports: false,
        }
    }

    /// Look up a named export by name within this surface.
    ///
    /// Checks `named_exports` first, then `prototype_members`.
    /// Returns the `TypeId` if found. This is the canonical way to check
    /// whether a specific named export exists in a CommonJS module's surface
    /// without re-scanning the AST.
    ///
    /// When [`Self::suppresses_expando_merge`] holds, `to_type_id` drops
    /// `named_exports` from the module's type entirely (TS7: the illegal
    /// `module.exports = X` + sibling-property mix keeps the module type
    /// exactly `X`) — a lookup must agree, or a consumer like a `require()`
    /// destructure sees a name that the type it actually got never had.
    pub fn lookup_named_export(
        &self,
        name: &str,
        types: &dyn tsz_solver::construction::TypeDatabase,
    ) -> Option<TypeId> {
        let name_atom = types.intern_string(name);
        if !self.suppresses_expando_merge()
            && let Some(prop) = self.named_exports.iter().find(|p| p.name == name_atom)
        {
            return Some(prop.type_id);
        }
        if let Some(prop) = self.prototype_members.iter().find(|p| p.name == name_atom) {
            return Some(prop.type_id);
        }
        None
    }

    /// Check whether this surface has a named export with the given name.
    pub fn has_named_export(
        &self,
        name: &str,
        types: &dyn tsz_solver::construction::TypeDatabase,
    ) -> bool {
        self.lookup_named_export(name, types).is_some()
    }

    /// TS7: a bare `module.exports = X` combined with sibling property exports
    /// (`module.exports.p = ...` / `exports.p = ...`) is an illegal mix that
    /// tsc reports as TS2309. In that case the module's type is exactly `X`;
    /// the siblings are NOT folded in as expando members (so accessing them
    /// surfaces TS2339), and the type is not tagged as a `typeof import("mod")`
    /// namespace.
    /// TS7: a bare `module.exports = X` combined with sibling property exports
    /// is an illegal mix that tsc reports as TS2309. This condition drives only
    /// the diagnostic — it holds even for the circular
    /// `module.exports = exports.default` self-reference (which tsc still flags).
    pub(crate) const fn has_commonjs_export_assignment_conflict(&self) -> bool {
        self.direct_export_type.is_some() && self.has_augmented_named_exports
    }

    /// Whether the sibling property exports must NOT be folded into the direct
    /// export's type — the module type is exactly `X`, so member accesses of the
    /// siblings surface TS2339. This is the assignment conflict minus the
    /// circular self-reference case, which tsc resolves to `any` (no member
    /// errors).
    pub(crate) const fn suppresses_expando_merge(&self) -> bool {
        self.has_commonjs_export_assignment_conflict() && !self.direct_export_reads_exports
    }

    /// Whether the synthesized export type is namespace-like — displayed as
    /// `typeof import("mod")` rather than the bare shape of a single
    /// `module.exports = X`. False once TS7 merge suppression applies, since
    /// the type is then exactly `X`.
    pub(crate) const fn is_namespace_like(&self) -> bool {
        !self.suppresses_expando_merge()
            && (self.direct_export_type.is_none()
                || self.has_augmented_named_exports
                || !self.prototype_members.is_empty())
    }

    /// Build the final TypeId for this export surface.
    /// Merges direct export type with named exports into a single type.
    pub fn to_type_id(&self, checker: &mut CheckerState<'_>) -> Option<TypeId> {
        if !self.has_commonjs_exports {
            return None;
        }

        let factory = checker.ctx.types.factory();
        let can_merge_named_exports = self.direct_export_type.is_none_or(|direct_export_type| {
            commonjs_direct_export_supports_named_props(checker.ctx.types, direct_export_type)
        });

        let namespace_type = if can_merge_named_exports && !self.named_exports.is_empty() {
            let mut named_exports = self.named_exports.clone();
            Self::normalize_property_declaration_order(&mut named_exports);
            Some(factory.object(named_exports))
        } else {
            None
        };

        match (self.direct_export_type, namespace_type) {
            // TS7: keep the module type as exactly `X` when the bare
            // `module.exports = X` is mixed with sibling property exports —
            // the siblings are illegal (TS2309), not expando members.
            (Some(dt), Some(_)) if self.suppresses_expando_merge() => Some(dt),
            (Some(dt), Some(ns)) => Some(
                self.merge_named_exports_into_direct_export_type(checker, dt)
                    .unwrap_or_else(|| factory.intersection2(dt, ns)),
            ),
            (Some(dt), None) => Some(dt),
            (None, Some(ns)) => Some(ns),
            (None, None) => None,
        }
    }

    /// Build the final TypeId, also storing the display name for diagnostics.
    ///
    /// Only applies the display name when the result includes named exports
    /// (i.e., it's a namespace-like type). A bare `module.exports = X` returns
    /// the raw type without a namespace display name, preserving the original
    /// type shape in diagnostics (e.g., `{ a: number }` instead of `typeof import("mod")`).
    pub fn to_type_id_with_display_name(
        &self,
        checker: &mut CheckerState<'_>,
        display_name: Option<String>,
    ) -> Option<TypeId> {
        let type_id = self.to_type_id(checker)?;
        // Only tag with display name when the synthesized type is namespace-like
        // — i.e. either there's no direct module.exports = X, or the named
        // exports include genuine augmentation beyond the direct-export object
        // literal's own properties. A file that exports a single object
        // literal (`module.exports = { a: 0 }`) keeps the raw `{ a: number; }`
        // shape in diagnostics; tsc shows the literal, not `typeof import("mod")`.
        let synth_is_namespace_like = self.is_namespace_like();
        if let Some(name) = display_name
            && synth_is_namespace_like
            && !self.named_exports.is_empty()
            && self.direct_export_type.is_none_or(|direct_export_type| {
                commonjs_direct_export_supports_named_props(checker.ctx.types, direct_export_type)
            })
        {
            checker.ctx.namespace_module_names.insert(type_id, name);
        }
        Some(type_id)
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn commonjs_direct_export_supports_named_exports(
        &self,
        direct_export_type: TypeId,
    ) -> bool {
        commonjs_direct_export_supports_named_props(self.ctx.types, direct_export_type)
    }

    fn last_direct_module_export_assignment_for_file(
        &self,
        target_file_idx: usize,
    ) -> Option<(usize, tsz_parser::parser::NodeIndex)> {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        let mut last = None;

        for (stmt_ordinal, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            let rhs_expr = if stmt_node.kind
                == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
            {
                target_arena
                    .get_expression_statement(stmt_node)
                    .and_then(|stmt| {
                        self.direct_commonjs_module_export_assignment_rhs(
                            target_arena,
                            stmt.expression,
                        )
                    })
            } else if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                self.direct_commonjs_module_export_rhs_from_variable_statement(
                    target_arena,
                    stmt_idx,
                )
            } else {
                None
            };

            if let Some(rhs_expr) = rhs_expr {
                last = Some((stmt_ordinal, rhs_expr));
            }
        }

        last
    }

    /// The `module.exports` / `exports` target node of the file's last bare
    /// `module.exports = X` assignment (including `var y = module.exports = X`).
    /// Used to anchor the TS2309 diagnostic when the module also has sibling
    /// property exports (TS7).
    pub(crate) fn last_direct_module_export_assignment_lhs_for_file(
        &self,
        target_file_idx: usize,
    ) -> Option<tsz_parser::parser::NodeIndex> {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        let mut last = None;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            let lhs = if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
            {
                target_arena
                    .get_expression_statement(stmt_node)
                    .and_then(|stmt| {
                        self.direct_commonjs_module_export_assignment_lhs(
                            target_arena,
                            stmt.expression,
                        )
                    })
            } else if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                self.direct_commonjs_module_export_lhs_from_variable_statement(
                    target_arena,
                    stmt_idx,
                )
            } else {
                None
            };

            if let Some(lhs) = lhs {
                last = Some(lhs);
            }
        }

        last
    }

    /// Whether `expr_idx` reads from the CommonJS `exports` / `module.exports`
    /// object — a bare `exports` identifier, a `module.exports` access, or any
    /// property/element access chain rooted at one of those (e.g.
    /// `exports.default`, `exports["default"]`, `module.exports.foo`).
    fn commonjs_expression_roots_at_exports(
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let idx = arena.skip_parenthesized(expr_idx);
        let Some(node) = arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports");
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(access) = arena.get_access_expr(node) else {
                return false;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let base = arena.skip_parenthesized(access.expression);
                let is_module = arena
                    .get_identifier_at(base)
                    .is_some_and(|ident| ident.escaped_text == "module");
                let is_exports = arena
                    .get_identifier_at(access.name_or_argument)
                    .is_some_and(|ident| ident.escaped_text == "exports");
                if is_module && is_exports {
                    return true;
                }
            }
            return Self::commonjs_expression_roots_at_exports(arena, access.expression);
        }

        false
    }

    fn direct_module_export_object_literal_seed_props(
        &mut self,
        direct_export_type: TypeId,
        force_optional: bool,
    ) -> Vec<PropertyInfo> {
        let shape = crate::query_boundaries::checkers::generic::get_object_shape(
            self.ctx.types,
            direct_export_type,
        )
        .map(|shape| shape.as_ref().clone())
        .or_else(|| {
            let widened = crate::query_boundaries::common::widen_freshness(
                self.ctx.types,
                direct_export_type,
            );
            crate::query_boundaries::checkers::generic::get_object_shape(self.ctx.types, widened)
                .map(|shape| shape.as_ref().clone())
        });
        let Some(shape) = shape else {
            return Vec::new();
        };

        let mut props = shape.properties;
        JsExportSurface::normalize_property_declaration_order(&mut props);
        props
            .into_iter()
            .map(|mut prop| {
                prop.optional = force_optional;
                prop
            })
            .collect()
    }

    fn all_direct_module_export_object_literal_seed_props_for_file(
        &mut self,
        target_file_idx: usize,
    ) -> Vec<PropertyInfo> {
        use rustc_hash::FxHashMap;

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let Some(source_file) = target_arena.source_files.first() else {
            return Vec::new();
        };

        let mut rhs_exprs = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            let rhs_expr = if stmt_node.kind
                == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
            {
                target_arena
                    .get_expression_statement(stmt_node)
                    .and_then(|stmt| {
                        self.direct_commonjs_module_export_assignment_rhs(
                            &target_arena,
                            stmt.expression,
                        )
                    })
            } else if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                self.direct_commonjs_module_export_rhs_from_variable_statement(
                    &target_arena,
                    stmt_idx,
                )
            } else {
                None
            };

            if let Some(rhs_expr) = rhs_expr {
                rhs_exprs.push(rhs_expr);
            }
        }

        let mut pending: FxHashMap<tsz_common::Atom, PropertyInfo> = FxHashMap::default();
        let mut ordered_names = Vec::new();

        let last_direct_index = rhs_exprs.len().saturating_sub(1);
        for (index, rhs_expr) in rhs_exprs.into_iter().enumerate() {
            let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr, None);
            let force_optional = index != last_direct_index;
            for prop in
                self.direct_module_export_object_literal_seed_props(rhs_type, force_optional)
            {
                if !pending.contains_key(&prop.name) {
                    ordered_names.push(prop.name);
                }
                pending.insert(prop.name, prop);
            }
        }

        ordered_names
            .into_iter()
            .enumerate()
            .filter_map(|(idx, name)| {
                pending.remove(&name).map(|mut prop| {
                    prop.declaration_order = idx as u32 + 1;
                    prop
                })
            })
            .collect()
    }

    /// Main entry point: resolve the complete JS export surface for a target file.
    ///
    /// This is the ONE AUTHORITY for synthesizing JS/CommonJS export shapes.
    /// All consumers should call this instead of independently re-deriving
    /// export properties from the AST.
    ///
    /// Results are cached per target file index.
    pub(crate) fn resolve_js_export_surface(&mut self, target_file_idx: usize) -> JsExportSurface {
        // Check cache first
        if let Some(cached) = self.ctx.js_export_surface_cache.get(&target_file_idx) {
            return cached.clone();
        }

        // Guard against self-recursive synthesis. This can happen when typing
        // `module.exports` asks for the current file's export surface while the
        // same surface is still being derived from `Object.defineProperty(...)`
        // calls in that file.
        if self
            .ctx
            .js_export_surface_resolution_set
            .contains_key(&target_file_idx)
        {
            return JsExportSurface::empty();
        }
        self.ctx
            .js_export_surface_resolution_set
            .insert(target_file_idx, None);

        let surface = self.compute_js_export_surface(target_file_idx);
        self.ctx
            .js_export_surface_resolution_set
            .remove(&target_file_idx);

        // Cache the result
        self.ctx
            .js_export_surface_cache
            .insert(target_file_idx, surface.clone());

        surface
    }

    /// Resolve JS export surface for a module specifier (resolves to file index first).
    pub(crate) fn resolve_js_export_surface_for_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<JsExportSurface> {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))?;

        Some(self.resolve_js_export_surface(target_file_idx))
    }

    /// Look up a single named export from a CommonJS module's export surface.
    ///
    /// This is the canonical replacement for `resolve_direct_commonjs_assignment_export_type`.
    /// Instead of re-scanning the target file's AST for `exports.foo = ...` patterns,
    /// it uses the cached `JsExportSurface` which already contains all named exports.
    pub(crate) fn resolve_js_export_named_type(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let surface = self.resolve_js_export_surface_for_module(module_name, source_file_idx)?;
        surface.lookup_named_export(export_name, self.ctx.types)
    }

    /// Resolve the declaring class symbol for a named CommonJS export.
    ///
    /// JSDoc type-position consumers use this provenance when the synthesized
    /// export surface only exposes a raw callable value for `exports.K = K`,
    /// but the exported RHS is a class declaration whose instance side is the
    /// actual annotation type.
    pub(crate) fn resolve_js_export_named_class_symbol(
        &self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<(SymbolId, usize)> {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))?;
        self.commonjs_named_export_class_symbol_for_file(target_file_idx, export_name)
    }

    /// Check whether a CommonJS module has a named export (without computing its type).
    ///
    /// Uses the cached export surface. Canonical way to suppress TS2305 for
    /// names that exist as `exports.foo = ...` or `module.exports.foo = ...`.
    pub(crate) fn js_export_surface_has_export(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        self.resolve_js_export_surface_for_module(module_name, source_file_idx)
            .is_some_and(|surface| surface.has_named_export(export_name, self.ctx.types))
    }

    /// Check whether a CommonJS module has an export surface but not the requested
    /// named export. This lets import validation prefer the semantic JS export
    /// surface over the binder's syntactic `module_exports` table for cases like
    /// `exports.x = void 0`, which tsc does not expose as a named export.
    pub(crate) fn js_commonjs_export_surface_lacks_export(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let Some(target_file_idx) = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_is_js = target_arena
            .source_files
            .first()
            .is_some_and(|source_file| is_js_file_name(&source_file.file_name));
        if !target_is_js {
            return false;
        }

        let surface = self.resolve_js_export_surface(target_file_idx);
        surface.has_commonjs_exports && !surface.has_named_export(export_name, self.ctx.types)
    }

    /// Whether `module_name` resolves to a CommonJS JS module with an export
    /// surface at all, without asking whether any particular name is present.
    ///
    /// `JsExportSurface::named_exports` records every `module.exports.p = …`
    /// / `exports.p = …` write syntactically, even ones a TS7
    /// `module.exports = X` assignment conflict (TS2309) later drops from
    /// the module's real merged type — `has_named_export` alone can't tell
    /// "genuinely exported" from "written but excluded by the conflict"
    /// (`JsExportSurface::suppresses_expando_merge`). A caller that has
    /// already resolved the `require()` call's own (conflict-aware) type and
    /// found a property missing from *that* only needs this to confirm the
    /// source is a real CJS module — not re-derive absence from the
    /// syntactic surface.
    pub(crate) fn js_commonjs_require_target_is_js_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let Some(target_file_idx) = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_is_js = target_arena
            .source_files
            .first()
            .is_some_and(|source_file| is_js_file_name(&source_file.file_name));
        if !target_is_js {
            return false;
        }

        self.resolve_js_export_surface(target_file_idx)
            .has_commonjs_exports
    }

    /// Build the namespace type for a CommonJS file from its export surface.
    ///
    /// This is the canonical replacement for `commonjs_namespace_type_for_file`.
    /// Instead of re-scanning the AST, it builds the namespace type from the
    /// cached `JsExportSurface`.
    pub(crate) fn js_export_surface_namespace_type(
        &mut self,
        target_file_idx: usize,
    ) -> Option<TypeId> {
        let surface = self.resolve_js_export_surface(target_file_idx);
        if !surface.has_commonjs_exports {
            return None;
        }
        let type_id = surface.to_type_id(self)?;
        // Mirror `to_type_id_with_display_name`: only tag the synthesized
        // namespace type when it actually represents a namespace-like surface.
        // A file that just does `module.exports = { … }` (no augmentation, no
        // prototype members) gets the bare literal type in diagnostics, not
        // `typeof import("mod")`.
        let synth_is_namespace_like = surface.is_namespace_like();
        if synth_is_namespace_like
            && let Some(specifier) = self.ctx.module_specifiers.get(&(target_file_idx as u32))
        {
            self.ctx
                .namespace_module_names
                .insert(type_id, specifier.clone());
        }
        Some(type_id)
    }

    /// Compute the JS export surface from scratch (uncached).
    fn compute_js_export_surface(&mut self, target_file_idx: usize) -> JsExportSurface {
        if self.source_file_idx_has_esm_syntax(target_file_idx) {
            return JsExportSurface::empty();
        }

        let mut surface = JsExportSurface::empty();
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let target_is_external_module = self
            .ctx
            .get_binder_for_file(target_file_idx)
            .is_some_and(tsz_binder::BinderState::is_external_module);

        let last_direct_export =
            self.last_direct_module_export_assignment_for_file(target_file_idx);

        // 1. Collect direct `module.exports = X` assignment
        surface.direct_export_type = last_direct_export
            .map(|(_, rhs_expr)| {
                // `module.exports = require('./y')` re-exports another module:
                // tsc checks every `module.exports.<name>` member write
                // (before or after it) against the required module's
                // typeof-import type — TS2339 on 'typeof import("y")' — not
                // against an expando-extensible `any`. Scoped to the current
                // file: `build_typeof_import_namespace_type` resolves the
                // specifier relative to `ctx.current_file_idx`.
                if target_file_idx == self.ctx.current_file_idx
                    && let Some(specifier) = self.get_require_module_specifier(rhs_expr)
                    && let Some(namespace_type) =
                        self.build_typeof_import_namespace_type(&specifier, None)
                {
                    return namespace_type;
                }
                // An explicit JSDoc `@type` on the assignment statement is a
                // declared type, like a variable's `: T` annotation — it must
                // reach later `module.exports` reads exactly as written, not
                // re-widened for "fresh literal" display the way a plain
                // object-literal export's inferred shape is. Widening it here
                // would turn e.g. `{ color: "red" | "blue" }`'s narrowed
                // members into `{ color: string }` while keeping the alias's
                // display name, producing a same-name-different-shape
                // mismatch against every other reference to the same alias.
                if target_file_idx == self.ctx.current_file_idx
                    && let Some(declared_type) =
                        self.commonjs_export_rhs_jsdoc_declared_type(rhs_expr)
                {
                    return declared_type;
                }
                let expando_root = target_arena
                    .get_identifier_at(rhs_expr)
                    .map(|ident| ident.escaped_text.as_str());
                let rhs_type =
                    self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr, expando_root);
                self.widen_type_for_display(rhs_type)
            })
            .filter(|&rhs_type| rhs_type != TypeId::UNDEFINED);

        // Record whether the bare `module.exports = X` reads from `exports`/
        // `module.exports` itself (e.g. `module.exports = exports.default`).
        // Such self-references are circular in tsc (resolved to `any`), so TS7
        // merge suppression must be skipped for them.
        surface.direct_export_reads_exports = last_direct_export.is_some_and(|(_, rhs_expr)| {
            Self::commonjs_expression_roots_at_exports(&target_arena, rhs_expr)
        });

        // Publish the direct export type for the rest of this computation:
        // step 2 below infers sibling `module.exports.p = ...` RHS types, and a
        // function RHS whose body reads `module.exports` re-enters
        // `resolve_js_export_surface`, which hands back the empty placeholder.
        // The read then must not be typed (and node-cached) as an empty
        // namespace — tsc types it as the export= target. The recursion-guard
        // entry (and this value with it) is removed by
        // `resolve_js_export_surface` once the computation finishes.
        if surface.direct_export_type.is_some() {
            self.ctx
                .js_export_surface_resolution_set
                .insert(target_file_idx, surface.direct_export_type);
        }

        // 2. Seed named exports from a direct object-like export, then collect later
        // property exports (`exports.foo = ...`, `module.exports.foo = ...`) that
        // augment the final export object after the last full `module.exports = ...`.
        let mut props =
            self.all_direct_module_export_object_literal_seed_props_for_file(target_file_idx);
        let seed_count = props.len();
        self.augment_namespace_props_with_commonjs_exports_for_file_after(
            target_file_idx,
            &mut props,
            None,
        );
        // Track whether the augment step contributed any real named exports
        // beyond what the direct-export object literal seeded — used downstream
        // to decide whether to tag the synthesized type as
        // `typeof import("mod")` (only when there is genuine augmentation).
        surface.has_augmented_named_exports = props.len() > seed_count;
        JsExportSurface::normalize_property_declaration_order(&mut props);
        surface.named_exports = props;

        // 3. Collect prototype property assignments for constructor functions
        surface.prototype_members = self.collect_prototype_exports_for_file(target_file_idx);

        // A file with `Object.defineProperty(exports, ...)` calls is a CommonJS
        // module even when every name argument resolves to a non-literal
        // expression. Without this, the file's synthesized export type is
        // dropped and import-side accesses fall back to ANY rather than
        // surfacing as TS2339. (tsc's binder recognizes the `defineProperty`
        // shape as an export indicator regardless of name extractability.)
        let has_define_property_call = self.file_has_define_property_export_call(target_file_idx);

        surface.has_commonjs_exports = surface.direct_export_type.is_some()
            || !surface.named_exports.is_empty()
            || (!target_is_external_module && !surface.prototype_members.is_empty())
            || has_define_property_call;

        surface
    }

    /// Compute the direct `module.exports = X` type for a target file.
    fn compute_direct_module_export_type(&mut self, target_file_idx: usize) -> Option<TypeId> {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        let mut rhs_expr = None;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT {
                let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                    continue;
                };
                if let Some(found_rhs) =
                    self.direct_commonjs_module_export_assignment_rhs(target_arena, stmt.expression)
                {
                    rhs_expr = Some(found_rhs);
                    continue;
                }
            }
            if stmt_node.kind != tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            if let Some(found_rhs) = self
                .direct_commonjs_module_export_rhs_from_variable_statement(target_arena, stmt_idx)
            {
                rhs_expr = Some(found_rhs);
            }
        }

        let rhs_expr = rhs_expr?;
        let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr, None);
        let rhs_type =
            crate::query_boundaries::common::widen_literal_type(self.ctx.types, rhs_type);
        (rhs_type != TypeId::UNDEFINED).then_some(rhs_type)
    }

    pub(crate) fn direct_commonjs_module_export_rhs_from_variable_statement(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        stmt_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<tsz_parser::parser::NodeIndex> {
        let stmt_node = arena.get(stmt_idx)?;
        let var_stmt = arena.get_variable(stmt_node)?;

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = arena.get(decl_list_idx)?;
            let decl_list = arena.get_variable(decl_list_node)?;
            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = arena.get(decl_idx)?;
                let decl = arena.get_variable_declaration(decl_node)?;
                if decl.initializer.is_none() {
                    continue;
                }
                if let Some(found_rhs) =
                    self.direct_commonjs_module_export_assignment_rhs(arena, decl.initializer)
                {
                    return Some(found_rhs);
                }
            }
        }

        None
    }

    /// The `module.exports` / `exports` target node of a bare CommonJS export
    /// assignment nested in a `var y = module.exports = X` initializer.
    fn direct_commonjs_module_export_lhs_from_variable_statement(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        stmt_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<tsz_parser::parser::NodeIndex> {
        let stmt_node = arena.get(stmt_idx)?;
        let var_stmt = arena.get_variable(stmt_node)?;

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = arena.get(decl_list_idx)?;
            let decl_list = arena.get_variable(decl_list_node)?;
            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = arena.get(decl_idx)?;
                let decl = arena.get_variable_declaration(decl_node)?;
                if decl.initializer.is_none() {
                    continue;
                }
                if let Some(found_lhs) =
                    self.direct_commonjs_module_export_assignment_lhs(arena, decl.initializer)
                {
                    return Some(found_lhs);
                }
            }
        }

        None
    }

    /// Collect prototype property assignments for constructor functions exported from a file.
    ///
    /// Scans for patterns like:
    /// - `Ctor.prototype.method = function() { ... }`
    /// - `Ctor.prototype = { method: function() { ... } }`
    fn collect_prototype_exports_for_file(&mut self, target_file_idx: usize) -> Vec<PropertyInfo> {
        use tsz_parser::parser::NodeIndex;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Phase 1: Collect prototype member assignments (ctor_name, member_name, rhs_idx)
        // from the arena. This borrows the arena immutably.
        let pending: Vec<(String, String, NodeIndex)> = {
            let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
            let Some(source_file) = target_arena.source_files.first() else {
                return Vec::new();
            };

            let mut pending = Vec::new();
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = target_arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    continue;
                }
                let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                    continue;
                };
                let Some(expr_node) = target_arena.get(stmt.expression) else {
                    continue;
                };
                if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                    continue;
                }
                let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                    continue;
                };
                if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                    continue;
                }

                if let Some((ctor_name, member_name)) =
                    Self::parse_prototype_member_assignment(target_arena, binary.left)
                {
                    pending.push((ctor_name, member_name, binary.right));
                }
            }
            pending
        };

        if pending.is_empty() {
            return Vec::new();
        }

        // Phase 2: Infer types for each RHS (borrows self mutably).
        let mut prototype_props: FxHashMap<String, Vec<(String, TypeId)>> = FxHashMap::default();
        for (ctor_name, member_name, rhs_idx) in pending {
            let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_idx, None);
            if rhs_type != TypeId::UNDEFINED {
                prototype_props
                    .entry(ctor_name)
                    .or_default()
                    .push((member_name, rhs_type));
            }
        }

        // Phase 3: Flatten into PropertyInfo entries
        let mut result = Vec::new();
        for members in prototype_props.values() {
            for (idx, (member_name, member_type)) in members.iter().enumerate() {
                let name_atom = self.ctx.types.intern_string(member_name);
                result.push(PropertyInfo {
                    name: name_atom,
                    type_id: *member_type,
                    write_type: *member_type,
                    optional: false,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: idx as u32 + 1,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                    non_widening: false,
                });
            }
        }

        result
    }

    /// Parse `Ctor.prototype.member` from the LHS of an assignment.
    /// Returns `(constructor_name, member_name)` if the pattern matches.
    fn parse_prototype_member_assignment(
        arena: &tsz_parser::parser::NodeArena,
        idx: tsz_parser::parser::NodeIndex,
    ) -> Option<(String, String)> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let outer_access = arena.get_access_expr(node)?;

        // Get the member name (rightmost part: `.member`)
        let member_ident = arena.get_identifier_at(outer_access.name_or_argument)?;
        let member_name = member_ident.escaped_text.to_string();

        // Check that the expression is `Ctor.prototype`
        let proto_node = arena.get(outer_access.expression)?;
        if proto_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let proto_access = arena.get_access_expr(proto_node)?;

        let is_prototype = arena
            .get_identifier_at(proto_access.name_or_argument)
            .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return None;
        }

        // Get the constructor name
        let ctor_node = arena.get(proto_access.expression)?;
        if ctor_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ctor_ident = arena.get_identifier(ctor_node)?;
        let ctor_name = ctor_ident.escaped_text.to_string();

        Some((ctor_name, member_name))
    }

    pub(crate) fn commonjs_named_export_class_symbol_for_file(
        &self,
        target_file_idx: usize,
        export_name: &str,
    ) -> Option<(SymbolId, usize)> {
        // A file with ESM syntax is not a CommonJS module: `module.exports.X`
        // there is an ordinary property write, not an export, so tsc keeps
        // reporting the member as missing (TS2694).
        if self.source_file_idx_has_esm_syntax(target_file_idx) {
            return None;
        }

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let source_file = target_arena.source_files.first()?;

        for &stmt_idx in source_file.statements.nodes.iter().rev() {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(rhs_idx) = Self::commonjs_named_export_assignment_rhs(
                target_arena,
                stmt.expression,
                export_name,
            ) else {
                continue;
            };
            let Some(rhs_node) = target_arena.get(rhs_idx) else {
                continue;
            };
            if rhs_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(sym_id) = target_binder.resolve_identifier(target_arena, rhs_idx) else {
                continue;
            };
            if target_binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS))
            {
                return Some((sym_id, target_file_idx));
            }
        }

        None
    }

    /// Whether a named CommonJS export originates from a `module.exports = { … }`
    /// object-literal assignment (`module.exports = { X }`). Such members carry
    /// only value meaning: TS7 reports a bare/import-type reference to them as
    /// TS2749/TS2694, unlike `exports.X = class` or `module.exports = Class`,
    /// which export type meaning.
    pub(crate) fn commonjs_named_export_is_object_literal_member(
        &self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let Some(target_file_idx) = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };
        for &stmt_idx in source_file.statements.nodes.iter().rev() {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = target_arena.get(stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16
                || !Self::is_module_exports_target_in_arena(target_arena, binary.left)
            {
                continue;
            }
            let Some(rhs_node) = target_arena.get(binary.right) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            let Some(obj) = target_arena.get_literal_expr(rhs_node) else {
                continue;
            };
            for &element_idx in &obj.elements.nodes {
                let Some(element_node) = target_arena.get(element_idx) else {
                    continue;
                };
                // Property assignment (`{ X: … }`) or shorthand (`{ X }`).
                let name_idx = if element_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    target_arena
                        .get_property_assignment(element_node)
                        .map(|prop| prop.name)
                } else if element_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    target_arena
                        .get_shorthand_property(element_node)
                        .map(|prop| prop.name)
                } else {
                    None
                };
                if let Some(name_idx) = name_idx
                    && crate::types_domain::queries::core::get_literal_property_name(
                        target_arena,
                        name_idx,
                    )
                    .as_deref()
                        == Some(export_name)
                {
                    return true;
                }
            }
        }
        false
    }

    fn commonjs_named_export_assignment_rhs(
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        export_name: &str,
    ) -> Option<NodeIndex> {
        let expr_node = arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        Self::commonjs_named_export_lhs_matches(arena, binary.left, export_name)
            .then_some(binary.right)
    }

    fn commonjs_named_export_lhs_matches(
        arena: &tsz_parser::parser::NodeArena,
        lhs_idx: NodeIndex,
        export_name: &str,
    ) -> bool {
        let Some(lhs_node) = arena.get(lhs_idx) else {
            return false;
        };
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = arena.get_access_expr(lhs_node) else {
            return false;
        };
        if Self::commonjs_static_member_name(arena, access.name_or_argument).as_deref()
            != Some(export_name)
        {
            return false;
        }

        if arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return true;
        }

        let Some(base_node) = arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && base_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(base_access) = arena.get_access_expr(base_node) else {
            return false;
        };
        arena
            .get_identifier_at(base_access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && Self::commonjs_static_member_name(arena, base_access.name_or_argument).as_deref()
                == Some("exports")
    }

    fn commonjs_static_member_name(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string()),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "js_exports/tests.rs"]
mod tests;
