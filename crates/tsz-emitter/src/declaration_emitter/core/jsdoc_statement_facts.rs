use super::DeclarationEmitter;
use crate::declaration_emitter::helpers::JsdocTypeAliasDecl;

pub(in crate::declaration_emitter) struct StatementJsdocDeclarationFacts {
    comments: Vec<String>,
    type_alias_decls: Vec<JsdocTypeAliasDecl>,
    has_type_alias_tag: bool,
    has_type_function_signature: bool,
    has_function_signature_tags: bool,
}

impl StatementJsdocDeclarationFacts {
    pub(in crate::declaration_emitter) fn comments(&self) -> &[String] {
        &self.comments
    }

    pub(in crate::declaration_emitter) const fn has_type_alias_tag(&self) -> bool {
        self.has_type_alias_tag
    }

    pub(in crate::declaration_emitter) const fn has_type_function_signature(&self) -> bool {
        self.has_type_function_signature
    }

    pub(in crate::declaration_emitter) const fn has_function_signature_tags(&self) -> bool {
        self.has_function_signature_tags
    }

    pub(in crate::declaration_emitter) fn type_alias_decls(&self) -> &[JsdocTypeAliasDecl] {
        &self.type_alias_decls
    }

    pub(in crate::declaration_emitter) fn comments_without_type_alias_tags(&self) -> Vec<String> {
        self.comments
            .iter()
            .filter(|jsdoc| !DeclarationEmitter::jsdoc_contains_type_alias_tag(jsdoc))
            .cloned()
            .collect()
    }

    pub(in crate::declaration_emitter) fn comments_without_type_or_alias_tags(
        &self,
    ) -> Vec<String> {
        DeclarationEmitter::jsdoc_chain_without_type_or_alias_tags(&self.comments)
    }
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn statement_jsdoc_declaration_facts(
        comments: Vec<String>,
        has_type_function_signature: bool,
    ) -> StatementJsdocDeclarationFacts {
        let has_type_alias_tag = comments
            .iter()
            .any(|jsdoc| Self::jsdoc_contains_type_alias_tag(jsdoc));
        let has_function_signature_tags = comments
            .iter()
            .any(|jsdoc| Self::jsdoc_has_function_signature_tags(jsdoc));
        let type_alias_decls = comments
            .iter()
            .filter_map(|jsdoc| Self::parse_jsdoc_type_alias_decl(jsdoc))
            .collect();

        StatementJsdocDeclarationFacts {
            comments,
            type_alias_decls,
            has_type_alias_tag,
            has_type_function_signature,
            has_function_signature_tags,
        }
    }

    pub(in crate::declaration_emitter) fn emit_jsdoc_type_alias_facts(
        &mut self,
        facts: &StatementJsdocDeclarationFacts,
        exported: bool,
    ) {
        for decl in facts.type_alias_decls() {
            self.emit_rendered_jsdoc_type_alias(decl.clone(), exported);
        }
    }
}
