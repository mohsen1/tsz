pub(in crate::emitter) struct ClassEs6EmitOptions<'a> {
    pub suppress_modifiers: bool,
    pub assignment_prefix: Option<(&'a str, String)>,
    pub assignment_alias: Option<&'a str>,
    pub static_initializer_self_alias: Option<&'a str>,
    pub emit_assignment_static_elements_as_statements: bool,
    pub assignment_suffix: Option<&'a str>,
}
