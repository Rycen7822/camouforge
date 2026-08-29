use std::path::PathBuf;
use std::sync::Arc;

use gpui::{actions, Global, *};
#[cfg(windows)]
use gpui_component::button::ButtonVariant;
#[cfg(windows)]
use gpui_component::dialog::DialogButtonProps;
use gpui_component::{Root, Theme, ThemeMode, WindowExt};
use gpui_component_assets::Assets;

use crate::logging::LogSink;
use crate::{fatal_startup_error, python_env, settings, state, store, supervisor};
#[cfg(windows)]
use crate::{platform, single, tray};

actions!(
    camoforge,
    [
        NewProfile,
        LaunchCurrent,
        StopCurrent,
        ValidateCurrent,
        GenerateFingerprint,
        Quit
    ]
);

struct MainWindow(gpui::AnyWindowHandle);
impl Global for MainWindow {}

fn shutdown_all(supervisor: &supervisor::WorkerSupervisor, cx: &mut App) {
    supervisor.shutdown();
    #[cfg(windows)]
    if let Some(tray) = cx.try_global::<tray::TrayGlobal>() {
        tray.0.shutdown();
    }
    cx.quit();
}

pub fn run(
    dir: PathBuf,
    log_sink: LogSink,
    runtime_layout: python_env::RuntimeLayout,
    launch_profile_id: Option<String>,
) {
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);

        cx.bind_keys([
            KeyBinding::new("ctrl-n", NewProfile, None),
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("f5", LaunchCurrent, None),
            KeyBinding::new("ctrl-shift-v", ValidateCurrent, None),
            KeyBinding::new("ctrl-shift-f", GenerateFingerprint, None),
        ]);

        let store = match store::ProfileStore::open(&dir) {
            Ok(s) => s,
            Err(e) => fatal_startup_error(&format!("Profile 存储初始化失败: {e}")),
        };

        // supervisor：持有最终 .venv 解释器路径，但不立即启动——
        // worker 等 Python 环境自举完成后再 start（窗口打开后异步执行）。
        let (sup, rx) = supervisor::WorkerSupervisor::new(
            runtime_layout.venv_python.to_string_lossy().into_owned(),
            runtime_layout.worker_script.clone(),
            dir.clone(),
            log_sink.clone(),
        );
        let sup = Arc::new(sup);

        let rx = Arc::new(std::sync::Mutex::new(rx));
        let sup_for_exit = sup.clone();
        #[cfg(windows)]
        let sup_for_close = sup.clone();
        #[cfg(windows)]
        let sup_for_tray = sup.clone();

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1180.), px(760.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("CamouForge".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.on_action(move |_: &Quit, cx: &mut App| {
            shutdown_all(&sup_for_exit, cx);
        });

        let auto_hide = launch_profile_id.is_some();
        cx.spawn(async move |cx| {
            let main_handle = cx
                .open_window(window_options, |window, cx| {
                    Theme::change(ThemeMode::Dark, Some(window), cx);

                    #[cfg(windows)]
                    tray::register_main_hwnd(platform::windows::main_hwnd(window));

                    // 关闭按钮拦截：行为由「应用设置 → 关闭窗口时」决定。
                    // 返回 false = 吞掉 WM_CLOSE，窗口保留、进程常驻。
                    #[cfg(windows)]
                    {
                        let sup_close = sup_for_close.clone();
                        window.on_window_should_close(cx, move |window, cx| {
                            match settings::close_action() {
                                settings::CloseAction::Exit => {
                                    // shutdown 不能阻塞 UI；进程树由 Job Object 兜底回收。
                                    shutdown_all(&sup_close, cx);
                                    true
                                }
                                settings::CloseAction::MinimizeToTray => {
                                    tray::hide_main_window();
                                    false
                                }
                                settings::CloseAction::Confirm => {
                                    if window.has_active_dialog(cx) {
                                        return false;
                                    }
                                    let sup = sup_close.clone();
                                    window.open_alert_dialog(cx, move |alert, _w, _cx| {
                                        // builder 是 Fn（可多次调用），不能把捕获 move 进 on_ok 闭包，clone 一份。
                                        let sup = sup.clone();
                                        alert
                                            .confirm()
                                            .title("关闭 CamouForge")
                                            .description("退出程序，还是最小化到托盘继续运行？")
                                            .button_props(
                                                DialogButtonProps::default()
                                                    .ok_text("退出程序")
                                                    .ok_variant(ButtonVariant::Danger)
                                                    .cancel_text("最小化到托盘")
                                                    .show_cancel(true),
                                            )
                                            .on_ok(move |_, _w, cx| {
                                                shutdown_all(&sup, cx);
                                                true
                                            })
                                            .on_cancel(|_, _w, _cx| {
                                                tray::hide_main_window();
                                                true
                                            })
                                    });
                                    false
                                }
                            }
                        });
                    }

                    let view = cx.new(|cx| {
                        let mut st = state::AppState::new(
                            dir.clone(),
                            store,
                            log_sink.clone(),
                            launch_profile_id.clone(),
                            runtime_layout.clone(),
                            window,
                            cx,
                        );
                        st.worker.supervisor = Some(sup.clone());
                        st.worker.event_rx = Some(rx.clone());
                        if st.profile.selected.is_none() {
                            if let Some(first) = st.profile.profiles.first() {
                                st.profile.selected = Some(first.id.clone());
                            }
                        }
                        st
                    });

                    let weak = view.downgrade();

                    let weak_env = weak.clone();
                    cx.spawn(async move |cx| {
                        let _ = weak_env.update(cx, |state, cx| {
                            state.ensure_python_env(python_env::EnsureMode::IfNeeded, cx)
                        });
                    })
                    .detach();

                    // 全局快捷键动作 → 视图（App 级 on_action 无 Window 访问权，经 WeakView 转发）
                    let weak_new = weak.clone();
                    cx.on_action(move |_: &NewProfile, cx: &mut App| {
                        let _ = weak_new.update(cx, |state, cx| state.new_profile(cx));
                    });
                    let weak_launch = weak.clone();
                    cx.on_action(move |_: &LaunchCurrent, cx: &mut App| {
                        let _ = weak_launch.update(cx, |state, cx| {
                            let running = state
                                .profile
                                .selected
                                .as_ref()
                                .map(|id| state.worker.running.contains_key(id))
                                .unwrap_or(false);
                            if running {
                                state.stop_current(cx);
                            } else {
                                state.launch_current(cx);
                            }
                        });
                    });
                    let weak_stop = weak.clone();
                    cx.on_action(move |_: &StopCurrent, cx: &mut App| {
                        let _ = weak_stop.update(cx, |state, cx| state.stop_current(cx));
                    });
                    let weak_validate = weak.clone();
                    cx.on_action(move |_: &ValidateCurrent, cx: &mut App| {
                        let _ = weak_validate.update(cx, |state, cx| state.validate_current(cx));
                    });
                    let weak_gen = weak.clone();
                    cx.on_action(move |_: &GenerateFingerprint, cx: &mut App| {
                        let _ =
                            weak_gen.update(cx, |state, cx| state.generate_fingerprint_current(cx));
                    });
                    cx.spawn(async move |cx| loop {
                        // 200ms 轮询限制快捷方式自动启动等待，同时避免 UI 忙轮询。
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(200))
                            .await;

                        // 单实例：处理其他实例转交的启动请求（再次双击 exe/快捷方式 → 激活窗口/自动启动）
                        #[cfg(windows)]
                        {
                            let reqs = single::take_launch_requests(&dir);
                            for req in reqs {
                                tray::show_main_window();
                                cx.update(|cx| {
                                    if let Some(MainWindow(h)) = cx.try_global::<MainWindow>() {
                                        let _ = (*h).update(cx, |_, w, _| w.activate_window());
                                    }
                                });
                                if let Some(id) = req {
                                    let _ =
                                        weak.update(cx, |state, cx| state.launch_profile(&id, cx));
                                }
                            }
                        }

                        let _ = weak.update(cx, |state, cx| {
                            state.dirty_tick(cx);
                        });
                    })
                    .detach();

                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("failed to open window");

            // 主窗口句柄入全局（托盘「显示」时聚焦用）。open_window 返回
            // WindowHandle<Root>，转成 AnyWindowHandle 存全局。
            let main_handle: gpui::AnyWindowHandle = main_handle.into();
            cx.update(|cx| cx.set_global(MainWindow(main_handle)));

            // 从快捷方式启动时：窗口打开后立即最小化到托盘，由 worker 就绪后自动启动 profile。
            #[cfg(windows)]
            if auto_hide {
                tray::hide_main_window();
            }

            #[cfg(windows)]
            {
                let tray = tray::Tray::init();
                if let Some(tray) = tray {
                    cx.update(|cx| cx.set_global(tray::TrayGlobal(tray.clone())));
                    let events = tray.events();
                    let sup_tray = sup_for_tray.clone();
                    cx.spawn(async move |cx| loop {
                        let evt = cx
                            .background_executor()
                            .spawn({
                                let events = events.clone();
                                async move { events.lock().unwrap().recv().ok() }
                            })
                            .await;
                        let Some(evt) = evt else { break };
                        match evt {
                            tray::TrayEvent::Show => {
                                tray::show_main_window();
                                cx.update(|cx| {
                                    if let Some(MainWindow(h)) = cx.try_global::<MainWindow>() {
                                        let _ = (*h).update(cx, |_, w, _| w.activate_window());
                                    }
                                });
                            }
                            tray::TrayEvent::Quit => {
                                cx.update(|cx| shutdown_all(&sup_tray, cx));
                                break;
                            }
                        }
                    })
                    .detach();
                }
            }
        })
        .detach();
    });
}
