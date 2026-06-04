use super::super::super::Printer;

use crate::context::transform::TransformDirective;

use tsz_parser::parser::node::{Node, NodeAccess};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_parser::syntax::transform_utils::collect_class_computed_name_this_references;

use tsz_scanner::SyntaxKind;

enum DecoratorMemberName {
    Literal(String),
    Computed { expr: NodeIndex, key: String },
}

#[derive(Clone, Copy)]
enum LegacyMemberDecoratorScopeFilter {
    RequiresPrivateNameScope,
    DoesNotRequirePrivateNameScope,
}

struct MetadataFallbackEntity {
    check: String,
    value: String,
}

impl LegacyMemberDecoratorScopeFilter {
    const fn matches(self, requires_private_name_scope: bool) -> bool {
        match self {
            Self::RequiresPrivateNameScope => requires_private_name_scope,
            Self::DoesNotRequirePrivateNameScope => !requires_private_name_scope,
        }
    }
}

impl DecoratorMemberName {
    fn dedupe_key(&self) -> String {
        match self {
            Self::Literal(text) => text.clone(),
            Self::Computed { key, .. } => key.clone(),
        }
    }
}

include!("decorators_parts/part1.rs");
include!("decorators_parts/part2.rs");
