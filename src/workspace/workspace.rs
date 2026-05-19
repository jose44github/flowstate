use std::{cell::Cell, collections::HashSet, path::PathBuf, rc::Rc};

use gpui::{
  AnyElement, App, Axis, Bounds, ClickEvent, Context, Entity, Hsla, InteractiveElement, IntoElement, MouseButton, PromptButton, PromptLevel, Render,
  ScrollHandle, SharedString, Subscription, WeakEntity, Window, WindowBounds, WindowControlArea, WindowOptions, PathPromptOptions, Pixels,
  TitlebarOptions, div, prelude::*,
  black, point, px, rgb, size, white,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::list::ListItem;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::tree::{TreeItem, TreeState, tree};
use gpui_component::{ActiveTheme as _, Disableable, IconName, PixelsExt, Root, Selectable, Sizable, Theme, ThemeRegistry, WindowExt as _, h_flex, v_flex};
use uuid::Uuid;

use crate::app_settings::{load_document_theme, save_document_theme, save_theme_name};
use crate::rich_text_element::{Document, DocumentTheme, ParagraphStyle, RichTextEditor, demo_document, load_or_create_document};
use crate::workspace::document_panel::DocumentPanel;
use crate::workspace::icons::{AppIcon, icon_button};

pub struct Workspace {
  document_panels: Vec<Entity<DocumentPanel>>,
  active_document_id: Option<Uuid>,
  active_editor: Option<Entity<RichTextEditor>>,
  ribbon_collapsed: bool,
  tab_bar_scroll_handle: ScrollHandle,
  body_resizable_state: Entity<ResizableState>,
  content_resizable_state: Entity<ResizableState>,
  outline_tree: Entity<TreeState>,
  outline_cache: Option<(Uuid, u64, u64)>,
  collapsed_outline_items: HashSet<usize>,
  outline_revision: u64,
  outline_caret_paragraph: Option<usize>,
  editor_subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
struct DocumentTab {
  id: Uuid,
  label: SharedString,
  active: bool,
}

impl Workspace {
  // User-triggerable workspace methods are intentionally kept as named public
  // methods. When adding a new user-triggerable action here, also add it to
  // `crate::commands::CommandId` and `COMMAND_SPECS` so menus, toolbar buttons,
  // rebinding UI, and "show shortcut" UI all see the same command surface.
  pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let this = Self {
      document_panels: Vec::new(),
      active_document_id: None,
      active_editor: None,
      ribbon_collapsed: false,
      tab_bar_scroll_handle: ScrollHandle::new(),
      body_resizable_state: cx.new(|_| ResizableState::default()),
      content_resizable_state: cx.new(|_| ResizableState::default()),
      outline_tree: cx.new(|cx| TreeState::new(cx)),
      outline_cache: None,
      collapsed_outline_items: HashSet::new(),
      outline_revision: 0,
      outline_caret_paragraph: None,
      editor_subscriptions: Vec::new(),
    };

    if let Some(path) = initial_path {
      // Initial window creation happens before GPUI has produced stable
      // layout bounds for the resizable document area. Documents opened later
      // already run after that first layout pass, so defer startup loading by
      // one frame to give the initial editor the same settled geometry.
      cx.on_next_frame(window, move |workspace, window, cx| {
        let document = load_or_create_document(&path).unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
        workspace.add_document_panel(document, Some(path), window, cx);
      });
    }

    this
  }

  fn create_document_panel(
    &mut self,
    mut document: Document,
    path: Option<PathBuf>,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Entity<DocumentPanel> {
    // DB8 stores style assignments, not style appearance. The render theme is
    // local user preference loaded from app settings.
    document.theme = load_document_theme();
    let editor = cx.new(|cx| RichTextEditor::new_with_path(document, path.clone(), cx));
    self.editor_subscriptions.push(cx.observe(&editor, |workspace, editor, cx| {
      let caret_paragraph = Some(editor.read(cx).caret_paragraph());
      if workspace.outline_caret_paragraph != caret_paragraph {
        workspace.outline_caret_paragraph = caret_paragraph;
        cx.notify();
      }
    }));
    let workspace = cx.entity().downgrade();
    let panel = cx.new(|cx| DocumentPanel::new(path, editor.clone(), workspace, cx));
    let id = panel.read(cx).id();
    self.active_document_id = Some(id);
    self.active_editor = Some(editor);
    self.document_panels.push(panel.clone());
    panel
  }

  pub fn set_active_document(&mut self, panel_id: Uuid, editor: Entity<RichTextEditor>, cx: &mut Context<Self>) {
    self.active_document_id = Some(panel_id);
    self.active_editor = Some(editor);
    cx.notify();
  }

  pub fn remove_document_panel(&mut self, panel_id: Uuid, _: &mut Window, cx: &mut Context<Self>) {
    self.document_panels.retain(|panel| panel.read(cx).id() != panel_id);
    if self.active_document_id == Some(panel_id) {
      self.active_document_id = self.document_panels.last().map(|panel| panel.read(cx).id());
      self.active_editor = self.document_panels.last().map(|panel| panel.read(cx).editor());
    }
    cx.notify();
  }

  pub fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.add_document_panel(demo_document(), None, window, cx);
  }

  pub fn open_demo_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let path = PathBuf::from("data/demo.db8");
    let document = load_or_create_document(&path).unwrap_or_else(|_| demo_document());
    self.add_document_panel(document, Some(path), window, cx);
  }

  pub fn prompt_open_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
      files: true,
      directories: false,
      multiple: false,
      prompt: Some("Open .db8 document".into()),
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |workspace, cx| {
      let Ok(Ok(Some(paths))) = paths.await else {
        return;
      };
      let Some(path) = paths.into_iter().next() else {
        return;
      };
      let document = match load_or_create_document(&path) {
        Ok(document) => document,
        Err(error) => {
          let detail = format!("Failed to open {}: {error}", path.display());
          let _ = window_handle.update(cx, |_, window, cx| {
            window.prompt(PromptLevel::Critical, "Open failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
          });
          return;
        },
      };
      let _ = window_handle.update(cx, |_, window, cx| {
        let _ = workspace.update(cx, |workspace, cx| {
          workspace.add_document_panel(document, Some(path), window, cx);
        });
      });
    })
    .detach();
  }

  fn add_document_panel(&mut self, document: Document, path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    self.create_document_panel(document, path, window, cx);
    cx.notify();
  }

  pub fn close_document_panel(&mut self, panel_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
    let Some(panel) = self.document_panels.iter().find(|panel| panel.read(cx).id() == panel_id).cloned() else {
      return;
    };
    let editor = panel.read(cx).editor();
    if !editor.read(cx).has_unsaved_changes() {
      self.remove_document_panel(panel_id, window, cx);
      return;
    }

    let answer = window.prompt(
      PromptLevel::Warning,
      "Save changes before closing?",
      Some("This document has unsaved changes."),
      &[PromptButton::ok("Save"), PromptButton::new("Don't Save"), PromptButton::cancel("Cancel")],
      cx,
    );
    let window_handle = window.window_handle();
    cx.spawn(async move |workspace, cx| {
      let should_close = match answer.await {
        Ok(0) => match editor.update(cx, |editor, cx| editor.save(cx)) {
          Ok(Ok(())) => true,
          Ok(Err(error)) => {
            eprintln!("failed to save before close: {error}");
            false
          },
          Err(error) => {
            eprintln!("failed to access editor before close: {error}");
            false
          },
        },
        Ok(1) => {
          let _ = editor.update(cx, |editor, _| editor.discard_recovery_file());
          true
        },
        _ => false,
      };

      if should_close {
        let _ = window_handle.update(cx, |_, window, cx| {
          let _ = workspace.update(cx, |workspace, cx| workspace.remove_document_panel(panel_id, window, cx));
        });
      }
    })
    .detach();
  }

  fn request_close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let dirty_editors = self.dirty_editors(cx);
    if dirty_editors.is_empty() {
      window.remove_window();
      return;
    }

    let message = if dirty_editors.len() == 1 {
      "This document has unsaved changes."
    } else {
      "One or more documents have unsaved changes."
    };
    let answer = window.prompt(
      PromptLevel::Warning,
      "Save changes before closing?",
      Some(message),
      &[PromptButton::ok("Save"), PromptButton::new("Don't Save"), PromptButton::cancel("Cancel")],
      cx,
    );
    let window_handle = window.window_handle();

    cx.spawn(async move |_, cx| {
      let should_close = match answer.await {
        Ok(0) => {
          let mut ok = true;
          for editor in dirty_editors {
            match editor.update(cx, |editor, cx| editor.save(cx)) {
              Ok(Ok(())) => {}
              Ok(Err(error)) => {
                ok = false;
                let detail = error.to_string();
                let _ = window_handle.update(cx, |_, window, cx| {
                  window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
                });
                break;
              },
              Err(error) => {
                ok = false;
                eprintln!("failed to access editor before close: {error}");
                break;
              },
            }
          }
          ok
        },
        Ok(1) => {
          for editor in dirty_editors {
            let _ = editor.update(cx, |editor, _| editor.discard_recovery_file());
          }
          true
        },
        _ => false,
      };

      if should_close {
        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
      }
    })
    .detach();
  }

  pub fn save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(editor) = self.active_editor.clone() else {
      return;
    };
    match editor.update(cx, |editor, cx| editor.save(cx)) {
      Ok(()) => {},
      Err(error) => {
        let detail = error.to_string();
        let _ = window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx);
      },
    }
    cx.notify();
  }

  pub fn toggle_ribbon(&mut self, cx: &mut Context<Self>) {
    self.ribbon_collapsed = !self.ribbon_collapsed;
    cx.notify();
  }

  fn refresh_outline_tree(&mut self, cx: &mut Context<Self>) {
    let Some(active_id) = self.active_document_id else {
      if self.outline_cache.is_some() {
        self.outline_cache = None;
        self.outline_tree.update(cx, |tree, cx| tree.set_items(Vec::<TreeItem>::new(), cx));
      }
      return;
    };
    let Some(editor) = &self.active_editor else {
      return;
    };
    let generation = editor.read(cx).edit_generation();
    if self.outline_cache == Some((active_id, generation, self.outline_revision)) {
      return;
    }

    let document = editor.read(cx).document().clone();
    let items = outline_tree_items(&document, &self.collapsed_outline_items);
    self.outline_cache = Some((active_id, generation, self.outline_revision));
    self.outline_tree.update(cx, |tree, cx| tree.set_items(items, cx));
  }

  pub fn scroll_active_editor_to_paragraph(&mut self, paragraph_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(editor) = &self.active_editor {
      editor.update(cx, |editor, cx| editor.scroll_to_paragraph(paragraph_ix, window, cx));
    }
  }

  fn toggle_outline_item(&mut self, paragraph_ix: usize, cx: &mut Context<Self>) {
    if !self.collapsed_outline_items.insert(paragraph_ix) {
      self.collapsed_outline_items.remove(&paragraph_ix);
    }
    self.outline_revision = self.outline_revision.wrapping_add(1);
    self.outline_cache = None;
    self.refresh_outline_tree(cx);
    cx.notify();
  }

  pub fn dirty_editors(&self, cx: &App) -> Vec<Entity<RichTextEditor>> {
    self
      .document_panels
      .iter()
      .filter_map(|panel| {
        let editor = panel.read(cx).editor();
        editor.read(cx).has_unsaved_changes().then_some(editor)
      })
      .collect()
  }

  fn activate_document_id(&mut self, panel_id: Uuid, cx: &mut Context<Self>) {
    let Some(panel) = self.document_panels.iter().find(|panel| panel.read(cx).id() == panel_id) else {
      return;
    };
    self.active_document_id = Some(panel_id);
    self.active_editor = Some(panel.read(cx).editor());
    cx.notify();
  }

  fn active_document_index(&self, cx: &App) -> Option<usize> {
    let active_id = self.active_document_id?;
    self.document_panels.iter().position(|panel| panel.read(cx).id() == active_id)
  }

  fn apply_document_theme_to_open_editors(&mut self, theme: DocumentTheme, cx: &mut Context<Self>) {
    for panel in &self.document_panels {
      let editor = panel.read(cx).editor();
      let theme = theme.clone();
      editor.update(cx, |editor, cx| {
        editor.update_document_theme(|document_theme| *document_theme = theme, cx);
      });
    }
    cx.notify();
  }

  fn document_tabs(&self, cx: &App) -> Vec<DocumentTab> {
    self
      .document_panels
      .iter()
      .map(|panel| {
        let panel = panel.read(cx);
        let title = panel.title_text();
        let dirty = panel.is_dirty(cx);
        let title = truncate_tab_title(&title, 32);
        let label = if dirty {
          format!("{title} *").into()
        } else {
          title.into()
        };
        DocumentTab {
          id: panel.id(),
          label,
          active: Some(panel.id()) == self.active_document_id,
        }
      })
      .collect()
  }

  fn active_outline_paragraph(&self, cx: &App) -> Option<usize> {
    let editor = self.active_editor.as_ref()?;
    let editor = editor.read(cx);
    let caret_paragraph = editor.caret_paragraph();
    active_visible_outline_paragraph(editor.document(), caret_paragraph, &self.collapsed_outline_items)
  }

  fn refresh_outline_caret(&mut self, cx: &mut Context<Self>) {
    let caret_paragraph = self
      .active_editor
      .as_ref()
      .map(|editor| editor.read(cx).caret_paragraph());
    if self.outline_caret_paragraph != caret_paragraph {
      self.outline_caret_paragraph = caret_paragraph;
      cx.notify();
    }
  }

}

