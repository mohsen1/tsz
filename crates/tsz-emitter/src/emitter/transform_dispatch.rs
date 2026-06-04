use super::*;

use crate::emitter::core::PrivateMemberInfo;

use crate::transforms::emit_utils::{get_extends_expression_index, hygienic_temp_name};

use crate::transforms::private_fields_es5::collect_private_fields_with_reserved;

use std::sync::Arc;

use tracing::debug;

use tsz_parser::parser::node::NodeAccess;

#[path = "transform_dispatch_directive.rs"]
mod transform_dispatch_directive;

use transform_dispatch_directive::EmitDirective;

#[path = "transform_dispatch_chain.rs"]
mod transform_dispatch_chain;

#[path = "transform_dispatch_class_binding.rs"]
mod transform_dispatch_class_binding;

#[path = "transform_dispatch_es5_class.rs"]
mod transform_dispatch_es5_class;

include!("transform_dispatch_parts/part1.rs");
include!("transform_dispatch_parts/part2.rs");
