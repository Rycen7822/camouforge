use super::*;

impl AppState {
    pub(super) fn render_log_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let surface = cx.theme().secondary;
        let fg = cx.theme().foreground;
        div()
            .id("log-panel")
            .flex()
            .flex_col()
            .h(px(170.))
            .flex_shrink_0()
            .border_t_1()
            .border_color(border)
            .bg(surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(28.))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(fg)
                            .child(format!("运行日志（{}）", self.logs.len())),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("close-logs")
                            .ghost()
                            .small()
                            .label("收起")
                            .on_click(cx.listener(|state, _e, _w, cx| {
                                state.show_logs = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("log-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_3()
                    .py_1()
                    .flex_col()
                    .gap_0p5()
                    .children(self.logs.iter().map(|l| {
                        div()
                            .text_xs()
                            .text_color(theme::log_level_color(l.level))
                            .child(l.text.clone())
                    })),
            )
    }

    pub(super) fn render_statusbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let surface = cx.theme().secondary;
        let last = self.logs.last().cloned();
        let (dot_color, env_text) = match &self.python.status {
            python_env::PythonEnvPhase::Ready => {
                if self.worker.ready {
                    (theme::GREEN, "worker 就绪".to_string())
                } else {
                    (theme::YELLOW, "Python 环境就绪，worker 连接中…".to_string())
                }
            }
            other => {
                let color = if *other == python_env::PythonEnvPhase::Failed {
                    theme::RED
                } else {
                    theme::YELLOW
                };
                (color, other.label())
            }
        };
        let running_count = self.worker.running.len();

        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .h(px(26.))
            .flex_shrink_0()
            .border_t_1()
            .border_color(border)
            .bg(surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(status_dot(dot_color))
                    .child(div().text_xs().text_color(muted).child(env_text)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("运行实例: {running_count}")),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .when_some(last, |this, log| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme::log_level_color(log.level))
                                .child(log.text),
                        )
                    }),
            )
            .child(
                Button::new("toggle-logs")
                    .ghost()
                    .small()
                    .label(format!("日志 {}", self.logs.len()))
                    .on_click(cx.listener(|state, _e, _w, cx| {
                        state.show_logs = !state.show_logs;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("open-data-dir")
                    .text_xs()
                    .text_color(muted)
                    .cursor_pointer()
                    .on_click(cx.listener(|state, _e, _w, cx| {
                        let dir = state.data_dir.clone();
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                        #[cfg(not(target_os = "windows"))]
                        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                        cx.notify();
                    }))
                    .child(format!("数据: {}", self.data_dir.display())),
            )
    }
}
