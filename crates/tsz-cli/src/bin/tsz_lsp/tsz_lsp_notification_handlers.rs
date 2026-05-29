use super::*;

impl LspServer {
    pub(super) fn handle_notification_method(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> bool {
        match method {
            "$/cancelRequest" => {
                self.handle_cancel_request(params);
                true
            }
            "initialized" => {
                self.initialized = true;
                self.handle_initialized();
                true
            }
            "exit" => {
                std::process::exit(i32::from(!self.shutdown_requested));
            }
            "textDocument/didOpen" => {
                self.handle_did_open(params);
                true
            }
            "textDocument/didChange" => {
                self.handle_did_change(params);
                true
            }
            "textDocument/didClose" => {
                self.handle_did_close(params);
                true
            }
            "textDocument/didSave" => {
                self.handle_did_save(params);
                true
            }
            "workspace/didChangeConfiguration" => {
                self.handle_did_change_configuration(params);
                true
            }
            "workspace/didChangeWatchedFiles" => {
                self.handle_did_change_watched_files(params);
                true
            }
            "workspace/didChangeWorkspaceFolders" => {
                self.handle_did_change_workspace_folders(params);
                true
            }
            "workspace/didRenameFiles" => {
                self.handle_did_rename_files(params);
                true
            }
            "workspace/didCreateFiles" => {
                self.handle_did_create_files(params);
                true
            }
            "workspace/didDeleteFiles" => {
                self.handle_did_delete_files(params);
                true
            }
            _ => false,
        }
    }
}
