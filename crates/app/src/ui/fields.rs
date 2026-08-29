use super::*;

impl AppState {
    pub(super) fn ensure_field_input(
        &mut self,
        spec: &camoforge_protocol::registry::FieldSpec,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let cache_key = format!("{}::{}", self.tab_key(), spec.key);
        if let Some(existing) = self.form.field_inputs.get(&cache_key) {
            return existing.clone();
        }

        let current = profile
            .config
            .get(spec.key)
            .map(panels::value_to_text)
            .unwrap_or_default();
        let key = spec.key.to_string();
        let error_key = cache_key.clone();
        let entity = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(spec.placeholder)
                .default_value(current)
        });
        cx.subscribe(
            &entity,
            move |this: &mut AppState,
                  input: Entity<InputState>,
                  event: &InputEvent,
                  cx: &mut Context<AppState>| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    this.set_config_value(&error_key, &key, &text, cx);
                }
            },
        )
        .detach();
        self.form.field_inputs.insert(cache_key, entity.clone());
        entity
    }

    pub(super) fn ensure_field_textarea(
        &mut self,
        spec: &camoforge_protocol::registry::FieldSpec,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let cache_key = format!("{}::{}", self.tab_key(), spec.key);
        if let Some(existing) = self.form.field_textareas.get(&cache_key) {
            return existing.clone();
        }

        let current = profile
            .config
            .get(spec.key)
            .map(panels::value_to_text)
            .unwrap_or_default();
        let key = spec.key.to_string();
        let error_key = cache_key.clone();
        let entity = cx.new(|cx| TextareaState::new(window, cx).default_value(current));
        cx.subscribe(
            &entity,
            move |this: &mut AppState,
                  textarea: Entity<TextareaState>,
                  event: &InputEvent,
                  cx: &mut Context<AppState>| {
                if matches!(event, InputEvent::Change) {
                    let text = textarea.read(cx).value().to_string();
                    this.set_config_value(&error_key, &key, &text, cx);
                }
            },
        )
        .detach();
        self.form.field_textareas.insert(cache_key, entity.clone());
        entity
    }

    pub(super) fn ensure_field_select(
        &mut self,
        spec: &camoforge_protocol::registry::FieldSpec,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<&'static str>>> {
        let cache_key = format!("{}::{}", self.tab_key(), spec.key);
        if let Some(existing) = self.form.field_selects.get(&cache_key) {
            return existing.clone();
        }

        let options: &'static [&'static str] = match spec.kind {
            camoforge_protocol::registry::FieldKind::Select(o)
            | camoforge_protocol::registry::FieldKind::SelectInt(o) => o,
            _ => &[],
        };
        let options_vec: Vec<&'static str> = options.to_vec();
        let current = profile
            .config
            .get(spec.key)
            .map(panels::value_to_text)
            .unwrap_or_default();
        let selected_index = options
            .iter()
            .position(|o| *o == current.as_str())
            .map(IndexPath::new);
        let key = spec.key.to_string();
        let error_key = cache_key.clone();
        let entity = cx.new(|cx| SelectState::new(options_vec, selected_index, window, cx));
        cx.subscribe(
            &entity,
            move |this: &mut AppState,
                  _select: Entity<SelectState<Vec<&'static str>>>,
                  event: &SelectEvent<Vec<&'static str>>,
                  cx: &mut Context<AppState>| {
                match event {
                    SelectEvent::Confirm(Some(value)) => {
                        this.set_config_value(&error_key, &key, value, cx);
                    }
                    // 清除按钮：移除该 config 键，回到未设置状态。
                    SelectEvent::Confirm(None) => {
                        this.set_config_value(&error_key, &key, "", cx);
                    }
                }
            },
        )
        .detach();
        self.form.field_selects.insert(cache_key, entity.clone());
        entity
    }

    pub(super) fn render_field_multiselect(
        &mut self,
        spec: &camoforge_protocol::registry::FieldSpec,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let options: &'static [&'static str] = match spec.kind {
            camoforge_protocol::registry::FieldKind::SelectMulti(o) => o,
            _ => &[],
        };
        let key = spec.key.to_string();

        let selected: Vec<String> = profile
            .config
            .get(&key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let add_cache = format!("{}::add::{}", self.tab_key(), key);
        let add_entity = if let Some(e) = self.form.field_selects.get(&add_cache) {
            e.clone()
        } else {
            let options_vec: Vec<&'static str> = options.to_vec();
            let e = cx.new(|cx| SelectState::new(options_vec, None, window, cx));
            let k = key.clone();
            let reset_key = add_cache.clone();
            cx.subscribe(
                &e,
                move |this: &mut AppState,
                      _select: Entity<SelectState<Vec<&'static str>>>,
                      event: &SelectEvent<Vec<&'static str>>,
                      cx: &mut Context<AppState>| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.add_config_list_item(&k, value, cx);
                        this.form.pending_select_resets.insert(reset_key.clone());
                        cx.notify();
                    }
                },
            )
            .detach();
            self.form.field_selects.insert(add_cache.clone(), e.clone());
            e
        };

        // 应用待重置（清空添加下拉选中，set_selected_index 需 window）。
        if self.form.pending_select_resets.remove(&add_cache) {
            if let Some(e) = self.form.field_selects.get(&add_cache).cloned() {
                e.update(cx, |s, cx| s.set_selected_index(None, window, cx));
            }
        }

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let surface = cx.theme().secondary;

        let mut chips = div().flex().flex_wrap().gap_1();
        for lang in &selected {
            let label = lang.clone();
            let remove_key = key.clone();
            let remove_val = lang.clone();
            let chip_id = SharedString::from(format!("lang-rm-{key}-{label}"));
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
                .child(div().text_xs().text_color(fg).child(label))
                .child(
                    div()
                        .id(chip_id)
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                            this.remove_config_list_item(&remove_key, &remove_val, cx);
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
                    .placeholder("添加…")
                    .into_any_element(),
            )
            .child(chips)
            .into_any_element()
    }

    pub(super) fn ensure_field_owned_select(
        &mut self,
        cache_key: &str,
        key: &str,
        current: String,
        options: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<String>>> {
        if let Some(e) = self.form.field_owned_selects.get(cache_key) {
            return e.clone();
        }
        let selected_index = options
            .iter()
            .position(|o| *o == current)
            .map(IndexPath::new);
        let entity =
            cx.new(|cx| SelectState::new(options, selected_index, window, cx).searchable(true));
        let key = key.to_string();
        let error_key = cache_key.to_string();
        cx.subscribe(
            &entity,
            move |this: &mut AppState,
                  _select: Entity<SelectState<Vec<String>>>,
                  event: &SelectEvent<Vec<String>>,
                  cx: &mut Context<AppState>| {
                match event {
                    SelectEvent::Confirm(Some(value)) => {
                        this.set_config_value(&error_key, &key, value, cx)
                    }
                    SelectEvent::Confirm(None) => this.set_config_value(&error_key, &key, "", cx),
                }
            },
        )
        .detach();
        self.form
            .field_owned_selects
            .insert(cache_key.to_string(), entity.clone());
        entity
    }

    pub(super) fn render_field_dynamic_multiselect(
        &mut self,
        spec: &camoforge_protocol::registry::FieldSpec,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
        options: &[String],
    ) -> gpui::AnyElement {
        let key = spec.key.to_string();

        let selected: Vec<String> = profile
            .config
            .get(&key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let add_cache = format!("{}::add::{}", self.tab_key(), key);
        let add_entity = if let Some(e) = self.form.field_owned_selects.get(&add_cache) {
            e.clone()
        } else {
            let options_vec = options.to_vec();
            let e = cx.new(|cx| SelectState::new(options_vec, None, window, cx).searchable(true));
            let k = key.clone();
            let reset_key = add_cache.clone();
            cx.subscribe(
                &e,
                move |this: &mut AppState,
                      _select: Entity<SelectState<Vec<String>>>,
                      event: &SelectEvent<Vec<String>>,
                      cx: &mut Context<AppState>| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.add_config_list_item(&k, value, cx);
                        this.form.pending_select_resets.insert(reset_key.clone());
                        cx.notify();
                    }
                },
            )
            .detach();
            self.form
                .field_owned_selects
                .insert(add_cache.clone(), e.clone());
            e
        };

        // 应用待重置（清空添加下拉，set_selected_index 需 window）。
        if self.form.pending_select_resets.remove(&add_cache) {
            if let Some(e) = self.form.field_owned_selects.get(&add_cache).cloned() {
                e.update(cx, |s, cx| s.set_selected_index(None, window, cx));
            }
        }

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let surface = cx.theme().secondary;

        let mut chips = div().flex().flex_wrap().gap_1();
        for item in &selected {
            let label = item.clone();
            let remove_key = key.clone();
            let remove_val = item.clone();
            let chip_id = SharedString::from(format!("dyn-rm-{key}-{label}"));
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
                            this.remove_config_list_item(&remove_key, &remove_val, cx);
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
                    .placeholder("添加…")
                    .into_any_element(),
            )
            .child(chips)
            .into_any_element()
    }

    pub(super) fn tab_key(&self) -> String {
        format!("{:?}", self.tab)
    }
}
