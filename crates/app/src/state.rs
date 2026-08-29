//! UI 根状态：Profile 列表、编辑输入框缓存、运行实例、supervisor 桥接。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use camoforge_protocol::{Profile, WebglCard};
use gpui::{Context, Entity, FocusHandle, Window};

use crate::browser::{self, DownloadMsg, DownloadPhase, DownloadState, Release};
use crate::logging::LogSink;
use crate::python_env;
use crate::settings::{AppSettings, CloseAction};
use crate::store::ProfileStore;
use crate::supervisor::{SupervisorEvent, WorkerSupervisor};
use crate::ui::browser_pick::VersionItems;
use gpui_component::input::{InputState, TextareaState};
use gpui_component::select::SelectState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Profiles,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    Launch,
    Navigator,
    ScreenWindow,
    WebglMedia,
    GeoLocale,
    Network,
    FontsAudio,
    FpSource,
    Advanced,
    RawJson,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[
        Tab::Launch,
        Tab::Navigator,
        Tab::ScreenWindow,
        Tab::WebglMedia,
        Tab::GeoLocale,
        Tab::Network,
        Tab::FontsAudio,
        Tab::FpSource,
        Tab::Advanced,
        Tab::RawJson,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Launch => "启动",
            Tab::Navigator => "导航器",
            Tab::ScreenWindow => "屏幕窗口",
            Tab::WebglMedia => "WebGL/媒体",
            Tab::GeoLocale => "地理/语言",
            Tab::Network => "网络",
            Tab::FontsAudio => "字体/声音",
            Tab::FpSource => "指纹来源",
            Tab::Advanced => "高级",
            Tab::RawJson => "原始 JSON",
        }
    }

    /// 对应 registry 分组列表（空 = 非 registry 驱动的面板）。
    pub fn group_ids(&self) -> &'static [&'static str] {
        match self {
            Tab::Navigator => &["navigator"],
            Tab::ScreenWindow => &["window", "screen", "document"],
            Tab::WebglMedia => &["webgl", "media_audio"],
            Tab::GeoLocale => &["geo_intl"],
            Tab::Network => &["webrtc", "headers"],
            Tab::FontsAudio => &["fonts_voices"],
            Tab::FpSource => &["battery", "humanize", "misc"],
            Tab::Advanced => &[],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: &'static str,
    pub text: String,
}

pub(crate) struct ProfileState {
    pub store: Arc<ProfileStore>,
    pub profiles: Vec<Profile>,
    pub selected: Option<String>,
    pub renaming: Option<String>,
    pub rename_input: Option<Entity<InputState>>,
    pub field_errors: HashMap<String, String>,
    pub filter: String,
    pub filter_input: Option<Entity<InputState>>,
    pub sidebar_focus: HashMap<String, FocusHandle>,
    pub raw_json_text: String,
}

impl ProfileState {
    fn new(store: ProfileStore) -> Self {
        let profiles = store.list();
        Self {
            store: Arc::new(store),
            profiles,
            selected: None,
            renaming: None,
            rename_input: None,
            field_errors: HashMap::new(),
            filter: String::new(),
            filter_input: None,
            sidebar_focus: HashMap::new(),
            raw_json_text: String::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct FormCache {
    pub field_inputs: HashMap<String, Entity<InputState>>,
    pub shortcut_inputs: HashMap<String, Entity<InputState>>,
    pub field_selects: HashMap<String, Entity<SelectState<Vec<&'static str>>>>,
    pub field_textareas: HashMap<String, Entity<TextareaState>>,
    pub field_owned_selects: HashMap<String, Entity<SelectState<Vec<String>>>>,
    pub pending_field_values: HashMap<String, String>,
    pub pending_select_resets: HashSet<String>,
    pub version_selects: HashMap<String, Entity<SelectState<VersionItems>>>,
    pub version_select_revisions: HashMap<String, u64>,
    pub installed_only_selects: HashSet<String>,
}

pub(crate) struct WorkerState {
    pub running: HashMap<String, camoforge_protocol::InstanceInfo>,
    pub launching: bool,
    pub validating: bool,
    pub generating: bool,
    pub ready: bool,
    pub supervisor: Option<Arc<WorkerSupervisor>>,
    pub event_rx: Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<SupervisorEvent>>>>,
    pub pending_auto_launch: Option<String>,
    pub launch_error: Option<String>,
}

impl WorkerState {
    fn new(pending_auto_launch: Option<String>) -> Self {
        Self {
            running: HashMap::new(),
            launching: false,
            validating: false,
            generating: false,
            ready: false,
            supervisor: None,
            event_rx: None,
            pending_auto_launch,
            launch_error: None,
        }
    }
}

pub(crate) struct BrowserCatalog {
    pub webgl_cards: Vec<WebglCard>,
    pub font_options: Vec<String>,
    pub voice_options: Vec<String>,
    pub browser_versions: Vec<String>,
    pub releases: Option<Vec<Release>>,
    pub releases_fetching: bool,
    pub releases_error: Option<String>,
    pub installed_versions: Vec<String>,
    pub releases_root: PathBuf,
    pub downloads: HashMap<String, DownloadState>,
    pub download_tx: Option<std::sync::mpsc::Sender<DownloadMsg>>,
    pub download_rx: Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<DownloadMsg>>>>,
    pub version_revision: u64,
    pub settings_release_pick: Option<String>,
}

impl BrowserCatalog {
    fn new(releases_root: PathBuf, installed_versions: Vec<String>) -> Self {
        let (download_tx, download_rx) = std::sync::mpsc::channel::<DownloadMsg>();
        Self {
            webgl_cards: Vec::new(),
            font_options: Vec::new(),
            voice_options: Vec::new(),
            browser_versions: Vec::new(),
            releases: None,
            releases_fetching: true,
            releases_error: None,
            installed_versions,
            releases_root,
            downloads: HashMap::new(),
            download_tx: Some(download_tx),
            download_rx: Some(Arc::new(std::sync::Mutex::new(download_rx))),
            version_revision: 0,
            settings_release_pick: None,
        }
    }
}

pub(crate) struct PythonEnvState {
    pub layout: python_env::RuntimeLayout,
    pub status: python_env::PythonEnvPhase,
    pub busy: bool,
    pub info: Option<python_env::PythonEnvInfo>,
    pub error: Option<String>,
    pub tx: Option<std::sync::mpsc::Sender<python_env::PythonEnvPhase>>,
    pub progress_rx:
        Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<python_env::PythonEnvPhase>>>>,
    pub guard: python_env::EnsureGuard,
}

impl PythonEnvState {
    fn new(layout: python_env::RuntimeLayout) -> Self {
        let (tx, progress_rx) = std::sync::mpsc::channel::<python_env::PythonEnvPhase>();
        Self {
            layout,
            status: python_env::PythonEnvPhase::Checking,
            busy: false,
            info: None,
            error: None,
            tx: Some(tx),
            progress_rx: Some(Arc::new(std::sync::Mutex::new(progress_rx))),
            guard: python_env::EnsureGuard::default(),
        }
    }
}

pub struct AppState {
    pub(crate) data_dir: PathBuf,
    pub(crate) profile: ProfileState,
    pub(crate) form: FormCache,
    pub(crate) worker: WorkerState,
    pub(crate) browsers: BrowserCatalog,
    pub(crate) python: PythonEnvState,
    pub(crate) current_view: AppView,
    pub(crate) tab: Tab,
    pub(crate) show_logs: bool,
    pub(crate) settings: AppSettings,
    pub(crate) settings_input: Option<Entity<InputState>>,
    pub(crate) logs: Vec<LogLine>,
    pub(crate) log_sink: LogSink,
    pub(crate) pending_notifications: Vec<(bool, String)>,
}

impl AppState {
    pub fn new(
        data_dir: PathBuf,
        store: ProfileStore,
        log_sink: LogSink,
        auto_launch: Option<String>,
        runtime_layout: python_env::RuntimeLayout,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AppSettings::load(&data_dir);
        // 无 view 上下文处（窗口关闭回调）读取的设置走全局镜像
        settings.sync_globals();
        let releases_root = browser::releases_root();
        // 启动时无进行中下载，此处的 .part-* 都是上次中断的残留
        browser::cleanup_part_dirs(&releases_root);
        let installed_versions = browser::scan_installed(&releases_root);
        let state = Self {
            data_dir,
            profile: ProfileState::new(store),
            form: FormCache::default(),
            worker: WorkerState::new(auto_launch),
            browsers: BrowserCatalog::new(releases_root, installed_versions),
            python: PythonEnvState::new(runtime_layout),
            current_view: AppView::Profiles,
            tab: Tab::Launch,
            show_logs: false,
            settings,
            settings_input: None,
            logs: vec![LogLine {
                level: "info",
                text: "CamouForge 就绪。新建或选择一个身份开始。".into(),
            }],
            log_sink,
            pending_notifications: Vec::new(),
        };
        // release 列表每次打开更新一次；失败不阻塞界面
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { browser::fetch_releases() })
                .await;
            weak.update(cx, |state, cx| {
                state.browsers.releases_fetching = false;
                match &result {
                    Ok(list) => {
                        state.browsers.releases = Some(list.clone());
                        state.log(
                            "info",
                            format!("已获取官方 release 列表（{} 个版本）", list.len()),
                        );
                    }
                    Err(e) => {
                        state.browsers.releases_error = Some(format!("{e:#}"));
                        state.log("warn", format!("release 列表拉取失败: {e:#}"));
                    }
                }
                state.browsers.version_revision += 1;
                cx.notify();
            })
            .ok();
        })
        .detach();
        state
    }

    pub fn current(&self) -> Option<&Profile> {
        self.profile
            .profiles
            .iter()
            .find(|p| Some(&p.id) == self.profile.selected.as_ref())
    }

    pub fn current_mut(&mut self) -> Option<&mut Profile> {
        self.profile
            .profiles
            .iter_mut()
            .find(|p| Some(&p.id) == self.profile.selected.as_ref())
    }

    pub(crate) fn mutate_current_profile(
        &mut self,
        cx: &mut Context<Self>,
        mutate: impl FnOnce(&mut Profile),
    ) {
        let Some(profile) = self.current_mut() else {
            return;
        };
        mutate(profile);
        profile.updated_at = camoforge_protocol::unix_ts();
        self.persist_current();
        cx.notify();
    }

    pub fn target_os(&self) -> String {
        self.current()
            .and_then(|p| p.launch.os.clone())
            .and_then(|v| v.first().cloned())
            .unwrap_or_else(|| "macos".to_string())
    }

    pub fn log(&mut self, level: &'static str, text: impl Into<String>) {
        let text = text.into();
        self.log_sink.write(level, &text);
        self.logs.push(LogLine { level, text });
        if self.logs.len() > 200 {
            self.logs.drain(..100);
        }
    }

    pub fn new_profile(&mut self, cx: &mut Context<Self>) {
        let mut n = self.profile.profiles.len() + 1;
        while self
            .profile
            .profiles
            .iter()
            .any(|p| p.name == format!("身份 {n}"))
        {
            n += 1;
        }
        let name = format!("身份 {n}");
        let p = Profile::new(name);
        match self.profile.store.save(&p) {
            Ok(_) => {
                self.profile.selected = Some(p.id.clone());
                self.current_view = AppView::Profiles;
                self.clear_field_caches();
                self.profile.profiles = self.profile.store.list();
                self.log("info", format!("已创建 {}", p.name));
                cx.notify();
            }
            Err(e) => self.log("error", format!("创建失败: {e}")),
        }
    }

    pub fn duplicate_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(src) = self.profile.profiles.iter().find(|p| p.id == id).cloned() {
            let mut copy = src.clone();
            copy.id = uuid::Uuid::new_v4().to_string();
            copy.name = format!("{} 副本", src.name);
            copy.created_at = camoforge_protocol::unix_ts();
            copy.updated_at = copy.created_at;
            if let Err(e) = self.profile.store.save(&copy) {
                self.log("error", format!("复制失败: {e}"));
                return;
            }
            self.profile.selected = Some(copy.id.clone());
            self.current_view = AppView::Profiles;
            self.clear_field_caches();
            self.profile.profiles = self.profile.store.list();
            self.log("info", format!("已复制为 {}", copy.name));
            cx.notify();
        }
    }

    pub fn delete_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.worker.running.contains_key(id) {
            self.log("error", "该身份正在运行，请先停止再删除");
            return;
        }
        match self.profile.store.delete(id) {
            Ok(_) => {
                self.log("info", "已删除");
                if self.profile.selected.as_deref() == Some(id) {
                    self.profile.selected = None;
                }
                if self.profile.renaming.as_deref() == Some(id) {
                    self.profile.renaming = None;
                    self.profile.rename_input = None;
                }
                self.clear_field_caches();
                self.profile.profiles = self.profile.store.list();
                cx.notify();
            }
            Err(e) => self.log("error", format!("删除失败: {e}")),
        }
    }

    pub fn start_rename(&mut self, id: String) {
        self.profile.renaming = Some(id);
        self.profile.rename_input = None;
    }

    pub fn rename_profile(&mut self, new_name: &str, cx: &mut Context<Self>) {
        let Some(id) = self.profile.renaming.clone() else {
            return;
        };
        let new_name = new_name.trim().to_string();
        self.profile.renaming = None;
        self.profile.rename_input = None;
        if new_name.is_empty() {
            self.log("warn", "名称不能为空，已取消重命名");
            cx.notify();
            return;
        }
        if let Some(src) = self.profile.profiles.iter().find(|p| p.id == id).cloned() {
            let mut updated = src;
            updated.name = new_name;
            updated.updated_at = camoforge_protocol::unix_ts();
            if let Err(e) = self.profile.store.save(&updated) {
                self.log("error", format!("重命名失败: {e}"));
                return;
            }
            self.profile.profiles = self.profile.store.list();
            self.log("info", "已重命名");
        }
        cx.notify();
    }

    /// 自动保存：把当前 profile 写入磁盘（原子写）。失败只记日志，不打断编辑。
    pub fn persist_current(&mut self) {
        let Some(p) = self.current().cloned() else {
            return;
        };
        if let Err(e) = self.profile.store.save(&p) {
            self.log("error", format!("自动保存失败: {e}"));
        }
    }

    fn clear_field_caches(&mut self) {
        self.form.field_inputs.clear();
        self.form.shortcut_inputs.clear();
        self.form.field_selects.clear();
        self.form.field_owned_selects.clear();
        self.form.field_textareas.clear();
        self.form.pending_select_resets.clear();
        self.profile.field_errors.clear();
        // 版本下拉的选中态派生自当前 profile 的 executable_path，随 profile 切换重建
        self.form.version_selects.clear();
        self.form.version_select_revisions.clear();
    }

    /// 若当前处于「原始 JSON」tab，把当前 config 序列化并同步到面板输入框。
    /// 修复：切换 tab / 切换 profile 时只刷新字符串、缓存输入框不更新的过期快照回滚问题。
    pub fn refresh_raw_json_snapshot(&mut self) {
        if self.tab != Tab::RawJson {
            return;
        }
        if let Some(p) = self.current() {
            let json = serde_json::to_string_pretty(&p.config).unwrap_or_default();
            self.profile.raw_json_text = json.clone();
            self.form
                .pending_field_values
                .insert("__raw_json__".to_string(), json);
        }
    }

    pub fn reload_profiles_from_disk(&mut self) {
        if let Err(e) = self.profile.store.reload() {
            self.log("error", format!("重读 profile 失败: {e}"));
        }
        self.profile.profiles = self.profile.store.list();
    }

    pub fn select_sidebar_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        self.current_view = AppView::Profiles;
        // 先重读磁盘：外部直接改 JSON 时，选中即显示最新值
        self.reload_profiles_from_disk();
        if self.profile.profiles.iter().any(|p| p.id == id) {
            self.profile.selected = Some(id.to_string());
        } else if let Some(first) = self.profile.profiles.first() {
            // 目标 profile 已被外部删除：退到列表第一个
            self.profile.selected = Some(first.id.clone());
        } else {
            self.profile.selected = None;
        }
        // 输入/下拉实体以 tab::key 命名，跨 tab 不冲突；profile 不同则值不同，需整体重建
        self.clear_field_caches();
        self.refresh_raw_json_snapshot();
        cx.notify();
    }

    pub fn filtered_profiles(&self) -> Vec<&Profile> {
        let f = self.profile.filter.to_lowercase();
        self.profile
            .profiles
            .iter()
            .filter(|p| f.is_empty() || p.name.to_lowercase().contains(&f))
            .collect()
    }

    pub fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.reload_profiles_from_disk();
        // Tab 键已进入缓存 key，切换时保留实体；Raw JSON 需单独推送新快照。
        self.profile.raw_json_text.clear();
        self.refresh_raw_json_snapshot();

        let os = self.target_os();
        match self.tab {
            Tab::FontsAudio
                if self.browsers.font_options.is_empty()
                    || self.browsers.voice_options.is_empty() =>
            {
                self.load_font_voices_catalog(&os, cx);
            }
            Tab::WebglMedia if self.browsers.webgl_cards.is_empty() => {
                self.load_webgl_cards(&os, cx);
            }
            Tab::Advanced if self.browsers.browser_versions.is_empty() => {
                self.load_browser_versions(cx);
            }
            _ => {}
        }
        cx.notify();
    }

    /// 侧栏键盘方向键导航：把选中/焦点移到相邻 profile。
    pub fn move_sidebar_focus(
        &mut self,
        current: &str,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filtered = self.filtered_profiles();
        let Some(idx) = filtered.iter().position(|p| p.id == current) else {
            return;
        };
        let last = filtered.len().saturating_sub(1) as isize;
        let next = (idx as isize + delta).clamp(0, last) as usize;
        let next_id = filtered[next].id.clone();
        if let Some(handle) = self.profile.sidebar_focus.get(&next_id).cloned() {
            window.focus(&handle, cx);
        }
        self.select_sidebar_profile(&next_id, cx);
    }

    pub fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.profile.renaming = None;
        self.profile.rename_input = None;
        cx.notify();
    }

    pub fn show_settings(&mut self, cx: &mut Context<Self>) {
        self.current_view = AppView::Settings;
        cx.notify();
    }

    pub fn set_close_action(&mut self, action: CloseAction, cx: &mut Context<Self>) {
        if self.settings.close_action == action {
            return;
        }
        self.settings.close_action = action;
        self.settings.sync_globals();
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.log("error", format!("设置保存失败: {e}"));
        }
        cx.notify();
    }

    pub fn set_camoufox_dir(&mut self, dir: &str, cx: &mut Context<Self>) {
        let dir = dir.trim().to_string();
        self.settings.camoufox_dir = if dir.is_empty() { None } else { Some(dir) };
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.log("error", format!("设置保存失败: {e}"));
        }
        let effective = self
            .settings
            .executable_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "自动探测".into());
        self.log("info", format!("本地 camoufox：{effective}"));
        cx.notify();
    }

    /// 启动/校验时把本地 camoufox 注入 profile（profile 显式设置了 executable_path 则保留用户值）。
    pub fn apply_local_camoufox(&self, p: &mut camoforge_protocol::Profile) {
        if p.launch.executable_path.is_none() {
            if let Some(exe) = self.settings.executable_path() {
                if exe.exists() {
                    p.launch.executable_path = Some(exe.to_string_lossy().into_owned());
                }
            }
        }
    }

    pub fn effective_camoufox_exe(&self) -> Option<String> {
        let exe = self.settings.executable_path()?;
        if exe.exists() {
            Some(exe.to_string_lossy().into_owned())
        } else {
            None
        }
    }

    pub fn dirty_tick(&mut self, cx: &mut Context<Self>) {
        self.drain_events(cx);
        self.drain_downloads(cx);
        self.drain_python_env(cx);
    }

    /// 后台自举/修复 Python 环境；成功后启动 worker。
    /// worker 或浏览器实例运行中不得原地替换 .venv，直接拒绝。
    pub fn ensure_python_env(&mut self, mode: python_env::EnsureMode, cx: &mut Context<Self>) {
        if self.python.busy {
            return;
        }
        let worker_running = self
            .worker
            .supervisor
            .as_ref()
            .map(|s| s.is_running())
            .unwrap_or(false);
        if worker_running || !self.worker.running.is_empty() {
            self.log(
                "warn",
                "worker 或浏览器实例运行中，不能修复 Python 环境；请先全部停止或退出应用",
            );
            cx.notify();
            return;
        }
        if !self.python.guard.try_enter() {
            return;
        }
        self.python.busy = true;
        self.python.status = python_env::PythonEnvPhase::Checking;
        self.python.error = None;
        let layout = self.python.layout.clone();
        let tx = self.python.tx.clone().expect("python env tx");
        let started = std::time::Instant::now();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    python_env::ensure_python_env(&layout, mode, |phase| {
                        let _ = tx.send(phase);
                    })
                })
                .await;
            weak.update(cx, |state, cx| {
                state.python.guard.exit();
                state.python.busy = false;
                match result {
                    Ok(info) => {
                        state.python.status = python_env::PythonEnvPhase::Ready;
                        state.python.error = None;
                        state.python.info = Some(info.clone());
                        state.log(
                            "info",
                            format!(
                                "Python 环境就绪（Python {} · uv {} · {} · 用时 {:.1}s）",
                                info.python_version,
                                info.uv_version,
                                info.venv.display(),
                                started.elapsed().as_secs_f32()
                            ),
                        );
                        if let Some(sup) = &state.worker.supervisor {
                            if sup.is_shutting_down() {
                                return;
                            }
                            if let Err(e) = sup.start() {
                                state.python.status = python_env::PythonEnvPhase::Failed;
                                state.python.error = Some(format!("worker 启动失败：{e:#}"));
                                state.log("error", format!("worker 启动失败: {e:#}"));
                            }
                        }
                    }
                    Err(e) => {
                        let full = format!("{e:#}");
                        state.python.status = python_env::PythonEnvPhase::Failed;
                        state.python.error = Some(clean_error(&full));
                        state.log("error", format!("Python 环境配置失败: {full}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn drain_python_env(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = &self.python.progress_rx else {
            return;
        };
        let rx = rx.lock().unwrap();
        let mut changed = false;
        while let Ok(phase) = rx.try_recv() {
            self.python.status = phase;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub fn refresh_releases(&mut self, cx: &mut Context<Self>) {
        if self.browsers.releases_fetching {
            return;
        }
        self.browsers.releases_fetching = true;
        self.log("info", "拉取官方 release 列表 …");
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { browser::fetch_releases() })
                .await;
            weak.update(cx, |state, cx| {
                state.browsers.releases_fetching = false;
                match &result {
                    Ok(list) => {
                        state.browsers.releases = Some(list.clone());
                        state.browsers.releases_error = None;
                        state.log(
                            "info",
                            format!("已获取官方 release 列表（{} 个版本）", list.len()),
                        );
                    }
                    Err(e) => {
                        state.browsers.releases_error = Some(format!("{e:#}"));
                        state.log("warn", format!("release 列表拉取失败: {e:#}"));
                    }
                }
                state.browsers.version_revision += 1;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn start_download_version(&mut self, full: &str, cx: &mut Context<Self>) {
        if self.browsers.downloads.contains_key(full) {
            return;
        }
        if self.browsers.installed_versions.iter().any(|v| v == full) {
            self.log("warn", format!("camoufox {full} 已安装，无需下载"));
            cx.notify();
            return;
        }
        if !self.browsers.downloads.is_empty() {
            self.log("warn", "已有下载任务进行中，请等待完成后再试");
            cx.notify();
            return;
        }
        let Some(rel) = self
            .browsers
            .releases
            .as_ref()
            .and_then(|list| list.iter().find(|r| r.full == full))
            .cloned()
        else {
            self.log(
                "error",
                format!("版本 {full} 不在官方 release 列表中，无法下载"),
            );
            cx.notify();
            return;
        };
        let Some(tx) = self.browsers.download_tx.clone() else {
            return;
        };
        self.browsers.downloads.insert(
            full.to_string(),
            DownloadState {
                full: full.to_string(),
                downloaded: 0,
                total: rel.size,
                phase: DownloadPhase::Downloading,
            },
        );
        self.browsers.version_revision += 1;
        self.log(
            "info",
            format!(
                "开始下载 camoufox {full}（{:.1} MB）",
                rel.size as f64 / 1048576.0
            ),
        );
        let root = self.browsers.releases_root.clone();
        std::thread::spawn(move || match browser::download_release(&rel, &root, &tx) {
            Ok(()) => {
                let _ = tx.send(DownloadMsg::Done {
                    full: rel.full.clone(),
                });
            }
            Err(e) => {
                let _ = tx.send(DownloadMsg::Failed {
                    full: rel.full.clone(),
                    error: format!("{e:#}"),
                });
            }
        });
        cx.notify();
    }

    fn drain_downloads(&mut self, cx: &mut Context<Self>) {
        let msgs: Vec<DownloadMsg> = {
            let Some(rx) = &self.browsers.download_rx else {
                return;
            };
            let rx = rx.lock().unwrap();
            std::iter::from_fn(|| rx.try_recv().ok()).collect()
        };
        if msgs.is_empty() {
            return;
        }
        for msg in msgs {
            match msg {
                DownloadMsg::Progress {
                    full,
                    downloaded,
                    total,
                } => {
                    if let Some(dl) = self.browsers.downloads.get_mut(&full) {
                        dl.downloaded = downloaded;
                        dl.total = total;
                    }
                }
                DownloadMsg::Extracting { full } => {
                    if let Some(dl) = self.browsers.downloads.get_mut(&full) {
                        dl.phase = DownloadPhase::Extracting;
                    }
                }
                DownloadMsg::Done { full } => {
                    self.browsers.downloads.remove(&full);
                    self.browsers.installed_versions =
                        browser::scan_installed(&self.browsers.releases_root);
                    self.browsers.version_revision += 1;
                    self.pending_notifications
                        .push((false, format!("camoufox {full} 下载完成")));
                    self.log("info", format!("camoufox {full} 下载完成"));
                }
                DownloadMsg::Failed { full, error } => {
                    self.browsers.downloads.remove(&full);
                    self.browsers.version_revision += 1;
                    self.pending_notifications
                        .push((true, format!("camoufox {full} 下载失败：{error}")));
                    self.log("error", format!("camoufox {full} 下载失败: {error}"));
                }
            }
        }
        cx.notify();
    }

    /// config 键值变更（来自 TextInput / Textarea on_change）。
    /// 解析失败不写入 config，改为记录行内错误；合法时清除错误并自动保存。
    pub fn set_config_value(
        &mut self,
        cache_key: &str,
        key: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let spec = camoforge_protocol::registry::find_spec(key);
        let parsed = if text.trim().is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            match spec {
                Some(s) => crate::ui::panels::text_to_value(text, s),
                None => Ok(serde_json::Value::String(text.to_string())),
            }
        };
        match parsed {
            Ok(value) => {
                self.profile.field_errors.remove(cache_key);
                if let Some(p) = self.current_mut() {
                    if value.is_null() {
                        p.config.remove(key);
                    } else {
                        p.config.insert(key.to_string(), value);
                    }
                    p.updated_at = camoforge_protocol::unix_ts();
                }
                self.persist_current();
            }
            Err(msg) => {
                // 保留上一次合法值；红字提示直到输入合法
                self.profile.field_errors.insert(cache_key.to_string(), msg);
            }
        }
        cx.notify();
    }

    pub fn add_config_list_item(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        self.mutate_current_profile(cx, |profile| {
            let entry = profile
                .config
                .entry(key.to_string())
                .or_insert_with(|| serde_json::Value::Array(vec![]));
            if let Some(items) = entry.as_array_mut() {
                if !items.iter().any(|item| item.as_str() == Some(value)) {
                    items.push(serde_json::Value::String(value.to_string()));
                }
            }
        });
    }

    pub fn remove_config_list_item(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        self.mutate_current_profile(cx, |profile| {
            let mut empty = false;
            if let Some(items) = profile
                .config
                .get_mut(key)
                .and_then(|value| value.as_array_mut())
            {
                items.retain(|item| item.as_str() != Some(value));
                empty = items.is_empty();
            }
            if empty {
                profile.config.remove(key);
            }
        });
    }

    pub fn add_launch_os(&mut self, value: &str, cx: &mut Context<Self>) {
        self.mutate_current_profile(cx, |profile| {
            let operating_systems = profile.launch.os.get_or_insert_with(Vec::new);
            if !operating_systems.iter().any(|item| item == value) {
                operating_systems.push(value.to_string());
            }
        });
    }

    pub fn remove_launch_os(&mut self, value: &str, cx: &mut Context<Self>) {
        self.mutate_current_profile(cx, |profile| {
            if let Some(operating_systems) = profile.launch.os.as_mut() {
                operating_systems.retain(|item| item != value);
                if operating_systems.is_empty() {
                    profile.launch.os = None;
                }
            }
        });
    }

    pub fn add_shortcut(&mut self, cx: &mut Context<Self>) {
        self.mutate_current_profile(cx, |profile| {
            profile.launch.shortcuts.push(camoforge_protocol::Shortcut {
                name: String::new(),
                url: String::new(),
            });
        });
    }

    /// 快捷方式：删除指定条目。删除会重排后续索引，故清空输入缓存让行内输入框重建。
    pub fn remove_shortcut(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(p) = self.current_mut() else { return };
        if idx < p.launch.shortcuts.len() {
            p.launch.shortcuts.remove(idx);
            p.updated_at = camoforge_protocol::unix_ts();
        }
        self.form.shortcut_inputs.clear();
        self.persist_current();
        cx.notify();
    }

    pub fn set_shortcut_field(
        &mut self,
        idx: usize,
        name: Option<&str>,
        url: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.mutate_current_profile(cx, |profile| {
            if let Some(shortcut) = profile.launch.shortcuts.get_mut(idx) {
                if let Some(name) = name {
                    shortcut.name = name.to_string();
                }
                if let Some(url) = url {
                    shortcut.url = url.to_string();
                }
            }
        });
    }

    pub fn apply_raw_json(&mut self, cx: &mut Context<Self>) {
        let text = self.profile.raw_json_text.clone();
        match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) {
            Ok(map) => {
                let n = map.len();
                {
                    if let Some(p) = self.current_mut() {
                        p.config = serde_json::Map::from_iter(map).into_iter().collect();
                        p.updated_at = camoforge_protocol::unix_ts();
                    }
                }
                // 配置被整体覆盖：清输入/下拉缓存，避免其他 tab 的缓存控件显示旧值
                self.clear_field_caches();
                self.log("info", format!("已应用 JSON（{n} 键）"));
                self.persist_current();
                self.refresh_raw_json_snapshot();
                cx.notify();
            }
            Err(e) => self.log("error", format!("JSON 解析失败: {e}")),
        }
    }

    pub fn launch_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.profile.profiles.iter().any(|p| p.id == id) {
            self.profile.selected = Some(id.to_string());
            self.launch_current(cx);
        } else {
            self.log("error", format!("找不到身份 {id}，无法自动启动"));
        }
    }

    pub fn launch_current(&mut self, cx: &mut Context<Self>) {
        let Some(mut p) = self.current().cloned() else {
            return;
        };
        self.apply_local_camoufox(&mut p);
        // 一致性校验：指纹 UA 的 OS 与 launch.os 不一致会让 camoufox 启动即失败
        if let Some(msg) = self.check_os_consistency(&p) {
            self.log("error", &msg);
            self.worker.launch_error = Some(msg);
            return;
        }
        let Some(sup) = self.worker.supervisor.clone() else {
            self.log("error", "worker 未启动");
            return;
        };
        if !sup.is_running() {
            self.log("error", "worker 进程未运行");
            return;
        }
        if self.worker.launching {
            self.log("warn", "已有启动任务进行中");
            return;
        }
        if self.worker.running.contains_key(&p.id) {
            self.log("warn", "该身份已在运行");
            return;
        }
        let pid = p.id.clone();
        let name = p.name.clone();
        self.worker.launching = true;
        self.log("info", format!("启动 {name} …"));

        // 后台线程执行阻塞 RPC（最长 200s），避免冻结 UI。
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.launch(&p) })
                .await;
            weak.update(cx, |state, cx| {
                state.worker.launching = false;
                match result {
                    Ok(info) => {
                        let inst = camoforge_protocol::InstanceInfo {
                            profile_id: info.profile_id,
                            profile_name: name.clone(),
                            pid: info.pid,
                            headless: info.headless,
                            started_at: camoforge_protocol::unix_ts(),
                            user_data_dir: info.user_data_dir,
                        };
                        state.worker.running.insert(pid.clone(), inst);
                        state.log("info", format!("{name} 已启动"));
                    }
                    Err(e) => {
                        let msg = clean_error(&e.to_string());
                        state.log("error", format!("启动失败: {msg}"));
                        state.worker.launch_error = Some(msg);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    #[cfg(windows)]
    pub fn create_shortcut_for_current(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.current().cloned() else {
            self.log("warn", "未选择身份，无法创建快捷方式");
            return;
        };

        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.log("error", format!("获取程序路径失败: {e}"));
                return;
            }
        };

        let default_name = format!("{}.lnk", profile.name);
        let args = format!("--profile {}", profile.id);
        let desc = format!("启动 CamouForge 身份：{}", profile.name);

        self.log("info", "正在选择快捷方式保存位置 …");
        // 系统「另存为」对话框和 .lnk 创建都放到独立线程：
        // 模态 Win32 对话框会运行嵌套消息循环，放在 GPUI 主线程上会与事件循环
        // 互相等待导致界面卡死（此前表现为点击按钮后直接卡退）。
        let dialog_thread = std::thread::spawn(move || {
            let Some(lnk_path) = crate::shortcut::pick_shortcut_path(&default_name) else {
                return Ok(None);
            };
            crate::shortcut::create_shortcut(&exe, &args, &lnk_path, &desc).map(|_| Some(lnk_path))
        });
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { dialog_thread.join() })
                .await;
            weak.update(cx, |state, cx| {
                match result {
                    Ok(Ok(Some(path))) => {
                        state.log("info", format!("已创建快捷方式：{}", path.display()))
                    }
                    Ok(Ok(None)) => {} // 用户取消
                    Ok(Err(e)) => state.log("error", format!("创建快捷方式失败: {e}")),
                    Err(_) => state.log("error", "创建快捷方式线程异常退出"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn check_os_consistency(&self, p: &Profile) -> Option<String> {
        let target = p.launch.os.as_ref()?.first()?;
        let fp = p.launch.fingerprint.as_ref()?;
        let fp_os = fingerprint_os(fp)?;
        if fp_os != target.as_str() {
            Some(format!(
                "指纹 UA 的系统是 {}，但目标 OS 是 {}，两者不一致。请重新生成指纹，或在「启动」页修改目标 OS。",
                os_label(fp_os),
                os_label(target)
            ))
        } else {
            None
        }
    }

    pub fn stop_current(&mut self, cx: &mut Context<Self>) {
        let Some(pid) = self.profile.selected.clone() else {
            return;
        };
        let Some(sup) = self.worker.supervisor.clone() else {
            return;
        };
        if !self.worker.running.contains_key(&pid) {
            self.log("warn", "该身份未在运行");
            return;
        }
        // 乐观移除但保留快照：停止失败时恢复运行状态，避免界面谎报
        let snapshot = self.worker.running.remove(&pid);
        self.log("info", "停止中 …");
        let pid_for_rpc = pid.clone();

        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.stop(&pid_for_rpc) })
                .await;
            weak.update(cx, |state, cx| {
                match result {
                    Ok(v) => {
                        let stopped = v.get("stopped").and_then(|s| s.as_bool()).unwrap_or(true);
                        if stopped {
                            state.log("info", "已停止");
                        } else {
                            let reason = v
                                .get("reason")
                                .and_then(|s| s.as_str())
                                .unwrap_or("未知原因");
                            if let Some(inst) = snapshot {
                                state.worker.running.insert(pid.clone(), inst);
                            }
                            state.log("error", format!("停止失败: {reason}"));
                        }
                    }
                    Err(e) => {
                        if let Some(inst) = snapshot {
                            state.worker.running.insert(pid.clone(), inst);
                        }
                        state.log("error", format!("停止失败: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn validate_current(&mut self, cx: &mut Context<Self>) {
        let Some(mut p) = self.current().cloned() else {
            return;
        };
        self.apply_local_camoufox(&mut p);
        // 与启动一致的校验：尽早暴露 UA 与 OS 不一致
        if let Some(msg) = self.check_os_consistency(&p) {
            self.log("error", &msg);
            return;
        }
        let Some(sup) = self.worker.supervisor.clone() else {
            self.log("error", "worker 未启动");
            return;
        };
        if self.worker.validating {
            return;
        }
        self.worker.validating = true;
        self.log("info", "校验配置 …");
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.validate(&p) })
                .await;
            weak.update(cx, |state, cx| {
                state.worker.validating = false;
                match result {
                    Ok(v) => {
                        // worker 干跑失败时 error 非空：必须优先展示，避免「校验通过」假象
                        if let Some(err) = v.error.as_deref().filter(|s| !s.trim().is_empty()) {
                            state.log("error", format!("校验失败: {err}"));
                        } else if v.warnings.is_empty() {
                            state.log("info", "校验通过：无泄漏告警");
                        } else {
                            for w in &v.warnings {
                                state.log("warn", format!("泄漏告警 [{}]: {}", w.code, w.message));
                            }
                        }
                    }
                    Err(e) => state.log("error", format!("校验失败: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn generate_fingerprint_current(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.current().cloned() else {
            return;
        };
        let Some(sup) = self.worker.supervisor.clone() else {
            self.log("error", "worker 未启动");
            return;
        };
        if self.worker.generating {
            return;
        }
        let os = p
            .launch
            .os
            .clone()
            .and_then(|v| v.first().cloned().map(|x| vec![x]));
        let ff_version = p.launch.ff_version;
        let locale = p.launch.locale.clone().and_then(|v| v.first().cloned());
        self.worker.generating = true;
        self.log("info", "生成指纹 …");
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.generate_fingerprint(os, ff_version, locale) })
                .await;
            weak.update(cx, |state, cx| {
                state.worker.generating = false;
                match result {
                    Ok(fp) => {
                        let s = &fp.summary;
                        let inferred_os = fingerprint_os(&fp.fingerprint).map(|o| o.to_string());
                        state.log(
                            "info",
                            format!(
                                "指纹已生成：UA={} | OS={} | WebGL={} / {}",
                                s.user_agent, s.os, s.webgl_vendor, s.webgl_renderer
                            ),
                        );
                        let mut sync_msg: Option<String> = None;
                        if let Some(p) = state.current_mut() {
                            p.launch.fingerprint = Some(fp.fingerprint);
                            // 同步 launch.os 为指纹的实际系统，避免 os / UA / WebGL 三者矛盾
                            if let Some(os) = &inferred_os {
                                let current = p
                                    .launch
                                    .os
                                    .as_ref()
                                    .and_then(|v| v.first().map(|s| s.as_str()));
                                if current != Some(os.as_str()) {
                                    p.launch.os = Some(vec![os.clone()]);
                                    sync_msg = Some(format!("已同步目标 OS 为 {}", os_label(os)));
                                }
                            }
                            p.updated_at = camoforge_protocol::unix_ts();
                        }
                        if let Some(msg) = sync_msg {
                            state.log("info", &msg);
                        }
                        state.persist_current();
                        // 生成后强制重建所有字段输入缓存并刷新面板，避免界面仍显示旧/空值。
                        // 文本域/输入框重建时会从已更新的 profile 读取最新指纹。
                        state.clear_field_caches();
                        state.refresh_raw_json_snapshot();
                        let json = state
                            .current()
                            .and_then(|p| p.launch.fingerprint.clone())
                            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                            .unwrap_or_default();
                        state
                            .form
                            .pending_field_values
                            .insert("launch::Fingerprint".to_string(), json);
                    }
                    Err(e) => state.log("error", format!("指纹生成失败: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn load_webgl_cards(&mut self, os: &str, cx: &mut Context<Self>) {
        let Some(sup) = self.worker.supervisor.clone() else {
            self.log("error", "worker 未启动");
            return;
        };
        let os = os.to_string();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.list_webgl(Some(os)) })
                .await;
            weak.update(cx, |state, cx| {
                match result {
                    Ok(r) => {
                        let n = r.cards.len();
                        state.browsers.webgl_cards = r.cards;
                        state.log("info", format!("已加载 {n} 个 WebGL 组合"));
                    }
                    Err(e) => state.log("error", format!("WebGL 列表加载失败: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn load_browser_versions(&mut self, cx: &mut Context<Self>) {
        let Some(sup) = self.worker.supervisor.clone() else {
            return;
        };
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.list_versions() })
                .await;
            weak.update(cx, |state, cx| {
                match result {
                    Ok(r) => state.browsers.browser_versions = r.versions,
                    Err(e) => state.log("error", format!("版本列表加载失败: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn load_font_voices_catalog(&mut self, os: &str, cx: &mut Context<Self>) {
        let Some(sup) = self.worker.supervisor.clone() else {
            return;
        };
        let os = os.to_string();
        cx.spawn(async move |weak, cx| {
            let fonts_sup = sup.clone();
            let fonts_os = os.clone();
            let fonts = cx
                .background_executor()
                .spawn(async move { fonts_sup.list_fonts(Some(fonts_os)) })
                .await;
            let voices = cx
                .background_executor()
                .spawn(async move { sup.list_voices(Some(os)) })
                .await;
            weak.update(cx, |state, cx| {
                match fonts {
                    Ok(r) => state.browsers.font_options = r.fonts,
                    Err(e) => state.log("error", format!("字体目录加载失败: {e}")),
                }
                match voices {
                    Ok(r) => state.browsers.voice_options = r.voices,
                    Err(e) => state.log("error", format!("语音目录加载失败: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn stop_all_current(&mut self, cx: &mut Context<Self>) {
        let Some(sup) = self.worker.supervisor.clone() else {
            self.log("error", "worker 未启动");
            return;
        };
        let snapshot = self.worker.running.clone();
        self.worker.running.clear();
        self.log("info", "停止全部 …");
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sup.stop_all() })
                .await;
            weak.update(cx, |state, cx| {
                match result {
                    Ok(v) => {
                        let failed = v.get("failed").and_then(|f| f.as_object());
                        match failed {
                            Some(f) if !f.is_empty() => {
                                // 只恢复没停掉的实例运行态；已停掉的不回滚。
                                for (pid, reason) in f {
                                    if let Some(inst) = snapshot.get(pid).cloned() {
                                        state.worker.running.insert(pid.clone(), inst);
                                    }
                                    state.log(
                                        "error",
                                        format!(
                                            "停止失败 {pid}: {}",
                                            reason.as_str().unwrap_or("unknown")
                                        ),
                                    );
                                }
                                state
                                    .log("warn", format!("其余实例已停止，{} 个停止失败", f.len()));
                            }
                            _ => state.log("info", "已停止全部"),
                        }
                    }
                    Err(e) => {
                        state.worker.running = snapshot;
                        state.log("error", format!("停止全部失败: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn apply_webgl_card(&mut self, vendor: &str, renderer: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.current_mut() {
            p.launch.webgl_config = Some(camoforge_protocol::WebglConfig {
                vendor: vendor.to_string(),
                renderer: renderer.to_string(),
            });
            p.updated_at = camoforge_protocol::unix_ts();
        }
        self.persist_current();
        self.form
            .pending_field_values
            .insert("launch::WebglVendor".to_string(), vendor.to_string());
        self.form
            .pending_field_values
            .insert("launch::WebglRenderer".to_string(), renderer.to_string());
        self.log("info", format!("已应用 WebGL: {vendor} / {renderer}"));
        cx.notify();
    }

    /// render 时同步待写入输入框/文本域的值（set_value 需要 window，故延迟到 render）。
    pub fn apply_pending_field_values(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.pending_field_values.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.form.pending_field_values);
        for (key, value) in pending {
            if key == "__camoufox_dir__" {
                if let Some(input) = self.settings_input.clone() {
                    input.update(cx, |i, cx| {
                        i.set_value(value.clone(), window, cx);
                    });
                }
                continue;
            }
            if let Some(input) = self.form.field_inputs.get(&key).cloned() {
                input.update(cx, |i, cx| {
                    i.set_value(value.clone(), window, cx);
                });
            }
            if let Some(textarea) = self.form.field_textareas.get(&key).cloned() {
                textarea.update(cx, |t, cx| {
                    t.set_value(value.clone(), window, cx);
                });
            }
        }
    }

    pub fn drain_events(&mut self, cx: &mut Context<Self>) {
        let mut events = Vec::new();
        if let Some(rx) = &self.worker.event_rx {
            let rx = rx.lock().unwrap();
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
        }
        for ev in events {
            match ev {
                SupervisorEvent::WorkerReady { camoufox, browser } => {
                    self.worker.ready = true;
                    self.log(
                        "info",
                        format!(
                            "worker 就绪：camoufox={} browser={}",
                            camoufox.as_deref().unwrap_or("?"),
                            browser.as_deref().unwrap_or("?")
                        ),
                    );
                    // 从快捷方式启动时，worker 就绪后立即启动指定 profile。
                    if let Some(id) = self.worker.pending_auto_launch.take() {
                        self.log("info", format!("快捷方式启动：自动启动身份 {id}"));
                        self.launch_profile(&id, cx);
                    }
                    cx.notify();
                }
                SupervisorEvent::WorkerRestarted { attempt } => {
                    // worker 崩溃重启后新进程实例表为空：清空本地 running，避免 UI
                    // 谎报「运行实例/停止按钮」指向已失去控制的浏览器。
                    self.worker.running.clear();
                    self.log(
                        "warn",
                        format!("worker 重启（第 {attempt} 次），已清空运行实例"),
                    );
                    cx.notify();
                }
                SupervisorEvent::WorkerFailed { reason } => {
                    self.worker.ready = false;
                    self.worker.running.clear();
                    self.log("error", format!("worker 退出: {reason}"));
                    cx.notify();
                }
                SupervisorEvent::InstanceExited { profile_id, reason } => {
                    let name = self
                        .profile
                        .profiles
                        .iter()
                        .find(|p| p.id == profile_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| profile_id.clone());
                    self.worker.running.remove(&profile_id);
                    self.log("info", format!("{name} 已退出: {reason}"));
                    cx.notify();
                }
            }
        }
    }
}

/// 从 fingerprint JSON 的 navigator.userAgent 推断系统（windows/macos/linux）。
fn fingerprint_os(fp: &serde_json::Value) -> Option<&'static str> {
    let ua = fp.get("navigator")?.get("userAgent")?.as_str()?;
    infer_os_from_ua(ua)
}

fn infer_os_from_ua(ua: &str) -> Option<&'static str> {
    if ua.contains("Windows NT") {
        Some("windows")
    } else if ua.contains("Macintosh") || ua.contains("Mac OS X") {
        Some("macos")
    } else if ua.contains("Linux") || ua.contains("X11") {
        Some("linux")
    } else {
        None
    }
}

fn os_label(os: &str) -> &str {
    match os {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

/// 精简错误信息：只保留首行关键信息（完整 traceback 已由 worker stderr 落盘到日志）。
fn clean_error(msg: &str) -> String {
    let first = msg.lines().next().unwrap_or("").trim();
    let mut s: String = first.chars().take(220).collect();
    if first.chars().count() > 220 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext as _;
    use std::path::Path;

    fn app_window(
        cx: &mut gpui::TestAppContext,
    ) -> (tempfile::TempDir, gpui::WindowHandle<AppState>) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("worker")).unwrap();
        for name in python_env::WORKER_FILES {
            std::fs::write(root.join("worker").join(name), b"").unwrap();
        }
        std::fs::write(root.join("pyproject.toml"), b"").unwrap();
        std::fs::write(root.join("uv.lock"), b"").unwrap();
        std::fs::write(root.join("uv.toml"), b"").unwrap();
        let layout =
            python_env::RuntimeLayout::resolve(&root, Path::new("D:/dev/camouforge/crates/app"))
                .unwrap();
        let store = crate::store::ProfileStore::open(dir.path()).unwrap();
        let log_sink = crate::logging::LogSink::new(dir.path().join("logs"));

        let window = cx.add_window(|window, cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, Some(window), cx);
            AppState::new(
                dir.path().to_path_buf(),
                store,
                log_sink,
                None,
                layout,
                window,
                cx,
            )
        });
        (dir, window)
    }

    /// 初始 busy 必须为 false，否则首次自举会被自身的重入护栏吞掉。
    #[gpui::test]
    fn first_bootstrap_is_not_blocked_by_initial_busy(cx: &mut gpui::TestAppContext) {
        let (_dir, window) = app_window(cx);

        let initial_free = std::cell::Cell::new(false);
        let entered = std::cell::Cell::new(false);
        let caches_empty = std::cell::Cell::new(false);
        window
            .update(cx, |state, _window, cx| {
                initial_free.set(!state.python.busy);
                caches_empty.set(
                    state.form.field_inputs.is_empty()
                        && state.form.field_selects.is_empty()
                        && state.browsers.downloads.is_empty(),
                );
                state.ensure_python_env(python_env::EnsureMode::IfNeeded, cx);
                entered.set(state.python.busy);
            })
            .unwrap();
        assert!(
            initial_free.get(),
            "初始 busy 必须为 false，否则首次自举被拦"
        );
        assert!(entered.get(), "首次调用必须真正进入自举流程");
        assert!(caches_empty.get(), "领域状态必须从空缓存和空下载集开始");
    }

    #[gpui::test]
    fn tab_switch_keeps_form_cache(cx: &mut gpui::TestAppContext) {
        let (_dir, window) = app_window(cx);
        let retained = std::cell::Cell::new(false);
        window
            .update(cx, |state, window, cx| {
                let input = cx.new(|cx| InputState::new(window, cx).default_value("cached"));
                state.form.field_inputs.insert("Launch::test".into(), input);
                state.select_tab(Tab::ScreenWindow, cx);
                retained.set(state.form.field_inputs.contains_key("Launch::test"));
            })
            .unwrap();
        assert!(retained.get(), "切换 Tab 不应重建其他 Tab 的控件缓存");
    }

    #[gpui::test]
    fn replacing_config_clears_controls_and_refreshes_raw_snapshot(cx: &mut gpui::TestAppContext) {
        let (_dir, window) = app_window(cx);
        let verified = std::cell::Cell::new(false);
        window
            .update(cx, |state, window, cx| {
                state.new_profile(cx);
                let input = cx.new(|cx| InputState::new(window, cx).default_value("stale"));
                state
                    .form
                    .field_inputs
                    .insert("Navigator::test".into(), input);
                state.tab = Tab::RawJson;
                state.profile.raw_json_text = r#"{"navigator.userAgent":"new"}"#.into();
                state.apply_raw_json(cx);

                let snapshot = state
                    .form
                    .pending_field_values
                    .get("__raw_json__")
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
                verified.set(
                    state.form.field_inputs.is_empty()
                        && snapshot
                            .as_ref()
                            .and_then(|value| value.get("navigator.userAgent"))
                            .and_then(|value| value.as_str())
                            == Some("new"),
                );
            })
            .unwrap();
        assert!(
            verified.get(),
            "整体替换配置后必须清缓存并刷新 Raw JSON 快照"
        );
    }
}
