# Ultralong Optimization Opportunities

Target fixture: `data/ultra_long.db8`, currently about 3.7 MB and 9000 paragraphs.

The observed slowdowns are expected from the current code. Demo speed stays good because the implementation has several whole-document costs that are hidden at small size and become visible at 9000 paragraphs.

## 1. Viewport-Aware Layout Is The Main Missing Architecture

Address:
- [src/rich_text_element.rs:2226](src/rich_text_element.rs:2226) `request_word_layout_for_editor`
- [src/rich_text_element.rs:2409](src/rich_text_element.rs:2409) `build_layout`

Problem:
- Startup and first interaction are slow because `build_layout` walks every paragraph and shapes/wraps all content before the first useful frame is ready.
- The transparent OS window at startup is consistent with GPUI having a window but not yet having completed first expensive layout/paint.
- Any width change invalidates the previous layout by width and forces a full-document rewrap.

Expected impact:
- Very high. This is the dominant startup/open/resize cost.

Safe optimization:
- Keep exact layout for visible and near-visible paragraphs only.
- Maintain a paragraph height cache for offscreen paragraphs.
- Use estimated heights for unmeasured paragraphs.
- Refine offscreen heights in background/incremental passes.
- Anchor scroll position when estimates become measured values.

Do not compromise:
- Visible paragraphs must always be exact.
- Printing/export must use full exact layout, not estimates.
- Selection/caret/page movement must handle unmeasured regions predictably.

## 2. Paint Scans The Entire Layout Several Times Per Frame

Address:
- [src/rich_text_element.rs:2972](src/rich_text_element.rs:2972) `paint_layout`
- [src/rich_text_element.rs:3101](src/rich_text_element.rs:3101) `paragraph_intersects_mask`
- [src/rich_text_element.rs:3152](src/rich_text_element.rs:3152) `paint_selection`

Problem:
- Paint currently loops through every laid-out paragraph four times:
  - paragraph borders
  - run background/highlight rects
  - text
  - underlines
- Selection paint scans all paragraphs again.
- For ultralong, even when only a screenful is visible, every frame pays O(total paragraphs) visibility checks.

Expected impact:
- High for scrolling and arrow-key scrolling.
- This explains why scrolling can freeze until key release: many repaint/layout cycles queue up and each one scans all 9000 paragraphs repeatedly.

Safe optimization:
- Add binary search over paragraph `top` / `bottom` to find the visible paragraph range.
- Paint only `visible_start..visible_end`.
- Share the visible range across border/background/text/underline/selection passes.
- Keep a small overscan margin to avoid edge artifacts.

Do not compromise:
- Keep existing paint order: borders/highlights/text/underlines/selection/caret.
- Do not skip offscreen paragraphs by approximate index; use y-ranges.

## 3. Layout Cache Reuse Still Clones Every Cached Paragraph

Address:
- [src/rich_text_element.rs:2422](src/rich_text_element.rs:2422) cached paragraph branch in `build_layout`
- [src/rich_text_element.rs:1957](src/rich_text_element.rs:1957) `LaidOutParagraph::shift_y`

Problem:
- Paragraph version cache keys avoid text hashing, but a cache hit still clones the full `LaidOutParagraph`, including lines, segments, shaped glyph data, rects, and underlines.
- On ultralong documents, a "cheap" relayout after a tiny edit can still clone thousands of offscreen shaped paragraphs.

Expected impact:
- High for typing/deleting and any state change that requests layout.

Safe optimization:
- Store paragraph layout geometry in a stable cache and avoid cloning shaped paragraph internals on cache hit.
- Represent y-position separately from immutable paragraph layout content.
- For full-layout mode, reuse `Rc<LaidOutParagraphBody>` plus per-pass y offsets.

Do not compromise:
- Paragraph version invalidation must still rebuild changed paragraphs.
- Layout output must still be deterministic for caret hit testing and painting.

## 4. Text Wrapping Repeatedly Shapes Candidate Lines

