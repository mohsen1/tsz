#![recursion_limit = "256"]

#[path = "tsz_lsp/tsz_lsp_dispatch.rs"]
mod tsz_lsp_dispatch;

#[path = "tsz_lsp/tsz_lsp_notification_handlers.rs"]
mod tsz_lsp_notification_handlers;

#[path = "tsz_lsp/tsz_lsp_request_handlers.rs"]
mod tsz_lsp_request_handlers;

#[path = "tsz_lsp/tsz_lsp_response_helpers.rs"]
mod tsz_lsp_response_helpers;

include!("tsz_lsp_parts/part1.rs");
include!("tsz_lsp_parts/part2.rs");
