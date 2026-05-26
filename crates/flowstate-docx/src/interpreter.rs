use std::{
  collections::{HashMap, HashSet},
  io,
  path::Path,
};

use rdocx::Document as RDocxDocument;
use rdocx_opc::OpcPackage;
use rdocx_oxml::document::CT_Document;
use rdocx_oxml::properties::{CT_PPr, CT_RPr};

use super::cleaner::{CleanedDocx, DocxCleanReport, clean_docx_path};
use flowstate_document::{
  Document, DocumentParagraphInput, DocumentRunInput, DocumentTheme, HighlightStyle, ParagraphStyle, RunSemanticStyle, RunStyles,
  document_from_paragraphs,
};

pub const RECOGNITION_RULES: &[RecognitionRule] = &[
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Heading1",
    db8_style: ParagraphStyle::Pocket,
  },
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Heading2",
    db8_style: ParagraphStyle::Hat,
  },
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Heading3",
    db8_style: ParagraphStyle::Block,
  },
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Heading4",
    db8_style: ParagraphStyle::Tag,
  },
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Analytic",
    db8_style: ParagraphStyle::Analytic,
  },
  RecognitionRule::ParagraphStyle {
    docx_style_id: "Undertag",
    db8_style: ParagraphStyle::Undertag,
  },
  RecognitionRule::ParagraphFallbackNormal,
  RecognitionRule::RunStyle {
    docx_style_id: "Style13ptBold",
    db8_semantic: RunSemanticStyle::Cite,
  },
  RecognitionRule::RunStyle {
    docx_style_id: "Emphasis",
    db8_semantic: RunSemanticStyle::Emphasis,
  },
  RecognitionRule::RunStyle {
    docx_style_id: "StyleUnderline",
    db8_semantic: RunSemanticStyle::Underline,
  },
  RecognitionRule::RunDirectUnderline,
  RecognitionRule::RunStrikethrough,
  RecognitionRule::RunHighlightToSpoken,
  RecognitionRule::RunShadingToSpoken,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecognitionRule {
  ParagraphStyle {
    docx_style_id: &'static str,
    db8_style: ParagraphStyle,
  },
  ParagraphFallbackNormal,
  RunStyle {
    docx_style_id: &'static str,
    db8_semantic: RunSemanticStyle,
  },
  RunDirectUnderline,
  RunStrikethrough,
  RunHighlightToSpoken,
  RunShadingToSpoken,
}

#[derive(Clone, Debug)]
pub struct DocxConversionReport {
  pub clean: DocxCleanReport,
  pub recognition_rules: &'static [RecognitionRule],
  pub paragraphs_imported: usize,
  pub runs_imported: usize,
  pub unknown_paragraph_styles: Vec<String>,
  pub unknown_run_styles: Vec<String>,
}

pub fn convert_docx_to_document(path: impl AsRef<Path>) -> io::Result<(Document, DocxConversionReport)> {
  let cleaned = clean_docx_path(path)?;
  convert_cleaned_docx_to_document(cleaned)
}

pub fn convert_docx_bytes_to_document(bytes: &[u8]) -> io::Result<(Document, DocxConversionReport)> {
  let cleaned = super::cleaner::clean_docx_bytes(bytes)?;
  convert_cleaned_docx_to_document(cleaned)
}

