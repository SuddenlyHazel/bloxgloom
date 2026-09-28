use super::*;

impl ClientApp {
    // Shared by winit dispatch and focused tests; opening is available from play,
    // not gated behind admin mode or another screen's focused input.
    pub(super) fn package_ui_key(
        &mut self,
        code: KeyCode,
        text: Option<&str>,
        repeat: bool,
    ) -> bool {
        if code == KeyCode::F6 && self.package_ui.is_some() {
            if !repeat {
                self.set_screen(if self.screen == UiScreen::Package {
                    UiScreen::Playing
                } else {
                    UiScreen::Package
                });
            }
            return true;
        }
        if self.screen != UiScreen::Package {
            return false;
        }
        match code {
            KeyCode::Escape => self.set_screen(UiScreen::Playing),
            KeyCode::PageDown if !repeat => {
                if let Some(session) = &mut self.package_ui {
                    session.next_document();
                }
                self.refresh_layout();
            }
            _ => {
                if let Some(session) = &mut self.package_ui {
                    match code {
                        KeyCode::Tab if !repeat => session.tab(self.shift_down),
                        // No dispatch: authored event IDs carry no gameplay authority.
                        KeyCode::Enter
                        | KeyCode::NumpadEnter
                        | KeyCode::Tab
                        | KeyCode::PageDown => {}
                        _ => session.edit(code == KeyCode::Backspace, text),
                    }
                }
            }
        }
        true
    }
}

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
                match pollster::block_on(Renderer::new_with_catalog(
                    Arc::clone(&window),
                    Arc::clone(&self.catalog),
                )) {
                    Ok(mut renderer) => {
                        if let Some(session) = &self.package_ui {
                            renderer.install_package_ui(session.resources());
                        }
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
                if self.screen != UiScreen::Playing
                    && !(self.screen == UiScreen::Inventory
                        && self.focused_control == Some(UiControl::InventorySearch))
                {
                    self.focused_control = self
                        .ui_layout
                        .as_ref()
                        .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1))
                        .filter(|control| *control != UiControl::InventorySearch);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_down = modifiers.state().shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if pressed && self.package_ui_key(code, event.text.as_deref(), event.repeat) {
                        return;
                    }
                    if pressed && self.screen == UiScreen::Admin {
                        match code {
                            KeyCode::Escape | KeyCode::F4 => self.set_screen(UiScreen::Playing),
                            KeyCode::Enter | KeyCode::NumpadEnter => self.admin_run(),
                            KeyCode::Backspace => {
                                self.admin_input.pop();
                            }
                            _ => {
                                if let Some(value) = &event.text {
                                    for character in value.chars().filter(|character| {
                                        character.is_ascii_graphic() || *character == ' '
                                    }) {
                                        if self.admin_input.len() < 96 {
                                            self.admin_input.push(character);
                                        }
                                    }
                                }
                            }
                        }
                        return;
                    }
                    if pressed
                        && self.screen == UiScreen::Inventory
                        && self.focused_control == Some(UiControl::InventorySearch)
                        && !matches!(
                            code,
                            KeyCode::Escape | KeyCode::Tab | KeyCode::Enter | KeyCode::NumpadEnter
                        )
                    {
                        self.inventory_search.edit(code, event.text.as_deref());
                        return;
                    }
                    if pressed && !event.repeat {
                        if matches!(self.screen, UiScreen::Playing | UiScreen::Inventory)
                            && let Some(slot) = digit_slot(code)
                        {
                            self.select_slot(slot);
                            return;
                        }
                        match code {
                            KeyCode::F4 if self.admin_enabled => {
                                self.set_screen(UiScreen::Admin);
                                return;
                            }
                            KeyCode::Escape => {
                                self.on_escape();
                                return;
                            }
                            KeyCode::KeyE => {
                                self.toggle_inventory();
                                return;
                            }
                            KeyCode::KeyR if self.screen == UiScreen::Playing => {
                                self.interact_aimed_entity(if self.shift_down {
                                    entities::kiln::TAKE_OUTPUT
                                } else {
                                    entities::kiln::INSERT_INPUT
                                });
                                return;
                            }
                            KeyCode::KeyF if self.screen == UiScreen::Playing => {
                                self.interact_aimed_entity(if self.shift_down {
                                    entities::kiln::TAKE_FUEL
                                } else {
                                    entities::kiln::INSERT_FUEL
                                });
                                return;
                            }
                            KeyCode::KeyQ
                                if matches!(
                                    self.screen,
                                    UiScreen::Inventory | UiScreen::Playing
                                ) =>
                            {
                                let slot = if self.screen == UiScreen::Playing {
                                    Some(self.config.selected_slot as u8)
                                } else {
                                    self.inventory_source.or(match self.focused_control {
                                        Some(UiControl::InventorySlot(slot)) => Some(slot),
                                        _ => None,
                                    })
                                };
                                if let Some(slot) = slot
                                    && let Some(stack) =
                                        self.inventory.slots[slot as usize].as_ref()
                                {
                                    let count = if self.shift_down { stack.count } else { 1 };
                                    let Some(action_id) = self.allocate_action_id() else {
                                        self.show_status("Action session pending or busy");
                                        return;
                                    };
                                    self.queue_command(ClientMessage::DropStack {
                                        action_id,
                                        slot,
                                        count,
                                    });
                                    self.inventory_source = None;
                                }
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
                                if matches!(
                                    self.screen,
                                    UiScreen::Settings | UiScreen::Graphics
                                ) =>
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
                if self.screen == UiScreen::Package {
                    if button == MouseButton::Left
                        && let Some(session) = &mut self.package_ui
                    {
                        session.click(self.cursor.0, self.cursor.1);
                    }
                    return;
                }
                if self.screen != UiScreen::Playing {
                    if button == MouseButton::Left
                        || (button == MouseButton::Right
                            && matches!(self.screen, UiScreen::Inventory | UiScreen::Container))
                    {
                        let control = self
                            .ui_layout
                            .as_ref()
                            .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1));
                        if self.screen == UiScreen::Inventory && button == MouseButton::Left {
                            self.focused_control = control;
                        }
                        if let Some(control) = control {
                            if self.screen == UiScreen::Container
                                && let UiControl::KilnSlot(slot) = control
                            {
                                self.kiln_click(slot, button == MouseButton::Right);
                            } else if self.screen == UiScreen::Container
                                && let UiControl::InventorySlot(slot) = control
                            {
                                self.kiln_inventory_click(slot, button == MouseButton::Right);
                            } else if let UiControl::InventorySlot(slot) = control {
                                self.inventory_click(slot, button == MouseButton::Right);
                            } else if button == MouseButton::Left {
                                self.activate_control(event_loop, control);
                            }
                        }
                    }
                } else if !self.grabbed {
                    self.set_grab(true);
                } else if button == MouseButton::Left {
                    self.edit_aimed_block(false);
                } else if button == MouseButton::Right
                    && (self.shift_down
                        || (!self.interact_aimed_mobile()
                            && !self.open_aimed_kiln()
                            && !self.open_item_actions()))
                {
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
