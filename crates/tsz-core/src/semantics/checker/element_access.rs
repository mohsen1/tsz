use crate::bind::{Meaning, ScopeId};
use crate::program::{JavaScriptAssignmentDisposition, SemanticCompletion};
use crate::semantics::relation::RelationMode;
use crate::semantics::types::{
    Completion, DeferredType, ElementAccessMode, IndexKeyKind, ObjectShape, Property, TypeId,
    TypeKind, UnionPolicy,
};
use crate::source::FileId;
use crate::standard_library::{LibraryCallMember, LibraryMemberLookup, LibraryReceiver};
use crate::syntax::{
    AssignmentOperator, Expression, ExpressionKind, ObjectProperty, parse_number_literal,
};

use super::call_model::InferredCallCallee;
use super::relation_diagnostic::RelationDiagnosticStyle;
use super::{Checker, DeclarationModel};

impl Checker<'_> {
    pub(super) fn infer_assignment(
        &mut self,
        file: FileId,
        scope: ScopeId,
        left: &Expression,
        right: &Expression,
    ) -> TypeId {
        match self
            .program
            .javascript_assignments
            .assignment(file, left.id)
        {
            None => self.infer_assignment_expression(file, scope, left, right),
            Some(JavaScriptAssignmentDisposition::Complete(_)) => {
                self.infer_expression(file, scope, right, None)
            }
            Some(JavaScriptAssignmentDisposition::Incomplete) => {
                self.observe_incomplete_javascript_assignment_target(file, scope, left);
                let source = self.infer_expression(file, scope, right, None);
                self.observe_file_completion(file, SemanticCompletion::Deferred);
                source
            }
        }
    }

    fn observe_incomplete_javascript_assignment_target(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) {
        match &expression.kind {
            ExpressionKind::Parenthesized(object) | ExpressionKind::Member { object, .. } => {
                self.observe_incomplete_javascript_assignment_target(file, scope, object);
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.observe_incomplete_javascript_assignment_target(file, scope, object);
                self.infer_expression(file, scope, index, None);
            }
            _ => {
                self.infer_expression(file, scope, expression, None);
            }
        }
    }

    pub(super) fn infer_expression_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) {
        self.infer_expression(file, scope, expression, None);
    }

    pub(super) fn infer_element_access_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        object: &Expression,
        index: &Expression,
        mode: ElementAccessMode,
    ) -> TypeId {
        let object = self.infer_expression(file, scope, object, None);
        let index = self.infer_expression(file, scope, index, None);
        let query = self
            .store
            .intern(TypeKind::Deferred(DeferredType::ElementAccess {
                object,
                index,
                mode,
            }));
        self.complete_type(query).unwrap_or(query)
    }

    pub(super) fn infer_element_access_call_callee(
        &mut self,
        file: FileId,
        scope: ScopeId,
        object: &Expression,
        index: &Expression,
    ) -> InferredCallCallee {
        let authored_readonly = self.authored_readonly_array_receiver(file, scope, object);
        let object = self.infer_expression(file, scope, object, None);
        let index = self.infer_expression(file, scope, index, None);
        let query = self
            .store
            .intern(TypeKind::Deferred(DeferredType::ElementAccess {
                object,
                index,
                mode: ElementAccessMode::Read,
            }));
        if let TypeKind::LiteralString(name, _) = self.store.kind(index).clone() {
            match self.standard_library_call_projection(object, &name, authored_readonly) {
                Completion::Complete(Some((ty, id))) => {
                    return InferredCallCallee {
                        ty,
                        library_member: Completion::Complete(Some(id)),
                    };
                }
                Completion::Complete(None) => {}
                Completion::Deferred | Completion::Cycle | Completion::Limit => {
                    return InferredCallCallee {
                        ty: query,
                        library_member: Completion::Deferred,
                    };
                }
            }
        }
        InferredCallCallee {
            ty: self.complete_type(query).unwrap_or(query),
            library_member: Completion::Complete(None),
        }
    }

    pub(super) fn infer_assignment_target(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> Option<TypeId> {
        let inferred = match &expression.kind {
            ExpressionKind::ElementAccess { object, index } => {
                let mode = if self.is_evolving_empty_array_target(file, scope, object) {
                    ElementAccessMode::EvolvingArrayWrite
                } else {
                    ElementAccessMode::Write
                };
                Some(self.infer_element_access_expression(file, scope, object, index, mode))
            }
            ExpressionKind::Parenthesized(inner) => {
                self.infer_assignment_target(file, scope, inner)
            }
            ExpressionKind::Object(properties) => {
                let targets = properties
                    .iter()
                    .filter_map(|property| {
                        self.infer_destructuring_target(file, scope, &property.value)
                            .map(|ty| Property {
                                name: property.name.clone(),
                                ty,
                                optional: false,
                                readonly: false,
                            })
                    })
                    .collect::<Vec<_>>();
                (targets.len() == properties.len()).then(|| self.store.object(targets))
            }
            ExpressionKind::Array(elements) => {
                let targets = elements
                    .iter()
                    .filter_map(|element| self.infer_destructuring_target(file, scope, element))
                    .collect::<Vec<_>>();
                let mut element = None;
                let homogeneous = targets.iter().all(|target| {
                    let Some(target) = self.complete_type(*target) else {
                        return false;
                    };
                    match element {
                        Some(element) => element == target,
                        None => {
                            element = Some(target);
                            true
                        }
                    }
                });
                if homogeneous && targets.len() == elements.len() {
                    let element = element.unwrap_or(self.store.builtins.never);
                    Some(self.store.intern(TypeKind::Array(element)))
                } else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                    None
                }
            }
            ExpressionKind::Identifier { .. } | ExpressionKind::Member { .. } => {
                Some(self.infer_expression(file, scope, expression, None))
            }
            _ => {
                self.infer_expression(file, scope, expression, None);
                let _ = self.require_completion(Completion::<()>::Deferred);
                None
            }
        };
        if let Some(inferred) = inferred {
            self.expression_type_origins
                .insert((file, expression.id), inferred);
        }
        inferred
    }

    fn infer_destructuring_target(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> Option<TypeId> {
        let ExpressionKind::Assignment {
            left,
            operator: AssignmentOperator::Assign,
            right,
            ..
        } = &expression.kind
        else {
            if let ExpressionKind::Parenthesized(inner) = &expression.kind {
                return self.infer_destructuring_target(file, scope, inner);
            }
            return self.infer_assignment_target(file, scope, expression);
        };
        let target = self.infer_assignment_target(file, scope, left);
        let source = self.infer_expression(file, scope, right, target);
        if let Some(target) = target {
            self.report_relation(
                source,
                target,
                right.span,
                Some(right),
                self.expression_order_origins.get(&(file, left.id)).cloned(),
                RelationMode::Assignment,
                RelationDiagnosticStyle::Type,
            );
        }
        target
    }

    pub(super) fn report_object_literal_shorthand_default(
        &mut self,
        file: FileId,
        property: &ObjectProperty,
    ) {
        if property.shorthand
            && let Some(span) = property.shorthand_equals_span
        {
            self.push_diagnostic(
                file,
                span,
                "Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern.".to_string(),
                1312,
            );
        }
    }

    fn is_evolving_empty_array_target(
        &self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> bool {
        let expression = expression.peel_parentheses();
        let ExpressionKind::Identifier {
            name,
            entity_name: true,
            ..
        } = &expression.kind
        else {
            return false;
        };
        let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Value) else {
            return false;
        };
        matches!(
            self.models.get(&declaration),
            Some(DeclarationModel::Variable { declaration, .. })
                if declaration.annotation.is_none()
                    && matches!(
                        declaration.initializer.as_ref().map(|initializer| &initializer.kind),
                        Some(ExpressionKind::Array(elements)) if elements.is_empty()
                    )
        )
    }

    pub(super) fn infer_assignment_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        left: &Expression,
        right: &Expression,
    ) -> TypeId {
        if let Some(source) = self.infer_paired_destructuring_assignment(file, scope, left, right) {
            return source;
        }
        let target = self.infer_assignment_target(file, scope, left);
        let source = self.infer_expression(file, scope, right, target);
        if let Some(target) = target {
            self.report_relation(
                source,
                target,
                right.span,
                Some(right),
                self.expression_order_origins.get(&(file, left.id)).cloned(),
                RelationMode::Assignment,
                RelationDiagnosticStyle::Type,
            );
        }
        source
    }

    fn infer_paired_destructuring_assignment(
        &mut self,
        file: FileId,
        scope: ScopeId,
        left: &Expression,
        right: &Expression,
    ) -> Option<TypeId> {
        let mut pairs = Vec::new();
        if !matches!(
            &left.peel_parentheses().kind,
            ExpressionKind::Array(_) | ExpressionKind::Object(_)
        ) || !collect_paired_assignment_leaves(left, right, &mut pairs)
        {
            return None;
        }
        let source = self.infer_expression(file, scope, right, None);
        for (target_expression, source_expression) in pairs {
            let target = self
                .infer_destructuring_target(file, scope, target_expression)
                .expect("paired assignment targets are preflighted");
            let mut source_element = self.expression_type_origins[&(file, source_expression.id)];
            if self.options.effective_strict_null_checks()
                && matches!(target_expression.kind, ExpressionKind::Assignment { .. })
            {
                match self.store.kind(source_element).clone() {
                    TypeKind::Undefined | TypeKind::Void => {
                        source_element = self.store.builtins.never
                    }
                    TypeKind::Deferred(_) | TypeKind::Intersection(_) | TypeKind::Union(_) => {
                        let _ = self.require_completion(Completion::<()>::Deferred);
                        continue;
                    }
                    _ => {}
                }
            }
            let diagnostic_target = match &target_expression.kind {
                ExpressionKind::Assignment { left, .. } => left.peel_parentheses(),
                _ => target_expression,
            };
            self.report_relation(
                source_element,
                target,
                diagnostic_target.span,
                Some(source_expression),
                self.expression_order_origins
                    .get(&(file, diagnostic_target.id))
                    .cloned(),
                RelationMode::Assignment,
                RelationDiagnosticStyle::Type,
            );
        }
        Some(source)
    }

    pub(super) fn observe_delete_operand(&mut self, operand: TypeId) {
        if !matches!(
            self.store.kind(operand),
            TypeKind::Error | TypeKind::Invalid(_)
        ) {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
    }

    pub(super) fn evaluate_element_access(
        &mut self,
        object: TypeId,
        index: TypeId,
        mode: ElementAccessMode,
        depth: usize,
    ) -> Completion<TypeId> {
        let [object, index] = self.force_operands([object, index], depth);
        let object = completed!(object);
        let index = completed!(index);
        self.evaluate_resolved_element_access(object, index, mode)
    }

    fn evaluate_resolved_element_access(
        &mut self,
        object: TypeId,
        index: TypeId,
        mode: ElementAccessMode,
    ) -> Completion<TypeId> {
        if matches!(
            self.store.kind(index),
            TypeKind::Error | TypeKind::Invalid(_)
        ) {
            return Completion::Complete(index);
        }
        match self.store.kind(object).clone() {
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(object),
            TypeKind::Never => Completion::Complete(self.store.builtins.never),
            TypeKind::Array(element) if is_number_index_type(self.store.kind(index)) => {
                if mode == ElementAccessMode::EvolvingArrayWrite
                    && matches!(self.store.kind(element), TypeKind::Never)
                {
                    Completion::Deferred
                } else {
                    Completion::Complete(element)
                }
            }
            TypeKind::Array(_)
                if !self.options.effective_no_implicit_any()
                    && is_property_key_like(self.store.kind(index)) =>
            {
                Completion::Complete(self.store.builtins.any)
            }
            TypeKind::Tuple(elements) => self.tuple_element_access(&elements, index, mode),
            TypeKind::String | TypeKind::LiteralString(_, _)
                if mode.is_read() && is_number_index_type(self.store.kind(index)) =>
            {
                Completion::Complete(self.store.builtins.string)
            }
            TypeKind::String | TypeKind::LiteralString(_, _)
                if mode.is_read()
                    && !self.options.effective_no_implicit_any()
                    && is_property_key_like(self.store.kind(index)) =>
            {
                Completion::Complete(self.store.builtins.any)
            }
            TypeKind::ClassConstructor { declaration, .. } => {
                self.standard_library_value_element_access(declaration, index)
            }
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => self.object_element_access(&shape, index, mode),
            TypeKind::Union(members) if mode.is_read() => {
                let mut values = Vec::with_capacity(members.len());
                for member in members {
                    values.push(completed!(
                        self.evaluate_resolved_element_access(member, index, mode)
                    ));
                }
                Completion::Complete(self.store.union(values, UnionPolicy::Canonical))
            }
            _ => Completion::Deferred,
        }
    }

    fn tuple_element_access(
        &mut self,
        elements: &[TypeId],
        index: TypeId,
        mode: ElementAccessMode,
    ) -> Completion<TypeId> {
        match self.store.kind(index) {
            TypeKind::LiteralNumber(index, _) => exact_tuple_index(index.array_index(), elements),
            TypeKind::LiteralString(index, _) => {
                exact_tuple_index(canonical_array_index(index), elements)
            }
            TypeKind::Number | TypeKind::Any if mode.is_read() => Completion::Complete(
                self.store
                    .union(elements.iter().copied(), UnionPolicy::Canonical),
            ),
            _ => Completion::Deferred,
        }
    }

    fn object_element_access(
        &mut self,
        shape: &ObjectShape,
        index: TypeId,
        mode: ElementAccessMode,
    ) -> Completion<TypeId> {
        let value = match self.store.kind(index).clone() {
            TypeKind::LiteralString(key, _) => property(shape, &key, mode)
                .or_else(|| index_value(shape, IndexKeyKind::String, mode)),
            TypeKind::LiteralNumber(key, _) => property(shape, key.display(), mode)
                .or_else(|| index_value(shape, IndexKeyKind::Number, mode))
                .or_else(|| index_value(shape, IndexKeyKind::String, mode)),
            TypeKind::String => index_value(shape, IndexKeyKind::String, mode),
            TypeKind::Number => index_value(shape, IndexKeyKind::Number, mode)
                .or_else(|| index_value(shape, IndexKeyKind::String, mode)),
            TypeKind::Any => index_value(shape, IndexKeyKind::Number, mode)
                .or_else(|| index_value(shape, IndexKeyKind::String, mode))
                .or(Some(Completion::Complete(self.store.builtins.any))),
            TypeKind::Error | TypeKind::Invalid(_) => Some(Completion::Complete(index)),
            TypeKind::Union(members) if mode.is_read() => {
                let mut values = Vec::with_capacity(members.len());
                for member in members {
                    values.push(completed!(self.object_element_access(shape, member, mode)));
                }
                return Completion::Complete(self.store.union(values, UnionPolicy::Canonical));
            }
            _ => None,
        };
        value.unwrap_or_else(|| {
            if !self.options.effective_no_implicit_any()
                && is_property_key_like(self.store.kind(index))
            {
                Completion::Complete(self.store.builtins.any)
            } else {
                Completion::Deferred
            }
        })
    }

    fn standard_library_value_element_access(
        &mut self,
        declaration: crate::source::DeclId,
        index: TypeId,
    ) -> Completion<TypeId> {
        let lookup = match self.store.kind(index) {
            TypeKind::LiteralString(name, _) => {
                self.standard_library_call_member(LibraryReceiver::Declaration(declaration), name)
            }
            _ => LibraryMemberLookup::Missing,
        };
        match lookup {
            LibraryMemberLookup::Found(LibraryCallMember::ToString) => Completion::Complete(
                self.store
                    .function(None, false, Vec::new(), self.store.builtins.string),
            ),
            LibraryMemberLookup::Missing
                if self.program.standard_library.is_array_value(declaration)
                    && !self.options.effective_no_implicit_any()
                    && is_property_key_like(self.store.kind(index)) =>
            {
                Completion::Complete(self.store.builtins.any)
            }
            LibraryMemberLookup::Found(_)
            | LibraryMemberLookup::DeferredUntilMemberMerging
            | LibraryMemberLookup::Missing => Completion::Deferred,
        }
    }
}

