use std::{fs, io::Cursor, path::Path};

use rdocx_opc::OpcPackage;

pub const CLEANING_RULES: &[CleanAction] = &[
  CleanAction::ReadWithRdocx,
  CleanAction::NormalizeUnsupportedFormattingValues,
  CleanAction::RecognizeKnownParagraphAndRunStyles,
  CleanAction::ResolveRunProperties,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanAction {
  ReadWithRdocx,
  NormalizeUnsupportedFormattingValues,
  RecognizeKnownParagraphAndRunStyles,
  ResolveRunProperties,
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
  pub underline_values_normalized: usize,
  pub highlight_values_normalized: usize,
  pub border_values_normalized: usize,
  pub justification_values_normalized: usize,
  pub tab_values_normalized: usize,
  pub section_values_normalized: usize,
  pub style_type_values_normalized: usize,
  pub styles_normalized: usize,
  pub styles_removed: usize,
  pub paragraphs_restyled: usize,
  pub runs_restyled: usize,
  pub hyperlinks_flattened: usize,
}

pub fn clean_docx_path(path: impl AsRef<Path>) -> std::io::Result<CleanedDocx> {
  clean_docx_vec(fs::read(path)?)
}

pub fn clean_docx_bytes(bytes: &[u8]) -> std::io::Result<CleanedDocx> {
  clean_docx_vec(bytes.to_vec())
}

fn clean_docx_vec(bytes: Vec<u8>) -> std::io::Result<CleanedDocx> {
  let (bytes, stats) = normalize_docx_formatting_values(bytes)?;
  Ok(CleanedDocx {
    bytes,
    report: DocxCleanReport {
      stats,
      actions: CLEANING_RULES,
    },
  })
}

fn normalize_docx_formatting_values(bytes: Vec<u8>) -> std::io::Result<(Vec<u8>, DocxCleanStats)> {
  let mut package = match OpcPackage::from_reader(Cursor::new(&bytes)) {
    Ok(package) => package,
    Err(_) => return Ok((bytes, DocxCleanStats::default())),
  };
  let mut stats = DocxCleanStats::default();

  for part in package.parts.values_mut() {
    if !part_might_contain_word_xml(part) {
      continue;
    }
    let Ok(xml) = std::str::from_utf8(part) else {
      continue;
    };
    let (normalized, part_stats) = normalize_formatting_values_in_xml(xml);
    if part_stats.has_changes() {
      *part = normalized.into_bytes();
      stats.merge(part_stats);
    }
  }

  if !stats.has_changes() {
    return Ok((bytes, stats));
  }

  let mut output = Cursor::new(Vec::new());
  package
    .write_to(&mut output)
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
  Ok((output.into_inner(), stats))
}

fn part_might_contain_word_xml(part: &[u8]) -> bool {
  part.starts_with(b"<?xml")
    || part
      .windows(3)
      .any(|window| window == b"<w:" || window == b"<u ")
}

fn normalize_formatting_values_in_xml(xml: &str) -> (String, DocxCleanStats) {
  let mut normalized = String::with_capacity(xml.len());
  let mut cursor = 0usize;
  let mut stats = DocxCleanStats::default();

  while let Some(relative_start) = xml[cursor..].find('<') {
    let tag_start = cursor + relative_start;
    let Some(relative_end) = xml[tag_start..].find('>') else {
      break;
    };
    let tag_end = tag_start + relative_end + 1;
    normalized.push_str(&xml[cursor..tag_start]);
    let tag = &xml[tag_start..tag_end];
    let (tag, tag_stats) = normalize_formatting_tag(tag);
    normalized.push_str(&tag);
    stats.merge(tag_stats);
    cursor = tag_end;
  }
  normalized.push_str(&xml[cursor..]);

  if stats.has_changes() {
    (normalized, stats)
  } else {
    (xml.to_string(), stats)
  }
}

fn normalize_formatting_tag(tag: &str) -> (String, DocxCleanStats) {
  let Some(name) = tag_local_name(tag) else {
    return (tag.to_string(), DocxCleanStats::default());
  };
  let mut tag = tag.to_string();
  let mut stats = DocxCleanStats::default();

  match name {
    "style" => normalize_attr(
      &mut tag,
      "type",
      supported_style_type,
      "paragraph",
      &mut stats.style_type_values_normalized,
    ),
    "jc" | "lvlJc" => normalize_attr(
      &mut tag,
      "val",
      supported_justification_value,
      "left",
      &mut stats.justification_values_normalized,
    ),
    "u" => normalize_attr(
      &mut tag,
      "val",
      supported_underline_value,
      "single",
      &mut stats.underline_values_normalized,
    ),
    "highlight" => normalize_attr(
      &mut tag,
      "val",
      supported_highlight_value,
      "yellow",
      &mut stats.highlight_values_normalized,
    ),
    "tab" => {
      normalize_attr(&mut tag, "val", supported_tab_alignment_value, "left", &mut stats.tab_values_normalized);
      normalize_attr(&mut tag, "leader", supported_tab_leader_value, "none", &mut stats.tab_values_normalized);
    },
    "type" => normalize_attr(
      &mut tag,
      "val",
      supported_section_type_value,
      "continuous",
      &mut stats.section_values_normalized,
    ),
    "pgSz" => normalize_attr(
      &mut tag,
      "orient",
      supported_page_orientation_value,
      "portrait",
      &mut stats.section_values_normalized,
    ),
    "top" | "left" | "bottom" | "right" | "insideH" | "insideV" | "tl2br" | "tr2bl" | "bar" => {
      normalize_attr(&mut tag, "val", supported_border_value, "single", &mut stats.border_values_normalized);
    },
    _ => {},
  }

  (tag, stats)
}

fn tag_local_name(tag: &str) -> Option<&str> {
  if tag.starts_with("</") || tag.starts_with("<?") || tag.starts_with("<!") {
    return None;
  }
  let name_end = tag
    .char_indices()
    .find_map(|(ix, ch)| (ch.is_whitespace() || ch == '/' || ch == '>').then_some(ix))
    .unwrap_or(tag.len());
  let name = tag[1..name_end].rsplit(':').next().unwrap_or("");
  (!name.is_empty()).then_some(name)
}

fn normalize_attr(tag: &mut String, attr_name: &str, supported: fn(&str) -> bool, fallback: &str, count: &mut usize) {
  let Some((value_start, value_end)) = attr_value_range(tag, attr_name) else {
    return;
  };
  let value = &tag[value_start..value_end];
  if supported(value) {
    return;
  }

  let mut normalized = String::with_capacity(tag.len() + fallback.len().saturating_sub(value.len()));
  normalized.push_str(&tag[..value_start]);
  normalized.push_str(fallback);
  normalized.push_str(&tag[value_end..]);
  *tag = normalized;
  *count += 1;
}

fn attr_value_range(tag: &str, target_attr_name: &str) -> Option<(usize, usize)> {
  let mut cursor = 0usize;
  while let Some(relative_val) = tag[cursor..].find(target_attr_name) {
    let val_start = cursor + relative_val;
    let attr_name_start = tag[..val_start]
      .char_indices()
      .rev()
      .find_map(|(ix, ch)| (ch.is_whitespace() || ch == '<').then_some(ix + ch.len_utf8()))
      .unwrap_or(0);
    let attr_name = &tag[attr_name_start..val_start + target_attr_name.len()];
    if attr_name.rsplit(':').next() != Some(target_attr_name) {
      cursor = val_start + target_attr_name.len();
      continue;
    }
    let mut ix = val_start + target_attr_name.len();
    while ix < tag.len() && tag[ix..].chars().next().is_some_and(char::is_whitespace) {
      ix += tag[ix..].chars().next().unwrap().len_utf8();
    }
    if !tag[ix..].starts_with('=') {
      cursor = ix;
      continue;
    }
    ix += 1;
    while ix < tag.len() && tag[ix..].chars().next().is_some_and(char::is_whitespace) {
      ix += tag[ix..].chars().next().unwrap().len_utf8();
    }
    let quote = tag[ix..].chars().next()?;
    if quote != '"' && quote != '\'' {
      cursor = ix;
      continue;
    }
    let value_start = ix + quote.len_utf8();
    let value_end = tag[value_start..]
      .find(quote)
      .map(|relative| value_start + relative)?;
    return Some((value_start, value_end));
  }
  None
}

impl DocxCleanStats {
  fn has_changes(self) -> bool {
    self.underline_values_normalized
      + self.highlight_values_normalized
      + self.border_values_normalized
      + self.justification_values_normalized
      + self.tab_values_normalized
      + self.section_values_normalized
      + self.style_type_values_normalized
      > 0
  }

  fn merge(&mut self, other: Self) {
    self.underline_values_normalized += other.underline_values_normalized;
    self.highlight_values_normalized += other.highlight_values_normalized;
    self.border_values_normalized += other.border_values_normalized;
    self.justification_values_normalized += other.justification_values_normalized;
    self.tab_values_normalized += other.tab_values_normalized;
    self.section_values_normalized += other.section_values_normalized;
    self.style_type_values_normalized += other.style_type_values_normalized;
  }
}

fn supported_style_type(value: &str) -> bool {
  matches!(value, "paragraph" | "character" | "table" | "numbering")
}

fn supported_justification_value(value: &str) -> bool {
  matches!(value, "start" | "left" | "end" | "right" | "center" | "both" | "justify" | "distribute")
}

fn supported_underline_value(value: &str) -> bool {
  matches!(
    value,
    "none" | "single" | "words" | "double" | "thick" | "dotted" | "dash" | "dotDash" | "dotDotDash" | "wave"
  )
}

fn supported_highlight_value(value: &str) -> bool {
  matches!(
    value,
    "black"
      | "blue"
      | "cyan"
      | "darkBlue"
      | "darkCyan"
      | "darkGray"
      | "darkGreen"
      | "darkMagenta"
      | "darkRed"
      | "darkYellow"
      | "green"
      | "lightGray"
      | "magenta"
      | "none"
      | "red"
      | "white"
      | "yellow"
  )
}

fn supported_border_value(value: &str) -> bool {
  matches!(
    value,
    "none"
      | "nil"
      | "single"
      | "thick"
      | "double"
      | "dotted"
      | "dashed"
      | "dotDash"
      | "dotDotDash"
      | "triple"
      | "thinThickSmallGap"
      | "thickThinSmallGap"
      | "thinThickMediumGap"
      | "thickThinMediumGap"
      | "thinThickLargeGap"
      | "thickThinLargeGap"
      | "wave"
      | "doubleWave"
      | "threeDEmboss"
      | "threeDEngrave"
      | "outset"
      | "inset"
  )
}

fn supported_tab_alignment_value(value: &str) -> bool {
  matches!(value, "left" | "start" | "center" | "right" | "end" | "decimal" | "bar" | "clear" | "num")
}

fn supported_tab_leader_value(value: &str) -> bool {
  matches!(value, "none" | "dot" | "hyphen" | "underscore" | "heavy" | "middleDot")
}

fn supported_section_type_value(value: &str) -> bool {
  matches!(value, "nextPage" | "continuous" | "evenPage" | "oddPage" | "nextColumn")
}

fn supported_page_orientation_value(value: &str) -> bool {
  matches!(value, "portrait" | "landscape")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unsupported_underline_values_normalize_to_single() {
    let xml = r#"<w:rPr><w:u w:val="dashHeavy"/><w:u w:val='wavyDouble'/><w:u w:val="none"/></w:rPr>"#;
    let (normalized, stats) = normalize_formatting_values_in_xml(xml);

    assert_eq!(stats.underline_values_normalized, 2);
    assert!(normalized.contains(r#"<w:u w:val="single"/>"#));
    assert!(normalized.contains(r#"<w:u w:val='single'/>"#));
    assert!(normalized.contains(r#"<w:u w:val="none"/>"#));
  }

  #[test]
  fn supported_underline_values_are_preserved() {
    let xml = r#"<w:rPr><w:u w:val="dash"/><w:u w:val="wave"/></w:rPr>"#;
    let (normalized, stats) = normalize_formatting_values_in_xml(xml);

    assert_eq!(stats.underline_values_normalized, 0);
    assert_eq!(normalized, xml);
  }

  #[test]
  fn unsupported_parser_enum_values_are_normalized() {
    let xml = r#"<w:style w:type="weird"><w:jc w:val="thaiDistribute"/><w:highlight w:val="pink"/><w:top w:val="dashSmallGap"/><w:tab w:val="list" w:leader="equals"/><w:type w:val="other"/><w:pgSz w:orient="sideways"/></w:style>"#;
    let (normalized, stats) = normalize_formatting_values_in_xml(xml);

    assert_eq!(stats.style_type_values_normalized, 1);
    assert_eq!(stats.justification_values_normalized, 1);
    assert_eq!(stats.highlight_values_normalized, 1);
    assert_eq!(stats.border_values_normalized, 1);
    assert_eq!(stats.tab_values_normalized, 2);
    assert_eq!(stats.section_values_normalized, 2);
    assert!(normalized.contains(r#"w:type="paragraph""#));
    assert!(normalized.contains(r#"<w:jc w:val="left"/>"#));
    assert!(normalized.contains(r#"<w:highlight w:val="yellow"/>"#));
    assert!(normalized.contains(r#"<w:top w:val="single"/>"#));
    assert!(normalized.contains(r#"<w:tab w:val="left" w:leader="none"/>"#));
    assert!(normalized.contains(r#"<w:type w:val="continuous"/>"#));
    assert!(normalized.contains(r#"<w:pgSz w:orient="portrait"/>"#));
  }
}
