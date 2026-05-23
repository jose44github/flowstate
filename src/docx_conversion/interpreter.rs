use std::{io, path::Path};

use rdocx::Document as RDocxDocument;

use super::cleaner::{CleanedDocx, DocxCleanReport, clean_docx_path};
use crate::rich_text_element::{
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
  let mut paragraphs = Vec::new();
  let mut runs_imported = 0usize;
  let mut unknown_paragraph_styles = Vec::new();
  let mut unknown_run_styles = Vec::new();

  for paragraph in docx.paragraphs() {
    let style_id = paragraph.style_id();
    let style = recognize_paragraph_style(style_id);
    if style == ParagraphStyle::Normal
      && let Some(style_id) = style_id
      && !matches!(style_id, "Normal")
    {
      push_unique(&mut unknown_paragraph_styles, style_id);
    }

    let mut runs = Vec::new();
    for run in paragraph.runs() {
      let text = run.text();
      if text.is_empty() {
        continue;
      }
      let run_style_id = run.style_id();
      if let Some(style_id) = run_style_id
        && recognize_run_semantic(style_id).is_none()
      {
        push_unique(&mut unknown_run_styles, style_id);
      }

      let effective = docx.resolve_run_properties(style_id, run_style_id);
      let styles = RunStyles {
        semantic: recognize_run_semantic(run_style_id.unwrap_or_default()).unwrap_or_default(),
        direct_underline: underline_is_on(&effective.underline),
        strikethrough: effective.strike == Some(true) || effective.dstrike == Some(true),
        highlight: if effective.highlight.is_some() || effective.shading.is_some() {
          Some(HighlightStyle::Spoken)
        } else {
          None
        },
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

fn recognize_paragraph_style(style_id: Option<&str>) -> ParagraphStyle {
  match style_id.unwrap_or("Normal") {
    "Heading1" => ParagraphStyle::Pocket,
    "Heading2" => ParagraphStyle::Hat,
    "Heading3" => ParagraphStyle::Block,
    "Heading4" => ParagraphStyle::Tag,
    "Analytic" => ParagraphStyle::Analytic,
    "Undertag" => ParagraphStyle::Undertag,
    _ => ParagraphStyle::Normal,
  }
}

fn recognize_run_semantic(style_id: &str) -> Option<RunSemanticStyle> {
  match style_id {
    "Style13ptBold" => Some(RunSemanticStyle::Cite),
    "Emphasis" => Some(RunSemanticStyle::Emphasis),
    "StyleUnderline" => Some(RunSemanticStyle::Underline),
    _ => None,
  }
}

fn underline_is_on<T: std::fmt::Debug>(underline: &Option<T>) -> bool {
  underline
    .as_ref()
    .is_some_and(|value| format!("{value:?}") != "None")
}

fn push_unique(values: &mut Vec<String>, value: &str) {
  if !values.iter().any(|existing| existing == value) {
    values.push(value.to_string());
  }
}

fn rdocx_error(error: rdocx::Error) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error)
}