fn property(shape: &ObjectShape, key: &str, mode: ElementAccessMode) -> Option<Completion<TypeId>> {
    shape
        .properties
        .iter()
        .find(|property| property.name == key)
        .map(|property| {
            if property.optional || mode.is_write() && property.readonly {
                Completion::Deferred
            } else {
                Completion::Complete(property.ty)
            }
        })
}

fn index_value(
    shape: &ObjectShape,
    key: IndexKeyKind,
    mode: ElementAccessMode,
) -> Option<Completion<TypeId>> {
    shape.index(key).map(|index| {
        if mode.is_write() && index.readonly {
            Completion::Deferred
        } else {
            Completion::Complete(index.value)
        }
    })
}

const fn is_number_like(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Number | TypeKind::LiteralNumber(_, _))
}

fn is_number_index_type(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Any)
        || is_number_like(kind)
        || matches!(kind, TypeKind::LiteralString(value, _) if is_numeric_literal_name(value))
}

fn is_numeric_literal_name(value: &str) -> bool {
    if matches!(value, "NaN" | "Infinity" | "-Infinity") {
        return true;
    }
    let (negative, source) = value
        .strip_prefix('-')
        .map_or((false, value), |source| (true, source));
    let Some(parsed) = parse_number_literal(source) else {
        return false;
    };
    if negative {
        parsed.display != "0" && source == parsed.display
    } else {
        value == parsed.display
    }
}

