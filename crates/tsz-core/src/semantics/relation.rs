use super::{Completion, TypeId, TypeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationMode {
    Subtype,
    Assignment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationFailureKind {
    Incompatible,
    MissingProperty(String),
    Property(String),
    Parameter(usize),
    Return,
    Cycle,
    ComplexityLimit,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationFailure {
    pub source: TypeId,
    pub target: TypeId,
    pub kind: RelationFailureKind,
}

pub(crate) trait RelationContext {
    fn force_type(&mut self, ty: TypeId, depth: usize) -> Completion<TypeId>;
    fn type_kind(&self, ty: TypeId) -> TypeKind;
    fn strict_null_checks(&self) -> bool;
}

pub(crate) fn relate<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
) -> Result<(), RelationFailure> {
    relate_inner(context, source, target, mode, 0)
}

fn relate_inner<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
    depth: usize,
) -> Result<(), RelationFailure> {
    if depth > 100 {
        return Err(failure(
            source,
            target,
            RelationFailureKind::ComplexityLimit,
        ));
    }
    let source = force(context, source, target, depth)?;
    let target = force(context, target, source, depth)?;
    if source == target {
        return Ok(());
    }

    let source_kind = context.type_kind(source);
    let target_kind = context.type_kind(target);
    if matches!(source_kind, TypeKind::Error) || matches!(target_kind, TypeKind::Error) {
        return Ok(());
    }
    if matches!(source_kind, TypeKind::Never) || matches!(target_kind, TypeKind::Unknown) {
        return Ok(());
    }
    if mode == RelationMode::Assignment
        && (matches!(source_kind, TypeKind::Any) || matches!(target_kind, TypeKind::Any))
    {
        return Ok(());
    }
    if !context.strict_null_checks() && matches!(source_kind, TypeKind::Null | TypeKind::Undefined)
    {
        return Ok(());
    }

    match (&source_kind, &target_kind) {
        (TypeKind::LiteralString(_), TypeKind::String)
        | (TypeKind::LiteralNumber(_), TypeKind::Number)
        | (TypeKind::LiteralBoolean(_), TypeKind::Boolean)
        | (
            TypeKind::Object(_) | TypeKind::Array(_) | TypeKind::Tuple(_) | TypeKind::Function(_),
            TypeKind::ObjectKeyword,
        ) => Ok(()),
        (TypeKind::LiteralString(left), TypeKind::LiteralString(right)) if left == right => Ok(()),
        (TypeKind::LiteralNumber(left), TypeKind::LiteralNumber(right)) if left == right => Ok(()),
        (TypeKind::LiteralBoolean(left), TypeKind::LiteralBoolean(right)) if left == right => {
            Ok(())
        }
        (TypeKind::Union(members), _) => {
            for member in members {
                relate_inner(context, *member, target, mode, depth + 1)?;
            }
            Ok(())
        }
        (_, TypeKind::Union(members)) => {
            let mut last_failure = None;
            for member in members {
                match relate_inner(context, source, *member, mode, depth + 1) {
                    Ok(()) => return Ok(()),
                    Err(error) => last_failure = Some(error),
                }
            }
            Err(last_failure
                .unwrap_or_else(|| failure(source, target, RelationFailureKind::Incompatible)))
        }
        (_, TypeKind::Intersection(members)) => {
            for member in members {
                relate_inner(context, source, *member, mode, depth + 1)?;
            }
            Ok(())
        }
        (TypeKind::Intersection(members), _) => {
            let mut last_failure = None;
            for member in members {
                match relate_inner(context, *member, target, mode, depth + 1) {
                    Ok(()) => return Ok(()),
                    Err(error) => last_failure = Some(error),
                }
            }
            Err(last_failure
                .unwrap_or_else(|| failure(source, target, RelationFailureKind::Incompatible)))
        }
        (TypeKind::Array(left), TypeKind::Array(right)) => {
            relate_inner(context, *left, *right, mode, depth + 1)
        }
        (TypeKind::Tuple(left), TypeKind::Tuple(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                relate_inner(context, *left, *right, mode, depth + 1)?;
            }
            Ok(())
        }
        (TypeKind::Tuple(elements), TypeKind::Array(element)) => {
            for source_element in elements {
                relate_inner(context, *source_element, *element, mode, depth + 1)?;
            }
            Ok(())
        }
        (TypeKind::Object(source_properties), TypeKind::Object(target_properties)) => {
            for target_property in target_properties {
                let Some(source_property) = source_properties
                    .iter()
                    .find(|property| property.name == target_property.name)
                else {
                    if target_property.optional {
                        continue;
                    }
                    return Err(failure(
                        source,
                        target,
                        RelationFailureKind::MissingProperty(target_property.name.clone()),
                    ));
                };
                if let Err(mut error) = relate_inner(
                    context,
                    source_property.ty,
                    target_property.ty,
                    mode,
                    depth + 1,
                ) {
                    error.kind = RelationFailureKind::Property(target_property.name.clone());
                    return Err(error);
                }
            }
            Ok(())
        }
        (TypeKind::Function(source_signature), TypeKind::Function(target_signature)) => {
            let target_required = target_signature
                .parameters
                .iter()
                .filter(|parameter| !parameter.optional && !parameter.rest)
                .count();
            if source_signature.parameters.len() < target_required {
                return Err(failure(source, target, RelationFailureKind::Incompatible));
            }
            for (index, (source_parameter, target_parameter)) in source_signature
                .parameters
                .iter()
                .zip(&target_signature.parameters)
                .enumerate()
            {
                if let Err(mut error) = relate_inner(
                    context,
                    target_parameter.ty,
                    source_parameter.ty,
                    RelationMode::Subtype,
                    depth + 1,
                ) {
                    error.kind = RelationFailureKind::Parameter(index);
                    return Err(error);
                }
            }
            if matches!(
                context.type_kind(target_signature.return_type),
                TypeKind::Void
            ) {
                return Ok(());
            }
            relate_inner(
                context,
                source_signature.return_type,
                target_signature.return_type,
                mode,
                depth + 1,
            )
            .map_err(|mut error| {
                error.kind = RelationFailureKind::Return;
                error
            })
        }
        _ => Err(failure(source, target, RelationFailureKind::Incompatible)),
    }
}

fn force<C: RelationContext>(
    context: &mut C,
    ty: TypeId,
    other: TypeId,
    depth: usize,
) -> Result<TypeId, RelationFailure> {
    match context.force_type(ty, depth) {
        Completion::Complete(value) => Ok(value),
        Completion::Cycle => Err(failure(ty, other, RelationFailureKind::Cycle)),
        Completion::Limit => Err(failure(ty, other, RelationFailureKind::ComplexityLimit)),
        Completion::Deferred => Err(failure(ty, other, RelationFailureKind::Deferred)),
    }
}

const fn failure(source: TypeId, target: TypeId, kind: RelationFailureKind) -> RelationFailure {
    RelationFailure {
        source,
        target,
        kind,
    }
}
