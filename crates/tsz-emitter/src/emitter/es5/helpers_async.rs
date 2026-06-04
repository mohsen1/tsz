use super::super::*;

use super::helpers::ArraySegment;

use super::helpers_class_expression_static::Es5StaticClassExpressionElement;

use crate::emitter::core::PropertyNameEmit;

use crate::emitter::declarations::class::replace_identifier;

use crate::transforms::emit_utils;

use crate::transforms::ir::IRNode;

use crate::transforms::ir_printer::IRPrinter;

use std::sync::Arc;

include!("helpers_async_parts/part1.rs");
include!("helpers_async_parts/part2.rs");
