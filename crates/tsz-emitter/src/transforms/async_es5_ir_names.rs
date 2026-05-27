use super::*;

impl<'a> AsyncES5Transformer<'a> {
    pub(in crate::transforms) fn push_lowering_hoist(&self, name: String) {
        self.pending_lowering_hoists.borrow_mut().push(name);
    }

    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    pub(in crate::transforms) fn extract_trailing_line_comment_in_node(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(idx)?;
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let mut scan_end = std::cmp::min(node.end as usize, bytes.len());
        while scan_end < bytes.len() && !matches!(bytes[scan_end], b'\n' | b'\r') {
            scan_end += 1;
        }
        for comment in tsz_common::comments::get_comment_ranges(source_text) {
            if comment.is_multi_line || comment.pos < node.pos || comment.end as usize > scan_end {
                continue;
            }
            let mut line_start = comment.pos as usize;
            while line_start > 0 && !matches!(bytes[line_start - 1], b'\n' | b'\r') {
                line_start -= 1;
            }
            if !source_text[line_start..comment.pos as usize]
                .trim()
                .is_empty()
            {
                return Some(comment.get_text(source_text).to_string());
            }
        }
        None
    }

    pub fn with_class_super_context(
        mut self,
        has_super: bool,
        super_name: String,
        is_static: bool,
    ) -> Self {
        self.class_has_super = has_super;
        self.class_super_name = super_name;
        self.class_super_is_static = is_static;
        self
    }

