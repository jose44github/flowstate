use gpui::{
  AnyElement, Context, Entity, Hsla, IntoElement, Keystroke, ParentElement as _, Render,
  Styled as _, Window, div, prelude::*, px, relative,
};
use gpui_component::button::{
  Button, ButtonGroup, ButtonVariants as _, Toggle, ToggleVariants as _,
};
use gpui_component::kbd::Kbd;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, StyledExt as _};
use serde::{Deserialize, Serialize};

use crate::commands::{CommandId, default_keys_for};
use crate::rich_text_element::{
  ArmedInlineTool, DocumentTheme, HighlightStyle, ParagraphStyle, RichTextEditor, RichTextEditorStyleState,
  RunSemanticStyle, SelectionState,
};
use crate::ribbon::style_catalog::{
  HIGHLIGHT_STYLE_SPECS, PARAGRAPH_STYLE_SPECS, SEMANTIC_STYLE_SPECS,
};

/// User-selectable ribbon renderer.
///
/// This enum is intentionally serializable so a future settings panel can save
/// `editor.ribbon_mode` without touching the render implementations.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RibbonMode {
  Legacy,
  #[default]
  Modern,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RibbonDensity {
  Full,
  Compact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutVisibility {
  Always,
  HideInCompact,
  HoverOnly,
  Hidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModernRibbonOptions {
  pub density: RibbonDensity,
  pub shortcut_visibility: ShortcutVisibility,
}

impl Default for ModernRibbonOptions {
  fn default() -> Self {
    Self {
      density: RibbonDensity::Full,
      shortcut_visibility: ShortcutVisibility::Always,
    }
  }
}

/// Switching layer for the editor styles ribbon.
///
/// `LegacyStylesRibbon` and `ModernStylesRibbon` stay separate so the old
/// ribbon can be restored by changing only this mode value.
pub struct EditorRibbon {
  editor: Entity<RichTextEditor>,
  mode: RibbonMode,
  modern_options: ModernRibbonOptions,
}

/// Compatibility name for code that wants to talk in settings terms.
pub type StylesRibbon = EditorRibbon;

impl EditorRibbon {
  pub fn new(editor: Entity<RichTextEditor>) -> Self {
    Self::new_with_mode(editor, RibbonMode::default())
  }

  pub fn new_with_mode(editor: Entity<RichTextEditor>, mode: RibbonMode) -> Self {
    Self {
      editor,
      mode,
      modern_options: ModernRibbonOptions::default(),
    }
  }

  pub fn mode(&self) -> RibbonMode {
    self.mode
  }

  /// Future settings panels can call this after updating
  /// `settings.editor.ribbon_mode`.
  pub fn set_mode(&mut self, mode: RibbonMode, cx: &mut Context<Self>) {
    if self.mode != mode {
      self.mode = mode;
      cx.notify();
    }
  }

  pub fn set_modern_options(
    &mut self,
    modern_options: ModernRibbonOptions,
    cx: &mut Context<Self>,
  ) {
    if self.modern_options != modern_options {
      self.modern_options = modern_options;
      cx.notify();
    }
  }

  fn paragraph_selected(state: &RichTextEditorStyleState, style: ParagraphStyle) -> bool {
    matches!(state.paragraph_style, SelectionState::Uniform(current) if current == style)
  }

  fn semantic_selected(
    state: &RichTextEditorStyleState,
    armed_tool: Option<ArmedInlineTool>,
    style: RunSemanticStyle,
  ) -> bool {
    matches!(armed_tool, Some(ArmedInlineTool::Semantic(current)) if current == style)
      || matches!(state.semantic, SelectionState::Uniform(current) if current == style)
  }

  fn underline_selected(
    state: &RichTextEditorStyleState,
    armed_tool: Option<ArmedInlineTool>,
  ) -> bool {
    matches!(armed_tool, Some(ArmedInlineTool::Underline))
      || matches!(state.underline, SelectionState::Uniform(true))
  }

  fn highlight_selected(
    state: &RichTextEditorStyleState,
    armed_tool: Option<ArmedInlineTool>,
    style: HighlightStyle,
  ) -> bool {
    matches!(armed_tool, Some(ArmedInlineTool::Highlight(current)) if current == style)
      || matches!(state.highlight, SelectionState::Uniform(Some(current)) if current == style)
  }
}

impl Render for EditorRibbon {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let (style_state, armed_tool, document_theme) = {
      let editor = self.editor.read(cx);
      (
        editor.style_state(),
        editor.armed_inline_tool(),
        editor.document_theme(),
      )
    };

    match self.mode {
      RibbonMode::Legacy => {
        LegacyStylesRibbon::render(self.editor.clone(), &style_state, armed_tool, cx)
      }
      RibbonMode::Modern => ModernStylesRibbon::render(
        self.editor.clone(),
        &style_state,
        armed_tool,
        &document_theme,
        self.modern_options,
        cx,
      ),
    }
  }
}

pub struct LegacyStylesRibbon;

impl LegacyStylesRibbon {
  fn render(
    editor: Entity<RichTextEditor>,
    style_state: &RichTextEditorStyleState,
    armed_tool: Option<ArmedInlineTool>,
    cx: &mut Context<EditorRibbon>,
  ) -> AnyElement {
    div()
      .w_full()
      .flex()
      .flex_row()
      .items_center()
      .gap_3()
      .px_3()
      .py_2()
      .border_b_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().secondary)
      .child(legacy_ribbon_group(
        "Styles",
        ButtonGroup::new("paragraph-styles")
          .compact()
          .outline()
          .children(PARAGRAPH_STYLE_SPECS.iter().map(|spec| {
            let editor = editor.clone();
            let style = spec.style;
            Button::new(("paragraph-style", style as u64))
              .label(spec.label)
              .selected(EditorRibbon::paragraph_selected(style_state, style))
              .tooltip(spec.label)
              .on_click(move |_, _, cx| {
                editor.update(cx, |editor, cx| {
                  editor.set_paragraph_style_for_selection(style, cx);
                });
              })
          })),
        cx,
      ))
      .child(legacy_ribbon_group(
        "Inline",
        div()
          .flex()
          .flex_row()
          .items_center()
          .gap_1()
          .children(SEMANTIC_STYLE_SPECS.iter().map(|spec| {
            let editor = editor.clone();
            let style = spec.style;
            Toggle::new(("semantic-style", style as u64))
              .label(spec.label)
              .small()
              .outline()
              .checked(EditorRibbon::semantic_selected(style_state, armed_tool, style))
              .on_click(move |_, _, cx| {
                editor.update(cx, |editor, cx| {
                  editor.toggle_inline_tool(ArmedInlineTool::Semantic(style), cx);
                });
              })
          }))
          .child({
            let editor = editor.clone();
            Toggle::new("underline-style")
              .label("Underline")
              .small()
              .outline()
              .checked(EditorRibbon::underline_selected(style_state, armed_tool))
              .on_click(move |_, _, cx| {
                editor.update(cx, |editor, cx| {
                  editor.toggle_inline_tool(ArmedInlineTool::Underline, cx);
                });
              })
          }),
        cx,
      ))
      .child(legacy_ribbon_group(
        "Highlight",
        div()
          .flex()
          .flex_row()
          .items_center()
          .gap_1()
          .children(HIGHLIGHT_STYLE_SPECS.iter().map(|spec| {
            let editor = editor.clone();
            let highlight = spec.style;
            Toggle::new(("highlight-style", highlight as u64))
              .label(spec.label)
              .small()
              .outline()
              .checked(EditorRibbon::highlight_selected(style_state, armed_tool, highlight))
              .on_click(move |_, _, cx| {
                editor.update(cx, |editor, cx| {
                  editor.toggle_inline_tool(ArmedInlineTool::Highlight(highlight), cx);
                });
              })
          }))
          .child({
            let editor = editor.clone();
            Button::new("clear-highlight")
              .label("Clear")
              .small()
              .ghost()
              .on_click(move |_, _, cx| {
                editor.update(cx, |editor, cx| {
                  editor.clear_armed_inline_tool(cx);
                  editor.set_highlight_for_selection(None, cx);
                });
              })
          }),
        cx,
      ))
      .child(legacy_ribbon_group(
        "Reset",
        {
          let editor = editor.clone();
          Button::new("clear-formatting")
            .label("Clear Formatting")
            .small()
            .ghost()
            .on_click(move |_, _, cx| {
              editor.update(cx, |editor, cx| {
                editor.clear_formatting(cx);
              });
            })
        },
        cx,
      ))
      .into_any_element()
  }
}

