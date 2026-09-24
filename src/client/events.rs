use super::*;

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Bloxgloom")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                match pollster::block_on(Renderer::new(Arc::clone(&window))) {
                    Ok(renderer) => {
                        self.renderer = Some(renderer);
                        self.window = Some(window);
                        self.refresh_layout();
                        self.apply_fullscreen();
                    }
                    Err(error) => {
                        eprintln!("renderer initialization: {error:?}");
                        event_loop.exit();
                    }
                }
            }
            Err(error) => {
                eprintln!("window creation: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
                self.refresh_layout();
            }
            WindowEvent::Focused(false) => {
                if self.screen == UiScreen::Playing {
                    self.set_screen(UiScreen::Pause);
                } else {
                    self.set_grab(false);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if self.screen != UiScreen::Playing {
                    self.focused_control = self
                        .ui_layout
                        .as_ref()
                        .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1));
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_down = modifiers.state().shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if pressed && !event.repeat {
                        if matches!(self.screen, UiScreen::Playing | UiScreen::Inventory)
                            && let Some(slot) = digit_slot(code)
                        {
                            self.select_slot(slot);
                            return;
                        }
                        match code {
                            KeyCode::Escape => {
                                self.on_escape();
                                return;
                            }
                            KeyCode::KeyE => {
                                self.toggle_inventory();
                                return;
                            }
                            KeyCode::F3 => {
                                self.config.debug_hud = !self.config.debug_hud;
                                self.config_writer.request_save(&self.config);
                                return;
                            }
                            KeyCode::Tab if self.screen != UiScreen::Playing => {
                                self.advance_focus(self.shift_down);
                                return;
                            }
                            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                                if self.screen != UiScreen::Playing =>
                            {
                                if let Some(control) = self.focused_control {
                                    self.activate_control(event_loop, control);
                                }
                                return;
                            }
                            KeyCode::ArrowLeft | KeyCode::ArrowRight
                                if self.screen == UiScreen::Settings =>
                            {
                                if let Some(
                                    UiControl::Decrease(setting) | UiControl::Increase(setting),
                                ) = self.focused_control
                                {
                                    self.change_setting(setting, code == KeyCode::ArrowRight);
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                    if self.screen != UiScreen::Playing {
                        return;
                    }
                    match code {
                        KeyCode::KeyW => self.keys.forward = pressed,
                        KeyCode::KeyS => self.keys.back = pressed,
                        KeyCode::KeyA => self.keys.left = pressed,
                        KeyCode::KeyD => self.keys.right = pressed,
                        KeyCode::Space => self.keys.up = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.down = pressed,
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if self.screen != UiScreen::Playing {
                    if button == MouseButton::Left {
                        let control = self
                            .ui_layout
                            .as_ref()
                            .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1));
                        if let Some(control) = control {
                            self.activate_control(event_loop, control);
                        }
                    }
                } else if !self.grabbed {
                    self.set_grab(true);
                } else if button == MouseButton::Left {
                    self.edit_aimed_block(false);
                } else if button == MouseButton::Right {
                    self.edit_aimed_block(true);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.screen == UiScreen::Playing => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                if y != 0.0 {
                    let shift = if y > 0.0 { -1 } else { 1 };
                    self.select_slot(
                        (self.config.selected_slot as i32 + shift).rem_euclid(9) as usize
                    );
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if self.screen == UiScreen::Playing
            && self.grabbed
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            self.yaw += dx as f32 * self.config.sensitivity;
            self.pitch = (self.pitch - dy as f32 * self.config.sensitivity).clamp(-1.55, 1.55);
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.config_writer.request_save(&self.config);
        self.config_writer.finish();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.disconnected {
            event_loop.exit();
            return;
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
}
