impl RichTextEditor {
  fn paragraph_chunk_containing_byte(&self, paragraph_ix: usize, byte: usize, width: Pixels) -> Option<(usize, Rc<LayoutState>)> {
    let paragraph = self.document.paragraphs.get(paragraph_ix)?;
    let paragraph_len = paragraph_text_len(paragraph);
    let key = paragraph_cache_key(&self.document, paragraph);
    self
      .paragraph_chunk_layout_cache
      .get(paragraph_ix)
      .and_then(|entry| entry.as_ref())
      .filter(|entry| entry.key == key && entry.width == width && entry.invisibility_mode == self.invisibility_mode)
      .and_then(|entry| {
        entry
          .chunks
          .iter()
          .enumerate()
          .find(|(_, chunk)| byte >= chunk.start_byte && (byte < chunk.end_byte || (byte == chunk.end_byte && chunk.end_byte == paragraph_len)))
          .map(|(ix, chunk)| (ix, chunk.layout.clone()))
      })
  }

  fn ensure_paragraph_chunk_containing_byte(
    &mut self,
    paragraph_ix: usize,
    byte: usize,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<usize> {
    loop {
      if let Some((chunk_ix, _)) = self.paragraph_chunk_containing_byte(paragraph_ix, byte, width) {
        return Some(chunk_ix);
      }
      let before_len = self
        .paragraph_chunk_layout_cache
        .get(paragraph_ix)
        .and_then(|entry| entry.as_ref())
        .map(|entry| entry.chunks.len())
        .unwrap_or(0);
      if !self.ensure_next_paragraph_chunk(paragraph_ix, width, window, cx) {
        return None;
      }
      let after = self
        .paragraph_chunk_layout_cache
        .get(paragraph_ix)
        .and_then(|entry| entry.as_ref())?;
      if after.complete && after.chunks.len() == before_len {
        return self
          .paragraph_chunk_containing_byte(paragraph_ix, byte, width)
          .map(|(chunk_ix, _)| chunk_ix);
      }
    }
  }

  fn ensure_vertical_navigation_chunks(&mut self, head: DocumentOffset, dir: VDir, width: Pixels, window: &mut Window, cx: &mut Context<Self>) {
    let Some(chunk_ix) = self.ensure_paragraph_chunk_containing_byte(head.paragraph, head.byte, width, window, cx) else {
      return;
    };
    match dir {
      VDir::Down => {
        let needs_next_chunk = self
          .paragraph_chunk_layout_cache
          .get(head.paragraph)
          .and_then(|entry| entry.as_ref())
          .is_some_and(|entry| chunk_ix + 1 >= entry.chunks.len() && !entry.complete);
        if needs_next_chunk {
          self.ensure_next_paragraph_chunk(head.paragraph, width, window, cx);
        }
      },
      VDir::Up => {
        if chunk_ix == 0
          && let Some(prev) = head.paragraph.checked_sub(1)
        {
          self.ensure_next_paragraph_chunk(prev, width, window, cx);
        }
      },
    }
  }

  fn paragraph_remainder_estimate(&self, paragraph_ix: usize, width: Pixels) -> Pixels {
    let estimated_total = estimate_paragraph_item_height_with_visibility(&self.document, paragraph_ix, width, self.invisibility_mode);
    let exact_height = self
      .paragraph_chunk_layout_cache
      .get(paragraph_ix)
      .and_then(|entry| entry.as_ref())
      .map(|entry| entry.exact_height)
      .unwrap_or(px(0.0));
    let remaining = (estimated_total - exact_height).max(self.document.theme.body_font_size * self.document.theme.line_spacing);
    let text_len = self
      .document
      .paragraphs
      .get(paragraph_ix)
      .map(paragraph_text_len)
      .unwrap_or(0);
    if text_len > 16 * 1024 || estimated_total > self.scroll_handle.bounds().size.height.max(px(700.0)) * 1.5 {
      remaining.max(self.scroll_handle.bounds().size.height.max(px(700.0)) + px(1024.0))
    } else {
      remaining
    }
  }

  fn ensure_exact_interaction_chunks(&mut self, width: Pixels, window: &mut Window, cx: &mut Context<Self>) {
    let paragraph_count = self.document.paragraphs.len();
    if paragraph_count == 0 {
      return;
    }

    let mut ranges = vec![self.predicted_visible_height_range(width), self.active_height_range()];
    if !self.visible_layout_range.is_empty() {
      let visible_paragraph_range = self.paragraph_range_for_item_range(self.visible_layout_range.clone());
      ranges.push(expand_paragraph_range(visible_paragraph_range, paragraph_count, 2));
    }

    let mut queued = vec![false; paragraph_count];
    for range in ranges {
      for paragraph_ix in range {
        if paragraph_ix >= paragraph_count || queued[paragraph_ix] || !self.paragraph_visible_in_current_mode(paragraph_ix) {
          continue;
        }
        queued[paragraph_ix] = true;
        self.ensure_next_paragraph_chunk(paragraph_ix, width, window, cx);
      }
    }
  }

  fn ensure_exact_initial_viewport_chunks(&mut self, width: Pixels, window: &mut Window, cx: &mut Context<Self>) {
    let paragraph_count = self.document.paragraphs.len();
    if paragraph_count == 0 {
      return;
    }

    let viewport_height = self.scroll_handle.bounds().size.height.max(px(700.0));
    let target_height = viewport_height + px(512.0);
    let mut accumulated = px(0.0);

    for paragraph_ix in 0..paragraph_count {
      if !self.paragraph_visible_in_current_mode(paragraph_ix) {
        continue;
      }
      loop {
        let before = self
          .paragraph_chunk_layout_cache
          .get(paragraph_ix)
          .and_then(|entry| entry.as_ref())
          .map(|entry| entry.chunks.len())
          .unwrap_or(0);
        if !self.ensure_next_paragraph_chunk(paragraph_ix, width, window, cx) {
          break;
        }
        let Some(entry) = self
          .paragraph_chunk_layout_cache
          .get(paragraph_ix)
          .and_then(|entry| entry.as_ref())
        else {
          break;
        };
        if let Some(chunk) = entry.chunks.get(before) {
          accumulated += chunk.height;
        }
        if accumulated >= target_height || entry.complete {
          break;
        }
      }
      if accumulated >= target_height {
        break;
      }
    }
  }

}
