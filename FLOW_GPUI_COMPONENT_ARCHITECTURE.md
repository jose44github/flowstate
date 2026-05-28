# Flow GPUI Component Architecture

## Crate Boundary

`crates/flowstate-flow` owns all non-UI flow behavior. It has no GPUI dependency.

- `document.rs` defines the `.fl0` tree: root -> flows -> boxes.
- `actions.rs` defines deterministic action bundles for add, update, delete, move, replace, and identity operations.
- `history.rs` stores undo/redo by owner, where flow edits are owned by the selected flow and flow-order edits are owned by root.
- `persistence.rs` reads and writes versioned `.fl0` JSON and upgrades legacy debate-flow node arrays.
- `styles.rs` stores debate-style templates and column metadata.

This crate is the correct future integration point for CRDT or P2P sync. UI code never edits flow order or cells directly; it asks `FlowDocument` to apply action bundles and stores inverse bundles for undo.

## Native UI Boundary

`src/flow` owns the GPUI surface.

- `editor.rs` renders the flow board and owns focus, input state, folded boxes, dirty state, and command methods.
- `ribbon.rs` renders the modal flow ribbon with gpui-component controls.
- `panel.rs` adapts the editor and ribbon into the workspace panel system.
- `mod.rs` exports `FlowEditor`, `FlowPanel`, and `FlowRibbon`.

## Flow Ribbon

`FlowRibbon` is the active ribbon when a `.fl0` tab is focused. It uses gpui-component `Button`, `Toggle`, `Input`, and `Select` controls.

The ribbon reads command state from `FlowEditor::command_state()` and sends commands back through public editor methods:

- setup: debate style select, LD TOC toggle, speaker switch toggle, template add buttons
- flow: selected flow title input and delete-flow button
- edit: undo, redo, add response, add above, add below, extend, delete selected
- format: bold, cross out, fold

`Workspace::render_ribbon` chooses the rich-text ribbon for `.db8` tabs and the flow ribbon for `.fl0` tabs.

## Flow Outline

When a `.fl0` tab is active, the left outline becomes a flow-order outline instead of a document paragraph outline.

`Workspace::render_left_nav` branches to `render_flow_nav`, which uses gpui-component `ListItem` rows styled like the existing outline panel. Rows select flows through `FlowEditor::select_flow`.

Flow order is sortable by GPUI drag/drop:

- drag payload: `FlowOutlineDrag`
- drop target: each flow row plus an end drop zone
- mutation: `FlowEditor::move_flow_to_index`
- model operation: `flowstate_flow::move_node_actions`

## Flow Board

`FlowEditor` renders only the board, not an internal toolbar or sorter.

The board is a recursive column tree:

- columns come from the selected flow template
- headers use theme-derived accents and include per-column add buttons
- boxes render through gpui-component `Input`
- empty placeholder boxes reserve horizontal column space without forcing their descendants into a 10px-tall row
- selected, bold, crossed, extension, and folded states are rendered from the flow model plus editor-local folded state

All colors are derived from `cx.theme()`; no hardcoded flow palette is used.

## Workspace Integration

Workspace integration is split across existing workspace modules:

- file open/save/save-as recognizes `.fl0`
- tab state tracks rich-text panels and flow panels together
- active tab styling uses document-paper colors only for `.db8`; `.fl0` uses the app theme
- file search includes `.fl0`
- empty state and top-bar file menu include New Flow

The `.fl0` tab behaves like a first-class document panel while keeping flow model logic isolated in `flowstate-flow`.
