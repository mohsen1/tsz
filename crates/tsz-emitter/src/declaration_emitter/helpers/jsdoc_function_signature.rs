//! JSDoc function type signature parsing helpers.

pub(in crate::declaration_emitter) type JsdocFunctionTypeParam = String;
pub(in crate::declaration_emitter) type JsdocFunctionParam = (String, String);
pub(in crate::declaration_emitter) type JsdocFunctionTypeSignature =
    (Vec<JsdocFunctionTypeParam>, Vec<JsdocFunctionParam>, String);

pub(in crate::declaration_emitter) fn parse_jsdoc_function_type_signature(
    type_text: &str,
) -> Option<JsdocFunctionTypeSignature> {
    let mut rest = type_text.trim();
    let mut type_params = Vec::new();
    if let Some(after_open) = rest.strip_prefix('<') {
        let close = after_open.find('>')?;
        type_params = after_open[..close]
            .split(',')
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .map(str::to_string)
            .collect();
        rest = after_open[close + 1..].trim_start();
    }

    let after_params = rest.strip_prefix('(')?;
    let close = after_params.find(')')?;
    let params_text = &after_params[..close];
    let after_close = after_params[close + 1..].trim_start();
    let return_type = after_close.strip_prefix("=>")?.trim();
    if return_type.is_empty() {
        return None;
    }

    let mut params = Vec::new();
    for raw_param in params_text.split(',') {
        let raw_param = raw_param.trim();
        if raw_param.is_empty() {
            continue;
        }
        let colon = raw_param.find(':')?;
        let name = raw_param[..colon].trim();
        let type_text = raw_param[colon + 1..].trim();
        if name.is_empty() || type_text.is_empty() {
            return None;
        }
        params.push((name.to_string(), type_text.to_string()));
    }

    Some((type_params, params, return_type.to_string()))
}
