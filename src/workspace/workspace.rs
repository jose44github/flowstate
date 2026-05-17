use std::{cell::Cell, collections::HashSet, path::PathBuf, rc::Rc};

use gpui::{
  App, Bounds, Context, Entity, IntoElement, PromptButton, PromptLevel, Render, ScrollHandle, Window, WindowBounds,
  WindowOptions, PathPromptOptions, div, prelude::*, px, rgb, size,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::list::ListItem;
use gpui_component::tab::{Tab, TabBar};
use gpui_component::tree::{TreeItem, TreeState, tree};
use gpui_component::{Disableable, IconName, Selectable, Sizable, h_flex, v_flex};
use uuid::Uuid;

use crate::rich_text_element::{Document, ParagraphStyle, RichTextEditor, demo_document, load_or_create_document};
use crate::workspace::document_panel::DocumentPanel;
use crate::workspace::icons::{AppIcon, icon_button};

pub struct Workspace {
  document_panels: Vec<Entity<DocumentPanel>>,
  active_document_id: Option<Uuid>,
  active_editor: Option<Entity<RichTextEditor>>,
  ribbon_collapsed: bool,
  tab_bar_scroll_handle: ScrollHandle,
  outline_tree: Entity<TreeState>,
  outline_cache: Option<(Uuid, u64, u64)>,
  collapsed_outline_items: HashSet<usize>,
  outline_revision: u64,
}

impl Workspace {
  // User-triggerable workspace methods are intentionally kept as named public
  // methods. When adding a new user-triggerable action here, also add it to
  // `crate::commands::CommandId` and `COMMAND_SPECS` so menus, toolbar buttons,
  // rebinding UI, and "show shortcut" UI all see the same command surface.
  pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let mut this = Self {
      document_panels: Vec::new(),
      active_document_id: None,
      active_editor: None,
      ribbon_collapsed: false,
      tab_bar_scroll_handle: ScrollHandle::new(),
      outline_tree: cx.new(|cx| TreeState::new(cx)),
      outline_cache: None,
      collapsed_outline_items: HashSet::new(),
      outline_revision: 0,
    };

    if let Some(path) = initial_path {
      let document = load_or_create_document(&path).unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
      this.add_document_panel(document, Some(path), window, cx);
    }

    this
  }

  fn create_document_panel(
    &mut self,
    document: Document,
    path: Option<PathBuf>,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Entity<DocumentPanel> {
    let editor = cx.new(|cx| RichTextEditor::new_with_path(document, path.clone(), cx));
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

  fn activate_document_index(&mut self, index: usize, cx: &mut Context<Self>) {
    let Some(panel) = self.document_panels.get(index) else {
      return;
    };
    self.active_document_id = Some(panel.read(cx).id());
    self.active_editor = Some(panel.read(cx).editor());
    cx.notify();
  }

  fn active_document_index(&self, cx: &App) -> Option<usize> {
    let active_id = self.active_document_id?;
    self.document_panels.iter().position(|panel| panel.read(cx).id() == active_id)
  }

}

impl Render for Workspace {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    v_flex()
      .size_full()
      .bg(rgb(0xf1f5f9))
      .child(self.render_top_bar(cx))
      .when(!self.ribbon_collapsed, |this| this.child(self.render_ribbon()))
      .child(self.render_workspace_body(cx))
      .child(self.render_status_bar(cx))
  }
}

impl Workspace {
  fn render_top_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    let _ = cx;

    h_flex()
      .h(px(36.0))
      .w_full()
      .items_center()
      .px_2()
      .border_b_1()
      .border_color(rgb(0xdbe3ee))
      .bg(rgb(0xffffff))
      .child(div().text_xs().text_color(rgb(0x64748b)).child("Top bar placeholder"))
  }

  fn render_ribbon(&self) -> impl IntoElement {
    h_flex()
      .h(px(76.0))
      .w_full()
      .items_center()
      .px_2()
      .border_b_1()
      .border_color(rgb(0xdbe3ee))
      .bg(rgb(0xf8fafc))
      .child(div().text_xs().text_color(rgb(0x64748b)).child("Ribbon placeholder"))
  }

  fn render_workspace_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    h_flex()
      .flex_1()
      .w_full()
      .h_full()
      .overflow_hidden()
      .child(self.render_left_nav(cx))
      .child(self.render_document_pane(cx))
      .child(self.render_toolkit())
  }

  fn render_left_nav(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    self.refresh_outline_tree(cx);
    let workspace = cx.entity().downgrade();
    v_flex()
      .w(px(240.0))
      .h_full()
      .gap_1()
      .p_2()
      .border_r_1()
      .border_color(rgb(0xdbe3ee))
      .bg(rgb(0xf8fafc))
      .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Outline"))
      .child(
        div()
          .flex_1()
          .w_full()
          .overflow_hidden()
          .child(tree(&self.outline_tree, move |ix, entry, selected, _window, _cx| {
            let paragraph_ix = outline_paragraph_ix(entry.item().id.as_ref());
            let label = entry.item().label.clone();
            let is_folder = entry.is_folder();
            let is_expanded = entry.is_expanded();
            let workspace = workspace.clone();
            ListItem::new(("outline-tree-item", ix))
              .selected(selected)
              .disabled(true)
              .pl(px(4.0) + px(12.0) * entry.depth())
              .pr_1()
              .py_0()
              .text_xs()
              .child(
                h_flex()
                  .w_full()
                  .items_center()
                  .gap_1()
                  .child(
                    Button::new(("outline-toggle", ix))
                      .icon(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                      .xsmall()
                      .ghost()
                      .disabled(!is_folder)
                      .on_click({
                        let workspace = workspace.clone();
                        move |_, _, cx| {
                          if let Some(paragraph_ix) = paragraph_ix {
                            let _ = workspace.update(cx, |workspace, cx| workspace.toggle_outline_item(paragraph_ix, cx));
                          }
                        }
                      }),
                  )
                  .child(
                    Button::new(("outline-label", ix))
                      .xsmall()
                      .ghost()
                      .flex_1()
                      .min_w_0()
                      .overflow_hidden()
                      .child(
                        div()
                          .w_full()
                          .min_w_0()
                          .overflow_hidden()
                          .text_ellipsis()
                          .whitespace_nowrap()
                          .text_left()
                          .text_xs()
                          .child(label),
                      )
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
      .bg(rgb(0xffffff))
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
    TabBar::new("document-tab-bar")
      .xsmall()
      .track_scroll(&self.tab_bar_scroll_handle)
      .menu(true)
      .selected_index(active_index)
      .children(self.document_panels.iter().enumerate().map(|(ix, panel)| {
        let panel_id = panel.read(cx).id();
        let title = panel.read(cx).title_text();
        let label = if panel.read(cx).is_dirty(cx) {
          format!("{title} *")
        } else {
          title.to_string()
        };
        Tab::new()
          .label(label)
          .selected(ix == active_index)
          .on_click(cx.listener(move |workspace, _, _, cx| workspace.activate_document_index(ix, cx)))
          .suffix(
            icon_button(("close-tab", panel_id.as_u128() as u64), AppIcon::Close)
              .tooltip("Close document")
              .on_click(cx.listener(move |workspace, _, window, cx| {
                workspace.close_document_panel(panel_id, window, cx);
              })),
          )
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
      .bg(rgb(0xffffff))
      .child(div().text_xl().font_weight(gpui::FontWeight::SEMIBOLD).child("No document open"))
      .child(
        h_flex()
          .gap_2()
          .child(Button::new("empty-new-document").icon(IconName::Plus).label("New").primary().on_click(new_doc))
          .child(Button::new("empty-open-demo").icon(IconName::FolderOpen).label("Open Demo").on_click(open_demo)),
      )
  }

  fn render_toolkit(&self) -> impl IntoElement {
    v_flex()
      .w(px(300.0))
      .h_full()
      .gap_2()
      .p_3()
      .border_l_1()
      .border_color(rgb(0xdbe3ee))
      .bg(rgb(0xf8fafc))
      .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Toolkit"))
      .child(div().text_sm().text_color(rgb(0x64748b)).child("Search, file tools, and document utilities will live here."))
  }

  fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let _ = cx;
    h_flex()
      .h(px(26.0))
      .w_full()
      .items_center()
      .px_2()
      .border_t_1()
      .border_color(rgb(0xdbe3ee))
      .bg(rgb(0xffffff))
      .child(div().text_xs().text_color(rgb(0x64748b)).child("Bottom bar placeholder"))
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
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
      },
      |window, cx| {
        window.set_window_title("Odrenrir - Debate Processor");
        let workspace = cx.new(|cx| Workspace::new(Some(document_path), window, cx));
        install_workspace_close_prompt(workspace.clone(), window, cx);
        workspace
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
  let expanded = !collapsed_items.contains(&paragraph_ix);
  TreeItem::new(
    outline_item_id(paragraph_ix),
    format!("{}: {}", outline_style_label(node.style), node.text),
  )
  .children(
    node
      .children
      .into_iter()
      .map(|child| outline_node_to_tree_item(child, collapsed_items)),
  )
  .expanded(expanded)
  .disabled(true)
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

fn outline_style_label(style: ParagraphStyle) -> &'static str {
  match style {
    ParagraphStyle::Pocket => "Pocket",
    ParagraphStyle::Hat => "Hat",
    ParagraphStyle::Block => "Block",
    ParagraphStyle::Tag => "Tag",
    ParagraphStyle::Analytic => "Analytic",
    ParagraphStyle::Undertag => "Undertag",
    ParagraphStyle::Normal => "Normal",
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