pub fn convert_cleaned_docx_to_document(cleaned: CleanedDocx) -> io::Result<(Document, DocxConversionReport)> {
  let docx = RDocxDocument::from_bytes(&cleaned.bytes).map_err(rdocx_error)?;
  let direct_properties = direct_run_properties_by_paragraph(&cleaned.bytes)?;
  let style_resolver = StyleResolver::new(&docx);
  let docx_paragraphs = docx.paragraphs();
  let mut paragraphs = Vec::with_capacity(docx_paragraphs.len());
  let mut paragraph_property_cache: HashMap<Option<String>, CT_PPr> = HashMap::new();
  let mut run_property_cache: HashMap<(Option<String>, Option<String>), CT_RPr> = HashMap::new();
  let mut runs_imported = 0usize;
  let mut unknown_paragraph_styles = Vec::new();
  let mut unknown_run_styles = Vec::new();
  let mut unknown_paragraph_style_seen = HashSet::new();
  let mut unknown_run_style_seen = HashSet::new();
  let mut current_section_has_underline = false;
  let mut after_heading_seeking_text = false;

  for (paragraph_ix, paragraph) in docx_paragraphs.into_iter().enumerate() {
    let style_id = paragraph.style_id();
    let paragraph_style_key = style_id.map(str::to_string);
    let paragraph_properties = paragraph_property_cache
      .entry(paragraph_style_key.clone())
      .or_insert_with(|| docx.resolve_paragraph_properties(style_id));
    let paragraph_properties: &CT_PPr = paragraph_properties;
    let run_facts = paragraph
      .runs()
      .enumerate()
      .map(|(run_ix, run)| {
        let text = run.text();
        let run_style_id = run.style_id().map(str::to_string);
        let run_style_id_ref = run_style_id.as_deref();
        let effective = run_property_cache
          .entry((paragraph_style_key.clone(), run_style_id.clone()))
          .or_insert_with(|| docx.resolve_run_properties(style_id, run_style_id_ref));
        let effective: &CT_RPr = effective;
        let direct = direct_properties
          .get(paragraph_ix)
          .and_then(|paragraph| paragraph.get(run_ix))
          .cloned()
          .unwrap_or_default();
        let run_size = run.size();
        let source_size_pt = run_size.or(direct.size_pt);
        RunFact {
          text,
          style_id: run_style_id,
          bold: run.is_bold() || direct.bold || effective.bold == Some(true) || effective.bold_cs == Some(true),
          bold_off: direct.bold_off || (effective.bold == Some(false) && effective.bold_cs != Some(true)),
          underline: direct.underline || underline_is_on(&effective.underline),
          strikethrough: direct.strikethrough || effective.strike == Some(true) || effective.dstrike == Some(true),
          highlight: direct.highlight || effective.highlight.is_some() || effective.shading.is_some(),
          border: false,
          source_size_pt,
          size_pt: source_size_pt.or_else(|| effective.sz.map(|size| size.to_pt())),
          color: run.color().is_some() || direct.color || effective.color.is_some() || effective.color_theme.is_some(),
        }
      })
      .collect::<Vec<_>>();

    let style = recognize_paragraph_style(style_id, paragraph_properties, &run_facts, &style_resolver);
    if style == ParagraphStyle::Normal
      && let Some(style_id) = style_id
      && !style_resolver.is_known_paragraph_style(style_id)
    {
      push_unique_with_seen(&mut unknown_paragraph_styles, &mut unknown_paragraph_style_seen, style_id);
    }

    let is_heading = matches!(
      style,
      ParagraphStyle::Pocket | ParagraphStyle::Hat | ParagraphStyle::Block | ParagraphStyle::Tag | ParagraphStyle::Analytic
    );
    let structural_underline_is_direct = matches!(style, ParagraphStyle::Tag | ParagraphStyle::Analytic | ParagraphStyle::Undertag);
    let suppress_semantic_underline = matches!(
      style,
      ParagraphStyle::Pocket
        | ParagraphStyle::Hat
        | ParagraphStyle::Block
        | ParagraphStyle::Tag
        | ParagraphStyle::Analytic
        | ParagraphStyle::Undertag
    );
    let mut can_process_citations = false;
    if is_heading {
      current_section_has_underline = false;
      after_heading_seeking_text = true;
    } else if after_heading_seeking_text {
      let has_text = run_facts.iter().any(|run| !run.text.trim().is_empty());
      if has_text && style != ParagraphStyle::Undertag {
        can_process_citations = true;
        after_heading_seeking_text = false;
      }
    }
    if !is_heading && run_facts.iter().any(|run| run.underline && !run.bold) {
      current_section_has_underline = true;
    }

    let bold_paragraph_overrides = if can_process_citations {
      entirely_bold_paragraph_overrides(&run_facts)
    } else {
      None
    };

    let mut runs = Vec::new();
    for (run_ix, run) in run_facts.iter().enumerate() {
      let text = run.text.clone();
      if text.is_empty() {
        continue;
      }
      if let Some(style_id) = run.style_id.as_deref()
        && recognize_run_semantic(style_id, &style_resolver).is_none()
      {
        push_unique_with_seen(&mut unknown_run_styles, &mut unknown_run_style_seen, style_id);
      }

      let styles = RunStyles {
        semantic: recognize_run_semantic_for_context(
          run,
          run_ix,
          bold_paragraph_overrides.as_deref(),
          suppress_semantic_underline,
          style,
          can_process_citations,
          current_section_has_underline,
          &style_resolver,
        ),
        direct_underline: structural_underline_is_direct && run.underline,
        strikethrough: run.strikethrough,
        highlight: if run.highlight { Some(HighlightStyle::Spoken) } else { None },
      };

      runs.push(DocumentRunInput { text, styles });
      runs_imported += 1;
    }

    if runs.is_empty() {
      let text = paragraph.text();
      if !text.is_empty() {
        runs.push(DocumentRunInput {
          text,
          styles: RunStyles::default(),
        });
        runs_imported += 1;
      }
    }

    paragraphs.push(DocumentParagraphInput { style, runs });
  }

  let paragraphs_imported = paragraphs.len();
  let document = document_from_paragraphs(DocumentTheme::default(), paragraphs);
  let report = DocxConversionReport {
    clean: cleaned.report,
    recognition_rules: RECOGNITION_RULES,
    paragraphs_imported,
    runs_imported,
    unknown_paragraph_styles,
    unknown_run_styles,
  };
  Ok((document, report))
}

