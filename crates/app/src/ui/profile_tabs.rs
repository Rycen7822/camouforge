use super::*;

impl AppState {
    pub(super) fn render_tab_content(
        &mut self,
        profile: &camoforge_protocol::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use camoforge_protocol::registry;

        match self.tab.clone() {
            Tab::Launch => launch_panel::render(self, profile, window, cx),
            Tab::Advanced => launch_panel::render_advanced(self, profile, window, cx),
            Tab::RawJson => self.render_raw_json_panel(window, cx),
            other => {
                let group_ids = other.group_ids();
                if group_ids.is_empty() {
                    return div().into_any_element();
                }
                let tab_name = format!("{other:?}");
                let mut col = div()
                    .id(SharedString::from(format!("scroll-{tab_name}")))
                    .flex()
                    .flex_col()
                    .p_3()
                    .overflow_y_scroll()
                    .flex_1();

                for group_id in group_ids {
                    let Some(group) = registry::group_by_id(group_id) else {
                        continue;
                    };
                    col = col.child(widgets::section_header(group.title, cx));
                    for spec in group.fields {
                        let control: gpui::AnyElement = if spec.key == "fonts"
                            && !self.browsers.font_options.is_empty()
                        {
                            let opts = self.browsers.font_options.clone();
                            self.render_field_dynamic_multiselect(spec, profile, window, cx, &opts)
                        } else if spec.key == "voices" && !self.browsers.voice_options.is_empty() {
                            let opts = self.browsers.voice_options.clone();
                            self.render_field_dynamic_multiselect(spec, profile, window, cx, &opts)
                        } else if spec.key == "webGl:vendor"
                            && !self.browsers.webgl_cards.is_empty()
                        {
                            let cards = self.browsers.webgl_cards.clone();
                            let mut vendors: Vec<String> =
                                cards.iter().map(|c| c.vendor.clone()).collect();
                            vendors.sort();
                            vendors.dedup();
                            let current = profile
                                .config
                                .get("webGl:vendor")
                                .map(panels::value_to_text)
                                .unwrap_or_default();
                            let cache_key = format!("{}::{}", self.tab_key(), spec.key);
                            let sel = self.ensure_field_owned_select(
                                &cache_key, spec.key, current, vendors, window, cx,
                            );
                            Select::new(&sel)
                                .placeholder("选择 vendor")
                                .cleanable(true)
                                .into_any_element()
                        } else if spec.key == "webGl:renderer"
                            && !self.browsers.webgl_cards.is_empty()
                        {
                            let cards = self.browsers.webgl_cards.clone();
                            let vendor = profile
                                .config
                                .get("webGl:vendor")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let mut renderers: Vec<String> = cards
                                .iter()
                                .filter(|c| vendor.is_empty() || c.vendor == vendor)
                                .map(|c| c.renderer.clone())
                                .collect();
                            renderers.sort();
                            renderers.dedup();
                            let current = profile
                                .config
                                .get("webGl:renderer")
                                .map(panels::value_to_text)
                                .unwrap_or_default();
                            let cache_key = format!("{}::{}", self.tab_key(), spec.key);
                            let sel = self.ensure_field_owned_select(
                                &cache_key, spec.key, current, renderers, window, cx,
                            );
                            Select::new(&sel)
                                .placeholder("选择 renderer")
                                .cleanable(true)
                                .into_any_element()
                        } else {
                            match spec.kind {
                                registry::FieldKind::Select(_)
                                | registry::FieldKind::SelectInt(_) => {
                                    let select =
                                        self.ensure_field_select(spec, profile, window, cx);
                                    Select::new(&select)
                                        .placeholder(spec.placeholder)
                                        .cleanable(true)
                                        .into_any_element()
                                }
                                registry::FieldKind::SelectMulti(_) => {
                                    self.render_field_multiselect(spec, profile, window, cx)
                                }
                                registry::FieldKind::Bool => {
                                    let checked = profile
                                        .config
                                        .get(spec.key)
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    let key = spec.key.to_string();
                                    let cache_key = format!("{}::{}", self.tab_key(), spec.key);
                                    let switch_id = cache_key.clone();
                                    Switch::new(switch_id)
                                        .checked(checked)
                                        .on_click(cx.listener(
                                            move |state, checked: &bool, _w, cx| {
                                                let v = if *checked { "true" } else { "false" };
                                                state.set_config_value(&cache_key, &key, v, cx);
                                            },
                                        ))
                                        .into_any_element()
                                }
                                registry::FieldKind::Multiline | registry::FieldKind::Json => {
                                    let entity =
                                        self.ensure_field_textarea(spec, profile, window, cx);
                                    Textarea::new(&entity).h(px(96.)).into_any_element()
                                }
                                _ => {
                                    let entity = self.ensure_field_input(spec, profile, window, cx);
                                    Input::new(&entity).into_any_element()
                                }
                            }
                        };
                        let hint_extra = match spec.kind {
                            registry::FieldKind::TextList
                                if spec.key != "fonts" && spec.key != "voices" =>
                            {
                                "（逗号分隔）"
                            }
                            _ => "",
                        };
                        let hint = if hint_extra.is_empty() {
                            spec.hint.to_string()
                        } else {
                            format!("{} {hint_extra}", spec.hint)
                        };
                        let cache_key = format!("{}::{}", self.tab_key(), spec.key);
                        let error = self.profile.field_errors.get(&cache_key).cloned();
                        col = col.child(widgets::field_row_with_error(
                            spec.label,
                            &hint,
                            error.as_deref(),
                            control,
                            cx,
                        ));
                    }
                }
                col.into_any_element()
            }
        }
    }

    fn render_raw_json_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let text = self.profile.raw_json_text.clone();
        let entity = self
            .form
            .field_textareas
            .entry("__raw_json__".to_string())
            .or_insert_with(|| cx.new(|cx| TextareaState::new(window, cx).default_value(text)))
            .clone();

        div()
            .id("scroll-raw")
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .overflow_y_scroll()
            .flex_1()
            .child(widgets::section_header("config 原始 JSON", cx))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("直接编辑全部指纹注入键（点分键 → 值）。应用后覆盖表单值。"),
            )
            .child(Textarea::new(&entity).h(px(280.)))
            .child(
                Button::new("apply-json")
                    .primary()
                    .small()
                    .label("应用 JSON")
                    .on_click(cx.listener(|state, _e, _w, cx| {
                        if let Some(e) = state.form.field_textareas.get("__raw_json__") {
                            state.profile.raw_json_text = e.read(cx).value().to_string();
                        }
                        state.apply_raw_json(cx);
                    })),
            )
            .into_any_element()
    }
}
