impl<'a> FreeInferChecker<'a> {
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if matches!(key, TypeData::Infer(_)) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally (TypeParameter is included here because this
        // walker, by design, does not descend into TypeParameter
        // constraints/defaults). Short-circuit before the recursion-guard
        // enter/leave dance. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // TypeParameter/Infer: do NOT walk into constraints/defaults.
            // Structural `infer` patterns in constraints (e.g., from type alias
            // definitions like `type Foo = X extends Bar<infer V> ? V : never`)
            // are definitional, not live inference variables.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

/// Check whether `type_id` contains a *free* reference to a type parameter
/// other than `excluded_name`, treating each `TypeParameter`/`Infer` as a leaf.
///
/// A `TypeParameter`'s `constraint`/`default` are metadata, not uses:
/// the iteration variable `K` in `{ [K in keyof T as ...]: T[K] }` carries
/// a stale `keyof T` constraint after `T` is substituted, since the `K`
/// instances inside the body still reference the pre-substitution record.
pub fn contains_free_type_parameters_except_name(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    excluded_name: Atom,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }
            let Some(data) = types.lookup(current) else {
                continue;
            };
            match &data {
                TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                    if info.name != excluded_name {
                        return true;
                    }
                    // Skip the parameter's `constraint`/`default` — those are
                    // metadata for the parameter, not uses by the enclosing
                    // type. Same reason applies to Mapped/Function/Callable
                    // type-param lists handled in the visit_structural_children
                    // path below.
                    continue;
                }
                TypeData::ThisType | TypeData::BoundParameter(_) => return true,
                _ => {}
            }
            visit_structural_children(types, current, &data, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

/// Variant of [`super::visitor::for_each_child_by_id`] that skips type-
/// parameter `constraint`/`default` metadata on `Mapped`, `Function`, and
/// `Callable` types. Used by free-type-parameter checks that must treat
/// parameter-declaration metadata as bound by the host, not as free uses.
fn visit_structural_children<F>(db: &dyn TypeDatabase, type_id: TypeId, data: &TypeData, mut f: F)
where
    F: FnMut(TypeId),
{
    match data {
        TypeData::Mapped(mapped_id) => {
            let mapped = db.get_mapped(*mapped_id);
            f(mapped.constraint);
            f(mapped.template);
            if let Some(name_type) = mapped.name_type {
                f(name_type);
            }
        }
        TypeData::Function(func_id) => {
            let sig = db.function_shape(*func_id);
            f(sig.return_type);
            if let Some(this_type) = sig.this_type {
                f(this_type);
            }
            if let Some(predicate) = sig.type_predicate.as_ref()
                && let Some(predicate_type) = predicate.type_id
            {
                f(predicate_type);
            }
            for param in &sig.params {
                f(param.type_id);
            }
        }
        TypeData::Callable(callable_id) => {
            let callable = db.callable_shape(*callable_id);
            for sig in callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
            {
                f(sig.return_type);
                if let Some(this_type) = sig.this_type {
                    f(this_type);
                }
                if let Some(predicate) = sig.type_predicate.as_ref()
                    && let Some(predicate_type) = predicate.type_id
                {
                    f(predicate_type);
                }
                for param in &sig.params {
                    f(param.type_id);
                }
            }
            for prop in &callable.properties {
                f(prop.type_id);
                f(prop.write_type);
            }
            if let Some(sig) = callable.string_index.as_ref() {
                f(sig.key_type);
                f(sig.value_type);
            }
            if let Some(sig) = callable.number_index.as_ref() {
                f(sig.key_type);
                f(sig.value_type);
            }
        }
        _ => super::visitor::for_each_child_by_id(db, type_id, f),
    }
}

#[allow(dead_code)]
struct ShallowContainsTypeChecker<'a> {
    types: &'a dyn TypeDatabase,
    name: Atom,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

#[allow(dead_code)]
impl<'a> ShallowContainsTypeChecker<'a> {
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        // Direct match: is this type parameter the one we're looking for?
        if matches!(&key, TypeData::TypeParameter(info) if info.name == self.name) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally. Note: `TypeParameter(_)` is also a terminal
        // here — by design "shallow" does not descend into constraints —
        // but we exclude it from this short-circuit because the positive
        // match above already drained the matching name. Any remaining
        // `TypeParameter` is a non-match terminal. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // Do NOT traverse into TypeParameter constraints/defaults — that's
            // the whole point of the "shallow" variant. We only check if the
            // type parameter itself matches, not what its constraint contains.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

/// Check if a type is a literal type (`TypeDatabase` version).
pub fn is_literal_type_through_type_constraints(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a function type (`TypeDatabase` version).
pub fn is_function_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (`TypeDatabase` version).
pub fn is_object_like_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (`TypeDatabase` version).
pub fn is_empty_object_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let checker = EmptyObjectChecker::new(types);
    checker.check(type_id)
}

/// Classification of object types for freshness tracking.
pub enum ObjectTypeKind {
    /// A regular object type (no index signatures).
    Object(ObjectShapeId),
    /// An object type with index signatures.
    ObjectWithIndex(ObjectShapeId),
    /// Not an object type.
    NotObject,
}

/// Classify a type as an object type kind.
///
/// This is used by the freshness tracking system to determine if a type
/// is a fresh object literal that needs special handling.
pub fn classify_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> ObjectTypeKind {
    if type_id.is_intrinsic() {
        return ObjectTypeKind::NotObject;
    }
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
        _ => ObjectTypeKind::NotObject,
    }
}

/// Visitor to check if a type is a literal type.
struct LiteralTypeChecker;

impl LiteralTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types are never literal types EXCEPT for
        // `BOOLEAN_TRUE` (14) and `BOOLEAN_FALSE` (15) which are reserved
        // intrinsic IDs for the `true` / `false` literal types. All other
        // intrinsic IDs match no arm and fall through to `_ => false`.
        // `is_intrinsic()` is a free `TypeId`-range check; the explicit
        // exception preserves slow-path behaviour without `TypeData`
        // lookup. Same family as #2001 / #2005 / #2008 / #2009 / #2014
        // / #2019 / #2025.
        if type_id.is_intrinsic() {
            return type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE;
        }
        match types.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Enum(_, structural_type)) => Self::check(types, structural_type),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types match no arm. Skip lookup + dispatch.
        // Same family as #2001 / #2005 / #2008 / #2009 / #2014 / #2019 / #2025 / #2032.
        if type_id.is_intrinsic() {
            return false;
        }
        match types.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            // The global `Function` interface is typeof "function" at runtime.
            // Check if this Lazy type is the known boxed Function type.
            Some(TypeData::Lazy(def_id)) => {
                types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types match no arm. Skip lookup + dispatch.
        if type_id.is_intrinsic() {
            return false;
        }
        match types.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Mapped(_)
                | TypeData::Application(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| Self::check(types, constraint)),
            // Lazy types represent unresolved type references (interfaces, classes).
            // Most are object-like at runtime (interfaces/classes), but the global
            // `Function` interface is typeof "function". Check if this Lazy type
            // is the known boxed Function — if so, it's NOT object-like.
            Some(TypeData::Lazy(def_id)) => {
                !types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is an empty object type.
struct EmptyObjectChecker<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> EmptyObjectChecker<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn check(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => self.check(inner),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| self.check(c))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::TypeParamInfo;

    fn traversal_guard() -> crate::recursion::RecursionGuard<TypeId> {
        crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        )
    }

    #[test]
    fn predicate_checker_memo_entry_counts_are_observable() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));
        let u_infer = interner.infer(TypeParamInfo::simple(u_name));
        let wrapper = interner.readonly_type(t_param);

        let mut contains_checker = ContainsTypeChecker {
            types: &interner,
            predicate: |key| matches!(key, TypeData::TypeParameter(_)),
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(contains_checker.check(wrapper));
        assert!(contains_checker.memo_entries() > 0);

        let mut free_type_param_checker = FreeTypeParamChecker {
            types: &interner,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(free_type_param_checker.check(wrapper));
        assert!(free_type_param_checker.memo_entries() > 0);

        let mut free_infer_checker = FreeInferChecker {
            types: &interner,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(free_infer_checker.check(u_infer));
        assert!(free_infer_checker.memo_entries() > 0);

        let mut shallow_checker = ShallowContainsTypeChecker {
            types: &interner,
            name: t_name,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(shallow_checker.check(wrapper));
        assert!(shallow_checker.memo_entries() > 0);
    }
}
