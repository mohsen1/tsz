use super::super::Printer;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn next_disposable_env_names_allowing_error_gap(
        &mut self,
    ) -> (String, String, String, u32) {
        loop {
            let env_id = self.next_disposable_env_id;
            let env_name = format!("env_{env_id}");
            let result_name = format!("result_{env_id}");
            self.next_disposable_env_id += 1;

            if self.file_identifiers.contains(&env_name)
                || self.generated_temp_names.contains(&env_name)
                || self.file_identifiers.contains(&result_name)
                || self.generated_temp_names.contains(&result_name)
            {
                continue;
            }

            let mut error_id = env_id;
            loop {
                let error_name = format!("e_{error_id}");
                if self.file_identifiers.contains(&error_name)
                    || self.generated_temp_names.contains(&error_name)
                {
                    error_id += 1;
                    continue;
                }

                self.generated_temp_names.insert(env_name.clone());
                self.generated_temp_names.insert(error_name.clone());
                self.generated_temp_names.insert(result_name.clone());
                return (env_name, error_name, result_name, env_id);
            }
        }
    }
}
