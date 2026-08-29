use super::*;

impl AppState {
    pub(super) fn render_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg = cx.theme().sidebar;
        let border = cx.theme().border;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let list_hover = cx.theme().list_hover;
        let list_active = cx.theme().list_active;
        let view = cx.entity();

        let filter_entity = if let Some(e) = self.profile.filter_input.clone() {
            e
        } else {
            let e = cx.new(|cx| InputState::new(window, cx).placeholder("搜索身份…"));
            cx.subscribe(
                &e,
                |this: &mut AppState,
                 input: Entity<InputState>,
                 event: &InputEvent,
                 cx: &mut Context<AppState>| {
                    if matches!(event, InputEvent::Change) {
                        this.profile.filter = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            )
            .detach();
            self.profile.filter_input = Some(e.clone());
            e
        };

        let filter = self.profile.filter.to_lowercase();
        let mut list = div().flex().flex_col().gap_0p5();
        for p in self
            .profile
            .profiles
            .iter()
            .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(&filter))
        {
            let id = p.id.clone();
            let name = p.name.clone();
            let running = self.worker.running.contains_key(&id);
            let selected = self.profile.selected.as_deref() == Some(id.as_str());
            let renaming = self.profile.renaming.as_deref() == Some(id.as_str());

            if renaming {
                let entity = if let Some(e) = self.profile.rename_input.clone() {
                    e
                } else {
                    let e = cx.new(|cx| InputState::new(window, cx).default_value(name.clone()));
                    cx.subscribe(
                        &e,
                        move |this: &mut AppState,
                              input: Entity<InputState>,
                              event: &InputEvent,
                              cx: &mut Context<AppState>| {
                            match event {
                                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                                    let text = input.read(cx).value().to_string();
                                    this.rename_profile(&text, cx);
                                }
                                _ => {}
                            }
                        },
                    )
                    .detach();
                    self.profile.rename_input = Some(e.clone());
                    e
                };
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .h(px(34.))
                        .flex_shrink_0()
                        .rounded_md()
                        .bg(list_active)
                        .child(
                            div()
                                .id("rename-esc")
                                .flex_1()
                                .on_key_down(cx.listener(
                                    move |state, ev: &KeyDownEvent, _w, cx| {
                                        if ev.keystroke.key.as_str() == "escape" {
                                            state.cancel_rename(cx);
                                        }
                                    },
                                ))
                                .child(Input::new(&entity).appearance(false)),
                        ),
                );
            } else {
                let handle = self
                    .profile
                    .sidebar_focus
                    .entry(id.clone())
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone();
                let row_fg = if selected { fg } else { muted };
                let menu_id = id.clone();
                let nav_id = id.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("profile-{id}")))
                        .track_focus(&handle)
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .h(px(34.))
                        .flex_shrink_0()
                        .cursor_pointer()
                        .rounded_md()
                        .when(selected, |s| s.bg(list_active))
                        .when(!selected, |s| s.hover(|h| h.bg(list_hover)))
                        .focus(|s| s.bg(list_hover))
                        .on_click(cx.listener(move |state, e: &ClickEvent, _w, cx| {
                            if e.click_count() == 2 {
                                state.start_rename(id.clone());
                                cx.notify();
                            } else {
                                state.select_sidebar_profile(&id, cx);
                            }
                        }))
                        .on_key_down(cx.listener(move |state, ev: &KeyDownEvent, w, cx| {
                            let id = nav_id.clone();
                            match ev.keystroke.key.as_str() {
                                "enter" | "space" => state.select_sidebar_profile(&id, cx),
                                "arrowdown" => state.move_sidebar_focus(&id, 1, w, cx),
                                "arrowup" => state.move_sidebar_focus(&id, -1, w, cx),
                                _ => {}
                            }
                        }))
                        .child(
                            Icon::new(IconName::CircleUser)
                                .with_size(px(14.))
                                .text_color(row_fg),
                        )
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(div().text_sm().text_color(row_fg).child(name)),
                        )
                        .when(running, |this| this.child(status_dot(theme::GREEN)))
                        .context_menu({
                            let view = view.clone();
                            let rid = menu_id.clone();
                            move |menu, _window, _cx| {
                                let (v1, r1) = (view.clone(), rid.clone());
                                let (v2, r2) = (view.clone(), rid.clone());
                                let (v3, r3) = (view.clone(), rid.clone());
                                menu.item(PopupMenuItem::new("重命名").on_click(
                                    move |_ev, _w, cx| {
                                        v1.update(cx, |state, cx| {
                                            state.start_rename(r1.clone());
                                            cx.notify();
                                        });
                                    },
                                ))
                                .item(PopupMenuItem::new("复制").on_click(move |_ev, _w, cx| {
                                    v2.update(cx, |state, cx| state.duplicate_profile(&r2, cx));
                                }))
                                .separator()
                                .item(PopupMenuItem::new("删除").on_click(move |_ev, w, cx| {
                                    // 在事件处理期读出数据；对话框 builder 在 render 期被调用，
                                    // 不能再次读取正在渲染的 AppState（gpui 禁止双重借用）。
                                    let (name, running) = {
                                        let s = v3.read(cx);
                                        let name = s
                                            .profile
                                            .profiles
                                            .iter()
                                            .find(|p| p.id == r3)
                                            .map(|p| p.name.clone())
                                            .unwrap_or_else(|| r3.clone());
                                        (name, s.worker.running.contains_key(&r3))
                                    };
                                    let dlg_view = v3.clone();
                                    let dlg_rid = r3.clone();
                                    w.open_alert_dialog(cx, move |alert, _w, _cx| {
                                        let desc = if running {
                                            format!(
                                                "确定要删除身份「{name}」吗？该身份正在运行，请先停止再删除。"
                                            )
                                        } else {
                                            format!("确定要永久删除身份「{name}」吗？此操作不可恢复。")
                                        };
                                        let ok_view = dlg_view.clone();
                                        let ok_rid = dlg_rid.clone();
                                        alert
                                            .confirm()
                                            .title("删除身份")
                                            .description(desc)
                                            .button_props(
                                                DialogButtonProps::default()
                                                    .ok_text("删除")
                                                    .ok_variant(ButtonVariant::Danger)
                                                    .show_cancel(true)
                                                    .on_ok(move |_ev, _w, cx| {
                                                        ok_view.update(cx, |state, cx| {
                                                            state.delete_profile(&ok_rid, cx)
                                                        });
                                                        true
                                                    }),
                                            )
                                    });
                                }))
                            }
                        }),
                );
            }
        }
        let list = if self.profile.filter.is_empty() {
            list
        } else {
            list.child(
                div()
                    .px_2()
                    .py_4()
                    .child(div().text_xs().text_color(muted).child("无匹配身份")),
            )
        };
        let list = if self.profile.profiles.is_empty() {
            list.child(
                div().px_2().py_4().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("暂无身份，点击「新建身份」"),
                ),
            )
        } else {
            list
        };

        div()
            .w(px(240.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(bg)
            .border_r_1()
            .border_color(border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .px_3()
                    .pt_4()
                    .pb_3()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_base()
                                    .font_semibold()
                                    .text_color(fg)
                                    .child("CamouForge"),
                            )
                            .child(div().text_xs().text_color(muted).child("指纹浏览器控制台")),
                    )
                    .child(
                        Button::new("new-profile")
                            .primary()
                            .small()
                            .w_full()
                            .icon(IconName::Plus)
                            .label("新建身份")
                            .on_click(cx.listener(|state, _e, _w, cx| state.new_profile(cx))),
                    ),
            )
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(border)
                    .child(Input::new(&filter_entity)),
            )
            .child(
                div()
                    .id("profile-list")
                    .p_2()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(
                div().p_2().border_t_1().border_color(border).child(
                    Button::new("app-settings")
                        .ghost()
                        .small()
                        .w_full()
                        .icon(IconName::Settings)
                        .label("应用设置")
                        .selected(self.current_view == AppView::Settings)
                        .on_click(cx.listener(|state, _e, _w, cx| {
                            state.show_settings(cx);
                        })),
                ),
            )
    }
}
