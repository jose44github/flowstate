#[hotpath::measure_all]
impl RichTextEditor {
  pub fn toggle_underline(&mut self, cx: &mut Context<Self>) {
    if self.clear_matching_armed_inline_tool(ArmedInlineTool::Underline, cx) {
      return;
    }
    self.toggle_underline_kind(None, cx);
  }

  pub fn toggle_strikethrough(&mut self, cx: &mut Context<Self>) {
    if self.clear_matching_armed_inline_tool(ArmedInlineTool::Strikethrough, cx) {
      return;
    }
    if let Some(BlockSelection::TableCell { block_ix, row_ix, cell_ix }) = self.selected_block {
      let Some(selection_range) = self.table_cell_selection_range() else {
        self.armed_inline_tool = Some(ArmedInlineTool::Strikethrough);
        cx.notify();
        return;
      };
      let all_selected = self
        .selected_table_cell_paragraph()
        .map(|paragraph| table_cell_range_all_run_styles(paragraph, selection_range.clone(), |styles| styles.strikethrough))
        .unwrap_or(false);
      self.edit_table_cell_paragraph(block_ix, row_ix, cell_ix, cx, |paragraph| {
        if paragraph.paragraph.runs.is_empty() && !paragraph.text.is_empty() {
          paragraph.paragraph.runs.push(TextRun {
            len: paragraph.text.len(),
            styles: RunStyles::default(),
          });
        }
        mutate_table_cell_runs_in_range(paragraph, selection_range, |styles| styles.strikethrough = !all_selected);
      });
      return;
    }
    if self.selection.is_caret() {
      let mut styles = self.styles_at_caret();
      styles.strikethrough = !styles.strikethrough;
      self.pending_styles = Some(styles);
      cx.notify();
      return;
    }
    let range = self.selection.normalized();
    let all_selected = selection_all_run_styles(&self.document, range.clone(), |styles| styles.strikethrough);
    self.apply_document_edit(cx, |editor, cx| {
      mutate_runs_in_range(&mut editor.document, range, |styles| styles.strikethrough = !all_selected);
      editor.after_formatting_mutation(cx);
    });
  }

  /// Toggle any semantic inline style for the current selection or caret.
  ///
  /// The ribbon can call this generic method instead of matching each style to
  /// a shortcut-specific wrapper like `toggle_cite` or `toggle_emphasis`.
  pub fn toggle_semantic_style_for_selection(&mut self, semantic: RunSemanticStyle, cx: &mut Context<Self>) {
    if self.clear_matching_armed_inline_tool(ArmedInlineTool::Semantic(semantic), cx) {
      return;
    }
    self.toggle_semantic_style(semantic, cx);
  }

  pub fn toggle_emphasis(&mut self, cx: &mut Context<Self>) {
    self.toggle_semantic_style(RunSemanticStyle::Emphasis, cx);
  }

  pub fn toggle_cite(&mut self, cx: &mut Context<Self>) {
    self.toggle_semantic_style(RunSemanticStyle::Cite, cx);
  }

  pub fn toggle_condensed(&mut self, cx: &mut Context<Self>) {
    if self.selection.is_caret() && self.table_cell_selection_range().is_none() {
      self.apply_semantic_style_to_card_span(RunSemanticStyle::Condensed, cx);
      return;
    }
    self.toggle_semantic_style(RunSemanticStyle::Condensed, cx);
  }

  pub fn toggle_ultracondensed(&mut self, cx: &mut Context<Self>) {
    if self.selection.is_caret() && self.table_cell_selection_range().is_none() {
      self.apply_semantic_style_to_card_span(RunSemanticStyle::Ultracondensed, cx);
      return;
    }
    self.toggle_semantic_style(RunSemanticStyle::Ultracondensed, cx);
  }

