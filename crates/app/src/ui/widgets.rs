//! 通用 UI 辅助。

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::*;

use crate::state::AppState;

/// 字段行 + 行内校验错误：错误时控件加红框、label 下显示红字。
pub fn field_row_with_error(
    label: &str,
    hint: &str,
    error: Option<&str>,
    control: impl IntoElement,
    cx: &mut Context<AppState>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let fg = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let danger = crate::ui::theme::RED;
    let has_error = error.is_some();
    let control = control.into_any_element();
    div()
        .flex()
        .items_center()
        .gap_4()
        .py_2()
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .w(px(200.))
                .flex_shrink_0()
                .v_flex()
                .gap_0p5()
                .child(div().text_sm().text_color(fg).child(label.to_string()))
                .when(!hint.is_empty(), |this| {
                    this.child(div().text_xs().text_color(muted).child(hint.to_string()))
                })
                .when_some(error, |this, e| {
                    this.child(div().text_xs().text_color(danger).child(e.to_string()))
                }),
        )
        // 控件限宽左对齐：宽窗口下输入框不再拉满整行；出错时红框包裹
        .child({
            let wrapper = div().flex_1().max_w(px(520.)).flex().items_center();
            if has_error {
                wrapper.child(
                    div()
                        .flex_1()
                        .rounded_md()
                        .border_1()
                        .border_color(danger)
                        .px_1()
                        .child(control),
                )
            } else {
                wrapper.child(control)
            }
        })
}

pub fn section_header(title: &str, cx: &mut Context<AppState>) -> impl IntoElement {
    let accent = cx.theme().accent;
    let fg = cx.theme().foreground;
    div()
        .flex()
        .items_center()
        .gap_2()
        .py_2()
        .child(div().w(px(3.)).h(px(14.)).rounded_full().bg(accent))
        .child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(fg)
                .child(title.to_string()),
        )
}

pub fn status_dot(color: Hsla) -> impl IntoElement {
    div().size(px(6.)).rounded_full().bg(color)
}
