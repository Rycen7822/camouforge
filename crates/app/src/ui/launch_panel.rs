//! 启动参数面板：编辑 `Profile.launch`（camoufox launch_options() 全参数）。

use camoforge_protocol::Profile;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::switch::Switch;
use gpui_component::*;

use crate::state::AppState;
use crate::ui::browser_pick;
use crate::ui::launch_fields::{get_text, set_text, Spec, ADVANCED_SPECS, F, SPECS};
use crate::ui::widgets::{field_row_with_error, section_header};

fn apply_field_text(
    state: &mut AppState,
    field: F,
    text: &str,
    error_key: &str,
    cx: &mut Context<AppState>,
) {
    let result = state
        .current_mut()
        .map(|profile| {
            let result = set_text(profile, field, text);
            if result.is_ok() {
                profile.updated_at = camoforge_protocol::unix_ts();
            }
            result
        })
        .unwrap_or(Ok(()));

    match result {
        Ok(()) => {
            state.profile.field_errors.remove(error_key);
            state.schedule_persist_current(cx);
        }
        Err(message) => {
            state
                .profile
                .field_errors
                .insert(error_key.to_string(), message);
        }
    }
    cx.notify();
}

pub fn render(
    state: &mut AppState,
    profile: &Profile,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> gpui::AnyElement {
    render_specs(
        state,
        profile,
        window,
        cx,
        "launch",
        "启动参数（camoufox launch_options 全量）",
        "留空 = 由 SDK 默认行为接管。指纹优先级：fingerprint > config 手动键 > OS 自动生成。",
        SPECS,
    )
}

pub fn render_advanced(
    state: &mut AppState,
    profile: &Profile,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> gpui::AnyElement {
    render_specs(
        state,
        profile,
        window,
        cx,
        "advanced",
        "高级参数（addons / 调试 / 自定义二进制）",
        "这些参数通常无需修改，仅在有明确需求时填写；JSON 字段需合法 JSON 对象。",
        ADVANCED_SPECS,
    )
}

#[allow(clippy::too_many_arguments)] // 既有签名：title/hint/specs 与 key_prefix 逐项传参
fn render_specs(
    state: &mut AppState,
    profile: &Profile,
    window: &mut Window,
    cx: &mut Context<AppState>,
    key_prefix: &str,
    title: &str,
    hint: &str,
    specs: &[Spec],
) -> gpui::AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut col = div()
        .id(SharedString::from(format!("scroll-{key_prefix}")))
        .flex()
        .flex_col()
        .p_3()
        .gap_1()
        .overflow_y_scroll()
        .flex_1();

    col = col.child(section_header(title, cx));
    col = col.child(div().text_xs().text_color(muted).child(hint.to_string()));

    if key_prefix == "launch" {
        let settings_entity = if let Some(e) = state.settings_input.clone() {
            e
        } else {
            let current = state.settings.camoufox_dir.clone().unwrap_or_default();
            let e = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("如 D:\\dev\\camoufox")
                    .default_value(current)
            });
            cx.subscribe(
                &e,
                |this: &mut AppState,
                 input: Entity<InputState>,
                 event: &InputEvent,
                 cx: &mut Context<AppState>| {
                    match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let text = input.read(cx).value().to_string();
                            this.set_camoufox_dir(&text, cx);
                        }
                        _ => {}
                    }
                },
            )
            .detach();
            state.settings_input = Some(e.clone());
            e
        };

        col = col.child(section_header("本地 camoufox 浏览器", cx));
        col = col.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(
                    "指定后启动/校验都使用该目录下的 camoufox.exe（如 D:\\dev\\camoufox）；留空 = 自动探测（pip 缓存或兄弟目录）。",
                ),
        );
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(Input::new(&settings_entity)))
                .child(
                    Button::new("pick-camoufox")
                        .outline()
                        .small()
                        .label("选择…")
                        .on_click(cx.listener(|_state, _e, _w, cx| {
                            let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: false,
                                prompt: Some("选择 camoufox 浏览器目录".into()),
                            });
                            cx.spawn(async move |weak, cx| {
                                let picked = rx.await;
                                if let Ok(Ok(Some(paths))) = picked {
                                    if let Some(p) = paths.first() {
                                        let dir = p.to_string_lossy().into_owned();
                                        weak.update(cx, |state, cx| {
                                            state.set_camoufox_dir(&dir, cx);
                                            state
                                                .form
                                                .pending_field_values
                                                .insert("__camoufox_dir__".to_string(), dir);
                                        })
                                        .ok();
                                    }
                                }
                            })
                            .detach();
                        })),
                )
                .child(
                    Button::new("clear-camoufox")
                        .ghost()
                        .small()
                        .label("清除")
                        .on_click(cx.listener(|state, _e, _w, cx| {
                            state.set_camoufox_dir("", cx);
                            state
                                .form
                                .pending_field_values
                                .insert("__camoufox_dir__".to_string(), String::new());
                        })),
                ),
        );
        let (status_text, status_err) = if let Some(dir) = state.settings.camoufox_dir.clone() {
            match state.effective_camoufox_exe() {
                Some(exe) => (format!("当前生效：{exe}"), false),
                None => (format!("目录不存在或缺少 camoufox.exe：{dir}"), true),
            }
        } else {
            (
                "自动探测：CAMOUFOX_EXECUTABLE_PATH → camouforge 兄弟目录 → pip 缓存".into(),
                false,
            )
        };
        col = col.child(
            div()
                .text_xs()
                .text_color(if status_err {
                    crate::ui::theme::RED
                } else {
                    muted
                })
                .child(status_text),
        );

        let version_entity =
            browser_pick::version_select(state, "launch::downloaded-versions", true, window, cx);
        col = col.child(section_header("已下载的浏览器版本", cx));
        col = col.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!(
                    "目录 {} 下的版本；选中已安装版本即写入「Firefox 路径」，灰色未安装版本可点右侧 ↓ 下载。",
                    state.browsers.releases_root.display(),
                )),
        );
        col = col.child(
            Select::new(&version_entity)
                .placeholder("选择已下载版本…")
                .menu_max_h(px(320.)),
        );

        col = col.child(section_header("网页快捷方式", cx));
        col = col.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("启动后作为浏览器首页展示的快捷方式，点击即在当前标签页打开对应网站。"),
        );
        {
            let shortcuts = profile.launch.shortcuts.clone();
            if shortcuts.is_empty() {
                col = col.child(div().text_xs().text_color(muted).child("（暂无快捷方式）"));
            }
            for (idx, s) in shortcuts.iter().enumerate() {
                let name_key = format!("{key_prefix}::shortcut::{idx}::name");
                let url_key = format!("{key_prefix}::shortcut::{idx}::url");
                let name_entity = if let Some(e) = state.form.shortcut_inputs.get(&name_key) {
                    e.clone()
                } else {
                    let e = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("名称")
                            .default_value(s.name.clone())
                    });
                    cx.subscribe(
                        &e,
                        move |this: &mut AppState,
                              input: Entity<InputState>,
                              event: &InputEvent,
                              cx: &mut Context<AppState>| {
                            if matches!(event, InputEvent::Change) {
                                let text = input.read(cx).value().to_string();
                                this.set_shortcut_field(idx, Some(&text), None, cx);
                            }
                        },
                    )
                    .detach();
                    state
                        .form
                        .shortcut_inputs
                        .insert(name_key.clone(), e.clone());
                    e
                };
                let url_entity = if let Some(e) = state.form.shortcut_inputs.get(&url_key) {
                    e.clone()
                } else {
                    let e = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("网址，如 github.com")
                            .default_value(s.url.clone())
                    });
                    cx.subscribe(
                        &e,
                        move |this: &mut AppState,
                              input: Entity<InputState>,
                              event: &InputEvent,
                              cx: &mut Context<AppState>| {
                            if matches!(event, InputEvent::Change) {
                                let text = input.read(cx).value().to_string();
                                this.set_shortcut_field(idx, None, Some(&text), cx);
                            }
                        },
                    )
                    .detach();
                    state
                        .form
                        .shortcut_inputs
                        .insert(url_key.clone(), e.clone());
                    e
                };
                let rm_idx = idx;
                let rm_id = SharedString::from(format!("rm-shortcut-{idx}"));
                col = col.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .w(px(24.))
                                .child(format!("{}.", idx + 1)),
                        )
                        .child(div().flex_1().min_w_0().child(Input::new(&name_entity)))
                        .child(div().flex_1().min_w_0().child(Input::new(&url_entity)))
                        .child(Button::new(rm_id).ghost().small().label("删除").on_click(
                            cx.listener(move |this, _e, _w, cx| {
                                this.remove_shortcut(rm_idx, cx);
                            }),
                        )),
                );
            }
        }
        col = col.child(
            Button::new("add-shortcut")
                .outline()
                .small()
                .label("＋ 添加快捷方式")
                .on_click(cx.listener(|this, _e, _w, cx| {
                    this.add_shortcut(cx);
                })),
        );
    }

    let mut current_group: Option<&str> = None;
    for spec in specs {
        if current_group != Some(spec.group) {
            current_group = Some(spec.group);
            col = col.child(section_header(spec.group, cx));
        }
        let cache_key = format!("{key_prefix}::{:?}", spec.field);
        let element: gpui::AnyElement = if spec.field == F::Os {
            let options: &'static [&'static str] = &["windows", "macos", "linux"];
            let selected: Vec<String> = profile.launch.os.clone().unwrap_or_default();

            let add_cache = format!("{cache_key}::add");
            let add_entity = if let Some(e) = state.form.field_selects.get(&add_cache) {
                e.clone()
            } else {
                let options_vec: Vec<&'static str> = options.to_vec();
                let e = cx.new(|cx| SelectState::new(options_vec, None, window, cx));
                let reset_key = add_cache.clone();
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          _select: Entity<SelectState<Vec<&'static str>>>,
                          event: &SelectEvent<Vec<&'static str>>,
                          cx: &mut Context<AppState>| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.add_launch_os(value, cx);
                            this.form.pending_select_resets.insert(reset_key.clone());
                            cx.notify();
                        }
                    },
                )
                .detach();
                state
                    .form
                    .field_selects
                    .insert(add_cache.clone(), e.clone());
                e
            };

            // 应用待重置（清空添加下拉，set_selected_index 需 window）。
            if state.form.pending_select_resets.remove(&add_cache) {
                if let Some(e) = state.form.field_selects.get(&add_cache).cloned() {
                    e.update(cx, |s, cx| s.set_selected_index(None, window, cx));
                }
            }

            let fg = cx.theme().foreground;
            let border = cx.theme().border;
            let surface = cx.theme().secondary;

            let mut chips = div().flex().flex_wrap().gap_1();
            for os in &selected {
                let label = os.clone();
                let remove_val = os.clone();
                let chip_id = SharedString::from(format!("os-rm-{label}"));
                let chip = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(24.))
                    .rounded_md()
                    .bg(surface)
                    .border_1()
                    .border_color(border)
                    .child(div().text_xs().text_color(fg).child(label.clone()))
                    .child(
                        div()
                            .id(chip_id)
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                                this.remove_launch_os(&remove_val, cx);
                            }))
                            .child(
                                Icon::new(IconName::Close)
                                    .with_size(px(12.))
                                    .text_color(muted),
                            ),
                    );
                chips = chips.child(chip);
            }

            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Select::new(&add_entity)
                        .placeholder("添加 OS…")
                        .into_any_element(),
                )
                .child(chips)
                .into_any_element()
        } else if spec.field == F::Browser && !state.browsers.browser_versions.is_empty() {
            let cache_key = format!("{key_prefix}::Browser");
            let options = state.browsers.browser_versions.clone();
            let current = get_text(profile, F::Browser);
            let entity = if let Some(e) = state.form.field_owned_selects.get(&cache_key) {
                e.clone()
            } else {
                let selected_index = options
                    .iter()
                    .position(|o| *o == current)
                    .map(IndexPath::new);
                let e = cx.new(|cx| SelectState::new(options, selected_index, window, cx));
                let error_key = cache_key.clone();
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          _select: Entity<SelectState<Vec<String>>>,
                          event: &SelectEvent<Vec<String>>,
                          cx: &mut Context<AppState>| {
                        match event {
                            SelectEvent::Confirm(Some(value)) => {
                                apply_field_text(this, F::Browser, value, &error_key, cx);
                            }
                            SelectEvent::Confirm(None) => {
                                apply_field_text(this, F::Browser, "", &error_key, cx);
                            }
                        }
                    },
                )
                .detach();
                state
                    .form
                    .field_owned_selects
                    .insert(cache_key.clone(), e.clone());
                e
            };
            Select::new(&entity)
                .placeholder("选择已装版本")
                .cleanable(true)
                .into_any_element()
        } else if spec.is_bool {
            let checked = get_text(profile, spec.field) == "true";
            let field = spec.field;
            let error_key = cache_key.clone();
            Switch::new(cache_key.clone())
                .checked(checked)
                .on_click(cx.listener(move |this, checked: &bool, _w, cx| {
                    let v = if *checked { "true" } else { "false" };
                    apply_field_text(this, field, v, &error_key, cx);
                }))
                .into_any_element()
        } else if let Some(options) = spec.enum_options {
            let select_entity = if let Some(e) = state.form.field_selects.get(&cache_key) {
                e.clone()
            } else {
                let field = spec.field;
                let current = get_text(profile, spec.field);
                let options_vec: Vec<&'static str> = options.to_vec();
                let selected_index = options
                    .iter()
                    .position(|o| *o == current.as_str())
                    .map(IndexPath::new);
                let e = cx.new(|cx| SelectState::new(options_vec, selected_index, window, cx));
                let error_key = cache_key.clone();
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          _select: Entity<SelectState<Vec<&'static str>>>,
                          event: &SelectEvent<Vec<&'static str>>,
                          cx: &mut Context<AppState>| {
                        match event {
                            SelectEvent::Confirm(Some(value)) => {
                                apply_field_text(this, field, value, &error_key, cx);
                            }
                            SelectEvent::Confirm(None) => {
                                apply_field_text(this, field, "", &error_key, cx);
                            }
                        }
                    },
                )
                .detach();
                state
                    .form
                    .field_selects
                    .insert(cache_key.clone(), e.clone());
                e
            };
            Select::new(&select_entity)
                .placeholder(spec.placeholder)
                .cleanable(true)
                .into_any_element()
        } else if matches!(
            spec.field,
            F::Fingerprint | F::FirefoxUserPrefs | F::Env | F::ExtraLaunchOptions
        ) {
            let entity = if let Some(e) = state.form.field_textareas.get(&cache_key) {
                e.clone()
            } else {
                let field = spec.field;
                let current = get_text(profile, spec.field);
                let error_key = cache_key.clone();
                let e = cx.new(|cx| TextareaState::new(window, cx).default_value(current));
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          textarea: Entity<TextareaState>,
                          event: &InputEvent,
                          cx: &mut Context<AppState>| {
                        if matches!(event, InputEvent::Change) {
                            let text = textarea.read(cx).value().to_string();
                            apply_field_text(this, field, &text, &error_key, cx);
                        }
                    },
                )
                .detach();
                state
                    .form
                    .field_textareas
                    .insert(cache_key.clone(), e.clone());
                e
            };
            Textarea::new(&entity).h(px(96.)).into_any_element()
        } else if spec.field == F::DownloadsDir {
            let field = spec.field;
            let error_key = cache_key.clone();
            let entity = if let Some(e) = state.form.field_inputs.get(&cache_key) {
                e.clone()
            } else {
                let current = get_text(profile, spec.field);
                let e = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(spec.placeholder)
                        .default_value(current)
                });
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          input: Entity<InputState>,
                          event: &InputEvent,
                          cx: &mut Context<AppState>| {
                        if matches!(event, InputEvent::Change) {
                            let text = input.read(cx).value().to_string();
                            apply_field_text(this, field, &text, &error_key, cx);
                        }
                    },
                )
                .detach();
                state.form.field_inputs.insert(cache_key.clone(), e.clone());
                e
            };
            let pick_key = cache_key.clone();
            // w_full：field_row 的控件容器是横向 flex，不写宽度时整行会缩成内容宽，
            // 输入框被压到几乎不可见（选中的路径也"看起来没写上"）。
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w_0().child(Input::new(&entity)))
                .child(
                    Button::new("pick-downloads-dir")
                        .outline()
                        .small()
                        .label("选择…")
                        .on_click(cx.listener(move |_this, _e, _w, cx| {
                            let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: false,
                                prompt: Some("选择下载保存目录".into()),
                            });
                            let key = pick_key.clone();
                            cx.spawn(async move |weak, cx| {
                                let picked = rx.await;
                                if let Ok(Ok(Some(paths))) = picked {
                                    if let Some(p) = paths.first() {
                                        let dir = p.to_string_lossy().into_owned();
                                        weak.update(cx, move |this, cx| {
                                            apply_field_text(this, F::DownloadsDir, &dir, &key, cx);
                                            this.form.pending_field_values.insert(key, dir);
                                        })
                                        .ok();
                                    }
                                }
                            })
                            .detach();
                        })),
                )
                .into_any_element()
        } else {
            let entity = if let Some(e) = state.form.field_inputs.get(&cache_key) {
                e.clone()
            } else {
                let field = spec.field;
                let current = get_text(profile, spec.field);
                let error_key = cache_key.clone();
                let e = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(spec.placeholder)
                        .default_value(current)
                });
                cx.subscribe(
                    &e,
                    move |this: &mut AppState,
                          input: Entity<InputState>,
                          event: &InputEvent,
                          cx: &mut Context<AppState>| {
                        if matches!(event, InputEvent::Change) {
                            let text = input.read(cx).value().to_string();
                            apply_field_text(this, field, &text, &error_key, cx);
                        }
                    },
                )
                .detach();
                state.form.field_inputs.insert(cache_key.clone(), e.clone());
                e
            };
            let masked = spec.field == F::ProxyPass;
            Input::new(&entity)
                .when(masked, |i| i.mask_toggle())
                .into_any_element()
        };
        let error = state.profile.field_errors.get(&cache_key).cloned();
        col = col.child(field_row_with_error(
            spec.label,
            spec.hint,
            error.as_deref(),
            element,
            cx,
        ));
    }

    if key_prefix == "launch" {
        col = col.child(section_header("WebGL 组合（SDK 精确值）", cx));
        col = col.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("点击一行写入 vendor / renderer。先点「加载」从 SDK webgl 数据库读取。"),
        );
        col = col.child(
            Button::new("load-webgl")
                .outline()
                .small()
                .label("加载 WebGL 组合")
                .on_click(cx.listener(|state, _e, _w, cx| {
                    let os = state
                        .current()
                        .and_then(|p| p.launch.os.clone())
                        .and_then(|v| v.first().cloned())
                        .unwrap_or_else(|| "macos".into());
                    state.load_webgl_cards(&os, cx);
                })),
        );
        if !state.browsers.webgl_cards.is_empty() {
            let card_hover = cx.theme().secondary;
            for card in state.browsers.webgl_cards.iter().take(30) {
                let vendor = card.vendor.clone();
                let renderer = card.renderer.clone();
                let label = format!("{} — {} ({:.1}%)", vendor, renderer, card.weight * 100.0);
                col = col.child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .hover(move |s| s.bg(card_hover))
                        .cursor_pointer()
                        .text_xs()
                        .id(SharedString::from(format!("webgl-{vendor}-{renderer}")))
                        .on_click(cx.listener(move |state, _e, _w, cx| {
                            state.apply_webgl_card(&vendor, &renderer, cx);
                        }))
                        .child(label),
                );
            }
        }
    }

    col.into_any_element()
}