impl Render for Workspace {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    v_flex()
      .size_full()
      .bg(cx.theme().background)
      .child(self.render_top_bar(window, cx))
      .when(!self.ribbon_collapsed, |this| this.child(self.render_ribbon(cx)))
      .child(self.render_workspace_body(cx))
      .child(self.render_status_bar(cx))
  }
}

impl Workspace {
  fn document_styles_page(&self, workspace: WeakEntity<Workspace>, has_document: bool) -> SettingPage {
    SettingPage::new("Styles")
      .description("Customize the active document's style geometry, colors, and text metrics.")
      .resettable(false)
      .group(
        SettingGroup::new()
          .title("Apply to All")
          .description("Blank fields are left unchanged when Apply is pressed.")
          .item(SettingItem::render({
            let workspace = workspace.clone();
            move |_, window, cx| render_apply_all_styles(workspace.clone(), window, cx)
          })),
      )
      .group(
        SettingGroup::new()
          .title("Text")
          .description(if has_document { "Base font, color, and paragraph rhythm." } else { "Open a document to edit style values." })
          .item(style_string_item(workspace.clone(), "Font family", |theme| theme.default_font_family.clone(), |theme, value| {
            theme.default_font_family = value;
          }))
          .item(style_color_item(workspace.clone(), "Text color", |theme| theme.default_text_color, |theme, value| {
            theme.default_text_color = value;
          }))
          .item(style_number_item(workspace.clone(), "Body size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.body_font_size), |theme, value| {
            theme.body_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Line spacing", 0.25, 5.0, 0.01, |theme| theme.line_spacing as f64, |theme, value| {
            theme.line_spacing = value as f32;
          }))
          .item(style_number_item(workspace.clone(), "Paragraph after (pt)", 0.0, 200.0, 0.25, |theme| pixels_to_pt(theme.paragraph_after), |theme, value| {
            theme.paragraph_after = pt_to_pixels(value);
          })),
      )
      .group(
        SettingGroup::new()
          .title("Page")
          .description("Pageless document viewport insets.")
          .item(style_number_item(workspace.clone(), "Inset X (pt)", 0.0, 300.0, 0.25, |theme| pixels_to_pt(theme.pageless_inset_x), |theme, value| {
            theme.pageless_inset_x = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Inset top (pt)", 0.0, 300.0, 0.25, |theme| pixels_to_pt(theme.pageless_inset_top), |theme, value| {
            theme.pageless_inset_top = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Inset bottom (pt)", 0.0, 300.0, 0.25, |theme| pixels_to_pt(theme.pageless_inset_bottom), |theme, value| {
            theme.pageless_inset_bottom = pt_to_pixels(value);
          })),
      )
      .group(
        SettingGroup::new()
          .title("Hierarchy Sizes")
          .item(style_number_item(workspace.clone(), "Pocket size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.pocket_font_size), |theme, value| {
            theme.pocket_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Hat size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.hat_font_size), |theme, value| {
            theme.hat_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Block size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.block_font_size), |theme, value| {
            theme.block_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Tag size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.tag_font_size), |theme, value| {
            theme.tag_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Undertag size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.undertag_font_size), |theme, value| {
            theme.undertag_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Cite size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.cite_font_size), |theme, value| {
            theme.cite_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Condensed size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.condensed_font_size), |theme, value| {
            theme.condensed_font_size = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Ultra-condensed size (pt)", 1.0, 200.0, 0.25, |theme| pixels_to_pt(theme.ultracondensed_font_size), |theme, value| {
            theme.ultracondensed_font_size = pt_to_pixels(value);
          })),
      )
      .group(
        SettingGroup::new()
          .title("Hierarchy Spacing")
          .item(style_number_item(workspace.clone(), "Pocket before (pt)", 0.0, 200.0, 0.25, |theme| pixels_to_pt(theme.pocket_before), |theme, value| {
            theme.pocket_before = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Hat before (pt)", 0.0, 200.0, 0.25, |theme| pixels_to_pt(theme.hat_before), |theme, value| {
            theme.hat_before = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Block before (pt)", 0.0, 200.0, 0.25, |theme| pixels_to_pt(theme.block_before), |theme, value| {
            theme.block_before = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Tag before (pt)", 0.0, 200.0, 0.25, |theme| pixels_to_pt(theme.tag_before), |theme, value| {
            theme.tag_before = pt_to_pixels(value);
          })),
      )
      .group(
        SettingGroup::new()
          .title("Borders and Highlights")
          .item(style_number_item(workspace.clone(), "Pocket border width (pt)", 0.0, 20.0, 0.125, |theme| pixels_to_pt(theme.pocket_border_width), |theme, value| {
            theme.pocket_border_width = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Pocket border X space (pt)", 0.0, 40.0, 0.25, |theme| pixels_to_pt(theme.pocket_border_space_x), |theme, value| {
            theme.pocket_border_space_x = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Pocket border Y space (pt)", 0.0, 40.0, 0.25, |theme| pixels_to_pt(theme.pocket_border_space_y), |theme, value| {
            theme.pocket_border_space_y = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Emphasis border width (pt)", 0.0, 20.0, 0.125, |theme| pixels_to_pt(theme.emphasis_border_width), |theme, value| {
            theme.emphasis_border_width = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Emphasis paint width (pt)", 0.0, 20.0, 0.125, |theme| pixels_to_pt(theme.emphasis_border_paint_width), |theme, value| {
            theme.emphasis_border_paint_width = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Box padding left (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.box_padding_left), |theme, value| {
            theme.box_padding_left = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Box padding right (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.box_padding_right), |theme, value| {
            theme.box_padding_right = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Box padding top (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.box_padding_top), |theme, value| {
            theme.box_padding_top = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Box padding bottom (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.box_padding_bottom), |theme, value| {
            theme.box_padding_bottom = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Highlight pad X (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.highlight_pad_x), |theme, value| {
            theme.highlight_pad_x = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Highlight top extra", 0.0, 2.0, 0.01, |theme| theme.highlight_top_extra_fraction as f64, |theme, value| {
            theme.highlight_top_extra_fraction = value as f32;
          }))
          .item(style_number_item(workspace.clone(), "Highlight bottom extra", 0.0, 2.0, 0.01, |theme| theme.highlight_bottom_extra_fraction as f64, |theme, value| {
            theme.highlight_bottom_extra_fraction = value as f32;
          }))
          .item(style_color_item(workspace.clone(), "Spoken highlight", |theme| theme.highlight_spoken, |theme, value| {
            theme.highlight_spoken = value;
          }))
          .item(style_color_item(workspace.clone(), "Insert highlight", |theme| theme.highlight_insert, |theme, value| {
            theme.highlight_insert = value;
          }))
          .item(style_color_item(workspace.clone(), "Alternative highlight", |theme| theme.highlight_alternative, |theme, value| {
            theme.highlight_alternative = value;
          })),
      )
      .group(
        SettingGroup::new()
          .title("Underlines")
          .item(style_number_item(workspace.clone(), "Fallback top from baseline (pt)", -80.0, 80.0, 0.25, |theme| pixels_to_pt(theme.underline_fallback_top_from_baseline), |theme, value| {
            theme.underline_fallback_top_from_baseline = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Rule thickness (pt)", 0.0, 20.0, 0.125, |theme| pixels_to_pt(theme.underline_rule_thickness), |theme, value| {
            theme.underline_rule_thickness = pt_to_pixels(value);
          }))
          .item(style_bool_item(workspace.clone(), "Snap rules to pixels", |theme| theme.snap_underline_rules_to_pixels, |theme, value| {
            theme.snap_underline_rules_to_pixels = value;
          }))
          .item(style_number_item(workspace.clone(), "Double top from baseline (pt)", -80.0, 80.0, 0.25, |theme| pixels_to_pt(theme.double_underline_top_from_baseline), |theme, value| {
            theme.double_underline_top_from_baseline = pt_to_pixels(value);
          }))
          .item(style_number_item(workspace.clone(), "Double underline gap (pt)", 0.0, 80.0, 0.25, |theme| pixels_to_pt(theme.double_underline_gap), |theme, value| {
            theme.double_underline_gap = pt_to_pixels(value);
          })),
      )
      .group(
        SettingGroup::new()
          .title("Special Colors")
          .item(style_color_item(workspace.clone(), "Analytic text", |theme| theme.analytic_color, |theme, value| {
            theme.analytic_color = value;
          }))
          .item(style_color_item(workspace, "Undertag text", |theme| theme.undertag_color, |theme, value| {
            theme.undertag_color = value;
          })),
      )
  }

  fn render_top_bar(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
    h_flex()
      .h(px(36.0))
      .w_full()
      .items_center()
      .pl_2()
      .border_b_1()
      .border_color(cx.theme().title_bar_border)
      .bg(cx.theme().title_bar)
      // With a transparent system titlebar, this GPUI-drawn bar becomes the
      // visual titlebar. Let empty space in it drag the native window.
      .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
      .child(
        h_flex()
          .h_full()
          .items_center()
          .gap_1()
          .child(top_bar_button("top-file", "File"))
          .child(styles_top_bar_button(cx))
          .child(theme_top_bar_button(cx))
          .child(top_bar_button("top-settings", "Settings")),
      )
      .child(div().flex_1())
      .child(self.render_window_controls(window, cx))
  }

  fn render_window_controls(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
    h_flex()
      .h_full()
      .flex_none()
      .child(window_control_button(
        "window-minimize",
        IconName::WindowMinimize,
        WindowControlArea::Min,
        cx.listener(|_, _, window, cx| {
          cx.stop_propagation();
          window.minimize_window();
        }),
        false,
        cx,
      ))
      .child(window_control_button(
        "window-maximize",
        if window.is_maximized() { IconName::WindowRestore } else { IconName::WindowMaximize },
        WindowControlArea::Max,
        cx.listener(|_, _, window, cx| {
          cx.stop_propagation();
          window.zoom_window();
        }),
        false,
        cx,
      ))
      .child(window_control_button(
        "window-close",
        IconName::WindowClose,
        WindowControlArea::Close,
        cx.listener(|workspace, _, window, cx| {
          cx.stop_propagation();
          workspace.request_close_window(window, cx);
        }),
        true,
        cx,
      ))
  }

  fn render_ribbon(&self, cx: &mut Context<Self>) -> impl IntoElement {
    h_flex()
      .h(px(76.0))
      .w_full()
      .items_center()
      .px_2()
      .border_b_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().background)
      .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Ribbon placeholder"))
  }

  fn render_workspace_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    let panel_sizes = self.body_resizable_state.read(cx).sizes().clone();
    let nav_width = panel_sizes.first().copied().unwrap_or(px(240.0));

    h_resizable("workspace-body-resizable")
      .with_state(&self.body_resizable_state)
      .child(
        resizable_panel()
          .size(px(240.0))
          .size_range(px(180.0)..px(420.0))
          .grow(false)
          .child(self.render_left_nav(nav_width, cx)),
      )
      .child(
        resizable_panel()
          .size(px(860.0))
          .size_range(px(580.0)..Pixels::MAX)
          .child(self.render_content_area(cx)),
      )
  }

  fn render_content_area(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    h_resizable("workspace-content-resizable")
      .with_state(&self.content_resizable_state)
      .child(
        resizable_panel()
          .size(px(560.0))
          .size_range(px(360.0)..Pixels::MAX)
          .child(self.render_document_pane(cx)),
      )
      .child(
        resizable_panel()
          .size(px(300.0))
          .size_range(px(220.0)..px(520.0))
          .grow(false)
          .child(self.render_toolkit(cx)),
      )
  }

  fn render_left_nav(&mut self, nav_width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
    self.refresh_outline_tree(cx);
    self.refresh_outline_caret(cx);
    let workspace = cx.entity().downgrade();
    let active_outline_paragraph = self.active_outline_paragraph(cx);
    v_flex()
      .size_full()
      .h_full()
      .gap_1()
      .p_2()
      .border_r_1()
      .border_color(cx.theme().sidebar_border)
      .bg(cx.theme().sidebar)
      .text_color(cx.theme().sidebar_foreground)
      .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Outline"))
      .child(
        div()
          .flex_1()
          .w_full()
          .overflow_hidden()
          .child(tree(&self.outline_tree, move |ix, entry, _selected, window, cx| {
            let paragraph_ix = outline_paragraph_ix(entry.item().id.as_ref());
            let is_folder = entry.is_folder();
            let is_expanded = entry.is_expanded();
            let is_active_outline = paragraph_ix == active_outline_paragraph;
            let depth = entry.depth();
            let label_width = outline_label_width(nav_width, depth);
            let label = truncate_outline_label(entry.item().label.as_ref(), outline_label_text_width(label_width, window), window, cx);
            let workspace = workspace.clone();
            ListItem::new(("outline-tree-item", ix))
              .w_full()
              .min_w_0()
              .overflow_hidden()
              .pl(px(4.0) + px(12.0) * entry.depth())
              .pr_1()
              .py_0()
              .text_xs()
              .child(
                h_flex()
                  .w_full()
                  .min_w_0()
                  .overflow_hidden()
                  .items_center()
                  .gap_1()
                  .when(is_folder, |this| this.child(
                    Button::new(("outline-toggle", ix))
                      .icon(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                      .xsmall()
                      .ghost()
                      .flex_none()
                      .disabled(!is_folder)
                      .on_click({
                        let workspace = workspace.clone();
                        move |_, _, cx| {
                          cx.stop_propagation();
                          if let Some(paragraph_ix) = paragraph_ix {
                            let _ = workspace.update(cx, |workspace, cx| workspace.toggle_outline_item(paragraph_ix, cx));
                          }
                        }
                      }),
                  ))
                  .when(!is_folder, |this| this.child(div().w(px(20.0)).h(px(20.0)).flex_none()))
                  .child(
                    div()
                      .id(("outline-label", ix))
                      .relative()
                      .flex_1()
                      .min_w_0()
                      .px_1()
                      .overflow_hidden()
                      .text_color(cx.theme().sidebar_foreground)
                      .whitespace_nowrap()
                      .when(is_active_outline, |this| {
                        this.child(
                          div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .bg(cx.theme().sidebar_accent)
                            .border_1()
                            .border_color(cx.theme().primary)
                            .rounded(px(4.0)),
                        )
                      })
                      .child(label)
                      .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                      })
                      .on_click(move |_, window, cx| {
                        if let Some(paragraph_ix) = paragraph_ix {
                          let _ = workspace.update(cx, |workspace, cx| workspace.scroll_active_editor_to_paragraph(paragraph_ix, window, cx));
                        }
                      }),
                  ),
              )
          })),
      )
  }

  fn render_document_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    let active_index = self.active_document_index(cx).unwrap_or(0);
    v_flex()
      .flex_1()
      .w_full()
      .h_full()
      .overflow_hidden()
      .bg(cx.theme().background)
      .when(!self.document_panels.is_empty(), |this| this.child(self.render_document_tab_bar(active_index, cx)))
      .child(
        div()
          .flex_1()
          .w_full()
          .h_full()
          .overflow_hidden()
          .when_some(self.active_editor.clone(), |this, editor| this.child(editor))
          .when(self.active_editor.is_none(), |this| this.child(self.render_empty_state(cx))),
      )
  }

  fn render_document_tab_bar(&self, active_index: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let tabs = self.document_tabs(cx);
    let active_tab_fg = self
      .active_editor
      .as_ref()
      .map(|editor| editor.read(cx).document().theme.default_text_color)
      .unwrap_or_else(black);
    TabBar::new("document-tab-bar")
      .xsmall()
      .track_scroll(&self.tab_bar_scroll_handle)
      .menu(true)
      .active_tab_bg(white())
      .active_tab_fg(active_tab_fg)
      .selected_index(active_index)
      .on_click({
        let tabs = tabs.clone();
        cx.listener(move |workspace, ix: &usize, _, cx| {
          if let Some(tab) = tabs.get(*ix) {
            workspace.activate_document_id(tab.id, cx);
          }
        })
      })
      .children(tabs.into_iter().map(|tab| {
        let panel_id = tab.id;
        let close_button = icon_button(("close-tab", panel_id.as_u128() as u64), AppIcon::Close)
          .tooltip("Close document")
          .when(tab.active, |this| {
            this.custom(
              ButtonCustomVariant::new(cx)
                .foreground(active_tab_fg)
                .hover(active_tab_fg.opacity(0.12))
                .active(active_tab_fg.opacity(0.18)),
            )
          })
          .on_click(cx.listener(move |workspace, _, window, cx| {
            cx.stop_propagation();
            workspace.close_document_panel(panel_id, window, cx);
          }));
        Tab::new()
          // GPUI-component tabs size to their labels. Keep tab labels bounded
          // before rendering so long filenames cannot break the tab strip.
          .label(tab.label)
          .selected(tab.active)
          .suffix(close_button)
      }))
      .last_empty_space(div().flex_1().h_full())
  }

  fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
    // These buttons call command methods directly for now. When command
    // dispatch grows beyond direct callbacks, keep the buttons mapped to
    // `CommandId::NewDocument` and `CommandId::OpenDemoDocument`.
    let new_doc = cx.listener(|workspace, _, window, cx| workspace.new_document(window, cx));
    let open_demo = cx.listener(|workspace, _, window, cx| workspace.open_demo_document(window, cx));
    v_flex()
      .size_full()
      .items_center()
      .justify_center()
      .gap_3()
      .bg(cx.theme().background)
      .child(div().text_xl().font_weight(gpui::FontWeight::SEMIBOLD).child("No document open"))
      .child(
        h_flex()
          .gap_2()
          .child(Button::new("empty-new-document").icon(IconName::Plus).label("New").primary().on_click(new_doc))
          .child(Button::new("empty-open-demo").icon(IconName::FolderOpen).label("Open Demo").on_click(open_demo)),
      )
  }

  fn render_toolkit(&self, cx: &mut Context<Self>) -> impl IntoElement {
    v_flex()
      .size_full()
      .h_full()
      .gap_2()
      .p_3()
      .border_l_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().background)
      .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Toolkit"))
      .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Search, file tools, and document utilities will live here."))
  }

  fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    h_flex()
      .h(px(26.0))
      .w_full()
      .items_center()
      .px_2()
      .border_t_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().background)
      .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Bottom bar placeholder"))
  }
}