    /// Set the module kind so dynamic `import()` calls inside the generator
    /// body are lowered to the appropriate module-system form.
    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.module_kind = kind;
    }

    pub const fn set_downlevel_iteration(&mut self, enabled: bool) {
        self.downlevel_iteration = enabled;
    }

    pub(crate) fn set_lexical_this_capture(&self, capture: bool) {
        self.lexical_this_capture.set(capture);
    }

    pub(in crate::transforms) const fn captures_lexical_this(&self) -> bool {
        self.lexical_this_capture.get()
    }

    pub(in crate::transforms) const fn captures_this_references(&self) -> bool {
        self.capture_this_references.get()
    }

    pub(in crate::transforms) fn set_capture_this_references(&self, capture: bool) {
        self.capture_this_references.set(capture);
    }

    pub(in crate::transforms) fn reset_loop_exit_placeholders(&self) {
        self.loop_exit_placeholder_counter.set(0);
    }

    pub(in crate::transforms) fn next_loop_exit_placeholder(&self) -> u32 {
        let counter = self.loop_exit_placeholder_counter.get();
        self.loop_exit_placeholder_counter.set(counter + 1);
        u32::MAX - counter
    }

    pub(in crate::transforms) fn generate_hoisted_temp(&self) -> String {
        loop {
            let counter = self.temp_var_counter.get();
            let name = if counter < 26 {
                format!("_{}", (b'a' + counter as u8) as char)
            } else {
                format!("_{counter}")
            };
            self.temp_var_counter.set(counter + 1);
            if self.blocked_temp_names.borrow_mut().insert(name.clone()) {
                return name;
            }
        }
    }

    pub(in crate::transforms) fn set_temp_var_counter(&self, counter: u32) {
        self.temp_var_counter.set(counter);
    }

    pub const fn temp_var_counter(&self) -> u32 {
        self.temp_var_counter.get()
    }

    pub(in crate::transforms) fn reset_temp_name_reservations(&self, body_idx: NodeIndex) {
        let mut blocked_names = Vec::new();
        self.collect_body_binding_names(body_idx, &mut blocked_names);
        *self.blocked_temp_names.borrow_mut() = blocked_names.into_iter().collect();
    }

    pub(in crate::transforms) fn fresh_reserved_name(
        &self,
        preferred: impl Into<String>,
    ) -> String {
        let preferred = preferred.into();
        if self
            .blocked_temp_names
            .borrow_mut()
            .insert(preferred.clone())
        {
            return preferred;
        }
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{preferred}_{suffix}");
            if self
                .blocked_temp_names
                .borrow_mut()
                .insert(candidate.clone())
            {
                return candidate;
            }
            suffix += 1;
        }
    }

    pub(in crate::transforms) fn fresh_catch_binding_temp(
        &self,
        source_name: &str,
        catch_clause: NodeIndex,
    ) -> String {
        let ordinal = self.async_catch_binding_ordinal(catch_clause);
        self.fresh_reserved_name(format!("{source_name}_{ordinal}"))
    }

    fn async_catch_binding_ordinal(&self, catch_clause: NodeIndex) -> u32 {
        let Some(current_catch) = self.arena.get(catch_clause) else {
            return 1;
        };
        let mut ordinal = 1;
        for node in &self.arena.nodes {
            if node.kind != syntax_kind_ext::TRY_STATEMENT {
                continue;
            }
            let Some(try_data) = self.arena.get_try(node) else {
                continue;
            };
            if try_data.catch_clause.is_none() {
                continue;
            }
            let Some(previous_catch) = self.arena.get(try_data.catch_clause) else {
                continue;
            };
            if previous_catch.pos >= current_catch.pos {
                continue;
            }
            if self.contains_await_recursive(try_data.try_block)
                || self.contains_await_recursive(try_data.catch_clause)
                || self.contains_await_recursive(try_data.finally_block)
            {
                ordinal += 1;
            }
        }
        ordinal
    }

    pub fn set_disposable_env_context<I>(&mut self, next_id: u32, blocked_names: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.disposable_env_counter.set(next_id);
        self.blocked_disposable_env_names = blocked_names.into_iter().collect();
        self.generated_disposable_env_names.clear();
    }

    pub const fn disposable_env_counter(&self) -> u32 {
        self.disposable_env_counter.get()
    }

    pub fn take_generated_disposable_env_names(&mut self) -> Vec<String> {
        std::mem::take(&mut self.generated_disposable_env_names)
    }

    pub(in crate::transforms) fn next_disposable_env_names(&mut self) -> (String, String, String) {
        loop {
            let env_id = self.disposable_env_counter.get();
            let env_name = format!("env_{env_id}");
            let error_name = format!("e_{env_id}");
            let result_name = format!("result_{env_id}");
            self.disposable_env_counter.set(env_id + 1);

            if self.blocked_disposable_env_names.contains(&env_name)
                || self.blocked_disposable_env_names.contains(&error_name)
                || self.blocked_disposable_env_names.contains(&result_name)
            {
                continue;
            }

            self.blocked_disposable_env_names.insert(env_name.clone());
            self.blocked_disposable_env_names.insert(error_name.clone());
            self.blocked_disposable_env_names
                .insert(result_name.clone());
            self.generated_disposable_env_names.push(env_name.clone());
            self.generated_disposable_env_names.push(error_name.clone());
            self.generated_disposable_env_names
                .push(result_name.clone());
            return (env_name, error_name, result_name);
        }
    }

    pub(in crate::transforms) fn next_disposable_env_names_allowing_error_gap(
        &mut self,
    ) -> (String, String, String, u32) {
        loop {
            let env_id = self.disposable_env_counter.get();
            let env_name = format!("env_{env_id}");
            let result_name = format!("result_{env_id}");
            self.disposable_env_counter.set(env_id + 1);

            if self.blocked_disposable_env_names.contains(&env_name)
                || self.blocked_disposable_env_names.contains(&result_name)
            {
                continue;
            }

            let mut error_id = env_id;
            loop {
                let error_name = format!("e_{error_id}");
                if self.blocked_disposable_env_names.contains(&error_name) {
                    error_id += 1;
                    continue;
                }

                self.blocked_disposable_env_names.insert(env_name.clone());
                self.blocked_disposable_env_names.insert(error_name.clone());
                self.blocked_disposable_env_names
                    .insert(result_name.clone());
                self.generated_disposable_env_names.push(env_name.clone());
                self.generated_disposable_env_names.push(error_name.clone());
                self.generated_disposable_env_names
                    .push(result_name.clone());
                return (env_name, error_name, result_name, error_id);
            }
        }
    }

    pub(in crate::transforms) fn env_id_from_name(&self, name: &str) -> Option<u32> {
        name.strip_prefix("env_")?.parse().ok()
    }

    /// Get the helpers needed after transformation.
    pub const fn get_helpers_needed(&self) -> &HelpersNeeded {
        &self.helpers_needed
    }

    /// Take the helpers needed, consuming the transformer.
    pub fn take_helpers_needed(self) -> HelpersNeeded {
        self.helpers_needed
    }
}
