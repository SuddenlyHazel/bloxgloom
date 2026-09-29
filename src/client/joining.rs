//! Window-owned launcher. Preparing and live sessions never share catalogs,
//! replicas, UI registrations or GPU resources. The worker result is a candidate,
//! not permission to process snapshots: resource installation is the commit point.
use super::join_worker::Attempt;
use super::*;

pub(super) struct JoinApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    live: Option<ClientApp>,
    attempt: Option<Attempt>,
    prior_writer: Option<ConfigWriter>,
    address: String,
    config_path: PathBuf,
    error: Option<String>,
    pub(super) failure: Option<String>,
    admin_enabled: bool,
    first_frame: bool,
    next_frame: Instant,
}

impl JoinApp {
    pub(super) fn new(address: &str, admin_enabled: bool) -> Self {
        Self {
            window: None,
            renderer: None,
            live: None,
            attempt: None,
            prior_writer: None,
            address: address.chars().take(256).collect(),
            config_path: Config::default_path(),
            error: None,
            failure: None,
            admin_enabled,
            first_frame: true,
            next_frame: Instant::now(),
        }
    }

    fn status_renderer(&mut self) -> Result<(), String> {
        if self.renderer.is_none() {
            let mut renderer = pollster::block_on(Renderer::new_with_catalog(
                Arc::clone(self.window.as_ref().unwrap()),
                Arc::new(crate::content::Catalog::builtins()),
            ))
            .map_err(|error| error.to_string())?;
            renderer.clear_game_ui_intents();
            self.renderer = Some(renderer);
        }
        Ok(())
    }

    fn start(&mut self) {
        if self.attempt.is_some() || self.live.is_some() {
            return;
        }
        self.first_frame = false;
        self.error = None;
        match Attempt::start(
            self.address.clone(),
            self.config_path.clone(),
            self.prior_writer.take(),
        ) {
            Ok(attempt) => self.attempt = Some(attempt),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn action(&mut self) {
        if let Some(attempt) = &mut self.attempt {
            attempt.cancel();
        } else {
            self.start();
        }
    }

    fn retire_live(&mut self, reason: String) {
        if let Some(mut live) = self.live.take() {
            live.set_grab(false);
            live.retire_session();
            live.config_writer.request_save(&live.config);
            // Serialize retirement writes with the next attempt's settings
            // load, off-thread, without accumulating writers across switches.
            self.prior_writer = Some(live.config_writer);
        }
        self.error = Some(reason);
    }

    fn poll(&mut self) {
        let result = self.attempt.as_mut().and_then(Attempt::poll);
        if let Some(result) = result {
            self.attempt = None;
            match result {
                Err(error) => {
                    eprintln!("{error}");
                    self.error = Some(error.to_string().chars().take(2048).collect());
                }
                Ok(prepared) => {
                    // Retire the bootstrap surface before configuring the live
                    // one. GPU creation remains on the window thread; no network,
                    // config writes, package execution, light or mesh work does.
                    self.renderer = None;
                    let mut candidate =
                        ClientApp::new(prepared.network, prepared.config, self.config_path.clone());
                    candidate.admin_enabled = self.admin_enabled;
                    match candidate.install_window(Arc::clone(self.window.as_ref().unwrap())) {
                        Ok(()) => {
                            candidate.show_status("F2: leave session / change server");
                            self.window.as_ref().unwrap().set_title("Bloxgloom");
                            self.error = None;
                            self.live = Some(candidate);
                        }
                        Err(error) => {
                            candidate.fail_session(&error);
                            // A failed GPU install still created a config worker.
                            // Serialize its retirement before the next retry
                            // loads the same settings file.
                            self.prior_writer = Some(candidate.config_writer);
                            self.error = Some(error);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn finish(&mut self) {
        // Called after the event loop has closed, never during window dispatch.
        if self.live.is_some() {
            self.retire_live("Window closed".into());
        }
        self.renderer = None;
        self.window = None;
        if let Some(attempt) = self.attempt.take() {
            attempt.finish();
        }
        if let Some(mut writer) = self.prior_writer.take() {
            writer.finish();
        }
    }

    fn draw(&mut self, event_loop: &ActiveEventLoop) {
        self.poll();
        if let Some(live) = &mut self.live {
            live.frame();
            return;
        }
        if let Err(error) = self.status_renderer() {
            self.failure = Some(error);
            event_loop.exit();
            return;
        }
        let stage = self
            .attempt
            .as_ref()
            .map_or("starting", |a| a.control.label());
        let text = self.error.as_deref().unwrap_or(stage);
        let frame = UiFrame {
            screen: if self.error.is_some() {
                UiScreen::JoinFailed
            } else {
                UiScreen::Joining
            },
            status: Some(text),
            join_address: Some(&self.address),
            ..Default::default()
        };
        let camera = Camera {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y_radians: 1.2,
        };
        if let Err(error) = self.renderer.as_mut().unwrap().render(camera, &frame) {
            self.failure = Some(error.to_string());
            event_loop.exit();
            return;
        }
        for intent in self.renderer.as_mut().unwrap().take_game_ui_intents() {
            match intent {
                crate::render::GameUiIntent::JoinAddress(address) if self.attempt.is_none() => {
                    self.address = address
                        .chars()
                        .filter(|character| character.is_ascii_graphic())
                        .take(256)
                        .collect();
                }
                crate::render::GameUiIntent::JoinAction => self.action(),
                _ => {}
            }
        }
        // Present at least one preparing frame before starting slow work.
        if self.first_frame {
            self.first_frame = false;
            self.start();
        }
    }
}

#[cfg(test)]
pub(super) mod tests;

impl ApplicationHandler for JoinApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match event_loop.create_window(
            Window::default_attributes()
                .with_title("Bloxgloom - Joining")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
        ) {
            Ok(window) => {
                self.window = Some(Arc::new(window));
            }
            Err(error) => {
                self.failure = Some(error.to_string());
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        // F2 always leaves the current session and opens the address/retry UI.
        if matches!(&event, WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed && !event.repeat && event.physical_key == PhysicalKey::Code(KeyCode::F2))
            && self.live.is_some()
        {
            self.retire_live("Session closed. Edit the server address or retry.".into());
            return;
        }
        if let Some(live) = &mut self.live {
            live.window_event(event_loop, id, event);
            return;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.game_ui_event(&event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(event_loop),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        if let Some(attempt) = &mut self.attempt {
                            attempt.cancel();
                        }
                    }
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter)
                        if !event.repeat && self.attempt.is_none() =>
                    {
                        self.start()
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(live) = &mut self.live {
            live.device_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(live) = &self.live
            && live.disconnected
        {
            let reason = live
                .failure
                .clone()
                .unwrap_or_else(|| "Server disconnected".into());
            self.retire_live(reason);
        }
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_frame += FRAME;
            if self.next_frame <= now {
                self.next_frame = now + FRAME;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(attempt) = &mut self.attempt {
            attempt.cancel();
        }
        self.retire_live("Window closed".into());
        self.renderer = None;
        self.window = None;
    }
}