Address:
- [src/rich_text_element.rs:2490](src/rich_text_element.rs:2490) `wrap_lines`
- [src/rich_text_element.rs:2626](src/rich_text_element.rs:2626) `measure_line_width`
- [src/rich_text_element.rs:2655](src/rich_text_element.rs:2655) `shape_line`
- [src/rich_text_element.rs:2755](src/rich_text_element.rs:2755) `shape_fragment`

Problem:
- Wrapping calls `measure_line_width` for many candidate breakpoints.
- `measure_line_width` shapes fragments.
- Once a line break is chosen, `shape_line` shapes the selected text again.
- `first_overflow_line_end` does a binary search and shapes repeatedly.

Expected impact:
- Very high during first layout and width changes.

Safe optimization:
- Shape paragraph runs once into reusable measurement segments, then wrap using cached advances.
- Cache shaped fragments by paragraph version, run style, source range, and width-independent text content.
- At minimum, avoid shaping the final accepted line twice by returning measured fragments from wrap.

Do not compromise:
- Maintain current visual fidelity and GPUI text shaping.
- Do not hand-roll font metrics that diverge from GPUI shaping.

## 5. Editing Snapshots And Compares The Whole Document

Address:
- [src/rich_text_element.rs:1030](src/rich_text_element.rs:1030) `apply_document_edit`
- [src/rich_text_element.rs:1038](src/rich_text_element.rs:1038) `finish_document_edit`
- [src/rich_text_element.rs:453](src/rich_text_element.rs:453) `documents_equivalent`
- [src/rich_text_element.rs:441](src/rich_text_element.rs:441) `document_fingerprint`
- [src/rich_text_element.rs:1069](src/rich_text_element.rs:1069) `refresh_save_status`

Problem:
- Every text edit clones the full `Document` before the edit.
- Every accepted edit stores another full `Document` as `after_document`.
- `documents_equivalent` can compare full document text.
- `refresh_save_status` fingerprints the full document text after every edit.

Expected impact:
- Very high for typing and deleting in ultralong files.
- This directly matches delayed typing/deleting.

Safe optimization:
- Replace snapshot undo with the planned operation log.
- Track dirty state with a generation counter, not full-document fingerprint per keystroke.
- Store the saved generation and only compute expensive fingerprints for recovery/open conflict checks.
- Have edit primitives return `bool changed` instead of cloning and comparing whole documents.

Do not compromise:
- Undo/redo correctness.
- Saved/dirty correctness when undo returns to a saved state. This can be solved with operation log sequence IDs or saved history marker, not full fingerprinting each keystroke.

## 6. Per-Edit Paragraph Range Shifting Is O(paragraphs After Edit)

Address:
- [src/rich_text_element.rs:3575](src/rich_text_element.rs:3575) `insert_text_at`
- [src/rich_text_element.rs:3629](src/rich_text_element.rs:3629) `delete_range_in_paragraph`
- [src/rich_text_element.rs:3413](src/rich_text_element.rs:3413) `shift_paragraphs_after`
- [src/rich_text_element.rs:3442](src/rich_text_element.rs:3442) `split_paragraph_at`
- [src/rich_text_element.rs:3456](src/rich_text_element.rs:3456) `delete_cross_paragraph_range`

Problem:
- Inserting/deleting in paragraph N shifts `byte_range` for every later paragraph.
- In a 9000 paragraph document, typing near the top updates thousands of paragraph ranges per keystroke.

Expected impact:
- High for typing/deleting near the beginning or middle of ultralong documents.

Safe optimization:
- Stop storing absolute byte ranges that must be shifted eagerly.
- Store paragraph text lengths and a newline/paragraph prefix index.
- Use a Fenwick tree / segment tree / B-tree indexed offset table from an external crate or a simple well-tested internal structure if no crate fits.
- Compute global byte offsets lazily from paragraph index + local byte.

Do not compromise:
- Rope offsets must remain correct for mutation.
- Paragraph semantics must remain first-class.

