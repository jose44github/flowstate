pub fn install_workspace_close_prompt(workspace: Entity<Workspace>, window: &mut Window, cx: &mut App) {
  let prompt_open = Rc::new(Cell::new(false));
  let allow_close = Rc::new(Cell::new(false));
  let window_handle = window.window_handle();

  window.on_window_should_close(cx, move |window, cx| {
    if allow_close.get() {
      return true;
    }

    let dirty_editors = workspace.read(cx).dirty_editors(cx);
    if dirty_editors.is_empty() {
      return true;
    }

    if prompt_open.get() {
      return false;
    }
    prompt_open.set(true);

    let message = if dirty_editors.len() == 1 {
      "This document has unsaved changes."
    } else {
      "One or more documents have unsaved changes."
    };
    let answer = window.prompt(
      PromptLevel::Warning,
      "Save changes before closing?",
      Some(message),
      &[PromptButton::ok("Save"), PromptButton::new("Don't Save"), PromptButton::cancel("Cancel")],
      cx,
    );
    let prompt_open = prompt_open.clone();
    let allow_close = allow_close.clone();

    cx.spawn(async move |cx| {
      let should_close = match answer.await {
        Ok(0) => {
          let mut ok = true;
          for editor in dirty_editors {
            match editor.update(cx, |editor, cx| editor.save(cx)) {
              Ok(Ok(())) => {},
              Ok(Err(error)) => {
                ok = false;
                let detail = error.to_string();
                let _ = window_handle.update(cx, |_, window, cx| {
                  window.prompt(PromptLevel::Critical, "Save failed", Some(&detail), &[PromptButton::ok("Ok")], cx)
                });
                break;
              },
              Err(error) => {
                ok = false;
                eprintln!("failed to access editor before close: {error}");
                break;
              },
            }
          }
          ok
        },
        Ok(1) => {
          for editor in dirty_editors {
            let _ = editor.update(cx, |editor, _| editor.discard_recovery_file());
          }
          true
        },
        _ => false,
      };

      prompt_open.set(false);
      if should_close {
        allow_close.set(true);
        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
      }
    })
    .detach();

    false
  });
}

pub fn open_workspace_window(document_path: Option<PathBuf>, cx: &mut App) {
  let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
  cx.open_window(
    WindowOptions {
      window_bounds: Some(WindowBounds::Maximized(bounds)),
      titlebar: Some(TitlebarOptions {
        title: Some("Flowstate".into()),
        appears_transparent: true,
        traffic_light_position: Some(point(px(12.0), px(18.0))),
      }),
      ..Default::default()
    },
    |window, cx| {
      window.set_window_title("Flowstate");
      let workspace = cx.new(|cx| Workspace::new(document_path, window, cx));
      install_workspace_close_prompt(workspace.clone(), window, cx);
      cx.new(|cx| Root::new(workspace, window, cx))
    },
  )
  .unwrap();
}
