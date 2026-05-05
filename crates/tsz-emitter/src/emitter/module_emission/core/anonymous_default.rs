use crate::emitter::Printer;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn next_anonymous_default_export_name(&mut self) -> String {
        loop {
            self.next_anonymous_default_index += 1;
            let candidate = format!("default_{}", self.next_anonymous_default_index);
            if !self.file_identifiers.contains(&candidate)
                && !self.generated_temp_names.contains(&candidate)
            {
                self.generated_temp_names.insert(candidate.clone());
                return candidate;
            }
        }
    }
}
