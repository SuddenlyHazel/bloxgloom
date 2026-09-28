use super::*;

impl ClientApp {
    /// Do not drain the network mailbox until this all-or-nothing resource
    /// installation succeeds. Failure drops the candidate renderer/session.
    pub(super) fn install_window(&mut self, window: Arc<Window>) -> Result<(), String> {
        let mut renderer = pollster::block_on(Renderer::new_with_catalog(
            Arc::clone(&window),
            Arc::clone(&self.catalog),
        ))
        .map_err(|error| format!("renderer initialization: {error}"))?;
        if let Some(material) = self.network.package_material() {
            renderer
                .install_custom_material(material)
                .map_err(|error| format!("package material GPU preparation: {error}"))?;
        }
        if let Some(effect) = self.network.package_effect() {
            renderer
                .install_package_effect(effect)
                .map_err(|error| format!("package effect GPU preparation: {error}"))?;
        }
        if let Some(session) = &self.package_ui {
            renderer.install_package_ui(session.resources());
        }
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.refresh_layout();
        self.apply_fullscreen();
        Ok(())
    }

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
                        KeyCode::Enter | KeyCode::NumpadEnter if !repeat => session.activate(),
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

impl ClientApp {
    pub(super) fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
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
                        .and_then(|layout| {
                            if self.screen == UiScreen::Admin && self.admin_binding_mode {
                                layout
                                    .binding_hit(self.cursor.0, self.cursor.1)
                                    .or_else(|| layout.hit_test(self.cursor.0, self.cursor.1))
                            } else {
                                layout.hit_test(self.cursor.0, self.cursor.1)
                            }
                        })
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
                        if self.admin_binding_mode {
                            if event.repeat {
                                return;
                            }
                            if code == KeyCode::F4 {
                                self.set_screen(UiScreen::Playing);
                            } else if code == KeyCode::Escape {
                                if self.admin_binding_selected.take().is_none() {
                                    self.set_screen(UiScreen::Playing);
                                }
                            } else if self.admin_binding_selected.is_some() {
                                self.binding_capture(code);
                            } else if code == KeyCode::Tab {
                                self.advance_focus(self.shift_down);
                            } else if matches!(
                                code,
                                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                            ) && let Some(control) = self.focused_control
                            {
                                self.activate_control(event_loop, control);
                            }
                            return;
                        }
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
                                        if self.admin_input.len() + character.len_utf8() <= 1024 {
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
                            KeyCode::F4 => {
                                self.set_screen(UiScreen::Admin);
                                return;
                            }
                            KeyCode::Escape => {
                                self.on_escape();
                                return;
                            }
                            key if self.config.bindings.action(key)
                                == Some(crate::config::bindings::Action::Inventory) =>
                            {
                                self.toggle_inventory();
                                return;
                            }
                            key if self.screen == UiScreen::Playing
                                && self.config.bindings.action(key)
                                    == Some(crate::config::bindings::Action::KilnInput) =>
                            {
                                self.interact_aimed_entity(if self.shift_down {
                                    entities::kiln::TAKE_OUTPUT
                                } else {
                                    entities::kiln::INSERT_INPUT
                                });
                                return;
                            }
                            key if self.screen == UiScreen::Playing
                                && self.config.bindings.action(key)
                                    == Some(crate::config::bindings::Action::KilnFuel) =>
                            {
                                self.interact_aimed_entity(if self.shift_down {
                                    entities::kiln::TAKE_FUEL
                                } else {
                                    entities::kiln::INSERT_FUEL
                                });
                                return;
                            }
                            key if self.config.bindings.action(key)
                                == Some(crate::config::bindings::Action::Drop)
                                && matches!(
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
                        if self.screen == UiScreen::Playing
                            && let Some(action_key) = self.config.named_bindings.action(code)
                        {
                            // Local keys are inert across servers unless this exact
                            // command contract was advertised by the active catalog.
                            let action_key = action_key.to_owned();
                            if let Some(mut message) = super::actions::compose_named_command(
                                &self.catalog,
                                &action_key,
                                self.config.selected_slot as u8,
                                &self.inventory,
                                self.position.to_array().map(|v| v.floor() as i32),
                            ) {
                                if let Some(action_id) = self.allocate_action_id() {
                                    if let ClientMessage::EntityInteract { action_id: id, .. } =
                                        &mut message
                                    {
                                        *id = action_id;
                                    }
                                    self.queue_command(message);
                                } else {
                                    self.show_status("Action session pending or busy");
                                }
                            }
                            return;
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
                        let control = self.ui_layout.as_ref().and_then(|layout| {
                            if self.screen == UiScreen::Admin && self.admin_binding_mode {
                                layout
                                    .binding_hit(self.cursor.0, self.cursor.1)
                                    .or_else(|| layout.hit_test(self.cursor.0, self.cursor.1))
                            } else {
                                layout.hit_test(self.cursor.0, self.cursor.1)
                            }
                        });
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

    pub(super) fn device_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.screen == UiScreen::Playing
            && self.grabbed
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            self.yaw += dx as f32 * self.config.sensitivity;
            self.pitch = (self.pitch - dy as f32 * self.config.sensitivity).clamp(-1.55, 1.55);
        }
    }
}
