//! Legacy protocol handlers for tsz-server.
//!
//! Handles the legacy JSON-per-line protocol used for fast conformance testing.

use super::{
    CheckOptions, CheckResponse, ErrorResponse, LegacyRequest, LegacyResponse, OkResponse, Server,
    StatusResponse,
};
use rustc_hash::FxHashMap;
use std::time::Instant;

impl Server {
    pub(crate) fn handle_legacy_request(&mut self, request: LegacyRequest) -> LegacyResponse {
        match request {
            LegacyRequest::Check { id, files, options } => {
                self.handle_legacy_check(id, *files, *options)
            }
            LegacyRequest::Status { id } => self.handle_legacy_status(id),
            LegacyRequest::Recycle { id } => self.handle_legacy_recycle(id),
            LegacyRequest::Shutdown { id } => LegacyResponse::Ok(OkResponse { id, ok: true }),
        }
    }

    pub(crate) fn handle_legacy_check(
        &mut self,
        id: u64,
        files: FxHashMap<String, String>,
        options: CheckOptions,
    ) -> LegacyResponse {
        let start = Instant::now();
        match self.run_check(files, options) {
            Ok(result) => {
                self.checks_completed += 1;
                LegacyResponse::Check(CheckResponse {
                    id,
                    codes: result.codes,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            Err(e) => LegacyResponse::Error(ErrorResponse {
                id,
                error: e.to_string(),
            }),
        }
    }

    pub(crate) fn handle_legacy_status(&self, id: u64) -> LegacyResponse {
        let memory_mb = {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/proc/self/statm")
                    .ok()
                    .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
                    .map(|pages| pages * 4096 / 1024 / 1024)
                    .unwrap_or(0)
            }
            #[cfg(not(target_os = "linux"))]
            {
                0
            }
        };

        LegacyResponse::Status(StatusResponse {
            id,
            memory_mb,
            checks_completed: self.checks_completed,
            cached_libs: self.lib_cache.len(),
        })
    }

    pub(crate) fn handle_legacy_recycle(&mut self, id: u64) -> LegacyResponse {
        self.lib_cache.clear();
        self.unified_lib_cache = None;
        self.checks_completed = 0;
        LegacyResponse::Ok(OkResponse { id, ok: true })
    }
}
