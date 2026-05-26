//! Implement-interface code-fix planning and rendering.
//!
//! Extracted from `handlers_code_fixes.rs` so the top-level code-fix handler
//! can stay focused on request orchestration.

use super::Server;
use super::handlers_code_fixes_utils::{
    InterfaceMember, class_body_has_member, extract_type_identifiers, find_first_implements_class,
    parse_interface_properties, parse_named_import_map, resolve_module_path,
    should_import_identifier,
};
use tsz::lsp::position::LineMap;

#[derive(Debug)]
pub(super) struct ImplementInterfacePlan {
    pub(super) interface_name: String,
    pub(super) interface_file_path: String,
    class_open_brace: usize,
    class_close_brace: usize,
    class_imports: std::collections::HashMap<String, String>,
    interface_imports: std::collections::HashMap<String, String>,
    pub(super) missing_members: Vec<InterfaceMember>,
}

impl Server {
    pub(super) fn synthetic_implement_interface_codefix(
        &self,
        file_path: &str,
        content: &str,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_ending: Option<&str>,
        import_module_specifier_preference: Option<&str>,
        line_map: &LineMap,
    ) -> Option<serde_json::Value> {
        let mut plan = self.implement_interface_plan(file_path, content)?;
        let import_lines = self.implement_interface_import_lines(
            file_path,
            &mut plan,
            auto_import_file_exclude_patterns,
            auto_import_specifier_exclude_regexes,
            import_module_specifier_ending,
            import_module_specifier_preference,
        );
        let updated_content =
            Self::render_implement_interface_content(content, &plan, &import_lines)?;
        let end_pos = line_map.offset_to_position(content.len() as u32, content);

        Some(serde_json::json!({
            "fixName": "fixClassIncorrectlyImplementsInterface",
            "description": format!("Implement interface '{}'", plan.interface_name),
            "changes": [{
                "fileName": file_path,
                "textChanges": [{
                    "start": { "line": 1, "offset": 1 },
                    "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                    "newText": updated_content
                }]
            }],
            "fixId": "fixClassIncorrectlyImplementsInterface",
            "fixAllDescription": "Implement all unimplemented interfaces",
        }))
    }

    pub(super) fn implement_interface_plan(
        &self,
        file_path: &str,
        content: &str,
    ) -> Option<ImplementInterfacePlan> {
        let (_, interface_name, class_open_brace, class_close_brace) =
            find_first_implements_class(content)?;
        let class_imports = parse_named_import_map(content);
        let (interface_file_path, interface_content) =
            if let Some(interface_module_specifier) = class_imports.get(&interface_name).cloned() {
                let interface_file_path =
                    resolve_module_path(file_path, &interface_module_specifier, &self.open_files)?;
                let interface_content = self
                    .open_files
                    .get(&interface_file_path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&interface_file_path).ok())?;
                (interface_file_path, interface_content)
            } else if content.contains(&format!("interface {interface_name}")) {
                (file_path.to_string(), content.to_string())
            } else {
                return None;
            };

        let interface_properties = parse_interface_properties(&interface_content, &interface_name)?;
        if interface_properties.is_empty() {
            return None;
        }

        let class_body = content.get(class_open_brace + 1..class_close_brace)?;
        let mut missing_members = Vec::new();
        for member in interface_properties {
            if !class_body_has_member(class_body, member.name()) {
                missing_members.push(member);
            }
        }
        if missing_members.is_empty() {
            return None;
        }

        let interface_imports = parse_named_import_map(&interface_content);
        Some(ImplementInterfacePlan {
            interface_name,
            interface_file_path,
            class_open_brace,
            class_close_brace,
            class_imports,
            interface_imports,
            missing_members,
        })
    }

    fn implement_interface_import_lines(
        &self,
        file_path: &str,
        plan: &mut ImplementInterfacePlan,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_ending: Option<&str>,
        import_module_specifier_preference: Option<&str>,
    ) -> Vec<String> {
        let mut needed_imports: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for member in &plan.missing_members {
            for ident in extract_type_identifiers(member.referenced_types()) {
                if should_import_identifier(&ident) && !plan.class_imports.contains_key(&ident) {
                    needed_imports.insert(ident);
                }
            }
        }

        let mut import_lines = Vec::new();
        for ident in needed_imports {
            if !self.interface_symbol_import_is_usable(
                &plan.interface_file_path,
                &plan.interface_imports,
                &ident,
                auto_import_file_exclude_patterns,
            ) {
                continue;
            }
            if let Some(module_specifier) = self.best_import_module_specifier_for_name(
                file_path,
                &ident,
                auto_import_file_exclude_patterns,
                auto_import_specifier_exclude_regexes,
                import_module_specifier_ending,
                import_module_specifier_preference,
            ) && let std::collections::hash_map::Entry::Vacant(entry) =
                plan.class_imports.entry(ident.clone())
            {
                import_lines.push(format!("import {{ {ident} }} from '{module_specifier}';"));
                entry.insert(module_specifier);
            }
        }
        import_lines
    }

    pub(super) fn render_implement_interface_content(
        content: &str,
        plan: &ImplementInterfacePlan,
        import_lines: &[String],
    ) -> Option<String> {
        let class_body = content.get(plan.class_open_brace + 1..plan.class_close_brace)?;
        let members_text = plan
            .missing_members
            .iter()
            .map(|m| format!("    {}", m.render()))
            .collect::<Vec<_>>()
            .join("\n");
        let updated_body = if class_body.trim().is_empty() {
            format!("\n{members_text}\n")
        } else {
            format!("{}\n{}\n", class_body.trim_end(), members_text)
        };
        let mut updated_content = format!(
            "{}{}{}",
            &content[..plan.class_open_brace + 1],
            updated_body,
            &content[plan.class_close_brace..]
        );

        for import_line in import_lines.iter().rev() {
            if !updated_content.contains(import_line) {
                updated_content = format!("{import_line}\n{updated_content}");
            }
        }
        (updated_content != content).then_some(updated_content)
    }
}

#[cfg(test)]
#[path = "handlers_code_fixes_implement_interface_tests.rs"]
mod tests;