pub fn install_workspace_close_prompt(workspace: Entity<Workspace>, window: &mut Window, cx: &mut App) {
  let prompt_open = Rc::new(Cell::new(false));
  let allow_close = Rc::new(Cell::new(false));
  let window_handle = window.window_handle();

  window.on_window_should_close(cx, move |window, cx| {
    if allow_close.get() {
      return true;
    }

    let dirty_editors = workspace.read(cx).dirty_editors(cx);
    if dirty_editors.is_empty() {
      return true;
    }

    if prompt_open.get() {
      return false;
    }
    prompt_open.set(true);

    let message = if dirty_editors.len() == 1 {
      "This document has unsaved changes."
    } else {
      "One or more documents have unsaved changes."
    };
    let answer = window.prompt(
      PromptLevel::Warning,
      "Save changes before closing?",
      Some(message),
      &[PromptButton::ok("Save"), PromptButton::new("Don't Save"), PromptButton::cancel("Cancel")],
      cx,
    );
    let prompt_open = prompt_open.clone();
    let allow_close = allow_close.clone();

    cx.spawn(async move |cx| {
      let should_close = match answer.await {
        Ok(0) => {
          let mut ok = true;
          for editor in dirty_editors {
            match editor.update(cx, |editor, cx| editor.save(cx)) {
              Ok(Ok(())) => {},
              Ok(Err(error)) => {
                ok = false;
                let detail = error.to_string();
                let _ = window_handle.update(cx, |_, window, cx| {
                  window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
                });
                break;
              },
              Err(error) => {
                ok = false;
                eprintln!("failed to access editor before close: {error}");
                break;
              },
            }
          }
          ok
        },
        Ok(1) => {
          for editor in dirty_editors {
            let _ = editor.update(cx, |editor, _| editor.discard_recovery_file());
          }
          true
        },
        _ => false,
      };

      prompt_open.set(false);
      if should_close {
        allow_close.set(true);
        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
      }
    })
    .detach();

    false
  });
}

