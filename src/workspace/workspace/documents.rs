impl Workspace {
  pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let this = Self {
      document_panels: Vec::new(),
      active_document_id: None,
      active_editor: None,
      ribbon_collapsed: false,
      outline_collapsed: false,
      toolkit_collapsed: false,
      tab_bar_scroll_handle: ScrollHandle::new(),
      body_resizable_state: cx.new(|_| ResizableState::default()),
      content_resizable_state: cx.new(|_| ResizableState::default()),
      ribbon_resizable_state: cx.new(|_| ResizableState::default()),
      committed_ribbon_height: px(112.0),
      outline_tree: cx.new(|cx| TreeState::new(cx)),
      outline_cache: None,
      collapsed_outline_items: HashSet::new(),
      outline_revision: 0,
      outline_caret_paragraph: None,
      editor_subscriptions: Vec::new(),
      styles_settings_open: false,
      file_search_overlay: None,
    };

    if let Some(path) = initial_path {
      // Initial window creation happens before GPUI has produced stable
      // layout bounds for the resizable document area. Documents opened later
      // already run after that first layout pass, so defer startup loading by
      // one frame to give the initial editor the same settled geometry.
      cx.on_next_frame(window, move |workspace, window, cx| {
        let (document, document_path) =
          load_document_for_open(&path).unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
        workspace.add_document_panel(document, document_path, window, cx);
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
    let smart_word_selection = load_smart_word_selection();
    editor.update(cx, |editor, cx| {
      editor.set_smart_word_selection(smart_word_selection, cx);
    });
    let workspace = cx.entity().downgrade();
    let title = path
      .as_ref()
      .and_then(|path| path.file_name())
      .map(|name| name.to_string_lossy().to_string())
      .or_else(|| Some(self.next_untitled_title(cx)));
    let panel = cx.new(|cx| DocumentPanel::new_with_title(title, path, editor.clone(), workspace, cx));
    let id = panel.read(cx).id();
    self.editor_subscriptions.push((
      id,
      cx.observe(&editor, |workspace, editor, cx| {
        let caret_paragraph = Some(editor.read(cx).caret_paragraph());
        if workspace.outline_caret_paragraph != caret_paragraph {
          workspace.outline_caret_paragraph = caret_paragraph;
          cx.notify();
        }
      }),
    ));
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
    let closing_active_document = self.active_document_id == Some(panel_id);
    if let Some(panel) = self
      .document_panels
      .iter()
      .find(|panel| panel.read(cx).id() == panel_id)
    {
      let editor = panel.read(cx).editor();
      let _ = editor.update(cx, |editor, _| editor.dispose_for_close());
    }
    self
      .document_panels
      .retain(|panel| panel.read(cx).id() != panel_id);
    self.editor_subscriptions.retain(|(id, _)| *id != panel_id);
    if closing_active_document {
      self.active_document_id = self.document_panels.last().map(|panel| panel.read(cx).id());
      self.active_editor = self
        .document_panels
        .last()
        .map(|panel| panel.read(cx).editor());
      self.outline_cache = None;
      self.outline_caret_paragraph = self
        .active_editor
        .as_ref()
        .map(|editor| editor.read(cx).caret_paragraph());
    }
    if self.active_document_id.is_none() {
      self.outline_cache = None;
      self.outline_caret_paragraph = None;
      self.collapsed_outline_items.clear();
      self
        .outline_tree
        .update(cx, |tree, cx| tree.set_items(Vec::<TreeItem>::new(), cx));
    } else if closing_active_document {
      self.refresh_outline_tree(cx);
    }
    cx.notify();
  }

  pub fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.add_document_panel(new_blank_document(), None, window, cx);
  }

  pub fn open_demo_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let path = PathBuf::from("data/demo.db8");
    let document = load_or_create_document(&path).unwrap_or_else(|_| demo_document());
    self.add_document_panel(document, Some(path), window, cx);
  }

  pub fn open_document_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    match load_document_for_open(&path) {
      Ok((document, document_path)) => self.add_document_panel(document, document_path, window, cx),
      Err(error) => {
        let detail = format!("Failed to open {}: {error}", path.display());
        let _ = window.prompt(PromptLevel::Critical, "Open failed", Some(&detail), &[PromptButton::ok("Ok")], cx);
      },
    }
  }

  pub fn prompt_open_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
      files: true,
      directories: false,
      multiple: false,
      prompt: Some("Open .db8 or .docx document".into()),
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |workspace, cx| {
      let Ok(Ok(Some(paths))) = paths.await else {
        return;
      };
      let Some(path) = paths.into_iter().next() else {
        return;
      };
      let (document, document_path) = match load_document_for_open(&path) {
        Ok(result) => result,
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
          workspace.add_document_panel(document, document_path, window, cx);
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
    let Some(panel) = self
      .document_panels
      .iter()
      .find(|panel| panel.read(cx).id() == panel_id)
      .cloned()
    else {
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
    if editor.read(cx).document_path().is_none() {
      self.prompt_save_active_as(editor, window, cx);
      return;
    }
    match editor.update(cx, |editor, cx| editor.save(cx)) {
      Ok(()) => {},
      Err(error) => {
        let detail = error.to_string();
        let _ = window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx);
      },
    }
    cx.notify();
  }

  pub fn save_active_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(editor) = self.active_editor.clone() else {
      return;
    };
    self.prompt_save_active_as(editor, window, cx);
  }

  pub fn close_active_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(panel_id) = self.active_document_id else {
      return;
    };
    self.close_document_panel(panel_id, window, cx);
  }

  pub fn open_file_search_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(overlay) = self.file_search_overlay.clone() {
      overlay.update(cx, |overlay, cx| overlay.focus_search(window, cx));
      return;
    }

    let workspace = cx.entity().downgrade();
    let overlay = cx.new(|cx| FileSearchOverlay::new(workspace, window, cx));
    overlay.update(cx, |overlay, cx| overlay.focus_search(window, cx));
    self.file_search_overlay = Some(overlay);
    cx.notify();
  }

  pub fn close_file_search_overlay(&mut self, cx: &mut Context<Self>) {
    self.file_search_overlay = None;
    cx.notify();
  }

  fn prompt_save_active_as(&mut self, editor: Entity<RichTextEditor>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(panel_id) = self.active_document_id else {
      return;
    };
    let save_path = cx.prompt_for_new_path(&default_save_directory(), Some(UNTITLED_DOCUMENT_NAME));
    let window_handle = window.window_handle();
    cx.spawn(async move |workspace, cx| {
      let path = match save_path.await {
        Ok(Ok(Some(path))) => normalize_db8_path(path),
        Ok(Ok(None)) => return,
        Ok(Err(error)) => {
          let detail = error.to_string();
          let _ = window_handle.update(cx, |_, window, cx| {
            window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
          });
          return;
        },
        Err(error) => {
          eprintln!("save dialog was canceled before completion: {error}");
          return;
        },
      };

      match editor.update(cx, |editor, cx| editor.save_as(path.clone(), cx)) {
        Ok(Ok(())) => {
          let _ = workspace.update(cx, |workspace, cx| {
            if let Some(panel) = workspace
              .document_panels
              .iter()
              .find(|panel| panel.read(cx).id() == panel_id)
            {
              panel.update(cx, |panel, cx| panel.set_path(path, cx));
            }
            cx.notify();
          });
        },
        Ok(Err(error)) => {
          let detail = error.to_string();
          let _ = window_handle.update(cx, |_, window, cx| {
            window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
          });
        },
        Err(error) => {
          eprintln!("failed to access editor before save: {error}");
        },
      }
    })
    .detach();
  }

}
