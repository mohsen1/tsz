use serde::Deserialize;
use wasm_bindgen::prelude::JsValue;

use crate::lsp::{CodeActionContext, CodeActionKind, ImportCandidate, ImportCandidateKind};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportCandidateInput {
    module_specifier: String,
    local_name: String,
    kind: String,
    export_name: Option<String>,
    #[serde(default)]
    is_type_only: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeActionContextInput {
    #[serde(default)]
    diagnostics: Vec<tsz_lsp::diagnostics::LspDiagnostic>,
    #[serde(default)]
    only: Option<Vec<CodeActionKind>>,
    #[serde(default)]
    import_candidates: Vec<ImportCandidateInput>,
}

impl TryFrom<ImportCandidateInput> for ImportCandidate {
    type Error = JsValue;

    fn try_from(input: ImportCandidateInput) -> Result<Self, Self::Error> {
        let local_name = input.local_name;
        let kind = match input.kind.as_str() {
            "named" => {
                let export_name = input.export_name.unwrap_or_else(|| local_name.clone());
                ImportCandidateKind::Named { export_name }
            }
            "default" => ImportCandidateKind::Default,
            "namespace" => ImportCandidateKind::Namespace,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Unsupported import candidate kind: {other}"
                )));
            }
        };

        Ok(Self {
            module_specifier: input.module_specifier,
            local_name,
            kind,
            is_type_only: input.is_type_only,
        })
    }
}

pub(crate) fn parse_code_action_context(context: JsValue) -> Result<CodeActionContext, JsValue> {
    if context.is_null() || context.is_undefined() {
        return Ok(default_code_action_context());
    }

    let context_input: CodeActionContextInput = serde_wasm_bindgen::from_value(context)?;
    let import_candidates = context_input
        .import_candidates
        .into_iter()
        .map(ImportCandidate::try_from)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CodeActionContext {
        diagnostics: context_input.diagnostics,
        only: context_input.only,
        import_candidates,
    })
}

pub(crate) const fn default_code_action_context() -> CodeActionContext {
    CodeActionContext {
        diagnostics: Vec::new(),
        only: None,
        import_candidates: Vec::new(),
    }
}
