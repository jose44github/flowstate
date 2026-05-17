use super::*;

#[test]
fn paragraph_edit_helpers_preserve_text_and_styles() {
  let emphasized = RunStyles::default().with(RunStyle::Emphasis);
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![InputParagraph {
      style: ParagraphStyle::Normal,
      runs: vec![run("hello", RunStyles::default())],
    }],
  );

  insert_text_at(&mut document, 0, "he".len(), "y", RunStyles::default());
  assert_eq!(paragraph_text(&document, 0), "heyllo");
  assert_eq!(document.paragraphs[0].runs.len(), 1);

  apply_style_to_paragraph_range(&mut document, 0, "hey".len().."heyll".len(), RunStyle::Emphasis);
  assert_eq!(paragraph_text(&document, 0), "heyllo");
  assert_eq!(document.paragraphs[0].runs.len(), 3);
  assert_eq!(document.paragraphs[0].runs[1].styles, emphasized);

  delete_range_in_paragraph(&mut document, 0, "he".len().."heyll".len());
  assert_eq!(paragraph_text(&document, 0), "heo");
  assert_eq!(document.paragraphs[0].runs.len(), 1);
  assert_eq!(document.paragraphs[0].runs[0].styles, RunStyles::default());
}

#[test]
fn document_rope_edits_keep_utf8_byte_offsets() {
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![InputParagraph {
      style: ParagraphStyle::Normal,
      runs: vec![run("abé🚀cd", RunStyles::default())],
    }],
  );
  insert_text_at(&mut document, 0, "abé".len(), "Z", RunStyles::default());
  assert_eq!(paragraph_text(&document, 0), "abéZ🚀cd");

  let delete_start = "abé".len();
  let delete_end = "abéZ🚀".len();
  delete_range_in_paragraph(&mut document, 0, delete_start..delete_end);
  assert_eq!(paragraph_text(&document, 0), "abécd");
}

#[test]
fn db8_round_trip_preserves_text_structure_and_styles() {
  let document = demo_document();
  let dir = std::env::temp_dir();
  let path = dir.join(format!("debateprocessor-test-{}.db8", std::process::id()));
  write_db8(&path, &document).unwrap();
  let loaded = read_db8(&path).unwrap();
  let _ = std::fs::remove_file(path);

  assert_eq!(
    document_text_slice(&document, 0..document.text.byte_len()),
    document_text_slice(&loaded, 0..loaded.text.byte_len())
  );
  assert_eq!(document.paragraphs.len(), loaded.paragraphs.len());
  // Verify styles and run structure for every paragraph, not just the first.
  for (ix, (orig, loaded_para)) in document.paragraphs.iter().zip(loaded.paragraphs.iter()).enumerate() {
    assert_eq!(orig.style, loaded_para.style, "paragraph {ix} style mismatch");
    assert_eq!(orig.runs, loaded_para.runs, "paragraph {ix} runs mismatch");
  }
}

#[test]
fn split_and_merge_preserve_empty_styled_paragraphs() {
  let spoken = RunStyles::default().with(RunStyle::HighlightSpoken);
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![InputParagraph {
      style: ParagraphStyle::Pocket,
      runs: vec![run("Pocket", spoken)],
    }],
  );

  let first_len = paragraph_text_len(&document.paragraphs[0]);
  split_paragraph_at(&mut document, 0, first_len);
  assert_eq!(document.paragraphs.len(), 2);
  assert_eq!(document.paragraphs[1].style, ParagraphStyle::Pocket);
  assert_eq!(paragraph_text_len(&document.paragraphs[1]), 0);
  assert!(document.paragraphs[1].runs.is_empty());

  let join_byte = paragraph_text_len(&document.paragraphs[0]);
  delete_cross_paragraph_range(
    &mut document,
    DocumentOffset {
      paragraph: 0,
      byte: join_byte,
    }..DocumentOffset { paragraph: 1, byte: 0 },
  );
  assert_eq!(document.paragraphs.len(), 1);
  assert_eq!(paragraph_text(&document, 0), "Pocket");
  assert_eq!(document.paragraphs[0].runs, vec![TextRun { len: "Pocket".len(), styles: spoken }]);
}

#[test]
fn db8_round_trip_preserves_empty_styled_paragraphs() {
  let document = document_from_input(
    DocumentTheme::default(),
    vec![
      InputParagraph {
        style: ParagraphStyle::Pocket,
        runs: Vec::new(),
      },
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![plain("body")],
      },
    ],
  );
  let path = std::env::temp_dir().join(format!("debateprocessor-empty-{}.db8", std::process::id()));
  write_db8(&path, &document).unwrap();
  let loaded = read_db8(&path).unwrap();
  let _ = std::fs::remove_file(path);

  assert_eq!(loaded.paragraphs[0].style, ParagraphStyle::Pocket);
  assert_eq!(paragraph_text_len(&loaded.paragraphs[0]), 0);
  assert!(loaded.paragraphs[0].runs.is_empty());
  assert_eq!(paragraph_text(&loaded, 1), "body");
}

#[test]
fn selection_across_empty_paragraphs_and_clear_formatting_policy() {
  let emphasized = RunStyles::default().with(RunStyle::Emphasis);
  let mut document = document_from_input(
    DocumentTheme::default(),
    vec![
      InputParagraph {
        style: ParagraphStyle::Tag,
        runs: vec![run("tag", emphasized)],
      },
      InputParagraph {
        style: ParagraphStyle::Pocket,
        runs: Vec::new(),
      },
      InputParagraph {
        style: ParagraphStyle::Normal,
        runs: vec![run("body", emphasized)],
      },
    ],
  );
  let selection = DocumentOffset { paragraph: 0, byte: 1 }..DocumentOffset { paragraph: 2, byte: 1 };
  assert!(selection_contains_whole_paragraph(&document, selection.clone()));

  for paragraph_ix in selection.start.paragraph..=selection.end.paragraph {
    clear_whole_paragraph_formatting(&mut document, paragraph_ix);
  }

  for paragraph in &document.paragraphs {
    assert_eq!(paragraph.style, ParagraphStyle::Normal);
    assert!(paragraph.runs.iter().all(|run| run.styles == RunStyles::default()));
  }
}

#[test]
fn run_style_full_selection_toggle_policy() {
  let emphasized = RunStyles::default().with(RunStyle::Emphasis);
  let document = document_from_input(
    DocumentTheme::default(),
    vec![InputParagraph {
      style: ParagraphStyle::Normal,
      runs: vec![run("all", emphasized), plain(" plain")],
    }],
  );

  assert!(selection_all_run_styles(
    &document,
    DocumentOffset { paragraph: 0, byte: 0 }..DocumentOffset {
      paragraph: 0,
      byte: "all".len(),
    },
    |styles| styles.emphasis,
  ));
  assert!(!selection_all_run_styles(
    &document,
    DocumentOffset { paragraph: 0, byte: 0 }..DocumentOffset {
      paragraph: 0,
      byte: "all plain".len(),
    },
    |styles| styles.emphasis,
  ));
}