pub fn open_workspace_window(document_path: PathBuf, cx: &mut App) {
  let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
  cx
    .open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Maximized(bounds)),
        titlebar: Some(TitlebarOptions {
          title: Some("Odrenrir - Debate Processor".into()),
          appears_transparent: true,
          traffic_light_position: Some(point(px(12.0), px(18.0))),
        }),
        ..Default::default()
      },
      |window, cx| {
        window.set_window_title("Odrenrir - Debate Processor");
        let workspace = cx.new(|cx| Workspace::new(Some(document_path), window, cx));
        install_workspace_close_prompt(workspace.clone(), window, cx);
        cx.new(|cx| Root::new(workspace, window, cx))
      },
    )
    .unwrap();
}

#[derive(Clone)]
struct OutlineNode {
  paragraph_ix: usize,
  style: ParagraphStyle,
  text: String,
  children: Vec<OutlineNode>,
}

fn outline_tree_items(document: &Document, collapsed_items: &HashSet<usize>) -> Vec<TreeItem> {
  let mut roots = Vec::<OutlineNode>::new();
  for (paragraph_ix, paragraph) in document.paragraphs.iter().enumerate() {
    let Some(level) = outline_level(paragraph.style) else {
      continue;
    };
    insert_outline_node(
      &mut roots,
      level,
      OutlineNode {
        paragraph_ix,
        style: paragraph.style,
        text: outline_paragraph_label(document, paragraph_ix),
        children: Vec::new(),
      },
    );
  }
  roots
    .into_iter()
    .map(|node| outline_node_to_tree_item(node, collapsed_items))
    .collect()
}

