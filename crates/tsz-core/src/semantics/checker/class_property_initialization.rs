use std::collections::HashMap;

use crate::bind::{DeclarationKind, ScopeId};
use crate::source::FileId;
use crate::syntax::{ClassDeclaration, ClassMemberKind, PropertyNameKind};

use super::Checker;
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};

impl Checker<'_> {
    pub(super) fn check_unconstructed_class_properties(
        &mut self,
        file: FileId,
        class_scope: ScopeId,
        declaration: &ClassDeclaration,
    ) {
        if !self.options.effective_strict_null_checks()
            || !self.options.effective_strict_property_initialization()
            || declaration.declared
        {
            return;
        }
        let source_path = &self.program.files[file.0 as usize].source.path;
        if is_declaration_source(source_path) {
            return;
        }
        let source_supported = is_plain_typescript_source(source_path);
        let has_constructor = declaration
            .members
            .iter()
            .any(|member| matches!(member.kind, ClassMemberKind::Constructor { .. }));

        for member in &declaration.members {
            if matches!(
                member.name_kind,
                PropertyNameKind::StringLiteral | PropertyNameKind::NumericLiteral
            ) || member.modifiers.static_member
                || member.modifiers.abstract_member
                || member.modifiers.declared
                || member.modifiers.async_member
            {
                continue;
            }
            let ClassMemberKind::Property {
                annotation: Some(annotation),
                initializer: None,
                optional: false,
                definite: false,
            } = &member.kind
            else {
                continue;
            };
            if !declaration.member_syntax_recovery_free {
                let _ = self.require_completion(Completion::<()>::Deferred);
                continue;
            }

            let ty = self.resolve_type_node(file, class_scope, annotation, &HashMap::new());
            let required = self.property_initialization_requirement(ty);
            match self.require_completion(required) {
                Completion::Complete(true)
                    if has_constructor
                        || !source_supported
                        || member.name_kind != PropertyNameKind::Identifier =>
                {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                Completion::Complete(true) => self.push_diagnostic(
                    file,
                    member.name_span,
                    format!(
                        "Property '{}' has no initializer and is not definitely assigned in the constructor.",
                        member.name
                    ),
                    2564,
                ),
                Completion::Complete(false)
                | Completion::Deferred
                | Completion::Cycle
                | Completion::Limit => {}
            }
        }
    }

    fn property_initialization_requirement(&mut self, ty: TypeId) -> Completion<bool> {
        match self.store.kind(ty).clone() {
            TypeKind::Any | TypeKind::Unknown | TypeKind::Undefined | TypeKind::Error => {
                Completion::Complete(false)
            }
            TypeKind::Invalid(_) => Completion::Deferred,
            TypeKind::Union(members) => {
                let mut required = true;
                let mut incomplete = None;
                for member in members {
                    match self.property_initialization_requirement(member) {
                        Completion::Complete(member_required) => required &= member_required,
                        Completion::Deferred => {
                            incomplete = incomplete.or(Some(Completion::Deferred));
                        }
                        Completion::Cycle => incomplete = Some(Completion::Cycle),
                        Completion::Limit => incomplete = Some(Completion::Limit),
                    }
                }
                incomplete.unwrap_or(Completion::Complete(required))
            }
            TypeKind::Deferred(DeferredType::Reference { declaration, .. })
                if self.reference_has_required_initialization_identity(declaration) =>
            {
                Completion::Complete(true)
            }
            TypeKind::Deferred(_) => match self.force_type(ty, 0) {
                Completion::Complete(forced) if forced != ty => {
                    self.property_initialization_requirement(forced)
                }
                Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                Completion::Cycle => Completion::Cycle,
                Completion::Limit => Completion::Limit,
            },
            TypeKind::Never
            | TypeKind::Void
            | TypeKind::Null
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::ObjectKeyword
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::Array(_)
            | TypeKind::Tuple(_)
            | TypeKind::Intersection(_)
            | TypeKind::Object(_)
            | TypeKind::ClassInstance { .. }
            | TypeKind::ClassConstructor { .. }
            | TypeKind::Function(_)
            | TypeKind::ShapeFunction(_) => Completion::Complete(true),
        }
    }

    fn reference_has_required_initialization_identity(
        &self,
        declaration: crate::source::DeclId,
    ) -> bool {
        self.program
            .file(declaration.file)
            .and_then(|file| file.bindings.declaration(declaration))
            .is_some_and(|bound| {
                matches!(
                    bound.kind,
                    DeclarationKind::Class | DeclarationKind::Interface
                )
            })
    }
}

fn is_plain_typescript_source(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
}

fn is_declaration_source(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
        })
}
