/// Result of checking a single file in a multi-file program.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileCheckResultJson {
    pub(crate) file_name: String,
    pub(crate) parse_diagnostics: Vec<ParseDiagnosticJson>,
    pub(crate) check_diagnostics: Vec<CheckDiagnosticJson>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParseDiagnosticJson {
    pub(crate) message: String,
    pub(crate) start: u32,
    pub(crate) length: u32,
    pub(crate) code: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckDiagnosticJson {
    pub(crate) message_text: String,
    pub(crate) code: u32,
    pub(crate) start: u32,
    pub(crate) length: u32,
    pub(crate) category: String,
}