#[derive(Clone, Debug)]
struct RunFact {
  text: String,
  style_id: Option<String>,
  bold: bool,
  bold_off: bool,
  underline: bool,
  strikethrough: bool,
  highlight: bool,
  border: bool,
  source_size_pt: Option<f64>,
  size_pt: Option<f64>,
  color: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct DirectRunProperties {
  bold: bool,
  bold_off: bool,
  underline: bool,
  strikethrough: bool,
  highlight: bool,
  size_pt: Option<f64>,
  color: bool,
}

fn direct_run_properties_by_paragraph(bytes: &[u8]) -> io::Result<Vec<Vec<DirectRunProperties>>> {
  let package = OpcPackage::from_reader(std::io::Cursor::new(bytes)).map_err(rdocx_opc_error)?;
  let doc_part_name = package
    .main_document_part()
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "DOCX package has no main document part"))?;
  let doc_xml = package
    .get_part(&doc_part_name)
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "DOCX package has no main document XML"))?;
  let document = CT_Document::from_xml(doc_xml).map_err(rdocx_oxml_error)?;
  Ok(
    document
      .body
      .paragraphs()
      .map(|paragraph| {
        paragraph
          .runs
          .iter()
          .map(|run| {
            let Some(properties) = run.properties.as_ref() else {
              return DirectRunProperties::default();
            };
            DirectRunProperties {
              bold: properties.bold == Some(true) || properties.bold_cs == Some(true),
              bold_off: properties.bold == Some(false) && properties.bold_cs != Some(true),
              underline: underline_is_on(&properties.underline),
              strikethrough: properties.strike == Some(true) || properties.dstrike == Some(true),
              highlight: properties.highlight.is_some() || properties.shading.is_some(),
              size_pt: properties.sz.map(|size| size.to_pt()),
              color: properties.color.is_some() || properties.color_theme.is_some(),
            }
          })
          .collect()
      })
      .collect(),
  )
}

struct StyleResolver {
  names_by_id: HashMap<String, String>,
  known_paragraph_style_ids: HashSet<String>,
  run_semantics_by_id: HashMap<String, Option<RunSemanticStyle>>,
}

impl StyleResolver {
  fn new(docx: &RDocxDocument) -> Self {
    let mut names_by_id = HashMap::new();
    let mut known_paragraph_style_ids = HashSet::new();
    let mut run_semantics_by_id = HashMap::new();

    for style in docx.styles() {
      let style_id = style.style_id();
      let canonical_source = style.name().unwrap_or(style_id);
      if matches!(
        canonical_paragraph_style_name(canonical_source),
        Some("Heading1" | "Heading2" | "Heading3" | "Heading4" | "Analytic" | "Undertag" | "Normal")
      ) {
        known_paragraph_style_ids.insert(style_id.to_string());
      }
      run_semantics_by_id.insert(style_id.to_string(), run_semantic_from_canonical_name(canonical_source));
      if let Some(name) = style.name() {
        names_by_id.insert(style_id.to_string(), name.to_string());
      }
    }

    Self {
      names_by_id,
      known_paragraph_style_ids,
      run_semantics_by_id,
    }
  }

  fn name(&self, style_id: &str) -> Option<&str> {
    self.names_by_id.get(style_id).map(String::as_str)
  }

  fn canonical_name<'a>(&'a self, style_id: Option<&'a str>) -> &'a str {
    style_id
      .and_then(|id| self.name(id))
      .unwrap_or_else(|| style_id.unwrap_or("Normal"))
  }

  fn is_known_paragraph_style(&self, style_id: &str) -> bool {
    self.known_paragraph_style_ids.contains(style_id)
      || matches!(
        canonical_paragraph_style_name(self.canonical_name(Some(style_id))),
        Some("Heading1" | "Heading2" | "Heading3" | "Heading4" | "Analytic" | "Undertag" | "Normal")
      )
  }

