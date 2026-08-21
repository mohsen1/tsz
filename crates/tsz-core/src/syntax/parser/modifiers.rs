use super::Parser;
use crate::syntax::{ClassMember, ClassMemberKind, TokenKind};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Modifiers {
    pub(super) exported: bool,
    pub(super) default_export: bool,
    pub(super) declared: bool,
    pub(super) is_async: bool,
    pub(super) abstract_declaration: bool,
    pub(super) unsupported_for_overload_completion: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProductCapabilities {
    pub(super) functions_supported: bool,
    pub(super) classes_supported: bool,
    pub(super) commonjs_classes_supported: bool,
    has_bodyless_class: bool,
    has_module_export: bool,
}

impl ProductCapabilities {
    pub(super) const fn all_supported() -> Self {
        Self {
            functions_supported: true,
            classes_supported: true,
            commonjs_classes_supported: true,
            has_bodyless_class: false,
            has_module_export: false,
        }
    }

    pub(super) const fn observe_function(&mut self, modifiers: Modifiers, supported: bool) {
        self.functions_supported &=
            !modifiers.default_export && !modifiers.abstract_declaration && supported;
    }

    pub(super) const fn observe_module_export(&mut self) {
        self.has_module_export = true;
        self.commonjs_classes_supported &= !self.has_bodyless_class;
    }

    pub(super) const fn commonjs_classes_supported(&self) -> bool {
        self.commonjs_classes_supported
    }

    pub(super) fn observe_class(&mut self, modifiers: Modifiers, members: &[ClassMember]) {
        self.classes_supported &= !modifiers.abstract_declaration
            && !modifiers.is_async
            && !modifiers.unsupported_for_overload_completion
            && members.iter().all(|member| member.emit_products_supported);
        let has_bodyless_member = members.iter().any(|member| {
            matches!(
                &member.kind,
                ClassMemberKind::Constructor {
                    has_body: false,
                    ..
                } | ClassMemberKind::Method {
                    has_body: false,
                    ..
                }
            )
        });
        self.has_bodyless_class |= has_bodyless_member;
        self.commonjs_classes_supported &= !(has_bodyless_member && self.has_module_export);
    }
}

impl Parser<'_> {
    pub(super) fn parse_modifiers(&mut self) -> Modifiers {
        let mut modifiers = Modifiers::default();
        loop {
            match self.kind() {
                TokenKind::Export => {
                    self.product_capabilities.observe_module_export();
                    modifiers.unsupported_for_overload_completion |= modifiers.exported;
                    modifiers.exported = true;
                    self.bump();
                    let default_export = self.eat(TokenKind::Default);
                    modifiers.unsupported_for_overload_completion |=
                        default_export && modifiers.default_export;
                    modifiers.default_export |= default_export;
                }
                TokenKind::Declare => {
                    modifiers.unsupported_for_overload_completion |= modifiers.declared;
                    modifiers.declared = true;
                    self.bump();
                }
                TokenKind::Async => {
                    modifiers.unsupported_for_overload_completion |= modifiers.is_async;
                    modifiers.is_async = true;
                    self.bump();
                }
                TokenKind::Abstract => {
                    modifiers.unsupported_for_overload_completion |= modifiers.abstract_declaration;
                    modifiers.abstract_declaration = true;
                    self.bump();
                }
                _ => break,
            }
        }
        modifiers
    }
}