fn canonical_array_index(value: &str) -> Option<usize> {
    let parsed = parse_number_literal(value)?;
    (value == parsed.display
        && parsed.value.is_finite()
        && parsed.value >= 0.0
        && parsed.value.fract() == 0.0
        && parsed.value <= usize::MAX as f64)
        .then_some(parsed.value as usize)
}

fn exact_tuple_index(index: Option<usize>, elements: &[TypeId]) -> Completion<TypeId> {
    index
        .and_then(|index| elements.get(index).copied())
        .map_or(Completion::Deferred, Completion::Complete)
}

fn collect_paired_assignment_leaves<'a>(
    target: &'a Expression,
    source: &'a Expression,
    leaves: &mut Vec<(&'a Expression, &'a Expression)>,
) -> bool {
    let target = target.peel_parentheses();
    let source = source.peel_parentheses();
    match (&target.kind, &source.kind) {
        (ExpressionKind::Array(targets), ExpressionKind::Array(sources)) => {
            targets.len() == sources.len()
                && targets.iter().zip(sources).all(|(target, source)| {
                    collect_paired_assignment_leaves(target, source, leaves)
                })
        }
        (ExpressionKind::Object(targets), ExpressionKind::Object(sources)) => {
            if targets.len() != sources.len() {
                return false;
            }
            targets.iter().all(|target| {
                unique_property(targets, &target.name).is_some()
                    && unique_property(sources, &target.name).is_some_and(|source| {
                        collect_paired_assignment_leaves(&target.value, &source.value, leaves)
                    })
            })
        }
        (
            ExpressionKind::Assignment {
                left,
                operator: AssignmentOperator::Assign,
                ..
            },
            _,
        ) if is_plain_assignment_target(left) => {
            leaves.push((target, source));
            true
        }
        _ if is_plain_assignment_target(target) => {
            leaves.push((target, source));
            true
        }
        _ => false,
    }
}

fn is_plain_assignment_target(expression: &Expression) -> bool {
    matches!(
        &expression.peel_parentheses().kind,
        ExpressionKind::Identifier { .. }
            | ExpressionKind::Member { .. }
            | ExpressionKind::ElementAccess { .. }
    )
}

fn unique_property<'a>(properties: &'a [ObjectProperty], name: &str) -> Option<&'a ObjectProperty> {
    let mut matches = properties.iter().filter(|property| property.name == name);
    let property = matches.next()?;
    matches.next().is_none().then_some(property)
}

pub(super) const fn is_property_key_like(kind: &TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::String
            | TypeKind::Number
            | TypeKind::LiteralString(_, _)
            | TypeKind::LiteralNumber(_, _)
    )
}