fn insert_outline_node(nodes: &mut Vec<OutlineNode>, level: usize, node: OutlineNode) {
  if level == 0 {
    nodes.push(node);
    return;
  }

  if let Some(parent) = nodes.iter_mut().rev().find(|candidate| {
    outline_level(candidate.style)
      .map(|parent_level| parent_level < level)
      .unwrap_or(false)
  }) {
    insert_outline_node(&mut parent.children, level, node);
  } else {
    nodes.push(node);
  }
}

fn outline_node_to_tree_item(node: OutlineNode, collapsed_items: &HashSet<usize>) -> TreeItem {
  let paragraph_ix = node.paragraph_ix;
  TreeItem::new(
    outline_item_id(paragraph_ix),
    node.text,
  )
  .children(
    node
      .children
      .into_iter()
      .map(|child| outline_node_to_tree_item(child, collapsed_items)),
  )
  .expanded(!collapsed_items.contains(&paragraph_ix))
  .disabled(true)
}

fn outline_nodes(document: &Document) -> Vec<OutlineNode> {
  let mut roots = Vec::<OutlineNode>::new();
  for (paragraph_ix, paragraph) in document.paragraphs.iter().enumerate() {
    let Some(level) = outline_level(paragraph.style) else {
      continue;
    };
    insert_outline_node(
      &mut roots,
      level,
      OutlineNode {
        paragraph_ix,
        style: paragraph.style,
        text: outline_paragraph_label(document, paragraph_ix),
        children: Vec::new(),
      },
    );
  }
  roots
}

