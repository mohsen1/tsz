use tsz::checker::diagnostics::Diagnostic;
use tsz::parallel::MergedProgram;

pub(super) fn diagnostic_source_line<'a>(
    program: &'a MergedProgram,
    diagnostic: &Diagnostic,
) -> Option<&'a str> {
    let file = program.files.iter().find(|file| {
        file.file_name == diagnostic.file || file.file_name.ends_with(&diagnostic.file)
    })?;
    let source_file = file.arena.get_source_file_at(file.source_file)?;
    let source_text = source_file.text.as_ref();
    let start = (diagnostic.start as usize).min(source_text.len());
    let line_start = source_text[..start]
        .rfind('\n')
        .map_or(0, |idx| idx.saturating_add(1));
    let line_end = source_text[start..]
        .find('\n')
        .map_or(source_text.len(), |idx| start + idx);
    Some(&source_text[line_start..line_end])
}
