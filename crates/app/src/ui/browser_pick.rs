//! 浏览器版本下拉（官方 release + 已安装合并列表）与下载进度浮层。

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window,
};
use gpui_component::searchable_list::{
    SearchableListChange, SearchableListDelegate, SearchableListItem,
};
use gpui_component::select::{SelectEvent, SelectState};
use gpui_component::spinner::Spinner;
use gpui_component::{
    h_flex, ActiveTheme as _, Icon, IconName, IndexPath, Sizable as _, StyledExt as _,
};

use crate::browser::{self, DownloadPhase};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct BrowserVersionItem {
    pub full: String,
    pub label: String,
    pub installed: bool,
    pub downloading: bool,
    pub app: Entity<AppState>,
}

impl SearchableListItem for BrowserVersionItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.full
    }

    fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let color = if self.installed { fg } else { muted };
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .child(div().text_sm().text_color(color).child(self.label.clone()))
            .when(self.downloading, |row| row.child(Spinner::new()))
            .when(!self.installed && !self.downloading, |row| {
                let app = self.app.clone();
                let full = self.full.clone();
                row.child(
                    div()
                        .id(SharedString::from(format!("dl-{full}")))
                        .px_1()
                        .cursor_pointer()
                        .on_click(move |_e, _w, cx| {
                            cx.stop_propagation();
                            app.update(cx, |st, cx| st.start_download_version(&full, cx));
                        })
                        .child(
                            Icon::new(IconName::ArrowDown)
                                .with_size(px(14.))
                                .text_color(muted),
                        ),
                )
            })
    }
}

pub struct VersionItems {
    items: Vec<BrowserVersionItem>,
    matched: Vec<BrowserVersionItem>,
    select_installed_only: bool,
}

impl VersionItems {
    pub fn new(items: Vec<BrowserVersionItem>, select_installed_only: bool) -> Self {
        Self {
            matched: items.clone(),
            items,
            select_installed_only,
        }
    }
}

impl SearchableListDelegate for VersionItems {
    type Item = BrowserVersionItem;

    fn items_count(&self, _section: usize) -> usize {
        self.matched.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.matched.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.matched
            .iter()
            .position(|v| v.value() == value)
            .map(|ix| IndexPath::default().row(ix))
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        _cx: &mut App,
    ) -> gpui::Task<()> {
        let q = query.to_lowercase();
        self.matched = self
            .items
            .iter()
            .filter(|i| q.is_empty() || i.label.to_lowercase().contains(&q))
            .cloned()
            .collect();
        gpui::Task::ready(())
    }

    fn is_item_enabled(&self, _ix: IndexPath, item: &Self::Item, _cx: &App) -> bool {
        if self.select_installed_only {
            item.installed
        } else {
            true
        }
    }

    fn on_will_change(
        &mut self,
        selection: &mut Vec<(IndexPath, Self::Item)>,
        changes: &[SearchableListChange],
    ) {
        for change in changes {
            // 单选语义：Select 整体替换；Deselect 一律忽略，否则否决路径会先清空旧选择
            let SearchableListChange::Select { index } = change else {
                continue;
            };
            let Some(item) = self.item(*index) else {
                continue;
            };
            if self.select_installed_only && !item.installed {
                // 否决未安装项：选中态留在原版本（Confirm 会重发当前值，订阅方幂等跳过）
                continue;
            }
            selection.clear();
            selection.push((*index, item.clone()));
        }
    }
}