fn legacy_ribbon_group(
  label: &'static str,
  controls: impl IntoElement,
  cx: &mut Context<EditorRibbon>,
) -> impl IntoElement {
  div()
    .h_full()
    .flex()
    .flex_col()
    .gap_1()
    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
    .child(controls)
}

pub struct ModernStylesRibbon;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RibbonAccent {
  Blue,
  Purple,
  Green,
  Yellow,
  Gray,
  Color(Hsla),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowBehavior {
  KeepVisible,
  MoveToOverflow,
  HideInCompact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RibbonCommandId {
  Paragraph(ParagraphStyle),
  Semantic(RunSemanticStyle),
  Underline,
  Highlight(HighlightStyle),
  ClearHighlight,
  ClearFormatting,
}

#[derive(Clone, Debug)]
pub struct RibbonCommand {
  pub id: RibbonCommandId,
  pub label: &'static str,
  pub group_id: &'static str,
  pub shortcut: Option<String>,
  pub command_id: Option<CommandId>,
  pub priority: u8,
  pub accent: Option<RibbonAccent>,
  pub selected: bool,
  pub disabled: bool,
  pub overflow_behavior: OverflowBehavior,
}

#[derive(Clone, Debug)]
pub struct RibbonCommandGroup {
  pub id: &'static str,
  pub label: &'static str,
  pub commands: Vec<RibbonCommand>,
}

impl ModernStylesRibbon {
  fn render(
    editor: Entity<RichTextEditor>,
    style_state: &RichTextEditorStyleState,
    armed_tool: Option<ArmedInlineTool>,
    document_theme: &DocumentTheme,
    options: ModernRibbonOptions,
    cx: &mut Context<EditorRibbon>,
  ) -> AnyElement {
    let groups = modern_command_groups(style_state, armed_tool, document_theme);
    let overflow_commands = groups
      .iter()
      .flat_map(|group| group.commands.iter())
      .filter(|command| command.overflow_behavior == OverflowBehavior::HideInCompact)
      .cloned()
      .collect::<Vec<_>>();

    div()
      .w_full()
      .min_h(px(104.0))
      .px_2()
      .pt_0()
      .pb_2()
      .child(
        div()
          .w_full()
          .min_w_0()
          .flex()
          .flex_row()
          .flex_nowrap()
          .items_start()
          .gap_1()
          .bg(cx.theme().background)
          .px_2()
          .py_1p5()
          .child(
            div()
              .flex()
              .flex_initial()
              .flex_row()
              .flex_nowrap()
              .items_start()
              .gap_2()
              .min_w_0()
              .children(groups.iter().enumerate().map(|(index, group)| {
                modern_group(index > 0, group, editor.clone(), options, cx)
              })),
          )
          .child(more_menu(editor, overflow_commands, cx)),
      )
      .into_any_element()
  }
}

fn modern_group(
  has_divider: bool,
  group: &RibbonCommandGroup,
  editor: Entity<RichTextEditor>,
  options: ModernRibbonOptions,
  cx: &mut Context<EditorRibbon>,
) -> AnyElement {
  div()
    .flex()
    .flex_col()
    .flex_shrink()
    .min_w(group_min_width(group.id))
    .min_h(section_divider_height())
    .gap_0p5()
    .when(has_divider, |this| {
      this.pl_2().border_l_1().border_color(cx.theme().border.opacity(0.72))
    })
    .child(
      div()
        .text_xs()
        .font_medium()
        .text_color(cx.theme().muted_foreground)
        .child(group.label),
    )
    .child(
      div()
        .id(group.id)
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_1()
        .min_w_0()
        .children(group.commands.iter().map(|command| {
          modern_command_chip(command, editor.clone(), options, cx)
        })),
    )
    .into_any_element()
}

fn group_min_width(group_id: &str) -> gpui::Pixels {
  match group_id {
    "reset" => px(178.0),
    _ => px(112.0),
  }
}

fn section_divider_height() -> gpui::Pixels {
  px(88.0)
}

fn modern_command_chip(
  command: &RibbonCommand,
  editor: Entity<RichTextEditor>,
  options: ModernRibbonOptions,
  cx: &mut Context<EditorRibbon>,
) -> AnyElement {
  let command_id = command.id;
  let tooltip = command_tooltip(command);
  let shortcut = command.shortcut.clone();

  Button::new(("modern-ribbon-command", ribbon_command_key(command_id)))
    .small()
    .compact()
    .outline()
    .rounded(px(6.0))
    .selected(command.selected)
    .disabled(command.disabled)
    .tooltip(tooltip)
    .when(command.selected, |this| {
      this
        .border_color(cx.theme().blue)
        .bg(cx.theme().blue_light.opacity(0.22))
        .text_color(cx.theme().foreground)
    })
    .when_some(command.accent, |this, accent| {
      this.child(accent_dot(accent_color(accent, cx)))
    })
    .child(
      div()
        .flex_none()
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .whitespace_nowrap()
        .child(command.label),
    )
    .when(show_shortcut(options), |this| {
      this.when_some(shortcut, |this, shortcut| this.child(keycap(shortcut, cx)))
    })
    .on_click(move |_, _, cx| {
      editor.update(cx, |editor, cx| {
        perform_ribbon_command(editor, command_id, cx);
      });
    })
    .into_any_element()
}

fn more_menu(
  editor: Entity<RichTextEditor>,
  overflow_commands: Vec<RibbonCommand>,
  cx: &mut Context<EditorRibbon>,
) -> AnyElement {
  div()
    .h_full()
    .min_h(section_divider_height())
    .border_l_1()
    .border_color(cx.theme().border.opacity(0.72))
    .pl_2()
    .flex()
    .items_center()
    .child(
      Button::new("modern-ribbon-more")
        .label("More")
        .small()
        .ghost()
        .dropdown_caret(true)
        .tooltip("More style commands")
        .dropdown_menu(move |menu, _, _| {
          if overflow_commands.is_empty() {
            return menu.item(PopupMenuItem::label("No overflow commands"));
          }

          overflow_commands.iter().fold(menu, |menu, command| {
            let command_id = command.id;
            let editor = editor.clone();
            let label = match &command.shortcut {
              Some(shortcut) => format!("{}    {}", command.label, shortcut),
              None => command.label.to_string(),
            };
            menu.item(
              PopupMenuItem::new(label)
                .checked(command.selected)
                .disabled(command.disabled)
                .on_click(move |_, _, cx| {
                  editor.update(cx, |editor, cx| {
                    perform_ribbon_command(editor, command_id, cx);
                  });
                }),
            )
          })
        }),
    )
    .into_any_element()
}

fn modern_command_groups(
  state: &RichTextEditorStyleState,
  armed_tool: Option<ArmedInlineTool>,
  document_theme: &DocumentTheme,
) -> Vec<RibbonCommandGroup> {
  vec![
    RibbonCommandGroup {
      id: "styles",
      label: "Styles",
      commands: PARAGRAPH_STYLE_SPECS
        .iter()
        .map(|spec| {
          let command_id = paragraph_command_id(spec.style);
          RibbonCommand {
            id: RibbonCommandId::Paragraph(spec.style),
            label: spec.label,
            group_id: "styles",
            shortcut: command_id.and_then(shortcut_for),
            command_id,
            priority: paragraph_priority(spec.style),
            accent: None,
            selected: EditorRibbon::paragraph_selected(state, spec.style),
            disabled: false,
            overflow_behavior: paragraph_overflow_behavior(spec.style),
          }
        })
        .collect(),
    },
    RibbonCommandGroup {
      id: "inline",
      label: "Inline",
      commands: inline_commands(state, armed_tool),
    },
    RibbonCommandGroup {
      id: "highlight",
      label: "Highlight",
      commands: highlight_commands(state, armed_tool, document_theme),
    },
    RibbonCommandGroup {
      id: "reset",
      label: "Reset",
      commands: vec![RibbonCommand {
        id: RibbonCommandId::ClearFormatting,
        label: "Clear Formatting",
        group_id: "reset",
        shortcut: shortcut_for(CommandId::ClearFormatting),
        command_id: Some(CommandId::ClearFormatting),
        priority: 90,
        accent: None,
        selected: false,
        disabled: false,
        overflow_behavior: OverflowBehavior::KeepVisible,
      }],
    },
  ]
}

fn inline_commands(
  state: &RichTextEditorStyleState,
  armed_tool: Option<ArmedInlineTool>,
) -> Vec<RibbonCommand> {
  let mut commands = SEMANTIC_STYLE_SPECS
    .iter()
    .map(|spec| {
      let command_id = semantic_command_id(spec.style);
      RibbonCommand {
        id: RibbonCommandId::Semantic(spec.style),
        label: spec.label,
        group_id: "inline",
        shortcut: command_id.and_then(shortcut_for),
        command_id,
        priority: semantic_priority(spec.style),
        accent: None,
        selected: EditorRibbon::semantic_selected(state, armed_tool, spec.style),
        disabled: false,
        overflow_behavior: semantic_overflow_behavior(spec.style),
      }
    })
    .collect::<Vec<_>>();

  commands.push(RibbonCommand {
    id: RibbonCommandId::Underline,
    label: "Underline",
    group_id: "inline",
    shortcut: shortcut_for(CommandId::ToggleUnderline),
    command_id: Some(CommandId::ToggleUnderline),
    priority: 82,
    accent: None,
    selected: EditorRibbon::underline_selected(state, armed_tool),
    disabled: false,
    overflow_behavior: OverflowBehavior::KeepVisible,
  });

  commands
}

fn highlight_commands(
  state: &RichTextEditorStyleState,
  armed_tool: Option<ArmedInlineTool>,
  document_theme: &DocumentTheme,
) -> Vec<RibbonCommand> {
  let mut commands = HIGHLIGHT_STYLE_SPECS
    .iter()
    .map(|spec| {
      let command_id = highlight_command_id(spec.style);
      RibbonCommand {
        id: RibbonCommandId::Highlight(spec.style),
        label: spec.label,
        group_id: "highlight",
        shortcut: command_id.and_then(shortcut_for),
        command_id,
        priority: highlight_priority(spec.style),
        accent: Some(RibbonAccent::Color(highlight_color(spec.style, document_theme))),
        selected: EditorRibbon::highlight_selected(state, armed_tool, spec.style),
        disabled: false,
        overflow_behavior: highlight_overflow_behavior(spec.style),
      }
    })
    .collect::<Vec<_>>();

  commands.push(RibbonCommand {
    id: RibbonCommandId::ClearHighlight,
    label: "Clear",
    group_id: "highlight",
    shortcut: shortcut_for(CommandId::ClearHighlight),
    command_id: Some(CommandId::ClearHighlight),
    priority: 74,
    accent: Some(RibbonAccent::Gray),
    selected: matches!(state.highlight, SelectionState::Uniform(None)),
    disabled: false,
    overflow_behavior: OverflowBehavior::KeepVisible,
  });

  commands
}

fn perform_ribbon_command(
  editor: &mut RichTextEditor,
  command_id: RibbonCommandId,
  cx: &mut Context<RichTextEditor>,
) {
  match command_id {
    RibbonCommandId::Paragraph(style) => {
      editor.set_paragraph_style_for_selection(style, cx);
    }
    RibbonCommandId::Semantic(style) => {
      editor.toggle_inline_tool(ArmedInlineTool::Semantic(style), cx);
    }
    RibbonCommandId::Underline => {
      editor.toggle_inline_tool(ArmedInlineTool::Underline, cx);
    }
    RibbonCommandId::Highlight(style) => {
      editor.toggle_inline_tool(ArmedInlineTool::Highlight(style), cx);
    }
    RibbonCommandId::ClearHighlight => {
      editor.clear_armed_inline_tool(cx);
      editor.set_highlight_for_selection(None, cx);
    }
    RibbonCommandId::ClearFormatting => {
      editor.clear_formatting(cx);
    }
  }
}

fn paragraph_command_id(style: ParagraphStyle) -> Option<CommandId> {
  match style {
    ParagraphStyle::Normal => None,
    ParagraphStyle::Pocket => Some(CommandId::SetParagraphPocket),
    ParagraphStyle::Hat => Some(CommandId::SetParagraphHat),
    ParagraphStyle::Block => Some(CommandId::SetParagraphBlock),
    ParagraphStyle::Tag => Some(CommandId::SetParagraphTag),
    ParagraphStyle::Analytic => Some(CommandId::SetParagraphAnalytic),
    ParagraphStyle::Undertag => None,
  }
}

fn semantic_command_id(style: RunSemanticStyle) -> Option<CommandId> {
  match style {
    RunSemanticStyle::Cite => Some(CommandId::ToggleCite),
    RunSemanticStyle::Emphasis => Some(CommandId::ToggleEmphasis),
    RunSemanticStyle::Condensed => None,
    RunSemanticStyle::Ultracondensed => None,
    RunSemanticStyle::Underline => Some(CommandId::ToggleUnderline),
    RunSemanticStyle::Plain => Some(CommandId::ClearFormatting),
  }
}

fn highlight_command_id(style: HighlightStyle) -> Option<CommandId> {
  match style {
    HighlightStyle::Spoken => Some(CommandId::SetHighlightSpoken),
    HighlightStyle::Insert => None,
    HighlightStyle::Alternative => None,
  }
}

fn paragraph_priority(style: ParagraphStyle) -> u8 {
  match style {
    ParagraphStyle::Normal => 100,
    ParagraphStyle::Pocket => 96,
    ParagraphStyle::Hat => 94,
    ParagraphStyle::Block => 92,
    ParagraphStyle::Tag => 78,
    ParagraphStyle::Analytic => 76,
    ParagraphStyle::Undertag => 72,
  }
}

fn semantic_priority(style: RunSemanticStyle) -> u8 {
  match style {
    RunSemanticStyle::Cite => 92,
    RunSemanticStyle::Emphasis => 90,
    RunSemanticStyle::Condensed => 76,
    RunSemanticStyle::Ultracondensed => 70,
    RunSemanticStyle::Underline => 82,
    RunSemanticStyle::Plain => 0,
  }
}

fn highlight_priority(style: HighlightStyle) -> u8 {
  match style {
    HighlightStyle::Spoken => 88,
    HighlightStyle::Insert => 84,
    HighlightStyle::Alternative => 72,
  }
}

fn paragraph_overflow_behavior(style: ParagraphStyle) -> OverflowBehavior {
  match style {
    ParagraphStyle::Normal
    | ParagraphStyle::Pocket
    | ParagraphStyle::Hat
    | ParagraphStyle::Block => OverflowBehavior::KeepVisible,
    ParagraphStyle::Tag | ParagraphStyle::Analytic | ParagraphStyle::Undertag => {
      OverflowBehavior::MoveToOverflow
    }
  }
}

fn semantic_overflow_behavior(style: RunSemanticStyle) -> OverflowBehavior {
  match style {
    RunSemanticStyle::Cite | RunSemanticStyle::Emphasis | RunSemanticStyle::Underline => {
      OverflowBehavior::KeepVisible
    }
    RunSemanticStyle::Condensed | RunSemanticStyle::Ultracondensed => {
      OverflowBehavior::MoveToOverflow
    }
    RunSemanticStyle::Plain => OverflowBehavior::HideInCompact,
  }
}

fn highlight_overflow_behavior(style: HighlightStyle) -> OverflowBehavior {
  match style {
    HighlightStyle::Spoken | HighlightStyle::Insert => OverflowBehavior::KeepVisible,
    HighlightStyle::Alternative => OverflowBehavior::MoveToOverflow,
  }
}

fn highlight_color(style: HighlightStyle, theme: &DocumentTheme) -> Hsla {
  match style {
    HighlightStyle::Spoken => theme.highlight_spoken,
    HighlightStyle::Insert => theme.highlight_insert,
    HighlightStyle::Alternative => theme.highlight_alternative,
  }
}

fn shortcut_for(command_id: CommandId) -> Option<String> {
  default_keys_for(command_id).first().map(|key| {
    Keystroke::parse(key)
      .map(|stroke| Kbd::format(&stroke))
      .unwrap_or_else(|_| (*key).to_string())
  })
}

fn show_shortcut(options: ModernRibbonOptions) -> bool {
  match options.shortcut_visibility {
    ShortcutVisibility::Always => true,
    ShortcutVisibility::HideInCompact => options.density == RibbonDensity::Full,
    ShortcutVisibility::HoverOnly | ShortcutVisibility::Hidden => false,
  }
}

fn command_tooltip(command: &RibbonCommand) -> String {
  match &command.shortcut {
    Some(shortcut) => format!("{} ({})", command.label, shortcut),
    None => command.label.to_string(),
  }
}

fn keycap(shortcut: String, cx: &mut Context<EditorRibbon>) -> AnyElement {
  div()
    .flex_none()
    .whitespace_nowrap()
    .rounded(px(4.0))
    .border_1()
    .border_color(cx.theme().border)
    .bg(cx.theme().muted.opacity(0.68))
    .px_1()
    .py_0p5()
    .text_size(px(10.0))
    .line_height(relative(1.0))
    .text_color(cx.theme().muted_foreground)
    .child(shortcut)
    .into_any_element()
}

fn accent_dot(color: Hsla) -> AnyElement {
  div()
    .flex_none()
    .size(px(6.0))
    .rounded(px(3.0))
    .bg(color)
    .into_any_element()
}

fn accent_color(accent: RibbonAccent, cx: &mut Context<EditorRibbon>) -> Hsla {
  match accent {
    RibbonAccent::Blue => cx.theme().blue,
    RibbonAccent::Purple => cx.theme().magenta,
    RibbonAccent::Green => cx.theme().green,
    RibbonAccent::Yellow => cx.theme().yellow,
    RibbonAccent::Gray => cx.theme().muted_foreground,
    RibbonAccent::Color(color) => color,
  }
}

fn ribbon_command_key(command_id: RibbonCommandId) -> u64 {
  match command_id {
    RibbonCommandId::Paragraph(style) => 1_000 + style as u64,
    RibbonCommandId::Semantic(style) => 2_000 + style as u64,
    RibbonCommandId::Underline => 3_000,
    RibbonCommandId::Highlight(style) => 4_000 + style as u64,
    RibbonCommandId::ClearHighlight => 5_000,
    RibbonCommandId::ClearFormatting => 5_001,
  }
}