## 7. Recovery Scheduling Calls Dirty Check That Fingerprints The Whole Document

Address:
- [src/rich_text_element.rs:1080](src/rich_text_element.rs:1080) `schedule_recovery_write`
- [src/rich_text_element.rs:562](src/rich_text_element.rs:562) `has_unsaved_changes`
- [src/rich_text_element.rs:441](src/rich_text_element.rs:441) `document_fingerprint`

Problem:
- `schedule_recovery_write` calls `has_unsaved_changes`.
- `has_unsaved_changes` computes a full document fingerprint.
- This happens on the foreground path after edits.

Expected impact:
- High for typing/deleting.

Safe optimization:
- Make dirty state O(1).
- Use `edit_generation != saved_generation` for normal dirty checks.
- Reserve full fingerprinting for explicit save/recovery reconciliation paths.

Do not compromise:
- Recovery writes must remain off the render/event path.
- Snapshot creation should stay debounced and coalesced.

## 8. Word Navigation Builds A Full Document String

Address:
- [src/rich_text_element.rs:1134](src/rich_text_element.rs:1134) `word_left`
- [src/rich_text_element.rs:1141](src/rich_text_element.rs:1141) `word_right`
- [src/rich_text_element.rs:3335](src/rich_text_element.rs:3335) `full_document_text`
- [src/rich_text_element.rs:3353](src/rich_text_element.rs:3353) `global_to_document_offset`

Problem:
- Word-left/right calls `full_document_text`, allocating/copying the entire document.
- `global_to_document_offset` then linearly scans paragraphs.

Expected impact:
- High when holding Ctrl/Alt-arrow or word delete in ultralong docs.

Safe optimization:
- Implement word boundary scanning over the rope around the caret only.
- Search locally across paragraph boundaries as needed.
- Replace `global_to_document_offset` linear scan with paragraph index cache / prefix index / binary search over paragraph starts.

Do not compromise:
- Keep debate punctuation rules.
- Keep Unicode-safe boundaries.

## 9. Caret And Line Lookup Are Linear Within Layout

Address:
- [src/rich_text_element.rs:3917](src/rich_text_element.rs:3917) `locate_line`
- [src/rich_text_element.rs:3952](src/rich_text_element.rs:3952) `paragraph_layout`
- [src/rich_text_element.rs:3965](src/rich_text_element.rs:3965) `paragraph_layout_index`
- [src/rich_text_element.rs:3192](src/rich_text_element.rs:3192) `caret_bounds`

Problem:
- Most direct indexing works while full layout has every paragraph, but fallback paths linearly scan.
- Within a paragraph, locating a line is linear over paragraph lines.
- With viewport layout later, these paths need robust indexed access.

Expected impact:
- Medium now, higher after viewport layout if not handled.

Safe optimization:
- Keep paragraph index -> layout index map.
- Store line byte ranges in a searchable vector and binary search within paragraph.
- Preserve existing wrap seam bias behavior.

Do not compromise:
- Home/End and up/down caret behavior.
- Soft-wrap boundary bias.

## 10. Layout State Stores Full Shaped Glyph Data For Entire Document

Address:
- [src/rich_text_element.rs:1936](src/rich_text_element.rs:1936) `LayoutState`
- [src/rich_text_element.rs:2035](src/rich_text_element.rs:2035) `LaidOutSegment`

Problem:
- Full document layout stores every shaped segment/glyph for all 9000 paragraphs.
- Memory pressure increases and cloning/cache reuse become more expensive.

Expected impact:
- Medium to high on poor laptops and very large files.

Safe optimization:
- Keep exact shaped data only for visible/near-visible paragraphs.
- Store offscreen paragraph height estimates or measured heights without glyph payload.
- If exact offscreen layout is needed for search/print/export, compute it separately from interactive layout.

Do not compromise:
- Visible text fidelity.
- Caret hit testing in visible area.

## 11. `paragraph_text` Allocates Per Paragraph During Layout