/// 创建/复用版本下拉；数据代次（`version_revision`）变化时 set_items 刷新并按值重定位选中。
pub fn version_select(
    state: &mut AppState,
    cache_key: &str,
    installed_only: bool,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Entity<SelectState<VersionItems>> {
    // 渲染期重扫安装目录（仅几个 stat）：手动增删版本目录无需重启即被识别
    let now_installed = browser::scan_installed(&state.browsers.releases_root);
    if now_installed != state.browsers.installed_versions {
        state.browsers.installed_versions = now_installed;
        state.browsers.version_revision += 1;
        cx.notify();
    }

    let app = cx.entity();
    if let Some(e) = state.form.version_selects.get(cache_key).cloned() {
        if state.form.version_select_revisions.get(cache_key)
            != Some(&state.browsers.version_revision)
        {
            let items = version_items(state, &app);
            let relocate = current_index(
                &items,
                desired_version_value(state, installed_only),
                installed_only,
            );
            e.update(cx, |s, cx| {
                s.set_items(VersionItems::new(items, installed_only), window, cx);
                // set_items 不迁移选中态：列表重排后旧行索引会指向别的版本，按值重定位
                s.set_selected_index(relocate, window, cx);
            });
            state
                .form
                .version_select_revisions
                .insert(cache_key.to_string(), state.browsers.version_revision);
        }
        return e;
    }

    let items = version_items(state, &app);
    let selected = current_index(
        &items,
        desired_version_value(state, installed_only),
        installed_only,
    );
    let entity = cx.new(|cx| {
        SelectState::new(
            VersionItems::new(items, installed_only),
            selected,
            window,
            cx,
        )
        .searchable(true)
    });
    let sub_key = cache_key.to_string();
    cx.subscribe(
        &entity,
        move |this: &mut AppState,
              _select: Entity<SelectState<VersionItems>>,
              event: &SelectEvent<VersionItems>,
              cx: &mut Context<AppState>| {
            let SelectEvent::Confirm(Some(full)) = event else {
                return;
            };
            if this.form.installed_only_selects.contains(&sub_key) {
                let exe = this
                    .browsers
                    .releases_root
                    .join(full.as_str())
                    .join(browser::exe_name());
                let exe_text = exe.to_string_lossy().into_owned();
                if let Some(p) = this.current_mut() {
                    // 否决未安装项时 Confirm 会重发当前值：路径未变则跳过
                    if p.launch.executable_path.as_deref() == Some(exe_text.as_str()) {
                        return;
                    }
                    p.launch.executable_path = Some(exe_text.clone());
                    p.updated_at = camoforge_protocol::unix_ts();
                }
                this.persist_current();
                this.form
                    .pending_field_values
                    .insert("launch::ExecutablePath".to_string(), exe_text.clone());
                this.log("info", format!("浏览器路径已设为 {exe_text}"));
            } else {
                this.browsers.settings_release_pick = Some(full.clone());
            }
            cx.notify();
        },
    )
    .detach();
    if installed_only {
        state
            .form
            .installed_only_selects
            .insert(cache_key.to_string());
    }
    state
        .form
        .version_select_revisions
        .insert(cache_key.to_string(), state.browsers.version_revision);
    state
        .form
        .version_selects
        .insert(cache_key.to_string(), entity.clone());
    entity
}

/// 下拉应选中的版本：身份页 = profile 路径指向 releases_root 下的版本；
/// 设置页 = settings_release_pick（与下载按钮读的同一状态源）。
fn desired_version_value(state: &AppState, installed_only: bool) -> Option<String> {
    if !installed_only {
        return state.browsers.settings_release_pick.clone();
    }
    let exe = state.current()?.launch.executable_path.as_ref()?;
    let dir = std::path::Path::new(exe).parent()?;
    if dir.parent()? != state.browsers.releases_root {
        return None;
    }
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

fn current_index(
    items: &[BrowserVersionItem],
    desired: Option<String>,
    installed_only: bool,
) -> Option<IndexPath> {
    desired
        .and_then(|v| {
            items
                .iter()
                .position(|i| i.full == v && (!installed_only || i.installed))
        })
        .map(|row| IndexPath::default().row(row))
}

pub fn version_items(state: &AppState, app: &Entity<AppState>) -> Vec<BrowserVersionItem> {
    let mut items: Vec<BrowserVersionItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for full in &state.browsers.installed_versions {
        seen.insert(full.clone());
        items.push(BrowserVersionItem {
            label: format!("{full}（已安装）"),
            full: full.clone(),
            installed: true,
            downloading: false,
            app: app.clone(),
        });
    }
    if let Some(releases) = &state.browsers.releases {
        for rel in releases {
            if seen.contains(&rel.full) {
                continue;
            }
            let downloading = state.browsers.downloads.contains_key(&rel.full);
            items.push(BrowserVersionItem {
                label: rel.full.clone(),
                full: rel.full.clone(),
                installed: false,
                downloading,
                app: app.clone(),
            });
        }
    }
    items
}

pub fn render_download_overlay(
    state: &mut AppState,
    cx: &mut Context<AppState>,
) -> Option<AnyElement> {
    if state.browsers.downloads.is_empty() {
        return None;
    }
    let fg = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let surface = cx.theme().secondary;
    let border = cx.theme().border;
    let mut cards: Vec<AnyElement> = Vec::new();
    for dl in state.browsers.downloads.values() {
        let phase_text = match dl.phase {
            DownloadPhase::Downloading => {
                if dl.total > 0 {
                    format!(
                        "下载中 {:.1} / {:.1} MB",
                        dl.downloaded as f64 / 1048576.0,
                        dl.total as f64 / 1048576.0
                    )
                } else {
                    format!("下载中 {:.1} MB", dl.downloaded as f64 / 1048576.0)
                }
            }
            DownloadPhase::Extracting => "解压中…".to_string(),
        };
        cards.push(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(fg)
                                .child(format!("camoufox {}", dl.full)),
                        )
                        .child(div().text_xs().text_color(muted).child(phase_text)),
                )
                .child(
                    gpui_component::progress::Progress::new(SharedString::from(format!(
                        "dl-progress-{}",
                        dl.full
                    )))
                    .value(dl.percent()),
                )
                .into_any_element(),
        );
    }
    Some(
        div()
            .absolute()
            .bottom(px(36.))
            .right(px(12.))
            .w(px(340.))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(surface)
            .border_1()
            .border_color(border)
            .shadow_lg()
            .children(cards)
            .into_any_element(),
    )
}
