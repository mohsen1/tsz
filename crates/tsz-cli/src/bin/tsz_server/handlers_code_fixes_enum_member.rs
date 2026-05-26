//! Enum-member code-fix planning and rendering helpers.
//!
//! Extracted from `handlers_code_fixes.rs` so the top-level code-fix handler
//! can stay focused on request orchestration.

use super::Server;

#[derive(Debug, PartialEq, Eq)]
struct SimpleEnumMemberInsertionPlan {
    member_name: String,
    insertion_idx: usize,
    previous_member_idx: Option<usize>,
}

impl Server {
    pub(super) fn apply_add_missing_enum_member_simple_fallback(
        content: &str,
    ) -> Option<(String, String)> {
        if content
            .lines()
            .any(|line| line.trim_start().starts_with("////"))
        {
            let normalized = content
                .lines()
                .map(|line| {
                    let ws_len = line.len().saturating_sub(line.trim_start().len());
                    let ws = &line[..ws_len];
                    let trimmed = &line[ws_len..];
                    if let Some(rest) = trimmed.strip_prefix("////") {
                        format!("{ws}{rest}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if normalized != content {
                return Self::apply_add_missing_enum_member_simple_fallback(&normalized);
            }
        }

        let (enum_name, member_name) = Self::find_simple_missing_enum_member_reference(content)?;
        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        let plan = Self::plan_simple_missing_enum_member_insertion(
            &lines,
            &enum_name,
            member_name.clone(),
        )?;
        let updated = Self::render_simple_missing_enum_member_edit(lines, &plan);
        Some((member_name, updated.join("\n")))
    }

    fn find_simple_missing_enum_member_reference(content: &str) -> Option<(String, String)> {
        let mut enum_name = None::<String>;
        let mut member_name = None::<String>;
        for line in content.lines() {
            let t = line.trim().replace("/**/", "");
            if let Some(dot) = t.find('.') {
                let left = t[..dot]
                    .chars()
                    .rev()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                let right = t[dot + 1..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect::<String>();
                if !left.is_empty() && !right.is_empty() {
                    enum_name = Some(left);
                    member_name = Some(right);
                }
            }
        }

        Some((enum_name?, member_name?))
    }

    fn plan_simple_missing_enum_member_insertion(
        lines: &[String],
        enum_name: &str,
        member_name: String,
    ) -> Option<SimpleEnumMemberInsertionPlan> {
        let mut enum_start = None;
        let mut enum_end = None;
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t.starts_with(&format!("enum {enum_name}"))
                || t.starts_with(&format!("export enum {enum_name}"))
                || t.starts_with(&format!("export const enum {enum_name}"))
            {
                enum_start = Some(i);
                for (j, line) in lines.iter().enumerate().skip(i + 1) {
                    if line.trim() == "}" {
                        enum_end = Some(j);
                        break;
                    }
                }
                break;
            }
        }
        let (start, end) = (enum_start?, enum_end?);
        if lines[start + 1..end]
            .iter()
            .any(|l| l.trim_start().starts_with(&(member_name.clone() + " ")))
            || lines[start + 1..end]
                .iter()
                .any(|l| l.trim_start().starts_with(&(member_name.clone() + ",")))
        {
            return None;
        }

        let previous_member_idx = (end > start + 1).then_some(end - 1);
        Some(SimpleEnumMemberInsertionPlan {
            member_name,
            insertion_idx: end,
            previous_member_idx,
        })
    }

    fn render_simple_missing_enum_member_edit(
        mut lines: Vec<String>,
        plan: &SimpleEnumMemberInsertionPlan,
    ) -> Vec<String> {
        if let Some(previous_member_idx) = plan.previous_member_idx {
            let prev = &lines[previous_member_idx];
            let trimmed_len = prev.trim_end().len();
            let (head, trailing) = prev.split_at(trimmed_len);
            if !head.ends_with(',') && !head.ends_with('{') {
                lines[previous_member_idx] = format!("{head},{trailing}");
            }
        }
        lines.insert(plan.insertion_idx, format!("    {}", plan.member_name));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::Server;

    #[test]
    fn add_missing_enum_member_simple_fallback_builds_an_insertion_plan_before_rendering() {
        let content = "enum Color {\n    Red\n}\nconst choice = Color.Blue;\n";
        let (enum_name, member_name) =
            Server::find_simple_missing_enum_member_reference(content).unwrap();
        assert_eq!(enum_name, "Color");
        assert_eq!(member_name, "Blue");

        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        let plan =
            Server::plan_simple_missing_enum_member_insertion(&lines, &enum_name, member_name)
                .unwrap();
        assert_eq!(plan.insertion_idx, 2);
        assert_eq!(plan.previous_member_idx, Some(1));

        let updated = Server::render_simple_missing_enum_member_edit(lines, &plan).join("\n");
        assert_eq!(
            updated,
            "enum Color {\n    Red,\n    Blue\n}\nconst choice = Color.Blue;"
        );
    }

    #[test]
    fn add_missing_enum_member_simple_fallback_preserves_existing_output_shape() {
        let content = "enum Mode {\n}\nconst mode = Mode.Active;\n";
        let (member_name, updated) =
            Server::apply_add_missing_enum_member_simple_fallback(content).unwrap();

        assert_eq!(member_name, "Active");
        assert_eq!(
            updated,
            "enum Mode {\n    Active\n}\nconst mode = Mode.Active;"
        );
    }
}