fn active_visible_outline_paragraph(document: &Document, caret_paragraph: usize, collapsed_items: &HashSet<usize>) -> Option<usize> {
  let mut active = None;
  for node in outline_nodes(document) {
    active_visible_outline_paragraph_in_node(&node, caret_paragraph, collapsed_items, &mut active);
  }
  active
}

fn active_visible_outline_paragraph_in_node(
  node: &OutlineNode,
  caret_paragraph: usize,
  collapsed_items: &HashSet<usize>,
  active: &mut Option<usize>,
) {
  if node.paragraph_ix > caret_paragraph {
    return;
  }
  *active = Some(node.paragraph_ix);
  if collapsed_items.contains(&node.paragraph_ix) {
    return;
  }
  for child in &node.children {
    active_visible_outline_paragraph_in_node(child, caret_paragraph, collapsed_items, active);
  }
}

fn outline_level(style: ParagraphStyle) -> Option<usize> {
  match style {
    ParagraphStyle::Pocket => Some(0),
    ParagraphStyle::Hat => Some(1),
    ParagraphStyle::Block => Some(2),
    ParagraphStyle::Tag | ParagraphStyle::Analytic => Some(3),
    ParagraphStyle::Normal | ParagraphStyle::Undertag => None,
  }
}

fn outline_item_id(paragraph_ix: usize) -> String {
  format!("paragraph:{paragraph_ix}")
}

fn outline_paragraph_ix(id: &str) -> Option<usize> {
  id.strip_prefix("paragraph:")?.parse().ok()
}

