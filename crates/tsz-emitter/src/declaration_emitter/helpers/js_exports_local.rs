#[allow(unused_imports)]
use super::super::{
    DeclarationEmitter, ImportPlan, JsNestedModuleExportNamespaces, PlannedImportModule,
    PlannedImportSymbol,
};

#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};

#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;

#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};

#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

#[allow(unused_imports)]
use super::js_exports::JsLocalNamedExportPlan;

#[allow(unused_imports)]
use super::{
    JsClassDefinePropertyAccessor, JsClassDefinePropertySetter, JsClassLikePrototypeMembers,
    JsClassStaticMembers, JsCommonjsExpandoDeclKind, JsCommonjsExpandoDeclarations,
    JsCommonjsNamedExports, JsStaticMethodAugmentationEntry, JsStaticMethodAugmentationGroup,
    JsStaticMethodAugmentations, JsStaticMethodInfo, JsStaticMethodKey,
};

include!("js_exports_local_parts/part1.rs");
include!("js_exports_local_parts/part2.rs");
