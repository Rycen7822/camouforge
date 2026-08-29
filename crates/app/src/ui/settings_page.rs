use super::*;

impl AppState {
    fn render_python_env_card(&mut self, cx: &mut Context<AppState>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let surface = cx.theme().secondary;
        let border = cx.theme().border;

        let worker_running = self
            .worker
            .supervisor
            .as_ref()
            .map(|s| s.is_running())
            .unwrap_or(false);
        let busy = self.python.busy;
        let failed = self.python.status == python_env::PythonEnvPhase::Failed;
        let btn_label = if failed {
            "重试修复"
        } else {
            "检查并修复"
        };
        let phase_color = if failed { theme::RED } else { muted };

        let mut card = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_md()
            .bg(surface)
            .border_1()
            .border_color(border)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(fg)
                    .child("Python worker 运行环境"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "首次启动自动下载 uv 与托管 CPython 3.12 并按锁文件同步依赖；无需预装 Python。环境目录：{}；依赖真源：{} 与同目录 uv.lock/uv.toml",
                        self.python.layout.venv_dir.display(),
                        self.python.layout.pyproject.display()
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(phase_color)
                    .child(self.python.status.label()),
            );
        if let Some(info) = &self.python.info {
            card = card.child(div().text_xs().text_color(muted).child(format!(
                "Python {} · uv {} · {}",
                info.python_version,
                info.uv_version,
                info.python.display()
            )));
        }
        if let Some(err) = &self.python.error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(theme::RED)
                    .child(format!("错误：{err}")),
            );
        }
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("repair-python-env")
                        .small()
                        .disabled(busy || worker_running)
                        .label(if busy { "处理中…" } else { btn_label })
                        .on_click(cx.listener(|state, _e, _w, cx| {
                            state.ensure_python_env(python_env::EnsureMode::ForceRepair, cx)
                        })),
                )
                .child(div().text_xs().text_color(muted).child(if worker_running {
                    "worker 运行中，无法修复；请先退出应用再重试".to_string()
                } else if failed {
                    "修复会重建环境，已存在的环境会保留到重建成功".to_string()
                } else {
                    String::new()
                })),
        );
        card
    }

    pub(super) fn render_settings_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let surface = cx.theme().secondary;
        let border = cx.theme().border;
        let current = self.settings.close_action;

        let release_entity =
            browser_pick::version_select(self, "settings::release-download", false, window, cx);
        let release_status = if let Some(err) = self.browsers.releases_error.clone() {
            (format!("列表拉取失败：{err}（可点「刷新」重试）"), true)
        } else if self.browsers.releases_fetching {
            ("正在拉取官方 release 列表…".to_string(), false)
        } else if self.browsers.releases.is_none() {
            ("尚未获取到 release 列表".to_string(), false)
        } else {
            (
                format!(
                    "共 {} 个官方版本，已安装 {} 个；保存目录：{}",
                    self.browsers
                        .releases
                        .as_ref()
                        .map(|r| r.len())
                        .unwrap_or(0),
                    self.browsers.installed_versions.len(),
                    self.browsers.releases_root.display(),
                ),
                false,
            )
        };

        let options: &[(CloseAction, &str, &str)] = &[
            (
                CloseAction::Confirm,
                "询问",
                "关闭时弹窗：退出程序 / 最小化到托盘",
            ),
            (CloseAction::Exit, "直接退出", "关闭窗口直接退出程序"),
            (
                CloseAction::MinimizeToTray,
                "最小化到托盘",
                "关闭窗口直接最小化到托盘，进程常驻",
            ),
        ];

        div()
            .id("settings-page")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .p_4()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_semibold()
                    .text_color(fg)
                    .child("应用设置"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded_md()
                    .bg(surface)
                    .border_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(fg)
                            .child("camoufox 浏览器下载"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                "从 GitHub 官方 releases（daijro/camoufox）下载到上方目录的版本子文件夹；列表每次启动时自动更新。",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        Select::new(&release_entity)
                                            .placeholder("选择要下载的版本…")
                                            .menu_max_h(px(320.)),
                                    ),
                            )
                            .child(
                                Button::new("download-release")
                                    .primary()
                                    .small()
                                    .disabled(
                                        self.browsers.settings_release_pick.is_none()
                                            || !self.browsers.downloads.is_empty(),
                                    )
                                    .label(if self.browsers.downloads.is_empty() {
                                        "下载"
                                    } else {
                                        "下载中…"
                                    })
                                    .on_click(cx.listener(|state, _e, _w, cx| {
                                        let Some(full) = state.browsers.settings_release_pick.clone()
                                        else {
                                            state.log("warn", "请先选择要下载的版本");
                                            cx.notify();
                                            return;
                                        };
                                        state.start_download_version(&full, cx);
                                    })),
                            )
                            .child(
                                Button::new("refresh-releases")
                                    .ghost()
                                    .small()
                                    .disabled(self.browsers.releases_fetching)
                                    .label("刷新")
                                    .on_click(cx.listener(|state, _e, _w, cx| {
                                        state.refresh_releases(cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if release_status.1 { crate::ui::theme::RED } else { muted })
                            .child(release_status.0),
                    ),
            )
            .child(self.render_python_env_card(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded_md()
                    .bg(surface)
                    .border_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(fg)
                            .child("关闭窗口时"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(options.iter().map(|(action, label, desc)| {
                                let action = *action;
                                let selected = current == action;
                                let desc = desc.to_string();
                                div()
                                    .id(SharedString::from(format!("close-action-{label}")))
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .when(selected, |this| this.bg(surface))
                                    .hover(|this| this.bg(cx.theme().list_hover))
                                    .on_click(cx.listener(move |state, _e, _w, cx| {
                                        state.set_close_action(action, cx);
                                    }))
                                    .child(
                                        div()
                                            .w(px(20.))
                                            .h(px(20.))
                                            .rounded_full()
                                            .border_2()
                                            .border_color(if selected {
                                                cx.theme().primary
                                            } else {
                                                border
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(selected, |this| {
                                                this.child(
                                                    div()
                                                        .w(px(10.))
                                                        .h(px(10.))
                                                        .rounded_full()
                                                        .bg(cx.theme().primary),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(fg)
                                                    .child(label.to_string()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(desc),
                                            ),
                                    )
                                    .into_any_element()
                            })),
                    ),
            )
    }
}
