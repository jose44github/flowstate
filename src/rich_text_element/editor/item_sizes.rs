impl RichTextEditor {
  fn paragraph_item_sizes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Rc<Vec<Size<Pixels>>> {
    self
      .paragraph_height_cache
      .resize(self.document.paragraphs.len(), None);
    self
      .paragraph_chunk_layout_cache
      .resize(self.document.paragraphs.len(), None);
    let viewport_width = self.scroll_handle.bounds().size.width;
    let has_measured_viewport = viewport_width > px(1.0);
    if !has_measured_viewport {
      self.schedule_viewport_size_refresh(window, cx);
    }
    let width = self
      .measured_item_width
      .unwrap_or(if has_measured_viewport { viewport_width } else { px(900.0) });
    if has_measured_viewport && self.initial_layout_hidden {
      self.ensure_exact_initial_viewport_chunks(width, window, cx);
    }
    if let Some(cache) = &self.item_sizes_cache
      && cache.width == width
      && cache.block_count == self.document.blocks.len()
      && cache.invisibility_mode == self.invisibility_mode
      && cache.height_revision == self.paragraph_height_cache_revision
      && self.height_prefix_index.len() == cache.item_count
    {
      let sizes = cache.sizes.clone();
      self.maybe_resume_chunk_prefetch_after_typing(width, window, cx);
      return sizes;
    }
    let scroll_anchor = self.capture_scroll_anchor();
    self.ensure_exact_interaction_chunks(width, window, cx);
    if let Some(cache) = &self.item_sizes_cache
      && cache.width == width
      && cache.block_count == self.document.blocks.len()
      && cache.invisibility_mode == self.invisibility_mode
      && cache.height_revision == self.paragraph_height_cache_revision
      && self.height_prefix_index.len() == cache.item_count
    {
      let sizes = cache.sizes.clone();
      self.maybe_resume_chunk_prefetch_after_typing(width, window, cx);
      return sizes;
    }
    if let Some(sizes) = self.try_patch_item_sizes_cache(width, scroll_anchor.clone(), window, cx) {
      return sizes;
    }
    self.rebuild_item_sizes_cache(width, scroll_anchor, window, cx)
  }

  fn rebuild_item_sizes_cache(
    &mut self,
    width: Pixels,
    scroll_anchor: Option<ScrollAnchorSnapshot>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Rc<Vec<Size<Pixels>>> {
    self.rebuild_item_sizes_cache_with_prefetch(width, scroll_anchor, true, window, cx)
  }

  fn rebuild_item_sizes_cache_with_prefetch(
    &mut self,
    width: Pixels,
    scroll_anchor: Option<ScrollAnchorSnapshot>,
    schedule_prefetch: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Rc<Vec<Size<Pixels>>> {
    let (items, block_item_ranges, block_heights, sizes) = self.virtual_item_sizes(width, window, cx);
    let (paragraph_chunk_item_ranges, paragraph_remainder_items) = item_lookup_for_virtual_items(items.as_ref(), self.document.paragraphs.len());
    self.height_prefix_index.rebuild(sizes.as_ref());
    let item_count = sizes.len();
    self.pending_item_sizes_patch_range = None;
    self.item_sizes_cache = Some(ItemSizesCache {
      width,
      block_count: self.document.blocks.len(),
      item_count,
      invisibility_mode: self.invisibility_mode,
      height_revision: self.paragraph_height_cache_revision,
      items,
      block_item_ranges,
      block_heights,
      paragraph_chunk_item_ranges,
      paragraph_remainder_items,
      sizes: sizes.clone(),
    });
    self.restore_scroll_anchor(scroll_anchor);
    if schedule_prefetch {
      self.schedule_chunk_prefetch(width, window, cx);
    }
    sizes
  }

  fn try_patch_item_sizes_cache(
    &mut self,
    width: Pixels,
    scroll_anchor: Option<ScrollAnchorSnapshot>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<Rc<Vec<Size<Pixels>>>> {
    let range = self.pending_item_sizes_patch_range.clone()?;
    let paragraph_count = self.document.paragraphs.len();
    if range.start > range.end || range.end > paragraph_count || self.document_has_object_blocks() {
      return None;
    }

    let cache = self.item_sizes_cache.as_ref()?;
    if cache.width != width
      || cache.block_count != self.document.blocks.len()
      || cache.invisibility_mode != self.invisibility_mode
      || cache.paragraph_chunk_item_ranges.len() != paragraph_count
      || cache.paragraph_remainder_items.len() != paragraph_count
      || cache.block_item_ranges.len() != self.document.blocks.len()
      || cache.block_heights.len() != self.document.blocks.len()
      || self.height_prefix_index.len() != cache.item_count
    {
      return None;
    }

    let replace_start = cache
      .block_item_ranges
      .get(range.start)
      .map_or(cache.item_count, |range| range.start);
    let replace_end = if range.end == 0 {
      0
    } else {
      cache.block_item_ranges.get(range.end - 1)?.end
    };
    if replace_start > replace_end || replace_end > cache.item_count {
      return None;
    }

    let (replacement_items, replacement_block_ranges, replacement_block_heights, replacement_sizes) =
      self.virtual_item_sizes_for_paragraph_range(range.clone(), width, window, cx)?;
    let old_len = replace_end - replace_start;
    let new_len = replacement_items.len();
    let item_delta = new_len as isize - old_len as isize;

    let patched_sizes = {
      let cache = self.item_sizes_cache.as_mut()?;
      let items = Rc::make_mut(&mut cache.items);
      let sizes = Rc::make_mut(&mut cache.sizes);
      items.splice(replace_start..replace_end, replacement_items);
      sizes.splice(replace_start..replace_end, replacement_sizes.clone());

      for block_ix in range.clone() {
        let relative = &replacement_block_ranges[block_ix - range.start];
        cache.block_item_ranges[block_ix] = replace_start + relative.start..replace_start + relative.end;
        cache.block_heights[block_ix] = replacement_block_heights[block_ix - range.start];
      }
      if item_delta != 0 {
        for block_range in cache.block_item_ranges.iter_mut().skip(range.end) {
          block_range.start = block_range.start.checked_add_signed(item_delta)?;
          block_range.end = block_range.end.checked_add_signed(item_delta)?;
        }
      }

      let (paragraph_chunk_item_ranges, paragraph_remainder_items) = item_lookup_for_virtual_items(&items[..], paragraph_count);
      cache.paragraph_chunk_item_ranges = paragraph_chunk_item_ranges;
      cache.paragraph_remainder_items = paragraph_remainder_items;
      cache.item_count = sizes.len();
      cache.height_revision = self.paragraph_height_cache_revision;
      cache.sizes.clone()
    };
    if !self
      .height_prefix_index
      .replace_range(replace_start..replace_end, &replacement_sizes)
    {
      return None;
    }
    self.pending_item_sizes_patch_range = None;
    self.restore_scroll_anchor(scroll_anchor);
    self.schedule_chunk_prefetch(width, window, cx);
    Some(patched_sizes)
  }

  fn virtual_item_sizes_for_paragraph_range(
    &mut self,
    range: Range<usize>,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<(Vec<VirtualItem>, Vec<Range<usize>>, Vec<Pixels>, Vec<Size<Pixels>>)> {
    let mut items = Vec::with_capacity(range.len());
    let mut sizes = Vec::with_capacity(range.len());
    let mut block_item_ranges = Vec::with_capacity(range.len());
    let mut block_heights = Vec::with_capacity(range.len());

    for paragraph_ix in range {
      let paragraph = self.document.paragraphs.get(paragraph_ix)?.clone();
      if !matches!(self.document.blocks.get(paragraph_ix), Some(Block::Paragraph(_))) {
        return None;
      }

      let block_start = items.len();
      let mut block_height = px(0.0);
      if self.invisibility_mode && !paragraph_is_visible(&paragraph) {
        block_item_ranges.push(block_start..items.len());
        block_heights.push(px(0.0));
        continue;
      }

      self.ensure_current_chunk_cache_entry(paragraph_ix, width);
      let complete = self
        .paragraph_chunk_layout_cache
        .get(paragraph_ix)
        .and_then(|entry| entry.as_ref())
        .map(|entry| {
          for (chunk_ix, chunk) in entry.chunks.iter().enumerate() {
            items.push(VirtualItem::ParagraphChunk {
              block_ix: paragraph_ix,
              paragraph_ix,
              chunk_ix,
            });
            sizes.push(size(width, chunk.height));
            block_height += chunk.height;
          }
          entry.complete
        })
        .unwrap_or(false);

      if !complete {
        let estimate = self.paragraph_remainder_estimate(paragraph_ix, width);
        items.push(VirtualItem::ParagraphRemainder {
          block_ix: paragraph_ix,
          paragraph_ix,
        });
        sizes.push(size(width, estimate));
        block_height += estimate;
      } else if block_start == items.len() {
        self.ensure_next_paragraph_chunk(paragraph_ix, width, window, cx);
        if let Some(chunk) = self
          .paragraph_chunk_layout_cache
          .get(paragraph_ix)
          .and_then(|entry| entry.as_ref())
          .and_then(|entry| entry.chunks.first())
        {
          items.push(VirtualItem::ParagraphChunk {
            block_ix: paragraph_ix,
            paragraph_ix,
            chunk_ix: 0,
          });
          sizes.push(size(width, chunk.height));
          block_height += chunk.height;
        }
      }

      block_item_ranges.push(block_start..items.len());
      block_heights.push(block_height);
    }

    Some((items, block_item_ranges, block_heights, sizes))
  }

  pub(crate) fn benchmark_paragraph_item_sizes(
    &mut self,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> ItemSizeBenchmarkResult {
    self.measured_item_width = Some(width);
    let cache_hit = self.item_sizes_cache.as_ref().is_some_and(|cache| {
      cache.width == width
        && cache.block_count == self.document.blocks.len()
        && cache.invisibility_mode == self.invisibility_mode
        && cache.height_revision == self.paragraph_height_cache_revision
    });
    let start = Instant::now();
    let sizes = self.paragraph_item_sizes(window, cx);
    let elapsed = start.elapsed();
    let exact_height_count = self
      .paragraph_chunk_layout_cache
      .iter()
      .filter_map(|entry| entry.as_ref())
      .map(|entry| entry.chunks.len())
      .sum();
    let total_height = sizes
      .iter()
      .map(|size| {
        let height: f32 = size.height.into();
        height
      })
      .sum();
    ItemSizeBenchmarkResult {
      elapsed,
      cache_hit,
      item_count: sizes.len(),
      exact_height_count,
      total_height,
    }
  }

  pub(crate) fn benchmark_invalidate_document_layout_caches(&mut self) {
    self.invalidate_document_layout_caches();
  }

  fn virtual_item_sizes(
    &mut self,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> (Rc<Vec<VirtualItem>>, Vec<Range<usize>>, Vec<Pixels>, Rc<Vec<Size<Pixels>>>) {
    let block_count = self.document.blocks.len();
    let mut items = Vec::with_capacity(block_count);
    let mut sizes = Vec::with_capacity(block_count);
    let mut block_item_ranges = Vec::with_capacity(block_count);
    let mut block_heights = Vec::with_capacity(block_count);
    let mut paragraph_ix = 0usize;

    for block_ix in 0..block_count {
      let block_start = items.len();
      let mut block_height = px(0.0);

      match self.document.blocks.get(block_ix) {
        Some(Block::Paragraph(paragraph)) => {
          let current_paragraph_ix = paragraph_ix;
          paragraph_ix += 1;
          if self.invisibility_mode && !paragraph_is_visible(paragraph) {
            block_item_ranges.push(block_start..items.len());
            block_heights.push(px(0.0));
            continue;
          }
          self.ensure_current_chunk_cache_entry(current_paragraph_ix, width);
          let complete = self
            .paragraph_chunk_layout_cache
            .get(current_paragraph_ix)
            .and_then(|entry| entry.as_ref())
            .map(|entry| {
              for (chunk_ix, chunk) in entry.chunks.iter().enumerate() {
                items.push(VirtualItem::ParagraphChunk {
                  block_ix,
                  paragraph_ix: current_paragraph_ix,
                  chunk_ix,
                });
                sizes.push(size(width, chunk.height));
                block_height += chunk.height;
              }
              entry.complete
            })
            .unwrap_or(false);

          if !complete {
            let estimate = self.paragraph_remainder_estimate(current_paragraph_ix, width);
            items.push(VirtualItem::ParagraphRemainder {
              block_ix,
              paragraph_ix: current_paragraph_ix,
            });
            sizes.push(size(width, estimate));
            block_height += estimate;
          } else if block_start == items.len() {
            // A complete empty paragraph still needs one exact row.
            self.ensure_next_paragraph_chunk(current_paragraph_ix, width, window, cx);
            if let Some(chunk) = self
              .paragraph_chunk_layout_cache
              .get(current_paragraph_ix)
              .and_then(|entry| entry.as_ref())
              .and_then(|entry| entry.chunks.first())
            {
              items.push(VirtualItem::ParagraphChunk {
                block_ix,
                paragraph_ix: current_paragraph_ix,
                chunk_ix: 0,
              });
              sizes.push(size(width, chunk.height));
              block_height += chunk.height;
            }
          }
        },
        Some(Block::Image(_) | Block::Equation(_) | Block::Table(_)) => {
          if self.invisibility_mode {
            block_item_ranges.push(block_start..items.len());
            block_heights.push(px(0.0));
            continue;
          }
          let height = layout_structural_block_at(&self.document, block_ix, width, px(0.0), window, cx)
            .as_ref()
            .map(structural_block_height)
            .unwrap_or_else(|| estimate_structural_block_item_height(&self.document, block_ix, width))
            + self.document.theme.paragraph_after;
          items.push(VirtualItem::StructuralBlock { block_ix });
          sizes.push(size(width, height));
          block_height += height;
        },
        None => {
          items.push(VirtualItem::HiddenBlock { block_ix });
          sizes.push(size(width, px(0.0)));
        },
      }
      block_item_ranges.push(block_start..items.len());
      block_heights.push(block_height);
    }

    (Rc::new(items), block_item_ranges, block_heights, Rc::new(sizes))
  }

}