Address:
- [src/rich_text_element.rs:2441](src/rich_text_element.rs:2441) call site in `build_layout`
- [src/rich_text_element.rs:3299](src/rich_text_element.rs:3299) `paragraph_text`
- [src/rich_text_element.rs:3327](src/rich_text_element.rs:3327) `document_text_slice`

Problem:
- Layout copies each paragraph out of the rope into a `String`.
- This is acceptable for small files but becomes unnecessary allocation churn for ultralong docs.

Expected impact:
- Medium during full layout and resize.

Safe optimization:
- Cache paragraph text by paragraph version for layout.
- Or expose paragraph rope slices/chunks to wrapping code and only materialize the visible/current paragraph when shaping requires `&str`.
- Combine with viewport layout so this allocation only happens near the viewport.

Do not compromise:
- UTF-8 byte offsets must continue matching run lengths and caret offsets.

## 12. Paint Order Forces Multiple Passes, But Visible Range Can Make It Cheap

Address:
- [src/rich_text_element.rs:2972](src/rich_text_element.rs:2972) `paint_layout`

Problem:
- The multiple paint passes are intentional for fidelity, but currently each pass scans the full document.

Expected impact:
- High if unfixed, low after visible range indexing.

Safe optimization:
- Keep paint order.
- Precompute visible paragraph slice once.
- Iterate only that slice in all passes.

Do not compromise:
- Highlights must remain under text.
- Underlines must remain above text where current behavior requires it.
- Selection overlay must remain visible over highlights.

## 13. Initial Window Should Paint A Lightweight Loading Frame

Address:
- [src/main.rs](src/main.rs)
- [src/rich_text_element.rs:2208](src/rich_text_element.rs:2208) `request_word_layout`
- [src/rich_text_element.rs:2226](src/rich_text_element.rs:2226) `request_word_layout_for_editor`

Problem:
- The OS shows a transparent/uninitialized window while first full layout blocks.

Expected impact:
- Perception improvement immediately; real performance improvement only with viewport layout.

Safe optimization:
- Open with a cheap placeholder/loading surface immediately.
- Defer heavy document layout until after the first paint.
- Then render exact visible layout.

Do not compromise:
- Do not show stale or incorrect document content.

## 14. Release Builds Need Instrumented Timing Logs Before Major Refactors

Address:
- [src/rich_text_element.rs:2208](src/rich_text_element.rs:2208) `request_word_layout`
- [src/rich_text_element.rs:2409](src/rich_text_element.rs:2409) `build_layout`
- [src/rich_text_element.rs:2972](src/rich_text_element.rs:2972) `paint_layout`
- [src/rich_text_element.rs:1030](src/rich_text_element.rs:1030) `apply_document_edit`

Problem:
- The bottlenecks are clear from code, but exact priorities should be verified with timings on `data/ultra_long.db8`.

Expected impact:
- High for avoiding wasted optimization work.

Safe optimization:
- Add feature-gated or env-gated timing logs.
- Record:
  - DB8 read time
  - first layout time
  - paragraphs shaped vs reused
  - paint visible paragraphs count
  - edit command time
  - recovery snapshot time

Do not compromise:
- Keep instrumentation off by default.
- Avoid logging per keystroke unless explicitly enabled.

## Recommended Implementation Order

1. Make dirty checks O(1) and remove full-document fingerprinting from edit path.
2. Replace snapshot undo with operation log or at least remove full-document `documents_equivalent` checks from normal typing.
3. Add visible paragraph range binary search and restrict paint passes to visible paragraphs.
4. Fix word navigation to avoid `full_document_text`.
5. Stop eagerly shifting every later paragraph range on each edit.
6. Implement viewport-aware layout with height cache.
7. Avoid cloning full cached paragraph layout bodies on cache hits.
8. Reduce repeated shaping during wrapping.
9. Add a lightweight first-paint loading frame if first layout remains visible.
10. Add background incremental measurement once exact visible layout is stable.