  pub(super) fn apply_semantic_style_to_card_span(&mut self, semantic: RunSemanticStyle, cx: &mut Context<Self>) {
    if !matches!(semantic, RunSemanticStyle::Condensed | RunSemanticStyle::Ultracondensed) {
      return;
    }
    let Some(target_block_ix) = condensed_card_target_block_ix(&self.document, self.current_block_ix_for_condensed_card()) else {
      return;
    };
    let Some(span) = condensed_card_block_span(&self.document, target_block_ix) else {
      return;
    };

    let before_document = self.document.clone();
    let before_selection = self.selection.clone();
    let clear_semantic = condensed_card_span_all_eligible_runs_have_semantic(&self.document, span.clone(), semantic);
    let mut paragraph_ix = 0usize;
    for block_ix in 0..self.document.blocks.len() {
      let in_span = span.contains(&block_ix);
      match self.document.blocks.get(block_ix) {
        Some(Block::Paragraph(_)) => {
          if in_span {
            let changed = {
              let Some(paragraph) = paragraphs_mut(&mut self.document).get_mut(paragraph_ix) else {
                paragraph_ix += 1;
                continue;
              };
              apply_condensed_semantic_to_paragraph(paragraph, semantic, clear_semantic)
            };
            if changed {
              update_paragraph_block(&mut self.document, paragraph_ix);
            }
          }
          paragraph_ix += 1;
        },
        Some(Block::Table(_)) if in_span => {
          if let Some(Block::Table(table)) = Arc::make_mut(&mut self.document.blocks).get_mut(block_ix)
            && apply_condensed_semantic_to_table(table, semantic, clear_semantic)
          {
            table.version = table.version.wrapping_add(1);
          }
        },
        Some(Block::Image(_) | Block::Equation(_) | Block::Table(_)) | None => {},
      }
    }

    self.push_replace_document_history(before_document, before_selection, cx);
  }

  fn current_block_ix_for_condensed_card(&self) -> Option<usize> {
    match self.selected_block {
      Some(BlockSelection::TableCell { block_ix, .. }) | Some(BlockSelection::Table(block_ix)) => Some(block_ix),
      Some(BlockSelection::Equation(block_ix) | BlockSelection::Image(block_ix)) => Some(block_ix),
      None => block_ix_for_paragraph(&self.document, self.selection.head.paragraph),
    }
  }

  pub fn set_highlight(&mut self, highlight: HighlightStyle, cx: &mut Context<Self>) {
    self.current_highlight_style = highlight;
    self.current_highlight_choice = Some(highlight);
    if self.clear_matching_armed_inline_tool(ArmedInlineTool::Highlight(highlight), cx) {
      return;
    }
    self.set_highlight_internal(Some(highlight), cx);
  }

  /// Set or clear the highlight style for the current selection or caret.
  ///
  /// `None` clears highlights. `Some(...)` applies the requested highlight, or
  /// toggles it off when the whole selection already has that highlight.
  pub fn set_highlight_for_selection(&mut self, highlight: Option<HighlightStyle>, cx: &mut Context<Self>) {
    self.set_highlight_internal(highlight, cx);
  }

  pub fn clear_highlight(&mut self, cx: &mut Context<Self>) {
    self.set_highlight_internal(None, cx);
  }

  pub fn clear_formatting(&mut self, cx: &mut Context<Self>) {
    if let Some(BlockSelection::TableCell { block_ix, row_ix, cell_ix }) = self.selected_block {
      self.edit_table_cell_paragraph(block_ix, row_ix, cell_ix, cx, |paragraph| {
        paragraph.paragraph.style = ParagraphStyle::Normal;
        for run in &mut paragraph.paragraph.runs {
          run.styles = RunStyles::default();
        }
        paragraph.paragraph.runs = merge_adjacent_runs(std::mem::take(&mut paragraph.paragraph.runs));
        paragraph.paragraph.version = paragraph.paragraph.version.wrapping_add(1);
      });
      return;
    }
    self.apply_document_edit(cx, |editor, cx| {
      if editor.selection.is_caret() {
        let paragraph_ix = editor.selection.head.paragraph;
        clear_whole_paragraph_formatting(&mut editor.document, paragraph_ix);
      } else {
        let range = editor.selection.normalized();
        if selection_contains_whole_paragraph(&editor.document, range.clone()) {
          for paragraph_ix in range.start.paragraph..=range.end.paragraph {
            clear_whole_paragraph_formatting(&mut editor.document, paragraph_ix);
          }
        } else {
          mutate_runs_in_range(&mut editor.document, range, |styles| *styles = RunStyles::default());
        }
      }
      editor.pending_styles = None;
      editor.after_formatting_mutation(cx);
    });
  }

