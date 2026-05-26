fn item_lookup_for_virtual_items(items: &[VirtualItem], paragraph_count: usize) -> (Vec<Range<usize>>, Vec<Option<usize>>) {
  let mut paragraph_chunk_item_ranges = vec![0..0; paragraph_count];
  let mut paragraph_remainder_items = vec![None; paragraph_count];

  for (item_ix, item) in items.iter().enumerate() {
    match item {
      VirtualItem::ParagraphChunk {
        paragraph_ix, chunk_ix: _, ..
      } => {
        if let Some(range) = paragraph_chunk_item_ranges.get_mut(*paragraph_ix) {
          if range.start == range.end {
            *range = item_ix..item_ix + 1;
          } else {
            range.end = range.end.max(item_ix + 1);
          }
        }
      },
      VirtualItem::ParagraphRemainder { paragraph_ix, .. } => {
        if let Some(slot) = paragraph_remainder_items.get_mut(*paragraph_ix) {
          *slot = Some(item_ix);
        }
      },
      VirtualItem::HiddenBlock { .. } | VirtualItem::StructuralBlock { .. } => {},
    }
  }

  (paragraph_chunk_item_ranges, paragraph_remainder_items)
}

fn expand_paragraph_range(range: Range<usize>, paragraph_count: usize, padding: usize) -> Range<usize> {
  if paragraph_count == 0 {
    return 0..0;
  }
  let start = range.start.saturating_sub(padding).min(paragraph_count);
  let end = range
    .end
    .saturating_add(padding)
    .min(paragraph_count)
    .max(start);
  start..end
}

fn byte_at_ratio_in_paragraph(document: &Document, paragraph_ix: usize, start_byte: usize, end_byte: usize, ratio: f32) -> usize {
  let Some(paragraph) = document.paragraphs.get(paragraph_ix) else {
    return 0;
  };
  let start = start_byte.min(paragraph_text_len(paragraph));
  let end = end_byte.min(paragraph_text_len(paragraph)).max(start);
  if start == end {
    return start;
  }
  let target = start + ((end - start) as f32 * ratio.clamp(0.0, 1.0)).round() as usize;
  let text = paragraph_text(document, paragraph_ix);
  floor_char_boundary(&text, target.min(text.len()))
}

fn detach_document_for_background_write(document: &Document) -> Document {
  Document {
    text: document.text.clone(),
    paragraphs: Arc::new(document.paragraphs.as_ref().clone()),
    blocks: Arc::new(document.blocks.as_ref().clone()),
    assets: document.assets.clone(),
    offset_index: document.offset_index.clone(),
    theme: document.theme.clone(),
  }
}

fn floor_char_boundary(text: &str, mut byte: usize) -> usize {
  byte = byte.min(text.len());
  while byte > 0 && !text.is_char_boundary(byte) {
    byte -= 1;
  }
  byte
}

