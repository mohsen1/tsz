use super::super::Printer;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn insert_source_file_hoisted_temp_declarations(
        &mut self,
        hoist_byte_offset: usize,
        hoist_line: u32,
    ) {
        let mut ref_vars = Vec::new();
        ref_vars.extend(self.hoisted_assignment_temps.iter().cloned());
        ref_vars.extend(self.hoisted_for_of_temps.iter().cloned());

        if !self.hoisted_deferred_static_class_result_temps.is_empty() {
            let var_decl = format!(
                "var {};",
                self.hoisted_deferred_static_class_result_temps.join(", ")
            );
            self.writer
                .insert_line_at(hoist_byte_offset, hoist_line, &var_decl);
        }

        let file_level_class_temps = self.file_level_class_temps_not_in_ref_buckets(&ref_vars);
        if !ref_vars.is_empty() {
            let mut same_location_ref_vars = file_level_class_temps.clone();
            same_location_ref_vars.extend(ref_vars.iter().cloned());
            let var_decl = format!("var {};", same_location_ref_vars.join(", "));
            self.writer
                .insert_line_at(hoist_byte_offset, hoist_line, &var_decl);
        }
        if !file_level_class_temps.is_empty() && ref_vars.is_empty() {
            let var_decl = format!("var {};", file_level_class_temps.join(", "));
            self.writer
                .insert_line_at(hoist_byte_offset, hoist_line, &var_decl);
        }

        if !self.hoisted_assignment_value_temps.is_empty() {
            let var_decl = format!("var {};", self.hoisted_assignment_value_temps.join(", "));
            self.writer
                .insert_line_at(hoist_byte_offset, hoist_line, &var_decl);
        }
    }

    fn file_level_class_temps_not_in_ref_buckets(&self, ref_vars: &[String]) -> Vec<String> {
        self.hoisted_file_level_class_temps
            .iter()
            .filter(|name| {
                !ref_vars.contains(name)
                    && !self
                        .hoisted_deferred_static_class_result_temps
                        .contains(name)
            })
            .cloned()
            .collect()
    }

    pub(in crate::emitter) fn insert_cjs_destructuring_export_temp_declarations(&mut self) {
        if self.cjs_destructuring_export_temps.is_empty() {
            return;
        }
        let insertion_indent = self.writer.get_output()[self.cjs_destr_hoist_byte_offset..]
            .chars()
            .take_while(|&c| c == ' ')
            .collect::<String>();
        let var_decl = format!(
            "{}var {};",
            insertion_indent,
            self.cjs_destructuring_export_temps.join(", ")
        );
        self.writer.insert_line_at(
            self.cjs_destr_hoist_byte_offset,
            self.cjs_destr_hoist_line,
            &var_decl,
        );
    }
}
