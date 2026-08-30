//! GPUI 主窗口布局与控件实体缓存。

pub mod browser_pick;
mod fields;
mod launch_fields;
pub mod launch_panel;
pub mod panels;
mod profile_tabs;
mod settings_page;
mod sidebar;
mod status;
pub mod theme;
pub mod widgets;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::separator::Separator;
use gpui_component::switch::Switch;
use gpui_component::tab::{Tab as GpuiTab, TabBar};
use gpui_component::{WindowExt, *};

use crate::python_env;
use crate::settings::CloseAction;
use crate::state::{AppState, AppView, Tab};
use widgets::status_dot;

impl Render for AppState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_field_values(window, cx);
        if let Some(msg) = self.worker.launch_error.take() {
            window.open_alert_dialog(cx, move |alert, _w, _cx| {
                alert.title("启动失败").description(msg.clone())
            });
        }
        for (is_error, text) in std::mem::take(&mut self.pending_notifications) {
            let note = if is_error {
                gpui_component::notification::Notification::error(text)
            } else {
                gpui_component::notification::Notification::success(text)
            };
            window.push_notification(note, cx);
        }

        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let dl_overlay = browser_pick::render_download_overlay(self, cx);
        // Root 只渲染 view/tooltip 层，对话框/侧栏/通知层必须由应用自己挂载
        // （否则 open_dialog 只是把对话框 push 进 active_dialogs，永远不显示）。
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        div()
            .id("root")
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(bg)
            .text_color(fg)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.render_sidebar(window, cx))
                    .child(self.render_main(window, cx)),
            )
            .when(self.show_logs, |this| this.child(self.render_log_panel(cx)))
            .child(self.render_statusbar(cx))
            .children(dl_overlay)
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

impl AppState {
    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;

        if self.current_view == AppView::Settings {
            return self.render_settings_page(window, cx).into_any_element();
        }

        let Some(profile) = self.current().cloned() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("选择或新建一个身份"),
                )
                .into_any_element();
        };

        let selected_tab = self.tab.clone();
        let idx = Tab::ALL
            .iter()
            .position(|t| *t == selected_tab)
            .unwrap_or(0);
        let mut tab_bar = TabBar::new("tabs").underline();
        for t in Tab::ALL {
            tab_bar = tab_bar.child(GpuiTab::new().label(t.title()));
        }
        let tab_bar =
            tab_bar
                .selected_index(idx)
                .on_click(cx.listener(|state, ix: &usize, _w, cx| {
                    if let Some(t) = Tab::ALL.get(*ix) {
                        state.select_tab(t.clone(), cx);
                    }
                }));

        let running = self.worker.running.contains_key(&profile.id);
        let any_running = !self.worker.running.is_empty();
        let worker_ok = self.worker.ready;
        let toolbar = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(48.))
            .border_b_1()
            .border_color(border)
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .truncate()
                            .child(profile.name.clone()),
                    )
                    .when(running, |this| this.child(status_dot(theme::GREEN))),
            )
            .child(div().flex_1())
            .child(
                Button::new("genfp")
                    .outline()
                    .small()
                    .disabled(!worker_ok || self.worker.generating)
                    .loading(self.worker.generating)
                    .label(if self.worker.generating {
                        "生成中…"
                    } else {
                        "生成指纹"
                    })
                    .on_click(
                        cx.listener(|state, _e, _w, cx| state.generate_fingerprint_current(cx)),
                    ),
            )
            .child(
                Button::new("validate")
                    .outline()
                    .small()
                    .disabled(!worker_ok || self.worker.validating || self.worker.launching)
                    .loading(self.worker.validating)
                    .label(if self.worker.validating {
                        "校验中…"
                    } else {
                        "校验"
                    })
                    .on_click(cx.listener(|state, _e, _w, cx| state.validate_current(cx))),
            )
            .child(Separator::vertical())
            .child(
                Button::new("create-shortcut")
                    .outline()
                    .small()
                    .icon(IconName::ExternalLink)
                    .label("创建快捷方式")
                    .on_click(
                        cx.listener(|state, _e, _w, cx| state.create_shortcut_for_current(cx)),
                    ),
            )
            .when(!running, |this| {
                this.child(
                    Button::new("launch")
                        .primary()
                        .small()
                        .disabled(!worker_ok || self.worker.launching)
                        .loading(self.worker.launching)
                        .label(if self.worker.launching {
                            "启动中…"
                        } else {
                            "启动"
                        })
                        .on_click(cx.listener(|state, _e, _w, cx| state.launch_current(cx))),
                )
            })
            .when(running, |this| {
                this.child(
                    Button::new("stop")
                        .danger()
                        .small()
                        .label("停止")
                        .on_click(cx.listener(|state, _e, _w, cx| state.stop_current(cx))),
                )
            })
            .when(any_running, |this| {
                this.child(
                    Button::new("stop-all")
                        .ghost()
                        .small()
                        .label("停止全部")
                        .on_click(cx.listener(|state, _e, _w, cx| state.stop_all_current(cx))),
                )
            });

        let content = self.render_tab_content(&profile, window, cx);

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .pt_1()
                    .border_b_1()
                    .border_color(border)
                    .child(tab_bar),
            )
            .child(toolbar)
            .child(content)
            .into_any_element()
    }
}
