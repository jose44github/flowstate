use std::{env, fs, io, path::PathBuf};

use gpui::{Hsla, px};
use gpui_component::PixelsExt;
use serde::{Deserialize, Serialize};

use crate::rich_text_element::DocumentTheme;

#[derive(Default, Deserialize, Serialize)]
pub struct AppSettings {
  pub theme_name: Option<String>,
  pub document_theme: Option<DocumentThemeSettings>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DocumentThemeSettings {
  pub default_font_family: String,
  pub default_text_color: StoredHsla,
  pub pageless_inset_x: f32,
  pub pageless_inset_top: f32,
  pub pageless_inset_bottom: f32,
  pub body_font_size: f32,
  pub cite_font_size: f32,
  pub condensed_font_size: f32,
  pub ultracondensed_font_size: f32,
  pub pocket_font_size: f32,
  pub hat_font_size: f32,
  pub block_font_size: f32,
  pub tag_font_size: f32,
  pub undertag_font_size: f32,
  pub line_spacing: f32,
  pub line_gap_fraction: f32,
  pub paragraph_after: f32,
  pub pocket_before: f32,
  pub hat_before: f32,
  pub block_before: f32,
  pub tag_before: f32,
  pub pocket_border_width: f32,
  pub pocket_border_space_x: f32,
  pub pocket_border_space_y: f32,
  pub emphasis_border_width: f32,
  pub emphasis_border_paint_width: f32,
  pub box_padding_left: f32,
  pub box_padding_right: f32,
  pub box_padding_top: f32,
  pub box_padding_bottom: f32,
  pub highlight_pad_x: f32,
  pub highlight_top_extra_fraction: f32,
  pub highlight_bottom_extra_fraction: f32,
  pub underline_fallback_top_from_baseline: f32,
  pub underline_rule_thickness: f32,
  pub snap_underline_rules_to_pixels: bool,
  pub double_underline_top_from_baseline: f32,
  pub double_underline_gap: f32,
  pub highlight_spoken: StoredHsla,
  pub highlight_insert: StoredHsla,
  pub highlight_alternative: StoredHsla,
  pub analytic_color: StoredHsla,
  pub undertag_color: StoredHsla,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct StoredHsla {
  h: f32,
  s: f32,
  l: f32,
  a: f32,
}

pub fn load_app_settings() -> AppSettings {
  let Ok(text) = fs::read_to_string(settings_path()) else {
    return AppSettings::default();
  };
  serde_json::from_str(&text).unwrap_or_default()
}

pub fn load_document_theme() -> DocumentTheme {
  load_app_settings()
    .document_theme
    .map(DocumentTheme::from)
    .unwrap_or_default()
}

// Document style appearance is intentionally user-side. The DB8 file keeps
// semantic assignments only; this app setting decides how those semantics look.
pub fn save_theme_name(theme_name: &str) -> io::Result<()> {
  let mut settings = load_app_settings();
  settings.theme_name = Some(theme_name.to_string());
  save_app_settings(settings)
}

pub fn save_document_theme(theme: &DocumentTheme) -> io::Result<()> {
  let mut settings = load_app_settings();
  settings.document_theme = Some(DocumentThemeSettings::from(theme));
  save_app_settings(settings)
}

fn save_app_settings(settings: AppSettings) -> io::Result<()> {
  let path = settings_path();
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  let text = serde_json::to_string_pretty(&settings)?;
  fs::write(path, text)
}

impl From<&DocumentTheme> for DocumentThemeSettings {
  fn from(theme: &DocumentTheme) -> Self {
    Self {
      default_font_family: theme.default_font_family.to_string(),
      default_text_color: theme.default_text_color.into(),
      pageless_inset_x: theme.pageless_inset_x.as_f32(),
      pageless_inset_top: theme.pageless_inset_top.as_f32(),
      pageless_inset_bottom: theme.pageless_inset_bottom.as_f32(),
      body_font_size: theme.body_font_size.as_f32(),
      cite_font_size: theme.cite_font_size.as_f32(),
      condensed_font_size: theme.condensed_font_size.as_f32(),
      ultracondensed_font_size: theme.ultracondensed_font_size.as_f32(),
      pocket_font_size: theme.pocket_font_size.as_f32(),
      hat_font_size: theme.hat_font_size.as_f32(),
      block_font_size: theme.block_font_size.as_f32(),
      tag_font_size: theme.tag_font_size.as_f32(),
      undertag_font_size: theme.undertag_font_size.as_f32(),
      line_spacing: theme.line_spacing,
      line_gap_fraction: theme.line_gap_fraction,
      paragraph_after: theme.paragraph_after.as_f32(),
      pocket_before: theme.pocket_before.as_f32(),
      hat_before: theme.hat_before.as_f32(),
      block_before: theme.block_before.as_f32(),
      tag_before: theme.tag_before.as_f32(),
      pocket_border_width: theme.pocket_border_width.as_f32(),
      pocket_border_space_x: theme.pocket_border_space_x.as_f32(),
      pocket_border_space_y: theme.pocket_border_space_y.as_f32(),
      emphasis_border_width: theme.emphasis_border_width.as_f32(),
      emphasis_border_paint_width: theme.emphasis_border_paint_width.as_f32(),
      box_padding_left: theme.box_padding_left.as_f32(),
      box_padding_right: theme.box_padding_right.as_f32(),
      box_padding_top: theme.box_padding_top.as_f32(),
      box_padding_bottom: theme.box_padding_bottom.as_f32(),
      highlight_pad_x: theme.highlight_pad_x.as_f32(),
      highlight_top_extra_fraction: theme.highlight_top_extra_fraction,
      highlight_bottom_extra_fraction: theme.highlight_bottom_extra_fraction,
      underline_fallback_top_from_baseline: theme.underline_fallback_top_from_baseline.as_f32(),
      underline_rule_thickness: theme.underline_rule_thickness.as_f32(),
      snap_underline_rules_to_pixels: theme.snap_underline_rules_to_pixels,
      double_underline_top_from_baseline: theme.double_underline_top_from_baseline.as_f32(),
      double_underline_gap: theme.double_underline_gap.as_f32(),
      highlight_spoken: theme.highlight_spoken.into(),
      highlight_insert: theme.highlight_insert.into(),
      highlight_alternative: theme.highlight_alternative.into(),
      analytic_color: theme.analytic_color.into(),
      undertag_color: theme.undertag_color.into(),
    }
  }
}

impl From<DocumentThemeSettings> for DocumentTheme {
  fn from(settings: DocumentThemeSettings) -> Self {
    Self {
      default_font_family: settings.default_font_family.into(),
      default_text_color: settings.default_text_color.into(),
      pageless_inset_x: px(settings.pageless_inset_x),
      pageless_inset_top: px(settings.pageless_inset_top),
      pageless_inset_bottom: px(settings.pageless_inset_bottom),
      body_font_size: px(settings.body_font_size),
      cite_font_size: px(settings.cite_font_size),
      condensed_font_size: px(settings.condensed_font_size),
      ultracondensed_font_size: px(settings.ultracondensed_font_size),
      pocket_font_size: px(settings.pocket_font_size),
      hat_font_size: px(settings.hat_font_size),
      block_font_size: px(settings.block_font_size),
      tag_font_size: px(settings.tag_font_size),
      undertag_font_size: px(settings.undertag_font_size),
      line_spacing: settings.line_spacing,
      line_gap_fraction: settings.line_gap_fraction,
      paragraph_after: px(settings.paragraph_after),
      pocket_before: px(settings.pocket_before),
      hat_before: px(settings.hat_before),
      block_before: px(settings.block_before),
      tag_before: px(settings.tag_before),
      pocket_border_width: px(settings.pocket_border_width),
      pocket_border_space_x: px(settings.pocket_border_space_x),
      pocket_border_space_y: px(settings.pocket_border_space_y),
      emphasis_border_width: px(settings.emphasis_border_width),
      emphasis_border_paint_width: px(settings.emphasis_border_paint_width),
      box_padding_left: px(settings.box_padding_left),
      box_padding_right: px(settings.box_padding_right),
      box_padding_top: px(settings.box_padding_top),
      box_padding_bottom: px(settings.box_padding_bottom),
      highlight_pad_x: px(settings.highlight_pad_x),
      highlight_top_extra_fraction: settings.highlight_top_extra_fraction,
      highlight_bottom_extra_fraction: settings.highlight_bottom_extra_fraction,
      underline_fallback_top_from_baseline: px(settings.underline_fallback_top_from_baseline),
      underline_rule_thickness: px(settings.underline_rule_thickness),
      snap_underline_rules_to_pixels: settings.snap_underline_rules_to_pixels,
      double_underline_top_from_baseline: px(settings.double_underline_top_from_baseline),
      double_underline_gap: px(settings.double_underline_gap),
      highlight_spoken: settings.highlight_spoken.into(),
      highlight_insert: settings.highlight_insert.into(),
      highlight_alternative: settings.highlight_alternative.into(),
      analytic_color: settings.analytic_color.into(),
      undertag_color: settings.undertag_color.into(),
    }
  }
}

impl From<Hsla> for StoredHsla {
  fn from(color: Hsla) -> Self {
    Self {
      h: color.h,
      s: color.s,
      l: color.l,
      a: color.a,
    }
  }
}

impl From<StoredHsla> for Hsla {
  fn from(color: StoredHsla) -> Self {
    Hsla {
      h: color.h,
      s: color.s,
      l: color.l,
      a: color.a,
    }
  }
}

fn settings_path() -> PathBuf {
  if cfg!(target_os = "windows") {
    if let Some(appdata) = env::var_os("APPDATA") {
      return PathBuf::from(appdata).join("Odrenrir").join("settings.json");
    }
  }

  if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
    return PathBuf::from(config_home).join("odrenrir").join("settings.json");
  }

  if let Some(home) = env::var_os("HOME") {
    return PathBuf::from(home).join(".config").join("odrenrir").join("settings.json");
  }

  PathBuf::from("odrenrir-settings.json")
}
