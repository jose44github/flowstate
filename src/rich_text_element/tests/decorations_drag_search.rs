
#[test]
fn inline_decorations_merge_across_segment_splits() {
  let color = black();
  let merged = merge_inline_decorations(vec![
    Decoration {
      bounds: Bounds::new(point(px(0.0), px(12.0)), size(px(10.0), px(1.0))),
      color,
    },
    Decoration {
      bounds: Bounds::new(point(px(10.25), px(12.0)), size(px(6.0), px(1.0))),
      color,
    },
    Decoration {
      bounds: Bounds::new(point(px(30.0), px(12.0)), size(px(4.0), px(1.0))),
      color,
    },
  ]);

  assert_eq!(merged.len(), 2);
  assert_eq!(merged[0].bounds.origin.x, px(0.0));
  assert_eq!(merged[0].bounds.size.width, px(16.25));
  assert_eq!(merged[1].bounds.origin.x, px(30.0));
}

#[test]
fn inline_decorations_bridge_box_padding_between_emphasis_segments() {
  let color = black();
  let left_pad = px(1.28);
  let right_pad = px(1.35);
  let first_x = left_pad;
  let first_width = px(20.0);
  let second_x = first_x + first_width + right_pad + left_pad;
  let second_width = px(12.0);

  let merged = build_inline_decorations(
    vec![
      DecorationSource {
        segment_ix: 0,
        x: first_x,
        width: first_width,
        y: px(12.0),
        thickness: px(1.0),
        color,
        boxed: true,
      },
      DecorationSource {
        segment_ix: 1,
        x: second_x,
        width: second_width,
        y: px(12.0),
        thickness: px(1.0),
        color,
        boxed: true,
      },
    ],
    left_pad,
    right_pad,
  );

  assert_eq!(merged.len(), 1);
  assert_eq!(merged[0].bounds.origin.x, first_x);
  assert!((f32::from(merged[0].bounds.size.width) - f32::from(second_x + second_width - first_x)).abs() < 0.01);
}

#[test]
fn dragged_text_drop_offset_adjusts_after_source_deletion() {
  let source = DocumentOffset { paragraph: 0, byte: 2 }..DocumentOffset { paragraph: 0, byte: 5 };
  assert_eq!(
    adjust_drop_after_source_delete(DocumentOffset { paragraph: 0, byte: 8 }, source.clone()),
    DocumentOffset { paragraph: 0, byte: 5 }
  );
  assert_eq!(
    adjust_drop_after_source_delete(DocumentOffset { paragraph: 0, byte: 1 }, source),
    DocumentOffset { paragraph: 0, byte: 1 }
  );

  let cross = DocumentOffset { paragraph: 1, byte: 2 }..DocumentOffset { paragraph: 3, byte: 4 };
  assert_eq!(
    adjust_drop_after_source_delete(DocumentOffset { paragraph: 5, byte: 7 }, cross),
    DocumentOffset { paragraph: 3, byte: 7 }
  );
}

#[test]
fn move_rich_text_operation_undo_redo_restores_source_and_drop() {
  let emphasized = RunStyles::default().with(RunStyle::Emphasis);
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("abc "), run("MOVE", emphasized), plain(" def")],
      },
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("target")],
      },
    ],
  );
  let source = DocumentOffset {
    paragraph: 0,
    byte: "abc ".len(),
  }..DocumentOffset {
    paragraph: 0,
    byte: "abc MOVE".len(),
  };
  let fragment = selected_rich_fragment(&document, source.clone());
  let drop = DocumentOffset {
    paragraph: 1,
    byte: "tar".len(),
  };
  let adjusted_drop = adjust_drop_after_source_delete(drop, source.clone());
  delete_cross_paragraph_range(&mut document, source.clone());
  let inserted_end = insert_rich_fragment_at(&mut document, adjusted_drop, &fragment);
  let operation = EditOperation::MoveRichText {
    source_range: source,
    adjusted_drop,
    inserted_range: adjusted_drop..inserted_end,
    fragment,
  };

  assert_eq!(paragraph_text(&document, 0), "abc  def");
  assert_eq!(paragraph_text(&document, 1), "tarMOVEget");
  assert!(
    document.paragraphs[1]
      .runs
      .iter()
      .any(|run| run.styles.semantic == RunSemanticStyle::Emphasis)
  );

  operation.undo(&mut document);
  assert_eq!(paragraph_text(&document, 0), "abc MOVE def");
  assert_eq!(paragraph_text(&document, 1), "target");
  assert!(
    document.paragraphs[0]
      .runs
      .iter()
      .any(|run| run.styles.semantic == RunSemanticStyle::Emphasis)
  );

  operation.redo(&mut document);
  assert_eq!(paragraph_text(&document, 0), "abc  def");
  assert_eq!(paragraph_text(&document, 1), "tarMOVEget");
  assert!(
    document.paragraphs[1]
      .runs
      .iter()
      .any(|run| run.styles.semantic == RunSemanticStyle::Emphasis)
  );
}

#[test]
fn soft_line_break_stays_inside_paragraph_and_copies_as_newline() {
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![InputParagraph {
      style: ParagraphStyle::Normal,
      runs: vec![plain("alphaomega")],
    }],
  );
  insert_text_at(&mut document, 0, "alpha".len(), SOFT_LINE_BREAK_STR, RunStyles::default());

  assert_eq!(document.paragraphs.len(), 1);
  assert_eq!(paragraph_text(&document, 0), format!("alpha{SOFT_LINE_BREAK_STR}omega"));
  assert_eq!(
    selected_plain_text(
      &document,
      DocumentOffset { paragraph: 0, byte: 0 }..DocumentOffset {
        paragraph: 0,
        byte: paragraph_text_len(&document.paragraphs[0]),
      },
    ),
    "alpha\nomega"
  );
}

#[test]
fn find_text_ranges_returns_document_offsets_across_paragraphs() {
  let document = document_from_input(
    DocumentTheme::default(),
    vec![
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("alpha")],
      },
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("beta alpha")],
      },
    ],
  );
  let matches = find_text_ranges(&document, "alpha");
  assert_eq!(matches.len(), 2);
  assert_eq!(matches[0].start, DocumentOffset { paragraph: 0, byte: 0 });
  assert_eq!(
    matches[0].end,
    DocumentOffset {
      paragraph: 0,
      byte: "alpha".len()
    }
  );
  assert_eq!(
    matches[1].start,
    DocumentOffset {
      paragraph: 1,
      byte: "beta ".len()
    }
  );
  assert_eq!(
    matches[1].end,
    DocumentOffset {
      paragraph: 1,
      byte: "beta alpha".len()
    }
  );
}

#[test]
fn cross_paragraph_style_mutation_keeps_runs_and_unselected_text_intact() {
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("abc")],
      },
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("def")],
      },
    ],
  );
  mutate_runs_in_range(
    &mut document,
    DocumentOffset { paragraph: 0, byte: 1 }..DocumentOffset { paragraph: 1, byte: 2 },
    |styles| styles.semantic = RunSemanticStyle::Cite,
  );

  assert_eq!(paragraph_text(&document, 0), "abc");
  assert_eq!(paragraph_text(&document, 1), "def");
  assert_ne!(document.paragraphs[0].runs[0].styles.semantic, RunSemanticStyle::Cite);
  assert_eq!(document.paragraphs[0].runs[1].styles.semantic, RunSemanticStyle::Cite);
  assert_eq!(document.paragraphs[1].runs[0].styles.semantic, RunSemanticStyle::Cite);
  assert_ne!(document.paragraphs[1].runs[1].styles.semantic, RunSemanticStyle::Cite);
}