  pub fn apply_run_style_to_selection(&mut self, style: RunStyle, cx: &mut Context<Self>) {
    if let Some(BlockSelection::TableCell { block_ix, row_ix, cell_ix }) = self.selected_block {
      let Some(selection_range) = self.table_cell_selection_range() else {
        return;
      };
      self.edit_table_cell_paragraph(block_ix, row_ix, cell_ix, cx, |paragraph| {
        if paragraph.text.is_empty() {
          return;
        }
        if paragraph.paragraph.runs.is_empty() {
          paragraph.paragraph.runs.push(TextRun {
            len: paragraph.text.len(),
            styles: RunStyles::default(),
          });
        }
        mutate_table_cell_runs_in_range(paragraph, selection_range.clone(), |styles| styles.apply(style));
        paragraph.paragraph.runs = merge_adjacent_runs(std::mem::take(&mut paragraph.paragraph.runs));
        paragraph.paragraph.version = paragraph.paragraph.version.wrapping_add(1);
      });
      return;
    }
    if self.selection.is_caret() {
      return;
    }
    self.apply_document_edit(cx, |editor, cx| {
      let range = editor.selection.normalized();
      for paragraph_ix in range.start.paragraph..=range.end.paragraph {
        let start = if paragraph_ix == range.start.paragraph { range.start.byte } else { 0 };
        let end = if paragraph_ix == range.end.paragraph {
          range.end.byte
        } else {
          paragraph_text_len(&editor.document.paragraphs[paragraph_ix])
        };
        apply_style_to_paragraph_range(&mut editor.document, paragraph_ix, start..end, style);
      }
      editor.after_formatting_mutation(cx);
    });
  }

  pub fn set_paragraph_style_for_selection(&mut self, style: ParagraphStyle, cx: &mut Context<Self>) {
    if let Some(BlockSelection::TableCell { block_ix, row_ix, cell_ix }) = self.selected_block {
      self.edit_table_cell_paragraph(block_ix, row_ix, cell_ix, cx, |paragraph| {
        if paragraph.paragraph.style != style {
          paragraph.paragraph.style = style;
          paragraph.paragraph.version = paragraph.paragraph.version.wrapping_add(1);
        }
      });
      return;
    }
    self.apply_document_edit(cx, |editor, cx| {
      let range = editor.selection.normalized();
      for paragraph_ix in range.start.paragraph..=range.end.paragraph {
        if let Some(paragraph) = paragraphs_mut(&mut editor.document).get_mut(paragraph_ix)
          && paragraph.style != style
        {
          paragraph.style = style;
          bump_paragraph_version(paragraph);
        }
      }
      editor.after_formatting_mutation(cx);
    });
  }

  // -------- Action handlers (bound to keystrokes in main.rs) -----------
  // Each handler delegates to a movement/edit primitive defined below.
  // The signatures all match what `cx.listener(...)` expects:
  //   fn(&mut Self, &Action, &mut Window, &mut Context<Self>).

}

#[hotpath::measure]
fn condensed_card_target_block_ix(document: &Document, start_block_ix: Option<usize>) -> Option<usize> {
  let mut block_ix = start_block_ix?.min(document.blocks.len().saturating_sub(1));
  while block_ix < document.blocks.len() {
    if condensed_card_block_is_eligible(&document.blocks[block_ix]) {
      return Some(block_ix);
    }
    block_ix += 1;
  }
  None
}

#[hotpath::measure]
fn condensed_card_block_span(document: &Document, target_block_ix: usize) -> Option<Range<usize>> {
  if !document
    .blocks
    .get(target_block_ix)
    .is_some_and(condensed_card_block_is_eligible)
  {
    return None;
  }

  let mut start = target_block_ix;
  while start > 0 && condensed_card_block_is_eligible(&document.blocks[start - 1]) {
    start -= 1;
  }

  let mut end = target_block_ix + 1;
  while end < document.blocks.len() && condensed_card_block_is_eligible(&document.blocks[end]) {
    end += 1;
  }

  Some(start..end)
}

#[hotpath::measure]
fn condensed_card_block_is_eligible(block: &Block) -> bool {
  match block {
    Block::Paragraph(paragraph) => condensed_card_paragraph_is_eligible(paragraph),
    Block::Table(table) => !table_contains_cite(table),
    Block::Image(_) | Block::Equation(_) => false,
  }
}

#[hotpath::measure]
fn condensed_card_paragraph_is_eligible(paragraph: &Paragraph) -> bool {
  paragraph.style == ParagraphStyle::Normal && !paragraph_contains_cite(paragraph)
}

