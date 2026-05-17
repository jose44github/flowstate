#[cfg(target_os = "macos")]
use gpui::ParentElement as _;
use gpui_component::button::Button;
use gpui_component::button::ButtonVariants as _;
use gpui_component::{IconName, Sizable as _};

#[derive(Clone, Copy)]
pub enum AppIcon {
  Close,
}

pub fn icon_button(id: impl Into<gpui::ElementId>, icon: AppIcon) -> Button {
  platform_icon_button(Button::new(id), icon).xsmall().ghost()
}

#[cfg(target_os = "macos")]
fn platform_icon_button(button: Button, icon: AppIcon) -> Button {
  let symbol = match icon {
    AppIcon::Close => "xmark",
  };
  button.child(gpui_symbols::Icon::new(symbol).size(gpui::px(11.0)))
}

#[cfg(not(target_os = "macos"))]
fn platform_icon_button(button: Button, icon: AppIcon) -> Button {
  let icon_name = match icon {
    AppIcon::Close => IconName::WindowClose,
  };
  button.icon(icon_name)
}
