---
name: GPUI UI Development
description: Guidelines and patterns for writing GPUI components in the Hive application.
---

# GPUI Development in Hive

## Core Principles
- **No Egui!**: This project uses **GPUI** (Zed's UI framework), not egui or iced. Use `gpui::*` and `gpui_component::*` imports.
- **Styling**: Use the fluent builder pattern (`div().flex().bg(theme.bg_surface)`) for styling, referencing `HiveTheme`.
- **State Management**: UI components hold their own struct state. Use `cx.notify()` when state mutates so the view re-renders.
- **Async Interactions**: When performing async operations (like API calls), spawn them with `cx.spawn(...)`. 
  - Use `tx.send(result)` / `rx.await` for channeling results back to the main thread.
  - Update state using `this.update(app, |this, cx| ...)` inside the spawned task.

## Theme Usage
Always extract the global theme like this:
```rust
let theme = if cx.has_global::<AppTheme>() {
    cx.global::<AppTheme>().0.clone()
} else {
    HiveTheme::dark()
};
```
Use semantic colors: `theme.bg_surface`, `theme.text_primary`, `theme.accent_cyan`, etc. Avoid hardcoded hex values.

## Render Method
Always implement `gpui::Render` and return `impl IntoElement`.
Example:
```rust
impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .w_full()
            .h_full()
            .flex()
            .bg(theme.bg_surface)
            .child("Hello World")
    }
}
```
