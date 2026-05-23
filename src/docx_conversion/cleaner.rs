use std::{
  fs::File,
  io::{Cursor, Read, Write},
  path::Path,
};

use regex::{Captures, Regex};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const WORD_DOCUMENT: &str = "word/document.xml";
const WORD_STYLES: &str = "word/styles.xml";

pub const CLEANING_RULES: &[CleanAction] = &[
  CleanAction::NormalizeKnownStyle {
    style_id: "Normal",
    readable_name: "Normal",
    alias: None,
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading1",
    readable_name: "Heading 1",
    alias: Some("Pocket"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading2",
    readable_name: "Heading 2",
    alias: Some("Hat"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading3",
    readable_name: "Heading 3",
    alias: Some("Block"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading4",
    readable_name: "Heading 4",
    alias: Some("Tag"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Emphasis",
    readable_name: "Heading 1 Char",
    alias: Some("Pocket Char"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading2Char",
    readable_name: "Heading 2 Char",
    alias: Some("Hat Char"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading3Char",
    readable_name: "Heading 3 Char",
    alias: Some("Block Char"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Heading4Char",
    readable_name: "Heading 4 Char",
    alias: Some("Tag Char"),
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Style13ptBold",
    readable_name: "Style 13 pt Bold",
    alias: None,
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "StyleUnderline",
    readable_name: "Style Underline",
    alias: None,
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Analytic",
    readable_name: "Analytic",
    alias: None,
  },
  CleanAction::NormalizeKnownStyle {
    style_id: "Undertag",
    readable_name: "Undertag",
    alias: None,
  },
  CleanAction::RemoveUnknownParagraphOrCharacterStyle,
  CleanAction::FlattenHyperlinks,
  CleanAction::DetectHeadingByOutlineAndRunProperties,
  CleanAction::MapBorderedRunToEmphasis,
  CleanAction::MapCitationCandidateToStyle13ptBold,
  CleanAction::MapUnderlineOrHighlightToStyleUnderline,
  CleanAction::ClearConvertedDirectRunFormatting,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanAction {
  NormalizeKnownStyle {
    style_id: &'static str,
    readable_name: &'static str,
    alias: Option<&'static str>,
  },
  RemoveUnknownParagraphOrCharacterStyle,
  FlattenHyperlinks,
  DetectHeadingByOutlineAndRunProperties,
  MapBorderedRunToEmphasis,
  MapCitationCandidateToStyle13ptBold,
  MapUnderlineOrHighlightToStyleUnderline,
  ClearConvertedDirectRunFormatting,
}

#[derive(Clone, Debug)]
pub struct CleanedDocx {
  pub bytes: Vec<u8>,
  pub report: DocxCleanReport,
}

#[derive(Clone, Debug)]
pub struct DocxCleanReport {
  pub stats: DocxCleanStats,
  pub actions: &'static [CleanAction],
}

#[derive(Default, Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocxCleanStats {
  pub styles_normalized: usize,
  pub styles_removed: usize,
  pub paragraphs_restyled: usize,
  pub runs_restyled: usize,
  pub hyperlinks_flattened: usize,
}

pub fn clean_docx_path(path: impl AsRef<Path>) -> std::io::Result<CleanedDocx> {
  let file = File::open(path)?;
  clean_docx_reader(file)
}

pub fn clean_docx_bytes(bytes: &[u8]) -> std::io::Result<CleanedDocx> {
  clean_docx_reader(Cursor::new(bytes))
}

fn clean_docx_reader(reader: impl Read + std::io::Seek) -> std::io::Result<CleanedDocx> {
  let mut package = read_package(reader)?;
  let mut stats = DocxCleanStats::default();

  if let Some(styles) = package.get_mut(WORD_STYLES) {
    let (updated, style_stats) = clean_styles_xml(styles);
    *styles = updated;
    stats.styles_normalized += style_stats.styles_normalized;
    stats.styles_removed += style_stats.styles_removed;
  }

  if let Some(document) = package.get_mut(WORD_DOCUMENT) {
    let (updated, document_stats) = clean_document_xml(document);
    *document = updated;
    stats.paragraphs_restyled += document_stats.paragraphs_restyled;
    stats.runs_restyled += document_stats.runs_restyled;
    stats.hyperlinks_flattened += document_stats.hyperlinks_flattened;
  } else {
    return Err(io_invalid("DOCX package is missing word/document.xml"));
  }

  Ok(CleanedDocx {
    bytes: write_package(&package)?,
    report: DocxCleanReport {
      stats,
      actions: CLEANING_RULES,
    },
  })
}

fn read_package(reader: impl Read + std::io::Seek) -> std::io::Result<Vec<(String, Vec<u8>)>> {
  let mut zip = ZipArchive::new(reader)?;
  let mut parts = Vec::with_capacity(zip.len());
  for i in 0..zip.len() {
    let mut part = zip.by_index(i)?;
    let mut bytes = Vec::new();
    part.read_to_end(&mut bytes)?;
    parts.push((part.name().replace('\\', "/"), bytes));
  }
  Ok(parts)
}

fn write_package(parts: &[(String, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
  let cursor = Cursor::new(Vec::new());
  let mut writer = ZipWriter::new(cursor);
  let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

  for (name, bytes) in parts {
    if name.ends_with('/') {
      writer.add_directory(name, options)?;
    } else {
      writer.start_file(name, options)?;
      writer.write_all(bytes)?;
    }
  }

  Ok(writer.finish()?.into_inner())
}

fn clean_styles_xml(bytes: &[u8]) -> (Vec<u8>, DocxCleanStats) {
  let Ok(xml) = std::str::from_utf8(bytes) else {
    return (bytes.to_vec(), DocxCleanStats::default());
  };

  let style_re = Regex::new(r#"(?s)<w:style\b[^>]*?</w:style>"#).unwrap();
  let id_re = Regex::new(r#"w:styleId="([^"]+)""#).unwrap();
  let type_re = Regex::new(r#"w:type="([^"]+)""#).unwrap();
  let mut stats = DocxCleanStats::default();

  let updated = style_re.replace_all(xml, |caps: &Captures<'_>| {
    let style_xml = caps.get(0).unwrap().as_str();
    let Some(style_id) = id_re
      .captures(style_xml)
      .and_then(|caps| caps.get(1))
      .map(|m| m.as_str())
    else {
      return style_xml.to_string();
    };

    if let Some(CleanAction::NormalizeKnownStyle { readable_name, alias, .. }) = known_style(style_id) {
      stats.styles_normalized += 1;
      normalize_style(style_xml, readable_name, alias)
    } else if matches!(
      type_re
        .captures(style_xml)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str()),
      Some("paragraph" | "character")
    ) {
      stats.styles_removed += 1;
      String::new()
    } else {
      style_xml.to_string()
    }
  });

  (updated.into_owned().into_bytes(), stats)
}

fn clean_document_xml(bytes: &[u8]) -> (Vec<u8>, DocxCleanStats) {
  let Ok(xml) = std::str::from_utf8(bytes) else {
    return (bytes.to_vec(), DocxCleanStats::default());
  };

  let hyperlink_re = Regex::new(r#"(?s)<w:hyperlink\b[^>]*>(.*?)</w:hyperlink>"#).unwrap();
  let mut stats = DocxCleanStats::default();
  let no_hyperlinks = hyperlink_re.replace_all(xml, |caps: &Captures<'_>| {
    stats.hyperlinks_flattened += 1;
    caps
      .get(1)
      .map(|m| m.as_str())
      .unwrap_or_default()
      .to_string()
  });

  let paragraph_re = Regex::new(r#"(?s)<w:p\b[^>]*>.*?</w:p>"#).unwrap();
  let mut after_heading = false;
  let mut section_has_underline = false;
  let cleaned = paragraph_re.replace_all(&no_hyperlinks, |caps: &Captures<'_>| {
    let paragraph = caps.get(0).unwrap().as_str();
    let (updated, para_stats, is_heading) = clean_paragraph_xml(paragraph, after_heading, section_has_underline);

    stats.paragraphs_restyled += para_stats.paragraphs_restyled;
    stats.runs_restyled += para_stats.runs_restyled;

    if is_heading {
      after_heading = true;
      section_has_underline = false;
    } else if paragraph_has_text(paragraph) && !paragraph_style_starts_with(paragraph, "Undertag") {
      after_heading = false;
      if paragraph_has_plain_underline(paragraph) {
        section_has_underline = true;
      }
    }

    updated
  });

  (cleaned.into_owned().into_bytes(), stats)
}

fn clean_paragraph_xml(paragraph: &str, can_process_citations: bool, section_has_underline: bool) -> (String, DocxCleanStats, bool) {
  let mut stats = DocxCleanStats::default();
  let mut updated = paragraph.to_string();
  let mut is_heading = false;

  if let Some(style_id) = detect_heading_style(paragraph) {
    updated = set_paragraph_style(&updated, style_id);
    stats.paragraphs_restyled += 1;
    is_heading = true;
  } else if paragraph_style_contains(paragraph, "Analytic") || paragraph_style_contains(paragraph, "Analytics") {
    updated = set_paragraph_style(&updated, "Analytic");
    stats.paragraphs_restyled += 1;
    is_heading = true;
  }

  if is_heading {
    return (updated, stats, true);
  }

  let run_re = Regex::new(r#"(?s)<w:r\b[^>]*>.*?</w:r>"#).unwrap();
  let all_text_runs_bold = text_runs(&updated)
    .filter(|run| !run_text(run).trim().is_empty())
    .all(run_is_bold);

  let converted = run_re.replace_all(&updated, |caps: &Captures<'_>| {
    let run = caps.get(0).unwrap().as_str();
    let mut target_style = None;

    if run_has_border(run) {
      target_style = Some("Emphasis");
    } else if can_process_citations && (run_is_bold(run) || all_text_runs_bold) {
      target_style = Some("Style13ptBold");
    } else if run_is_underlined(run) && !run_is_bold(run) {
      target_style = Some("StyleUnderline");
    } else if run_is_bold(run) && run_is_underlined(run) {
      target_style = Some(if section_has_underline { "Emphasis" } else { "StyleUnderline" });
    } else if run_is_highlighted(run) {
      target_style = Some("StyleUnderline");
    }

    if let Some(style_id) = target_style {
      stats.runs_restyled += 1;
      clear_run_formatting(&set_run_style(run, style_id))
    } else {
      clear_incidental_run_formatting(run)
    }
  });

  (converted.into_owned(), stats, false)
}

fn detect_heading_style(paragraph: &str) -> Option<&'static str> {
  let outline = Regex::new(r#"<w:outlineLvl\b[^>]*w:val="(\d+)""#)
    .unwrap()
    .captures(paragraph)
    .and_then(|caps| caps.get(1))
    .and_then(|m| m.as_str().parse::<u32>().ok());

  match outline {
    Some(0) if paragraph_has_bold_size(paragraph, "52") => Some("Heading1"),
    Some(1) if paragraph_has_bold_size(paragraph, "44") => Some("Heading2"),
    Some(2) if paragraph_has_bold_size(paragraph, "32") && paragraph_has_underline(paragraph) => Some("Heading3"),
    Some(3) if paragraph_has_bold_color(paragraph) => Some("Heading4"),
    _ => None,
  }
}

fn normalize_style(style_xml: &str, name: &str, alias: Option<&str>) -> String {
  let mut out = replace_or_insert_child(style_xml, "w:name", &format!(r#"<w:name w:val="{}"/>"#, escape_attr(name)));
  out = if let Some(alias) = alias {
    replace_or_insert_child(&out, "w:aliases", &format!(r#"<w:aliases w:val="{}"/>"#, escape_attr(alias)))
  } else {
    Regex::new(r#"(?s)<w:aliases\b[^>]*/>|<w:aliases\b[^>]*>.*?</w:aliases>"#)
      .unwrap()
      .replace_all(&out, "")
      .into_owned()
  };
  out
}

fn set_paragraph_style(paragraph: &str, style_id: &str) -> String {
  let with_ppr = ensure_child_container(paragraph, "w:p", "w:pPr");
  replace_or_insert_child(&with_ppr, "w:pStyle", &format!(r#"<w:pStyle w:val="{style_id}"/>"#))
}

fn set_run_style(run: &str, style_id: &str) -> String {
  let with_rpr = ensure_child_container(run, "w:r", "w:rPr");
  replace_or_insert_child(&with_rpr, "w:rStyle", &format!(r#"<w:rStyle w:val="{style_id}"/>"#))
}

fn clear_run_formatting(run: &str) -> String {
  let keep = Regex::new(r#"(?s)<w:rStyle\b[^>]*/>|<w:rStyle\b[^>]*>.*?</w:rStyle>"#)
    .unwrap()
    .find(run)
    .map(|m| m.as_str().to_string());
  let mut cleaned = remove_run_formatting_elements(run);
  if let Some(style) = keep {
    cleaned = set_run_style(&cleaned, style_value(&style).unwrap_or("Normal"));
  }
  cleaned
}

fn clear_incidental_run_formatting(run: &str) -> String {
  let mut cleaned = run.to_string();
  for tag in ["w:rFonts", "w:sz", "w:szCs"] {
    cleaned = remove_element(&cleaned, tag);
  }
  cleaned
}

fn remove_run_formatting_elements(run: &str) -> String {
  let mut cleaned = run.to_string();
  for tag in [
    "w:b",
    "w:bCs",
    "w:i",
    "w:iCs",
    "w:u",
    "w:strike",
    "w:dstrike",
    "w:caps",
    "w:smallCaps",
    "w:color",
    "w:highlight",
    "w:shd",
    "w:bdr",
    "w:rFonts",
    "w:sz",
    "w:szCs",
    "w:vertAlign",
    "w:position",
    "w:spacing",
    "w:w",
  ] {
    cleaned = remove_element(&cleaned, tag);
  }
  cleaned
}

fn ensure_child_container(xml: &str, parent_tag: &str, child_tag: &str) -> String {
  if xml.contains(&format!("<{child_tag}")) {
    return xml.to_string();
  }
  let open_end = Regex::new(&format!(r#"(?s)<{}\b[^>]*>"#, regex::escape(parent_tag)))
    .unwrap()
    .find(xml)
    .map(|m| m.end());
  if let Some(pos) = open_end {
    let mut out = String::with_capacity(xml.len() + child_tag.len() * 2 + 5);
    out.push_str(&xml[..pos]);
    out.push_str(&format!("<{child_tag}></{child_tag}>"));
    out.push_str(&xml[pos..]);
    out
  } else {
    xml.to_string()
  }
}

fn replace_or_insert_child(xml: &str, child_tag: &str, replacement: &str) -> String {
  let child_re = Regex::new(&format!(
    r#"(?s)<{}\b[^>]*/>|<{}\b[^>]*>.*?</{}>"#,
    regex::escape(child_tag),
    regex::escape(child_tag),
    regex::escape(child_tag)
  ))
  .unwrap();

  if child_re.is_match(xml) {
    child_re.replace(xml, replacement).into_owned()
  } else if let Some(pos) = Regex::new(r#"(?s)<w:[A-Za-z0-9]+Pr\b[^>]*>"#)
    .unwrap()
    .find(xml)
    .map(|m| m.end())
  {
    let mut out = String::with_capacity(xml.len() + replacement.len());
    out.push_str(&xml[..pos]);
    out.push_str(replacement);
    out.push_str(&xml[pos..]);
    out
  } else if let Some(pos) = xml.find('>') {
    let mut out = String::with_capacity(xml.len() + replacement.len());
    out.push_str(&xml[..=pos]);
    out.push_str(replacement);
    out.push_str(&xml[pos + 1..]);
    out
  } else {
    xml.to_string()
  }
}

fn remove_element(xml: &str, tag: &str) -> String {
  Regex::new(&format!(
    r#"(?s)<{}\b[^>]*/>|<{}\b[^>]*>.*?</{}>"#,
    regex::escape(tag),
    regex::escape(tag),
    regex::escape(tag)
  ))
  .unwrap()
  .replace_all(xml, "")
  .into_owned()
}

fn known_style(style_id: &str) -> Option<CleanAction> {
  CLEANING_RULES.iter().copied().find(|action| match action {
    CleanAction::NormalizeKnownStyle { style_id: known_id, .. } => *known_id == style_id,
    _ => false,
  })
}

fn paragraph_has_text(paragraph: &str) -> bool {
  !Regex::new(r#"(?s)<w:t\b[^>]*>(.*?)</w:t>"#)
    .unwrap()
    .captures_iter(paragraph)
    .filter_map(|caps| caps.get(1))
    .map(|m| decode_minimal_xml(m.as_str()))
    .collect::<String>()
    .trim()
    .is_empty()
}

fn paragraph_has_bold_size(paragraph: &str, half_points: &str) -> bool {
  text_runs(paragraph).any(|run| {
    run_is_bold(run)
      && Regex::new(&format!(r#"<w:sz\b[^>]*w:val="{}""#, regex::escape(half_points)))
        .unwrap()
        .is_match(run)
  })
}

fn paragraph_has_bold_color(paragraph: &str) -> bool {
  text_runs(paragraph).any(|run| run_is_bold(run) && run.contains("<w:color"))
}

fn paragraph_has_underline(paragraph: &str) -> bool {
  text_runs(paragraph).any(run_is_underlined)
}

fn paragraph_has_plain_underline(paragraph: &str) -> bool {
  text_runs(paragraph).any(|run| run_is_underlined(run) && !run_is_bold(run))
}

fn paragraph_style_contains(paragraph: &str, needle: &str) -> bool {
  paragraph
    .split("<w:pPr")
    .nth(1)
    .and_then(|rest| rest.split("</w:pPr>").next())
    .is_some_and(|ppr| ppr.contains(needle))
}

fn paragraph_style_starts_with(paragraph: &str, prefix: &str) -> bool {
  Regex::new(r#"<w:pStyle\b[^>]*w:val="([^"]+)""#)
    .unwrap()
    .captures(paragraph)
    .and_then(|caps| caps.get(1))
    .is_some_and(|m| m.as_str().starts_with(prefix))
}

fn text_runs(xml: &str) -> impl Iterator<Item = &str> {
  Regex::new(r#"(?s)<w:r\b[^>]*>.*?</w:r>"#)
    .unwrap()
    .find_iter(xml)
    .map(|m| m.as_str())
    .collect::<Vec<_>>()
    .into_iter()
}

fn run_text(run: &str) -> String {
  Regex::new(r#"(?s)<w:t\b[^>]*>(.*?)</w:t>"#)
    .unwrap()
    .captures_iter(run)
    .filter_map(|caps| caps.get(1))
    .map(|m| decode_minimal_xml(m.as_str()))
    .collect()
}

fn run_is_bold(run: &str) -> bool {
  xml_tag_is_on(run, "w:b")
}

fn run_is_underlined(run: &str) -> bool {
  xml_tag_is_on(run, "w:u")
}

fn run_is_highlighted(run: &str) -> bool {
  run.contains("<w:highlight") || run.contains("<w:shd")
}

fn run_has_border(run: &str) -> bool {
  run.contains("<w:bdr")
}

fn style_value(style_xml: &str) -> Option<&str> {
  Regex::new(r#"w:val="([^"]+)""#)
    .unwrap()
    .captures(style_xml)
    .and_then(|caps| caps.get(1))
    .map(|m| m.as_str())
}

fn xml_tag_is_on(xml: &str, tag: &str) -> bool {
  let open = format!("<{tag}");
  let mut rest = xml;
  while let Some(start) = rest.find(&open) {
    rest = &rest[start + open.len()..];
    if rest
      .as_bytes()
      .first()
      .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b':' || *byte == b'_')
    {
      continue;
    }
    let tag_body = rest.split('>').next().unwrap_or_default();
    return !tag_body.contains(r#"w:val="0""#)
      && !tag_body.contains(r#"w:val="false""#)
      && !tag_body.contains(r#"w:val="off""#)
      && !tag_body.contains(r#"w:val="none""#);
  }
  false
}

fn escape_attr(value: &str) -> String {
  value
    .replace('&', "&amp;")
    .replace('"', "&quot;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
}

fn decode_minimal_xml(value: &str) -> String {
  value
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&apos;", "'")
    .replace("&amp;", "&")
}

fn io_invalid(message: &'static str) -> std::io::Error {
  std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

trait PackageParts {
  fn get_mut(&mut self, name: &str) -> Option<&mut Vec<u8>>;
}

impl PackageParts for Vec<(String, Vec<u8>)> {
  fn get_mut(&mut self, name: &str) -> Option<&mut Vec<u8>> {
    self
      .iter_mut()
      .find(|(part_name, _)| part_name.trim_start_matches('/') == name)
      .map(|(_, bytes)| bytes)
  }
}
