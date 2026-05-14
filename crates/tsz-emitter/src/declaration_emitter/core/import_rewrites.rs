use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn type_reference_only_in_matching_ambient_module(
        output: &str,
        name: &str,
        module: &str,
    ) -> bool {
        let reference = format!(": {name}<");
        let module_start = format!("declare module {module}");
        let mut ambient_depth = 0usize;
        let mut found_in_ambient_module = false;

        for line in output.lines() {
            let enters_matching_module =
                ambient_depth == 0 && line.trim_start().starts_with(&module_start);
            if enters_matching_module {
                ambient_depth = 1;
            }

            if line.contains(&reference) {
                if ambient_depth == 0 {
                    return false;
                }
                found_in_ambient_module = true;
            }

            if ambient_depth > 0 {
                if !enters_matching_module {
                    ambient_depth = ambient_depth.saturating_add(line.matches('{').count());
                }
                ambient_depth = ambient_depth.saturating_sub(line.matches('}').count());
            }
        }

        found_in_ambient_module
    }

    pub(in crate::declaration_emitter) fn prune_unused_named_import_specifiers_from_output(
        output: &str,
    ) -> String {
        let lines = output.lines().collect::<Vec<_>>();
        let mut changed = false;
        let mut rewritten = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("import { ") else {
                rewritten.push((*line).to_string());
                continue;
            };
            let Some((named, module_part)) = rest.split_once(" } from ") else {
                rewritten.push((*line).to_string());
                continue;
            };
            let module_part = module_part.trim_end_matches(';');
            if module_part.is_empty() {
                rewritten.push((*line).to_string());
                continue;
            }

            let body = lines
                .iter()
                .enumerate()
                .filter_map(|(idx, body_line)| (idx != line_idx).then_some(*body_line))
                .collect::<Vec<_>>()
                .join("\n");
            let kept = named
                .split(',')
                .filter_map(|specifier| {
                    let specifier = specifier.trim();
                    if specifier.is_empty() {
                        return None;
                    }
                    let local_name = specifier
                        .split_once(" as ")
                        .map_or(specifier, |(_, alias)| alias.trim());
                    Self::contains_whole_word_in_text(&body, local_name).then_some(specifier)
                })
                .collect::<Vec<_>>();

            if kept.len()
                == named
                    .split(',')
                    .filter(|part| !part.trim().is_empty())
                    .count()
            {
                rewritten.push((*line).to_string());
                continue;
            }
            changed = true;
            if !kept.is_empty() {
                let indent_len = line.len() - trimmed.len();
                rewritten.push(format!(
                    "{}import {{ {} }} from {};",
                    &line[..indent_len],
                    kept.join(", "),
                    module_part
                ));
            }
        }

        if changed {
            let mut text = rewritten.join("\n");
            if output.ends_with('\n') {
                text.push('\n');
            }
            text
        } else {
            output.to_string()
        }
    }
}