#[hotpath::measure]
fn paragraph_contains_cite(paragraph: &Paragraph) -> bool {
  paragraph
    .runs
    .iter()
    .any(|run| run.styles.semantic == RunSemanticStyle::Cite)
}

#[hotpath::measure]
fn table_contains_cite(table: &TableBlock) -> bool {
  table.rows.iter().any(|row| {
    row.cells.iter().any(|cell| {
      cell.blocks.iter().any(|block| match block {
        TableCellBlock::Paragraph(paragraph) => paragraph_contains_cite(&paragraph.paragraph),
        TableCellBlock::Table(table) => table_contains_cite(table),
      })
    })
  })
}

#[hotpath::measure]
fn condensed_card_span_all_eligible_runs_have_semantic(document: &Document, span: Range<usize>, semantic: RunSemanticStyle) -> bool {
  let mut saw_eligible = false;
  for block_ix in span {
    let Some(block) = document.blocks.get(block_ix) else {
      continue;
    };
    if !condensed_card_block_eligible_runs_have_semantic(block, semantic, &mut saw_eligible) {
      return false;
    }
  }
  saw_eligible
}

#[hotpath::measure]
fn condensed_card_block_eligible_runs_have_semantic(block: &Block, semantic: RunSemanticStyle, saw_eligible: &mut bool) -> bool {
  match block {
    Block::Paragraph(paragraph) => condensed_card_paragraph_eligible_runs_have_semantic(paragraph, semantic, saw_eligible),
    Block::Table(table) => condensed_card_table_eligible_runs_have_semantic(table, semantic, saw_eligible),
    Block::Image(_) | Block::Equation(_) => true,
  }
}

#[hotpath::measure]
fn condensed_card_paragraph_eligible_runs_have_semantic(
  paragraph: &Paragraph,
  semantic: RunSemanticStyle,
  saw_eligible: &mut bool,
) -> bool {
  for run in &paragraph.runs {
    if condensed_card_run_is_eligible(run.styles) {
      *saw_eligible = true;
      if run.styles.semantic != semantic {
        return false;
      }
    }
  }
  true
}

#[hotpath::measure]
fn condensed_card_table_eligible_runs_have_semantic(table: &TableBlock, semantic: RunSemanticStyle, saw_eligible: &mut bool) -> bool {
  for row in &table.rows {
    for cell in &row.cells {
      for block in &cell.blocks {
        let ok = match block {
          TableCellBlock::Paragraph(paragraph) => {
            condensed_card_paragraph_eligible_runs_have_semantic(&paragraph.paragraph, semantic, saw_eligible)
          },
          TableCellBlock::Table(table) => condensed_card_table_eligible_runs_have_semantic(table, semantic, saw_eligible),
        };
        if !ok {
          return false;
        }
      }
    }
  }
  true
}

#[hotpath::measure]
fn apply_condensed_semantic_to_paragraph(paragraph: &mut Paragraph, semantic: RunSemanticStyle, clear_semantic: bool) -> bool {
  let old_runs = paragraph.runs.clone();
  for run in &mut paragraph.runs {
    if condensed_card_run_is_eligible(run.styles) {
      run.styles.semantic = if clear_semantic { RunSemanticStyle::Plain } else { semantic };
    }
  }
  paragraph.runs = merge_adjacent_runs(std::mem::take(&mut paragraph.runs));
  let changed = paragraph.runs != old_runs;
  if changed {
    bump_paragraph_version(paragraph);
  }
  changed
}

#[hotpath::measure]
fn apply_condensed_semantic_to_table(table: &mut TableBlock, semantic: RunSemanticStyle, clear_semantic: bool) -> bool {
  let mut changed = false;
  for row in &mut table.rows {
    for cell in &mut row.cells {
      for block in &mut cell.blocks {
        match block {
          TableCellBlock::Paragraph(paragraph) => {
            changed |= apply_condensed_semantic_to_paragraph(&mut paragraph.paragraph, semantic, clear_semantic);
          },
          TableCellBlock::Table(table) => {
            changed |= apply_condensed_semantic_to_table(table, semantic, clear_semantic);
          },
        }
      }
    }
  }
  changed
}

#[hotpath::measure]
fn condensed_card_run_is_eligible(styles: RunStyles) -> bool {
  !styles.direct_underline && !matches!(styles.semantic, RunSemanticStyle::Underline | RunSemanticStyle::Emphasis)
}