  fn run_semantic(&self, style_id: &str) -> Option<RunSemanticStyle> {
    if let Some(semantic) = self.run_semantics_by_id.get(style_id) {
      return *semantic;
    }
    run_semantic_from_canonical_name(self.canonical_name(Some(style_id)))
  }
}

fn recognize_paragraph_style(
  style_id: Option<&str>,
  paragraph_properties: &impl ParagraphProperties,
  runs: &[RunFact],
  styles: &StyleResolver,
) -> ParagraphStyle {
  if paragraph_properties.outline_lvl() == Some(0) && runs.iter().any(|run| run.bold && run.size_pt == Some(26.0)) {
    return ParagraphStyle::Pocket;
  }
  if paragraph_properties.outline_lvl() == Some(1) && runs.iter().any(|run| run.bold && run.size_pt == Some(22.0)) {
    return ParagraphStyle::Hat;
  }
  if paragraph_properties.outline_lvl() == Some(2)
    && runs
      .iter()
      .any(|run| run.bold && run.underline && run.size_pt == Some(16.0))
  {
    return ParagraphStyle::Block;
  }
  if paragraph_properties.outline_lvl() == Some(3) && runs.iter().any(|run| run.bold && run.color) {
    return ParagraphStyle::Tag;
  }

  match canonical_paragraph_style_name(styles.canonical_name(style_id)) {
    Some("Heading1") => ParagraphStyle::Pocket,
    Some("Heading2") => ParagraphStyle::Hat,
    Some("Heading3") => ParagraphStyle::Block,
    Some("Heading4") => ParagraphStyle::Tag,
    Some("Analytic") => ParagraphStyle::Analytic,
    Some("Undertag") => ParagraphStyle::Undertag,
    _ => ParagraphStyle::Normal,
  }
}

trait ParagraphProperties {
  fn outline_lvl(&self) -> Option<u32>;
}

impl ParagraphProperties for rdocx_oxml::properties::CT_PPr {
  fn outline_lvl(&self) -> Option<u32> {
    self.outline_lvl
  }
}

fn recognize_run_semantic(style_id: &str, styles: &StyleResolver) -> Option<RunSemanticStyle> {
  styles.run_semantic(style_id)
}

fn run_semantic_from_canonical_name(name: &str) -> Option<RunSemanticStyle> {
  match canonical_run_style_name(name) {
    Some("Style13ptBold") => Some(RunSemanticStyle::Cite),
    Some("Emphasis") => Some(RunSemanticStyle::Emphasis),
    Some("StyleUnderline") => Some(RunSemanticStyle::Underline),
    _ => None,
  }
}

fn recognize_run_semantic_for_context(
  run: &RunFact,
  run_ix: usize,
  bold_paragraph_overrides: Option<&[bool]>,
  suppress_semantic_underline: bool,
  paragraph_style: ParagraphStyle,
  can_process_citations: bool,
  current_section_has_underline: bool,
  styles: &StyleResolver,
) -> RunSemanticStyle {
  if run.border {
    return RunSemanticStyle::Emphasis;
  }

  let explicit = run
    .style_id
    .as_deref()
    .and_then(|style_id| recognize_run_semantic(style_id, styles));

  if suppress_semantic_underline {
    return explicit
      .filter(|semantic| *semantic != RunSemanticStyle::Underline)
      .unwrap_or_default();
  }

  if run.bold_off && explicit == Some(RunSemanticStyle::Cite) {
    return RunSemanticStyle::default();
  }
  if explicit == Some(RunSemanticStyle::Cite) && !can_process_citations && !run.underline {
    return if run.highlight {
      RunSemanticStyle::Underline
    } else {
      RunSemanticStyle::default()
    };
  }
  if let Some(overrides) = bold_paragraph_overrides
    && overrides.get(run_ix) == Some(&true)
  {
    return RunSemanticStyle::Cite;
  }
  if can_process_citations && run.bold && !matches!(explicit, Some(RunSemanticStyle::Underline | RunSemanticStyle::Emphasis)) {
    return RunSemanticStyle::Cite;
  }
  if run.underline && !run.bold && !matches!(explicit, Some(RunSemanticStyle::Emphasis | RunSemanticStyle::Cite)) {
    return RunSemanticStyle::Underline;
  }
  if run.bold && run.underline {
    return if current_section_has_underline {
      RunSemanticStyle::Emphasis
    } else {
      RunSemanticStyle::Underline
    };
  }
  if run.highlight && explicit.is_none() {
    return RunSemanticStyle::Underline;
  }
  let semantic = explicit.unwrap_or_default();
  if semantic == RunSemanticStyle::Plain
    && paragraph_style == ParagraphStyle::Normal
    && !run.underline
    && !run.highlight
    && run.source_size_pt.is_some_and(|size| size <= 8.0)
  {
    return RunSemanticStyle::Condensed;
  }
  semantic
}

