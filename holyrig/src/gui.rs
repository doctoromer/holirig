use crate::{
    rig_settings::{BaudRate, DataBits, RigConfig, RigId, RigSettings, StopBits},
    serial::ManagerCommand,
    serial::manager::{ConnectionStatus, ManagerMessage, SerialPortEntry},
};
use eframe::egui;
use egui::{ComboBox, Grid, Ui};
use egui_dock::{
    AllowedSplits, DockArea, DockState, NodeIndex, SurfaceIndex, TabViewer,
    tab_viewer::OnCloseResponse,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

enum WindowBackend {
    Unknown,
    /// X11 or Windows — `Visible(false)` works
    NativeHide,
    /// Wayland — need raw surface manipulation
    #[cfg(target_os = "linux")]
    Wayland(*mut std::ffi::c_void),
}

// Safety: the Wayland surface pointer is only ever accessed from eframe's main thread
// via hide_window/show_window in App::update(). It is never used from another thread.
unsafe impl Send for WindowBackend {}

impl WindowBackend {
    fn detect(frame: &eframe::Frame) -> Self {
        match frame.window_handle().map(|h| h.as_raw()) {
            #[cfg(target_os = "linux")]
            Ok(RawWindowHandle::Wayland(handle)) => WindowBackend::Wayland(handle.surface.as_ptr()),
            Ok(RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => WindowBackend::NativeHide,
            Ok(RawWindowHandle::Win32(_)) => WindowBackend::NativeHide,
            _ => WindowBackend::NativeHide,
        }
    }

    fn hide_window(&self, ctx: &egui::Context) {
        match self {
            WindowBackend::NativeHide => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            #[cfg(target_os = "linux")]
            WindowBackend::Wayland(surface) => {
                wayland_hide::hide_surface(*surface);
            }
            WindowBackend::Unknown => {}
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        match self {
            WindowBackend::NativeHide => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            #[cfg(target_os = "linux")]
            WindowBackend::Wayland(_) => {
                // eframe's next render will attach a buffer and commit,
                // remapping the surface. Just request repaint + focus.
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            WindowBackend::Unknown => {}
        }
    }
}

#[cfg(target_os = "linux")]
mod wayland_hide {
    use std::ffi::c_void;
    use wayland_sys::client::{wayland_client_handle, wl_proxy};
    use wayland_sys::common::wl_argument;

    const WL_SURFACE_ATTACH: u32 = 1;
    const WL_SURFACE_COMMIT: u32 = 6;

    /// Hide a Wayland surface by attaching a null buffer and committing.
    pub fn hide_surface(surface: *mut c_void) {
        let lib = wayland_client_handle();
        unsafe {
            let mut args = [
                wl_argument {
                    o: std::ptr::null_mut(),
                }, // buffer = null
                wl_argument { i: 0 }, // x = 0
                wl_argument { i: 0 }, // y = 0
            ];
            (lib.wl_proxy_marshal_array)(
                surface as *mut wl_proxy,
                WL_SURFACE_ATTACH,
                args.as_mut_ptr(),
            );
            (lib.wl_proxy_marshal_array)(
                surface as *mut wl_proxy,
                WL_SURFACE_COMMIT,
                std::ptr::null_mut(),
            );
        }
    }
}

pub fn show_already_running_dialog() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 150.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "HolyRig",
        options,
        Box::new(|_ctx| Ok(Box::new(AlreadyRunningApp))),
    )
    .expect("Failed to run already running dialog");
}

struct AlreadyRunningApp;

impl eframe::App for AlreadyRunningApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("HolyRig is already running");
                ui.add_space(20.0);
                if ui.button("OK").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}

pub enum TrayAction {
    Show,
    Quit,
}

pub fn spawn_tray_watcher() -> (
    std::sync::mpsc::Receiver<TrayAction>,
    std::sync::mpsc::SyncSender<egui::Context>,
) {
    let (tray_tx, tray_rx) = std::sync::mpsc::channel::<TrayAction>();
    let (ctx_tx, ctx_rx) = std::sync::mpsc::sync_channel::<egui::Context>(1);

    std::thread::spawn(move || {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent, menu::MenuEvent};

        let ctx = match ctx_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };

        let tray_events = TrayIconEvent::receiver();
        let menu_events = MenuEvent::receiver();

        loop {
            crossbeam_channel::select! {
                recv(tray_events) -> msg => {
                    if let Ok(TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }) = msg {
                        tray_tx.send(TrayAction::Show).ok();
                        ctx.request_repaint();
                    }
                }
                recv(menu_events) -> msg => {
                    if let Ok(event) = msg {
                        let action = if event.id == "show" {
                            Some(TrayAction::Show)
                        } else if event.id == "quit" {
                            Some(TrayAction::Quit)
                        } else {
                            None
                        };
                        if let Some(action) = action {
                            tray_tx.send(action).ok();
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }
    });

    (tray_rx, ctx_tx)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct TabId(u64);

fn next_tab_id(counter: &mut u64) -> TabId {
    let id = TabId(*counter);
    *counter += 1;
    id
}

enum RigTabState {
    Draft,
    Registered {
        rig_id: RigId,
        status: ConnectionStatus,
    },
}

struct RigTab {
    tab_id: TabId,
    state: RigTabState,
    config: RigConfig,
}

struct AppTabViewer<'a> {
    current_index: usize,
    add_tab_request: bool,
    rig_types: Vec<String>,
    available_ports: Vec<SerialPortEntry>,
    sender: Sender<ManagerCommand>,
    error_message: Option<String>,
    active_tab_id: Option<TabId>,
    tab_count: usize,
    pending_creates: &'a mut Vec<(TabId, oneshot::Receiver<RigId>)>,
}

impl<'a> AppTabViewer<'a> {
    fn new(
        sender: Sender<ManagerCommand>,
        rig_types: Vec<String>,
        available_ports: Vec<SerialPortEntry>,
        active_tab_id: Option<TabId>,
        tab_count: usize,
        pending_creates: &'a mut Vec<(TabId, oneshot::Receiver<RigId>)>,
    ) -> Self {
        AppTabViewer {
            current_index: 0,
            add_tab_request: false,
            rig_types,
            available_ports,
            sender,
            error_message: None,
            active_tab_id,
            tab_count,
            pending_creates,
        }
    }
}

impl<'a> TabViewer for AppTabViewer<'a> {
    type Tab = RigTab;

    fn title(&mut self, _tab: &mut Self::Tab) -> egui::WidgetText {
        self.current_index += 1;
        format!("RIG {}", self.current_index).as_str().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let config = &mut tab.config;

        ui.group(|ui| {
            if let Some(error) = &self.error_message {
                ui.colored_label(egui::Color32::RED, error);
                ui.separator();
            }

            ui.style_mut().spacing.combo_width *= 0.75;

            Grid::new("rig_settings").num_columns(2).show(ui, |ui| {
                ui.label("Rig type:");
                ComboBox::from_id_salt("rig_type")
                    .selected_text(config.rig_type.to_string())
                    .show_ui(ui, |ui| {
                        for rig_type in &self.rig_types {
                            ui.selectable_value(
                                &mut config.rig_type,
                                rig_type.clone(),
                                rig_type.to_string(),
                            );
                        }
                    });
                ui.end_row();

                ui.label("Port:");
                ComboBox::from_id_salt("port")
                    .selected_text(if config.port.is_empty() {
                        "Select port...".to_string()
                    } else {
                        let mut display_name = self
                            .available_ports
                            .iter()
                            .find(|p| p.port_name == config.port)
                            .map(|p| p.display_name.clone())
                            .unwrap_or_else(|| config.port.clone());
                        display_name.truncate(30);
                        display_name
                    })
                    .show_ui(ui, |ui| {
                        for entry in &self.available_ports {
                            ui.selectable_value(
                                &mut config.port,
                                entry.port_name.clone(),
                                &entry.display_name,
                            );
                        }
                    });
                ui.end_row();

                ui.label("Baud Rate:");
                ComboBox::from_id_salt("baud_rate")
                    .selected_text(format!("{}", config.baud_rate))
                    .show_ui(ui, |ui| {
                        for rate in BaudRate::iter_rates() {
                            ui.selectable_value(&mut config.baud_rate, rate, format!("{rate}"));
                        }
                    });
                ui.end_row();

                ui.label("Data Bits:");
                ComboBox::from_id_salt("data_bits")
                    .selected_text(format!("{}", config.data_bits))
                    .show_ui(ui, |ui| {
                        for bits in DataBits::iter_data_bits() {
                            ui.selectable_value(&mut config.data_bits, bits, format!("{bits}"));
                        }
                    });
                ui.end_row();

                ui.label("Stop Bits:");
                ComboBox::from_id_salt("stop_bits")
                    .selected_text(format!("{}", config.stop_bits))
                    .show_ui(ui, |ui| {
                        for bits in [StopBits::Bits1, StopBits::Bits2] {
                            ui.selectable_value(&mut config.stop_bits, bits, format!("{bits}"));
                        }
                    });
                ui.end_row();

                ui.label("Parity:");
                ui.checkbox(&mut config.parity, "");
                ui.end_row();

                ui.label("RTS:");
                ui.checkbox(&mut config.rts, "");
                ui.end_row();

                ui.label("DTR:");
                ui.checkbox(&mut config.dtr, "");
                ui.end_row();

                ui.label("Poll Interval (ms):");
                ui.add(egui::DragValue::new(&mut config.poll_interval).range(10..=1000));
                ui.end_row();

                ui.label("Timeout (ms):");
                ui.add(egui::DragValue::new(&mut config.timeout).range(10..=5000));
                ui.end_row();
            });

            ui.separator();

            Grid::new("status_and_buttons")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.horizontal(|ui| match &tab.state {
                        RigTabState::Draft => {}
                        RigTabState::Registered { status, .. } => {
                            let (color, text) = match status {
                                ConnectionStatus::Connecting => {
                                    (egui::Color32::YELLOW, "Connecting...".to_string())
                                }
                                ConnectionStatus::Connected => {
                                    (egui::Color32::GREEN, "Connected".to_string())
                                }
                                ConnectionStatus::Error(err) => {
                                    let mut msg = format!("Error: {err}");
                                    msg.truncate(50);
                                    (egui::Color32::RED, msg)
                                }
                            };

                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.colored_label(color, &text);
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("OK").clicked() {
                            match config.validate() {
                                Ok(_) => {
                                    let sender = self.sender.clone();
                                    let config = config.clone();
                                    match &tab.state {
                                        RigTabState::Registered { rig_id, .. } => {
                                            let rig_id = *rig_id;
                                            tokio::task::spawn(async move {
                                                let _ = sender
                                                    .send(ManagerCommand::UpdateDevice {
                                                        device_id: rig_id,
                                                        config,
                                                    })
                                                    .await;
                                            });
                                        }
                                        RigTabState::Draft => {
                                            let (tx, rx) = oneshot::channel();
                                            self.pending_creates.push((tab.tab_id, rx));
                                            tokio::task::spawn(async move {
                                                let _ = sender
                                                    .send(ManagerCommand::CreateDevice {
                                                        config,
                                                        response: tx,
                                                    })
                                                    .await;
                                            });
                                        }
                                    }
                                }
                                Err(err) => {
                                    self.error_message = Some(err);
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.error_message = None;
                        }
                    });
                });
        });
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        self.tab_count > 1 && Some(tab.tab_id) == self.active_tab_id
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        if let RigTabState::Registered { rig_id, .. } = tab.state {
            let sender = self.sender.clone();
            tokio::task::spawn(async move {
                let _ = sender
                    .send(ManagerCommand::RemoveDevice { device_id: rig_id })
                    .await;
            });
        }
        OnCloseResponse::Close
    }

    fn on_add(&mut self, _surface: SurfaceIndex, _node: NodeIndex) {
        self.add_tab_request = true;
    }
}

struct AppTabs {
    dock_state: DockState<RigTab>,
    rig_types: Vec<String>,
    available_ports: Vec<SerialPortEntry>,
    sender: Sender<ManagerCommand>,
    next_tab_id: u64,
    pending_creates: Vec<(TabId, oneshot::Receiver<RigId>)>,
}

impl AppTabs {
    fn new(sender: Sender<ManagerCommand>, rig_types: Vec<String>) -> Self {
        let mut next = 0u64;
        let draft = RigTab {
            tab_id: next_tab_id(&mut next),
            state: RigTabState::Draft,
            config: RigConfig::default(),
        };
        let dock_state = DockState::new(vec![draft]);
        Self {
            dock_state,
            rig_types,
            available_ports: Vec::new(),
            sender,
            next_tab_id: next,
            pending_creates: Vec::new(),
        }
    }

    fn set_tabs(&mut self, settings: Vec<RigSettings>) {
        let tabs: Vec<RigTab> = if settings.is_empty() {
            vec![RigTab {
                tab_id: next_tab_id(&mut self.next_tab_id),
                state: RigTabState::Draft,
                config: RigConfig::default(),
            }]
        } else {
            settings
                .into_iter()
                .map(|s| RigTab {
                    tab_id: next_tab_id(&mut self.next_tab_id),
                    state: RigTabState::Registered {
                        rig_id: s.id,
                        status: ConnectionStatus::Connecting,
                    },
                    config: s.config,
                })
                .collect()
        };
        self.dock_state = DockState::new(tabs);
    }

    fn poll_pending_creates(&mut self) {
        self.pending_creates
            .retain_mut(|(tab_id, rx)| match rx.try_recv() {
                Ok(rig_id) => {
                    for (_, tab) in self.dock_state.iter_all_tabs_mut() {
                        if tab.tab_id == *tab_id {
                            tab.state = RigTabState::Registered {
                                rig_id,
                                status: ConnectionStatus::Connecting,
                            };
                            break;
                        }
                    }
                    false
                }
                Err(oneshot::error::TryRecvError::Empty) => true,
                Err(oneshot::error::TryRecvError::Closed) => {
                    for (_, tab) in self.dock_state.iter_all_tabs_mut() {
                        if tab.tab_id == *tab_id {
                            tab.state = RigTabState::Draft;
                            break;
                        }
                    }
                    false
                }
            });
    }

    fn update_device_status(&mut self, device_id: RigId, status: ConnectionStatus) {
        for (_, tab) in self.dock_state.iter_all_tabs_mut() {
            if let RigTabState::Registered { rig_id, status: s } = &mut tab.state
                && *rig_id == device_id
            {
                *s = status;
                break;
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        let active_tab_id = self
            .dock_state
            .find_active_focused()
            .map(|(_, tab)| tab.tab_id)
            .or_else(|| {
                self.dock_state
                    .iter_leaves()
                    .next()
                    .map(|(_, node)| node.tabs[0].tab_id)
            });

        let tab_count = self.dock_state.iter_all_tabs().count();

        let mut tab_viewer = AppTabViewer::new(
            self.sender.clone(),
            self.rig_types.clone(),
            self.available_ports.clone(),
            active_tab_id,
            tab_count,
            &mut self.pending_creates,
        );

        DockArea::new(&mut self.dock_state)
            .show_add_buttons(true)
            .show_close_buttons(true)
            .tab_context_menus(false)
            .draggable_tabs(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .allowed_splits(AllowedSplits::None)
            .show_inside(ui, &mut tab_viewer);

        if tab_viewer.add_tab_request {
            self.dock_state
                .main_surface_mut()
                .push_to_first_leaf(RigTab {
                    tab_id: next_tab_id(&mut self.next_tab_id),
                    state: RigTabState::Draft,
                    config: RigConfig::default(),
                });
        }
    }
}

pub struct App {
    message_receiver: broadcast::Receiver<ManagerMessage>,
    tabs: AppTabs,
    tray_rx: std::sync::mpsc::Receiver<TrayAction>,
    ctx_for_tray: Option<std::sync::mpsc::SyncSender<egui::Context>>,
    should_quit: bool,
    hidden: bool,
    window_backend: WindowBackend,
}

impl App {
    pub fn new(
        message_receiver: broadcast::Receiver<ManagerMessage>,
        serial_sender: Sender<ManagerCommand>,
        rig_types: Vec<String>,
        initial_rigs: Vec<RigSettings>,
        tray_rx: std::sync::mpsc::Receiver<TrayAction>,
        ctx_tx: std::sync::mpsc::SyncSender<egui::Context>,
    ) -> Self {
        let mut tabs = AppTabs::new(serial_sender, rig_types);
        tabs.set_tabs(initial_rigs);
        App {
            message_receiver,
            tabs,
            tray_rx,
            ctx_for_tray: Some(ctx_tx),
            should_quit: false,
            hidden: false,
            window_backend: WindowBackend::Unknown,
        }
    }
}

impl App {
    fn handle_tray_action(&mut self, action: TrayAction, ctx: &egui::Context) {
        match action {
            TrayAction::Show => {
                self.hidden = false;
                self.window_backend.show_window(ctx);
            }
            TrayAction::Quit => {
                self.hidden = false;
                self.should_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn drain_tray_actions(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            self.handle_tray_action(action, ctx);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pump GTK events so libappindicator can process D-Bus messages.
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }

        if let Some(tx) = self.ctx_for_tray.take() {
            self.window_backend = WindowBackend::detect(_frame);
            tx.send(ctx.clone()).ok();
        }

        if ctx.input(|i| i.viewport().close_requested()) && !self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hidden = true;
            self.window_backend.hide_window(ctx);
        }

        if self.hidden {
            // On Wayland, blocking in update() prevents eframe from rendering
            // a new buffer that would immediately remap the hidden surface.
            #[cfg(target_os = "linux")]
            if matches!(self.window_backend, WindowBackend::Wayland(_)) {
                self.window_backend.hide_window(ctx);
                loop {
                    while gtk::events_pending() {
                        gtk::main_iteration_do(false);
                    }
                    match self
                        .tray_rx
                        .recv_timeout(std::time::Duration::from_millis(50))
                    {
                        Ok(action) => {
                            self.handle_tray_action(action, ctx);
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            self.hidden = false;
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                if self.hidden {
                    return;
                }
            }

            self.drain_tray_actions(ctx);
            if self.hidden {
                return;
            }
        }

        self.drain_tray_actions(ctx);

        self.tabs.poll_pending_creates();

        let mut has_messages = false;
        loop {
            match self.message_receiver.try_recv() {
                Ok(message) => {
                    has_messages = true;
                    match message {
                        ManagerMessage::AvailablePorts(ports) => {
                            self.tabs.available_ports = ports;
                        }
                        ManagerMessage::ConnectionStatusChanged { device_id, status } => {
                            self.tabs.update_device_status(device_id, status);
                        }
                        ManagerMessage::StatusUpdate { .. } => {}
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        if has_messages {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // TODO
        ctx.set_pixels_per_point(1.3);
        egui::CentralPanel::default().show(ctx, |ui| self.tabs.ui(ui));
    }
}