fn outline_paragraph_label(document: &Document, paragraph_ix: usize) -> String {
  let paragraph = &document.paragraphs[paragraph_ix];
  let mut text = String::new();
  for chunk in document.text.byte_slice(paragraph.byte_range.clone()).chunks() {
    text.push_str(chunk);
  }
  let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
  let text = text.trim();
  if text.is_empty() {
    "(empty)".to_string()
  } else if text.len() > 80 {
    format!("{}...", &text[..safe_prefix_boundary(text, 77)])
  } else {
    text.to_string()
  }
}

fn outline_label_width(nav_width: Pixels, depth: usize) -> Pixels {
  // Mirrors the outline row layout: nav padding, row indentation, disclosure
  // slot, row gap, and right padding are fixed, so the remaining width is the
  // label rect. Keeping this deterministic avoids a first-paint measure/notify
  // cycle that visibly moves the tree after startup.
  (nav_width - px(56.0) - px(12.0) * depth).max(px(32.0))
}

fn outline_label_text_width(label_width: Pixels, window: &Window) -> Pixels {
  // The measured blue label rect includes `.px_1()` padding on both sides.
  // Truncation must target the inner text box, with a small paint tolerance so
  // the suffix glyph does not get clipped by the label's overflow boundary.
  (label_width - window.rem_size() * 0.5 - px(2.0)).max(px(1.0))
}

fn truncate_outline_label(label: &str, width: Pixels, window: &mut Window, cx: &mut App) -> SharedString {
  let text_style = window.text_style();
  // Keep this in sync with the outline row's `.text_xs()` style. GPUI's text
  // helper defines text_xs as 0.75rem; using the default 1rem style here makes
  // the app-level truncator think the label is much wider than it renders.
  let font_size = window.rem_size() * 0.75;
  let mut runs = vec![text_style.to_run(label.len())];
  cx
    .text_system()
    .line_wrapper(text_style.font(), font_size)
    .truncate_line(label.to_string().into(), width, "…", &mut runs)
    .into()
}

fn window_control_button(
  id: &'static str,
  icon: IconName,
  area: WindowControlArea,
  on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
  destructive: bool,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  div()
    .id(id)
    .window_control_area(area)
    .w(px(46.0))
    .h_full()
    .flex()
    .items_center()
    .justify_center()
    .text_size(px(12.0))
    .text_color(cx.theme().muted_foreground)
    .hover(|this| {
      if destructive {
        this.bg(cx.theme().danger).text_color(cx.theme().danger_foreground)
      } else {
        this.bg(cx.theme().secondary_hover).text_color(cx.theme().foreground)
      }
    })
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .on_click(on_click)
    .child(icon)
}

fn styles_top_bar_button(cx: &mut Context<Workspace>) -> impl IntoElement {
  div()
    .h_full()
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .child(
      Button::new("top-styles")
        .label("Styles")
        .xsmall()
        .ghost()
        .on_click(cx.listener(|workspace, _, window, cx| {
          let workspace_entity = cx.entity().downgrade();
          let has_document = workspace.active_editor.is_some();
          let page = workspace.document_styles_page(workspace_entity, has_document);
          cx.stop_propagation();
          window.open_sheet(cx, move |sheet, _, _| {
            sheet
              .title("Document Styles")
              .size(px(820.0))
              .margin_top(px(36.0))
              .child(
                Settings::new("document-style-settings")
                  .sidebar_width(px(180.0))
                  .page(page.clone()),
              )
          });
        })),
    )
}

fn top_bar_button(id: &'static str, label: &'static str) -> impl IntoElement {
  // The top bar itself starts native window dragging on mouse down. Each
  // button owns its mouse-down event so it behaves like a control instead of
  // dragging the window.
  div()
    .h_full()
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .child(
      Button::new(id)
        .label(label)
        .xsmall()
        .ghost()
        .on_click(|_, _, cx| cx.stop_propagation()),
    )
}

fn style_number_item(
  workspace: WeakEntity<Workspace>,
  title: &'static str,
  min: f64,
  max: f64,
  step: f64,
  get: fn(&DocumentTheme) -> f64,
  set: fn(&mut DocumentTheme, f64),
) -> SettingItem {
  let read_workspace = workspace.clone();
  let write_workspace = workspace;
  SettingItem::new(
    title,
    SettingField::number_input(
      NumberFieldOptions { min, max, step },
      move |cx| active_theme_value(cx, &read_workspace, get).unwrap_or_default(),
      move |value, cx| update_active_document_theme(cx, &write_workspace, move |theme| set(theme, value)),
    ),
  )
  .layout(Axis::Horizontal)
}

fn style_string_item(
  workspace: WeakEntity<Workspace>,
  title: &'static str,
  get: fn(&DocumentTheme) -> SharedString,
  set: fn(&mut DocumentTheme, SharedString),
) -> SettingItem {
  let read_workspace = workspace.clone();
  let write_workspace = workspace;
  SettingItem::new(
    title,
    SettingField::input(
      move |cx| active_theme_value(cx, &read_workspace, get).unwrap_or_default(),
      move |value, cx| update_active_document_theme(cx, &write_workspace, move |theme| set(theme, value)),
    ),
  )
  .layout(Axis::Horizontal)
}

fn style_bool_item(
  workspace: WeakEntity<Workspace>,
  title: &'static str,
  get: fn(&DocumentTheme) -> bool,
  set: fn(&mut DocumentTheme, bool),
) -> SettingItem {
  let read_workspace = workspace.clone();
  let write_workspace = workspace;
  SettingItem::new(
    title,
    SettingField::switch(
      move |cx| active_theme_value(cx, &read_workspace, get).unwrap_or_default(),
      move |value, cx| update_active_document_theme(cx, &write_workspace, move |theme| set(theme, value)),
    ),
  )
  .layout(Axis::Horizontal)
}

fn style_color_item(
  workspace: WeakEntity<Workspace>,
  title: &'static str,
  get: fn(&DocumentTheme) -> Hsla,
  set: fn(&mut DocumentTheme, Hsla),
) -> SettingItem {
  let read_workspace = workspace.clone();
  let write_workspace = workspace;
  SettingItem::new(
    title,
    SettingField::input(
      move |cx| active_theme_value(cx, &read_workspace, get).map(color_to_hex).unwrap_or_default().into(),
      move |value, cx| {
        if let Some(color) = parse_hex_color(&value) {
          update_active_document_theme(cx, &write_workspace, move |theme| set(theme, color));
        }
      },
    ),
  )
  .description("Hex color, e.g. #1f3864.")
  .layout(Axis::Horizontal)
}