fn entirely_bold_paragraph_overrides(runs: &[RunFact]) -> Option<Vec<bool>> {
  let text_run_indices = runs
    .iter()
    .enumerate()
    .filter_map(|(ix, run)| (!run.text.trim().is_empty()).then_some(ix))
    .collect::<Vec<_>>();
  if text_run_indices.is_empty() || text_run_indices.iter().any(|ix| !runs[*ix].bold) {
    return None;
  }

  let paragraph_text_len = text_run_indices
    .iter()
    .map(|ix| runs[*ix].text.as_str())
    .collect::<String>()
    .trim()
    .chars()
    .count();
  let mut cite = vec![false; runs.len()];
  if paragraph_text_len <= 60 {
    for ix in text_run_indices {
      cite[ix] = true;
    }
    return Some(cite);
  }

  if let Some(base_size) = most_common_half_point_size(runs, &text_run_indices) {
    let mut found = false;
    for ix in &text_run_indices {
      if runs[*ix].size_pt.is_some_and(|size| size > base_size + 0.5) {
        cite[*ix] = true;
        found = true;
      }
    }
    if found {
      return Some(cite);
    }
  }

  let highlighted = text_run_indices
    .iter()
    .filter(|ix| runs[**ix].highlight)
    .copied()
    .collect::<Vec<_>>();
  if !highlighted.is_empty() {
    for ix in highlighted {
      cite[ix] = true;
    }
    return Some(cite);
  }

  if let Some(first_digit_run) = text_run_indices
    .iter()
    .position(|ix| runs[*ix].text.chars().any(|ch| ch.is_ascii_digit()))
  {
    for ix in text_run_indices.iter().take(first_digit_run + 1) {
      cite[*ix] = true;
    }
    return Some(cite);
  }

  for ix in text_run_indices {
    cite[ix] = true;
  }
  Some(cite)
}

fn most_common_half_point_size(runs: &[RunFact], indices: &[usize]) -> Option<f64> {
  let mut counts: HashMap<i32, usize> = HashMap::new();
  for ix in indices {
    let Some(size) = runs[*ix].size_pt else {
      continue;
    };
    if (6.0..=72.0).contains(&size) {
      *counts.entry((size * 2.0).round() as i32).or_default() += 1;
    }
  }
  counts
    .into_iter()
    .max_by(|(size_a, count_a), (size_b, count_b)| count_a.cmp(count_b).then_with(|| size_b.cmp(size_a)))
    .map(|(half_points, _)| half_points as f64 / 2.0)
}

fn canonical_paragraph_style_name(name: &str) -> Option<&'static str> {
  match normalized_style_token(name).as_str() {
    "normal" => Some("Normal"),
    "heading1" | "pocket" => Some("Heading1"),
    "heading2" | "hat" => Some("Heading2"),
    "heading3" | "block" => Some("Heading3"),
    "heading4" | "tag" => Some("Heading4"),
    "analytic" | "analytics" => Some("Analytic"),
    "undertag" => Some("Undertag"),
    _ => None,
  }
}

fn canonical_run_style_name(name: &str) -> Option<&'static str> {
  match normalized_style_token(name).as_str() {
    "style13ptbold" | "cite" | "oldcite" => Some("Style13ptBold"),
    "styleunderline" | "underline" => Some("StyleUnderline"),
    "emphasis" => Some("Emphasis"),
    "heading1char" | "pocketchar" => Some("Style13ptBold"),
    "heading2char" | "hatchar" | "heading3char" | "blockchar" | "heading4char" | "tagchar" => Some("Emphasis"),
    _ => None,
  }
}

fn normalized_style_token(name: &str) -> String {
  name
    .chars()
    .filter(|ch| ch.is_ascii_alphanumeric())
    .flat_map(char::to_lowercase)
    .collect()
}

fn underline_is_on<T: std::fmt::Debug>(underline: &Option<T>) -> bool {
  underline
    .as_ref()
    .is_some_and(|value| format!("{value:?}") != "None")
}

fn push_unique_with_seen(values: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
  if !seen.contains(value) {
    let value = value.to_string();
    seen.insert(value.clone());
    values.push(value);
  }
}

fn rdocx_error(error: rdocx::Error) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error)
}

fn rdocx_opc_error(error: rdocx_opc::OpcError) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error)
}

fn rdocx_oxml_error(error: rdocx_oxml::error::OxmlError) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error)
}
