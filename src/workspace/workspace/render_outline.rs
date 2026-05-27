impl Workspace {
  fn render_left_nav(&mut self, nav_width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
    self.refresh_outline_tree(cx);
    self.refresh_outline_viewport(cx);
    let workspace = cx.entity().downgrade();
    let active_outline_paragraph = self.active_outline_paragraph(cx);
    self.scroll_outline_item_into_view(active_outline_paragraph, cx);
    v_flex()
      .size_full()
      .h_full()
      .gap_1()
      .p_2()
      .border_r_1()
      .border_color(cx.theme().sidebar_border)
      .bg(cx.theme().sidebar)
      .text_color(cx.theme().sidebar_foreground)
      .child(
        div()
          .w_full()
          .flex()
          .flex_row()
          .items_center()
          .justify_between()
          .child(
            div()
              .text_sm()
              .font_weight(gpui::FontWeight::SEMIBOLD)
              .child("Outline"),
          )
          .child(
            Button::new("collapse-outline-panel")
              .icon(IconName::PanelLeftClose)
              .xsmall()
              .ghost()
              .tooltip("Collapse outline")
              .on_click(cx.listener(|workspace, _, window, cx| {
                workspace.toggle_outline(window, cx);
              })),
          ),
      )
      .child(
        div()
          .flex_1()
          .w_full()
          .overflow_hidden()
          .child(tree(&self.outline_tree, move |ix, entry, _selected, window, cx| {
            let paragraph_ix = outline_paragraph_ix(entry.item().id.as_ref());
            let is_folder = entry.is_folder();
            let is_expanded = entry.is_expanded();
            let is_active_outline = paragraph_ix == active_outline_paragraph;
            let depth = entry.depth();
            let label_width = outline_label_width(nav_width, depth);
            let label = truncate_outline_label(entry.item().label.as_ref(), outline_label_text_width(label_width, window), window, cx);
            let workspace = workspace.clone();
            ListItem::new(("outline-tree-item", ix))
              .w_full()
              .min_w_0()
              .overflow_hidden()
              .pl(px(4.0) + px(12.0) * entry.depth())
              .pr_1()
              .py_0()
              .text_xs()
              .child(
                h_flex()
                  .w_full()
                  .min_w_0()
                  .overflow_hidden()
                  .items_center()
                  .gap_1()
                  .when(is_folder, |this| {
                    this.child(
                      Button::new(("outline-toggle", ix))
                        .icon(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                        .xsmall()
                        .ghost()
                        .flex_none()
                        .disabled(!is_folder)
                        .on_click({
                          let workspace = workspace.clone();
                          move |_, _, cx| {
                            cx.stop_propagation();
                            if let Some(paragraph_ix) = paragraph_ix {
                              let _ = workspace.update(cx, |workspace, cx| workspace.toggle_outline_item(paragraph_ix, cx));
                            }
                          }
                        }),
                    )
                  })
                  .when(!is_folder, |this| this.child(div().w(px(20.0)).h(px(20.0)).flex_none()))
                  .child(
                    div()
                      .id(("outline-label", ix))
                      .relative()
                      .flex_1()
                      .min_w_0()
                      .px_1()
                      .overflow_hidden()
                      .text_color(cx.theme().sidebar_foreground)
                      .whitespace_nowrap()
                      .when(is_active_outline, |this| {
                        this.child(
                          div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .bg(cx.theme().sidebar_accent)
                            .border_1()
                            .border_color(cx.theme().primary)
                            .rounded(px(4.0)),
                        )
                      })
                      .child(label)
                      .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                      })
                      .on_click(move |_, window, cx| {
                        if let Some(paragraph_ix) = paragraph_ix {
                          let _ = workspace.update(cx, |workspace, cx| workspace.scroll_active_editor_to_paragraph(paragraph_ix, window, cx));
                        }
                      }),
                  ),
              )
          })),
      )
  }

}
