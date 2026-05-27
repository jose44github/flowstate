impl Workspace {
  pub fn toggle_ribbon(&mut self, cx: &mut Context<Self>) {
    self.ribbon_collapsed = !self.ribbon_collapsed;
    cx.notify();
  }

  pub fn toggle_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.prepare_active_editor_for_container_resize(self.expected_document_width_after_outline_toggle(cx), window, cx);
    self.outline_collapsed = !self.outline_collapsed;
    cx.notify();
  }

  pub fn toggle_toolkit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.prepare_active_editor_for_container_resize(self.expected_document_width_after_toolkit_toggle(cx), window, cx);
    self.toolkit_collapsed = !self.toolkit_collapsed;
    cx.notify();
  }

  fn prepare_active_editor_for_container_resize(&mut self, expected_width: Option<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(editor) = self.active_editor.clone() else {
      return;
    };
    editor.update(cx, |editor, cx| editor.prepare_for_container_resize(expected_width, window, cx));
  }

  fn current_document_panel_width(&self, cx: &mut Context<Self>) -> Option<Pixels> {
    self
      .content_resizable_state
      .read(cx)
      .sizes()
      .first()
      .copied()
  }

  fn expected_document_width_after_outline_toggle(&self, cx: &mut Context<Self>) -> Option<Pixels> {
    let current_document_width = self.current_document_panel_width(cx)?;
    let outline_sizes = self.body_resizable_state.read(cx).sizes().clone();
    let current_outline_width = outline_sizes.first().copied().unwrap_or(px(240.0));
    let delta = if self.outline_collapsed {
      px(30.0) - px(240.0)
    } else {
      current_outline_width - px(30.0)
    };
    Some((current_document_width + delta).max(px(360.0)))
  }

  fn expected_document_width_after_toolkit_toggle(&self, cx: &mut Context<Self>) -> Option<Pixels> {
    let sizes = self.content_resizable_state.read(cx).sizes().clone();
    let current_document_width = sizes.first().copied()?;
    let current_toolkit_width = sizes.get(1).copied().unwrap_or(px(300.0));
    let delta = if self.toolkit_collapsed {
      px(30.0) - px(300.0)
    } else {
      current_toolkit_width - px(30.0)
    };
    Some((current_document_width + delta).max(px(360.0)))
  }

  fn refresh_outline_tree(&mut self, cx: &mut Context<Self>) {
    let Some(active_id) = self.active_document_id else {
      if self.outline_cache.is_some() {
        self.outline_cache = None;
        self
          .outline_tree
          .update(cx, |tree, cx| tree.set_items(Vec::<TreeItem>::new(), cx));
      }
      return;
    };
    let Some(editor) = &self.active_editor else {
      return;
    };
    let document = editor.read(cx).document().clone();
    let signature = outline_signature(&document);
    if self
      .outline_cache
      .as_ref()
      .is_some_and(|cache| cache.document_id == active_id && cache.signature == signature && cache.visible_revision == self.outline_revision)
    {
      return;
    }

    let rebuild_structure = self
      .outline_cache
      .as_ref()
      .is_none_or(|cache| cache.document_id != active_id || cache.signature != signature);
    if rebuild_structure {
      self.outline_cache = Some(OutlineCache::new(active_id, signature));
    }
    let Some(cache) = self.outline_cache.as_mut() else {
      return;
    };
    if cache.visible_revision != self.outline_revision {
      cache.rebuild_visible(self.outline_revision, &self.collapsed_outline_items);
    }
    let items = cache.tree_items.clone();
    self
      .outline_tree
      .update(cx, |tree, cx| tree.set_items(items, cx));
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
    self.outline_scrolled_paragraph = None;
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
    let Some(panel) = self
      .document_panels
      .iter()
      .find(|panel| panel.read(cx).id() == panel_id)
    else {
      return;
    };
    self.active_document_id = Some(panel_id);
    self.active_editor = Some(panel.read(cx).editor());
    cx.notify();
  }

  fn active_document_index(&self, cx: &App) -> Option<usize> {
    let active_id = self.active_document_id?;
    self
      .document_panels
      .iter()
      .position(|panel| panel.read(cx).id() == active_id)
  }

  fn activate_document_at_index(&mut self, index: usize, cx: &mut Context<Self>) {
    let Some(panel) = self.document_panels.get(index) else {
      return;
    };
    let panel = panel.read(cx);
    self.active_document_id = Some(panel.id());
    self.active_editor = Some(panel.editor());
    cx.notify();
  }

  fn navigate_active_tab(&mut self, offset: isize, cx: &mut Context<Self>) {
    let Some(active_index) = self.active_document_index(cx) else {
      return;
    };
    let target = if offset.is_negative() {
      active_index.checked_sub(offset.unsigned_abs())
    } else {
      active_index.checked_add(offset as usize)
    };
    if let Some(target) = target.filter(|target| *target < self.document_panels.len()) {
      self.activate_document_at_index(target, cx);
    }
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
        let label = if dirty { format!("*{title}").into() } else { title.into() };
        DocumentTab {
          id: panel.id(),
          label,
          active: Some(panel.id()) == self.active_document_id,
        }
      })
      .collect()
  }

  fn active_outline_paragraph(&self, cx: &App) -> Option<usize> {
    let viewport_paragraph = self.active_editor_viewport_paragraph(cx)?;
    let cache = self.outline_cache.as_ref()?;
    active_visible_outline_paragraph_from_visible(&cache.visible_paragraphs, viewport_paragraph)
  }

  fn active_editor_viewport_paragraph(&self, cx: &App) -> Option<usize> {
    self
      .active_editor
      .as_ref()
      .and_then(|editor| editor.read(cx).viewport_anchor_paragraph())
  }

  fn refresh_outline_viewport(&mut self, cx: &mut Context<Self>) {
    let viewport_paragraph = self.active_editor_viewport_paragraph(cx);
    if self.outline_viewport_paragraph != viewport_paragraph {
      self.outline_viewport_paragraph = viewport_paragraph;
      cx.notify();
    }
  }

  fn scroll_outline_item_into_view(&mut self, paragraph_ix: Option<usize>, cx: &mut Context<Self>) {
    let Some(paragraph_ix) = paragraph_ix else {
      return;
    };
    if self.outline_scrolled_paragraph == Some(paragraph_ix) {
      return;
    }
    let id = outline_item_id(paragraph_ix);
    self.outline_tree.update(cx, |tree, _| {
      if let Some(ix) = tree.item_index_by_id(&id) {
        tree.scroll_to_item(ix, gpui::ScrollStrategy::Center);
      }
    });
    self.outline_scrolled_paragraph = Some(paragraph_ix);
  }
}
