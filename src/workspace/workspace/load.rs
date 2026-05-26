fn load_document_for_open(path: &PathBuf) -> std::io::Result<(Document, Option<PathBuf>)> {
  if path
    .extension()
    .and_then(|extension| extension.to_str())
    .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
  {
    let (document, _) = convert_docx_to_document(path)?;
    return Ok((document, Some(path.with_extension("db8"))));
  }

  load_or_create_document(path).map(|document| (document, Some(path.clone())))
}