fn active_theme_value<T>(cx: &App, workspace: &WeakEntity<Workspace>, get: fn(&DocumentTheme) -> T) -> Option<T> {
  let workspace = workspace.upgrade()?;
  let workspace = workspace.read(cx);
  if let Some(editor) = workspace.active_editor.clone() {
    Some(get(&editor.read(cx).document().theme))
  } else {
    Some(get(&load_document_theme()))
  }
}

fn update_active_document_theme(cx: &mut App, workspace: &WeakEntity<Workspace>, update: impl FnOnce(&mut DocumentTheme)) {
  let _ = workspace.update(cx, |workspace, cx| {
    let mut theme = workspace
      .active_editor
      .as_ref()
      .map(|editor| editor.read(cx).document().theme.clone())
      .unwrap_or_else(load_document_theme);
    update(&mut theme);

    if let Err(error) = save_document_theme(&theme) {
      eprintln!("failed to save document style settings: {error}");
    }

    workspace.apply_document_theme_to_open_editors(theme, cx);
  });
}

fn render_apply_all_styles(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) -> AnyElement {
  let font_size = window.use_keyed_state("style-apply-all-font-size", cx, |window, cx| {
    cx.new(|cx| InputState::new(window, cx).placeholder("Font size pt").default_value(""))
  });
  let before = window.use_keyed_state("style-apply-all-before", cx, |window, cx| {
    cx.new(|cx| InputState::new(window, cx).placeholder("Before spacing pt").default_value(""))
  });
  let text_color = window.use_keyed_state("style-apply-all-text-color", cx, |window, cx| {
    cx.new(|cx| InputState::new(window, cx).placeholder("Text color").default_value(""))
  });
  let font_size_state = font_size.read(cx).clone();
  let before_state = before.read(cx).clone();
  let text_color_state = text_color.read(cx).clone();

  h_flex()
    .w_full()
    .gap_2()
    .items_center()
    .child(Input::new(&font_size_state).w_32())
    .child(Input::new(&before_state).w_32())
    .child(Input::new(&text_color_state).w_32())
    .child(
      Button::new("apply-all-document-styles")
        .label("Apply")
        .primary()
        .small()
        .on_click(move |_, _, cx| {
          let font_size = optional_f64(&font_size_state.read(cx).value());
          let before = optional_f64(&before_state.read(cx).value());
          let text_color = optional_hex_color(&text_color_state.read(cx).value());

          update_active_document_theme(cx, &workspace, move |theme| {
            if let Some(font_size) = font_size {
              let size = pt_to_pixels(font_size);
              theme.body_font_size = size;
              theme.cite_font_size = size;
              theme.condensed_font_size = size;
              theme.ultracondensed_font_size = size;
              theme.pocket_font_size = size;
              theme.hat_font_size = size;
              theme.block_font_size = size;
              theme.tag_font_size = size;
              theme.undertag_font_size = size;
            }
            if let Some(before) = before {
              let spacing = pt_to_pixels(before);
              theme.pocket_before = spacing;
              theme.hat_before = spacing;
              theme.block_before = spacing;
              theme.tag_before = spacing;
            }
            if let Some(color) = text_color {
              theme.default_text_color = color;
              theme.analytic_color = color;
              theme.undertag_color = color;
            }
          });
        }),
    )
    .into_any_element()
}

fn pixels_to_pt(value: Pixels) -> f64 {
  value.as_f64() * 72.0 / 96.0
}

fn pt_to_pixels(value: f64) -> Pixels {
  px((value as f32) * 96.0 / 72.0)
}

fn color_to_hex(color: Hsla) -> String {
  let rgba = color.to_rgb();
  let r = (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8;
  let g = (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8;
  let b = (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8;
  format!("#{r:02x}{g:02x}{b:02x}")
}

fn parse_hex_color(value: &str) -> Option<Hsla> {
  let value = value.trim().trim_start_matches('#');
  if value.len() != 6 {
    return None;
  }
  u32::from_str_radix(value, 16).ok().map(|hex| rgb(hex).into())
}

fn optional_f64(value: &str) -> Option<f64> {
  let value = value.trim();
  if value.is_empty() {
    None
  } else {
    value.parse::<f64>().ok()
  }
}

fn optional_hex_color(value: &str) -> Option<Hsla> {
  let value = value.trim();
  if value.is_empty() {
    None
  } else {
    parse_hex_color(value)
  }
}

fn theme_top_bar_button(cx: &mut Context<Workspace>) -> impl IntoElement {
  let current_theme = Theme::global(cx).theme_name().to_string();
  let theme_names = ThemeRegistry::global(cx)
    .sorted_themes()
    .into_iter()
    .map(|theme| theme.name.to_string())
    .collect::<Vec<_>>();

  div()
    .h_full()
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .child(
      Button::new("top-themes")
        .label("Themes")
        .xsmall()
        .ghost()
        .dropdown_menu(move |menu, _, _| {
          let menu = menu.scrollable(true);
          theme_names.iter().fold(menu, |menu, theme_name| {
            let selected = theme_name == &current_theme;
            let label = theme_name.clone();
            let theme_name = theme_name.clone();
            menu.item(
              PopupMenuItem::new(label)
                .checked(selected)
                .on_click(move |_, window, cx| {
                  apply_app_theme(&theme_name, Some(window), cx);
                }),
            )
          })
        }),
    )
}

fn apply_app_theme(theme_name: &str, window: Option<&mut Window>, cx: &mut App) {
  let Some(theme) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() else {
    return;
  };

  let mode = theme.mode;
  Theme::global_mut(cx).apply_config(&theme);
  Theme::change(mode, window, cx);
  cx.refresh_windows();

  if let Err(error) = save_theme_name(theme_name) {
    eprintln!("failed to save theme setting: {error}");
  }
}

fn safe_prefix_boundary(text: &str, max: usize) -> usize {
  if max >= text.len() {
    return text.len();
  }
  let mut boundary = 0;
  for (ix, _) in text.char_indices() {
    if ix > max {
      break;
    }
    boundary = ix;
  }
  boundary
}

fn truncate_tab_title(title: &str, max_chars: usize) -> String {
  let mut chars = title.chars();
  let mut short = String::new();
  for _ in 0..max_chars {
    let Some(ch) = chars.next() else {
      return title.to_string();
    };
    short.push(ch);
  }

  if chars.next().is_some() {
    short.push_str("...");
  }
  short
}
