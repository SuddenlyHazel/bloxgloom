//! Packaged GLB creatures share immutable CPU/GPU assets. Per-actor clocks and
//! appearance palettes are presentation-only, bounded by nearest-first admission.
mod animation;
mod gpu;
mod material;
#[cfg(test)]
pub(super) mod tests;
use super::{AvatarModel, MAX_AVATARS, VisualAvatar};
use crate::render::model_asset::{Appearance, Look, Model};
use bloxgloom_host_api::entity::{TintMode, VisualState};
use glam::Vec3;
use std::{collections::HashMap, sync::Arc, time::Instant};
struct Asset {
    model: Arc<Model>,
    // Keep the decoded-memory admission alive as long as GPU/CPU actors use it.
    _prepared: Arc<crate::content::models::Prepared>,
}
struct Binding {
    asset: usize,
    scale: f32,
    clips: [Option<usize>; 6],
    player: Option<bloxgloom_host_api::model::PlayerModel>,
    run_speed: f32,
}
struct Actor {
    asset: usize,
    animator: animation::Animator,
    visual: VisualState,
    appearance: Appearance,
    position: Vec3,
    completed: Option<(u64, u32)>,
    seen: u64,
    tool: Option<(bool, f32)>,
    tool_sequence: u32,
}
pub(super) struct Renderer {
    ray_assets: Vec<Option<Arc<crate::render::trace::dynamic::DynamicAsset>>>,
    pub(super) ray_targets: crate::render::trace::dynamic::DynamicTargets,
    gpu: gpu::Gpu,
    assets: Vec<Asset>,
    bindings: HashMap<AvatarModel, Binding>,
    pub(super) first_person: Option<super::FirstPersonView>,
    actors: HashMap<u64, Actor>,
    clock: Instant,
    pub(super) preview_dt: Option<f32>,
    frame: u64,
    instances: Vec<gpu::Instance>,
    joints: Vec<[f32; 16]>,
    parts: Vec<gpu::Part>,
    ranges: Vec<std::ops::Range<u32>>,
}
fn look(model: &Model, visual: &VisualState) -> Look {
    let mut look = Look::default();
    for (group, selected) in model.controls.variants.iter().zip(visual.variants) {
        if let Some(option) = group.options.get(selected as usize) {
            look.variants
                .insert(group.name.clone(), option.name.clone());
        }
    }
    for (layer, visible) in model.controls.layers.iter().zip(visual.layers) {
        if visible >= 0 {
            look.layers.insert(layer.name.clone(), visible != 0);
        }
    }
    for (tint, value) in model.controls.tints.iter().zip(visual.tints) {
        if let Some(value) = value {
            look.tints.insert(
                tint.name.clone(),
                crate::render::model_asset::Color {
                    rgb: value.rgb,
                    mode: match value.mode {
                        TintMode::Multiply => crate::render::model_asset::TintMode::Multiply,
                        TintMode::Replace => crate::render::model_asset::TintMode::Replace,
                    },
                },
            );
        }
    }
    look
}
fn same_look(a: &VisualState, b: &VisualState) -> bool {
    a.variants == b.variants && a.layers == b.layers && a.tints == b.tints
}
fn model_yaw(game_yaw: f32) -> f32 {
    // GLB authoring faces -Z; authoritative yaw zero faces +Z. Apply this
    // basis correction once to both color and shadow instances.
    game_yaw + std::f32::consts::PI
}
// Match the color/motion vertex path: scale the authored pose, then apply the
// owner-only offset in model axes, and finally rotate/translate into the world.
fn motion_world(avatar: &VisualAvatar, scale: f32, first_person_offset: [f32; 3]) -> glam::Mat4 {
    let rotation = glam::Quat::from_rotation_y(model_yaw(avatar.pose[0]));
    glam::Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        rotation,
        avatar.position + rotation * Vec3::from_array(first_person_offset),
    )
}
fn motion_identity(
    asset: usize,
    model: AvatarModel,
    scale: f32,
    first_person: bool,
    parts: &[gpu::Part],
) -> u64 {
    // Include both ordinary and owner-only visibility, plus colors. A model or
    // view-mode switch must not reuse a different primitive's submitted history.
    super::motion::fingerprint((
        asset,
        model,
        scale.to_bits(),
        first_person,
        bytemuck::cast_slice::<_, u8>(parts),
    ))
}
impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera: &wgpu::BindGroupLayout,
        catalog: &crate::content::Catalog,
    ) -> Self {
        let mut assets = Vec::<Asset>::new();
        let mut keys = HashMap::new();
        let mut bindings = HashMap::new();
        for (id, definition) in catalog.mobile_entities() {
            let Some(authored) = &definition.authored_model else {
                continue;
            };
            let Some(prepared) = catalog.model_by_key(&authored.key) else {
                continue;
            };
            let asset = *keys.entry(authored.key.clone()).or_insert_with(|| {
                let index = assets.len();
                assets.push(Asset {
                    model: prepared.model.clone(),
                    _prepared: prepared.clone(),
                });
                index
            });
            let clip = |name: &Option<String>| {
                name.as_ref()
                    .and_then(|name| prepared.model.clips.iter().position(|c| &c.name == name))
            };
            bindings.insert(
                AvatarModel::Registered(id),
                Binding {
                    asset,
                    scale: prepared.scale * authored.scale,
                    clips: [
                        clip(&authored.idle),
                        clip(&authored.walk),
                        clip(&authored.run),
                        None,
                        None,
                        None,
                    ],
                    player: None,
                    run_speed: definition.body.speed * 0.75,
                },
            );
        }
        for (id, prepared) in catalog.models() {
            let Some(player) = &prepared.player else {
                continue;
            };
            let key = Arc::as_ptr(&prepared.model);
            let asset = assets
                .iter()
                .position(|a| Arc::as_ptr(&a.model) == key)
                .unwrap_or_else(|| {
                    let index = assets.len();
                    assets.push(Asset {
                        model: prepared.model.clone(),
                        _prepared: prepared.clone(),
                    });
                    index
                });
            bindings.insert(
                AvatarModel::PackagedPlayer(id),
                Binding {
                    asset,
                    scale: prepared.scale,
                    clips: player.clips().map(|name| {
                        name.as_ref().and_then(|name| {
                            prepared.model.clips.iter().position(|c| &c.name == name)
                        })
                    }),
                    player: Some(player.clone()),
                    run_speed: 4.0,
                },
            );
        }
        let gpu = gpu::Gpu::new(device, queue, format, camera, &assets);
        let ranges = vec![0..0; assets.len()];
        let ray_assets = assets
            .iter()
            .map(|asset| {
                crate::render::trace::dynamic::enabled()
                    .then(|| super::ray_targets::authored(&asset.model))
            })
            .collect();
        Self {
            ray_assets,
            ray_targets: Default::default(),
            gpu,
            assets,
            bindings,
            first_person: None,
            actors: HashMap::new(),
            clock: Instant::now(),
            preview_dt: None,
            frame: 0,
            instances: Vec::new(),
            joints: Vec::new(),
            parts: Vec::new(),
            ranges,
        }
    }
    pub fn has_model(&self, id: crate::content::EntityTypeId) -> bool {
        self.bindings.contains_key(&AvatarModel::Registered(id))
    }
    pub fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        let now = Instant::now();
        let dt = now.duration_since(self.clock).as_secs_f32().min(0.1);
        self.clock = now;
        self.set_at(queue, avatars, self.preview_dt.unwrap_or(dt));
    }
    fn set_at(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar], dt: f32) {
        self.ray_targets.clear();
        self.frame = self.frame.wrapping_add(1);
        self.instances.clear();
        self.gpu.motion.history.clear_pending();
        self.joints.clear();
        self.parts.clear();
        for (asset_id, asset) in self.assets.iter().enumerate() {
            let first = self.instances.len() as u32;
            for avatar in avatars.iter().take(MAX_AVATARS) {
                let Some(binding) = self
                    .bindings
                    .get(&avatar.model)
                    .filter(|b| b.asset == asset_id)
                else {
                    continue;
                };
                let model = &asset.model;
                let visual = avatar.model_pose.unwrap_or_default();
                if self
                    .actors
                    .get(&avatar.id)
                    .is_some_and(|actor| actor.asset != asset_id)
                {
                    self.actors.remove(&avatar.id);
                }
                let actor = self.actors.entry(avatar.id).or_insert_with(|| Actor {
                    asset: asset_id,
                    animator: animation::Animator::new(model),
                    visual,
                    appearance: model
                        .appearance(&look(model, &visual))
                        .expect("validated model look"),
                    position: avatar.position,
                    completed: None,
                    seen: self.frame,
                    tool: None,
                    tool_sequence: 0,
                });
                if !same_look(&actor.visual, &visual) {
                    actor.appearance = model
                        .appearance(&look(model, &visual))
                        .expect("validated model look");
                }
                let distance = (avatar.position - actor.position).with_y(0.0).length();
                let speed = if dt > 0.0 { distance / dt } else { 0.0 };
                // Interpolation teleports and airborne movement do not invent a
                // grounded running gait. Explicit clips remain independent.
                let player = binding.player.is_some();
                let moving = if player {
                    avatar.character_pose[2] > 0.03 && !avatar.airborne
                } else {
                    speed > 0.03 && distance < 2.0 && !avatar.airborne
                };
                let auto = if !moving {
                    0
                } else if (if player {
                    avatar.character_pose[3] > 0.1
                } else {
                    speed >= binding.run_speed
                }) && binding.clips[2].is_some()
                {
                    2
                } else {
                    1
                };
                let auto = if player && avatar.character_crouch > 0.1 && binding.clips[3].is_some()
                {
                    3
                } else {
                    auto
                };
                let clip = binding.clips[auto].or(binding.clips[0]);
                let mut target = animation::Playback {
                    clip,
                    serial: None,
                    start_s: 0.0,
                    speed: 1.0,
                    looping: true,
                    fade_s: if actor.visual.playback.is_some() && visual.playback.is_none() {
                        visual.transition_s
                    } else {
                        binding
                            .player
                            .as_ref()
                            .map_or(visual.transition_s, |p| p.crossfade_s)
                    },
                };
                if player
                    && let Some((right, time)) = avatar
                        .character_tool
                        .filter(|(_, time)| time.is_finite() && *time >= 0.0)
                    && let Some(clip) = binding.clips[if right { 5 } else { 4 }]
                {
                    if actor
                        .tool
                        .is_none_or(|(old_right, old_time)| old_right != right || time < old_time)
                    {
                        actor.tool_sequence = actor.tool_sequence.wrapping_add(1);
                    }
                    target = animation::Playback {
                        clip: Some(clip),
                        serial: Some((u64::MAX, actor.tool_sequence)),
                        start_s: time,
                        speed: 1.0,
                        looping: false,
                        fade_s: target.fade_s,
                    };
                }
                actor.tool = avatar.character_tool;
                if let Some(playback) = visual.playback.filter(|p| {
                    (p.clip as usize) < model.clips.len()
                        && p.speed.is_finite()
                        && (0.0..=8.0).contains(&p.speed)
                        && p.crossfade_s.is_finite()
                        && (0.0..=5.0).contains(&p.crossfade_s)
                        && p.started_tick <= visual.sample_tick
                }) {
                    let identity = (playback.started_tick, playback.sequence);
                    if actor.visual.playback != visual.playback {
                        actor.completed = None;
                    }
                    if actor.completed != Some(identity) {
                        if actor.visual.playback == visual.playback
                            && actor.animator.finished(model)
                        {
                            actor.completed = Some(identity);
                        } else {
                            let elapsed = visual.sample_tick.saturating_sub(playback.started_tick)
                                as f64
                                * 0.02
                                * f64::from(playback.speed);
                            let duration = f64::from(model.clips[playback.clip as usize].duration);
                            let start_s = if playback.looping && duration > 0.0 {
                                elapsed.rem_euclid(duration)
                            } else {
                                elapsed.min(duration)
                            } as f32;
                            if !playback.looping && elapsed >= duration {
                                actor.completed = Some(identity);
                            } else {
                                target = animation::Playback {
                                    clip: Some(playback.clip as usize),
                                    serial: Some(identity),
                                    start_s,
                                    speed: playback.speed,
                                    looping: playback.looping,
                                    fade_s: playback.crossfade_s,
                                };
                            }
                        }
                    }
                } else {
                    actor.completed = None;
                }
                actor.animator.step(model, target, dt);
                actor.visual = visual;
                actor.position = avatar.position;
                actor.seen = self.frame;
                if let Some(asset) = &self.ray_assets[asset_id] {
                    let world = motion_world(avatar, binding.scale, [0.0; 3]);
                    let parts = model
                        .primitives
                        .iter()
                        .zip(&actor.appearance.colors)
                        .map(|(primitive, &color)| {
                            (color, actor.appearance.visible[primitive.node])
                        })
                        .collect();
                    self.ray_targets
                        .instances
                        .push(super::ray_targets::instance(
                            asset.clone(),
                            avatar,
                            world,
                            crate::render::trace::dynamic::Deformation::Authored,
                            actor.animator.matrices.clone(),
                            parts,
                        ));
                    self.ray_targets.instances.last_mut().unwrap().skip_primary =
                        player && self.first_person.is_some_and(|view| view.id == avatar.id);
                }
                let offsets = [self.joints.len() as u32, self.parts.len() as u32];
                self.joints
                    .extend(actor.animator.matrices.iter().map(|m| m.to_cols_array()));
                let first_person = self.first_person.filter(|v| v.id == avatar.id && player);
                let first_person_offset = binding
                    .player
                    .as_ref()
                    .filter(|_| first_person.is_some())
                    .map_or([0.0; 3], |p| p.first_person_offset);
                let hidden = |node: usize| {
                    let Some(settings) = binding.player.as_ref().filter(|_| first_person.is_some())
                    else {
                        return false;
                    };
                    let mut node = Some(node);
                    while let Some(index) = node {
                        if settings
                            .first_person_hide
                            .iter()
                            .any(|n| n == &model.nodes[index].name)
                        {
                            return true;
                        }
                        node = model.nodes[index].parent;
                    }
                    false
                };
                self.parts
                    .extend(model.primitives.iter().zip(&actor.appearance.colors).map(
                        |(primitive, &color)| gpu::Part {
                            color,
                            flags: [
                                u32::from(actor.appearance.visible[primitive.node]),
                                u32::from(hidden(primitive.node)),
                                0,
                                0,
                            ],
                        },
                    ));
                if self.gpu.motion.enabled {
                    let world = motion_world(avatar, binding.scale, first_person_offset);
                    let identity = motion_identity(
                        asset_id,
                        avatar.model,
                        binding.scale,
                        first_person.is_some(),
                        &self.parts[offsets[1] as usize..],
                    );
                    self.gpu.motion.history.stage(
                        avatar.id,
                        identity,
                        avatar.position,
                        actor.animator.matrices.iter().map(|m| world * *m).collect(),
                    );
                }
                self.instances.push(gpu::Instance {
                    origin: avatar.position.to_array(),
                    yaw_scale: [model_yaw(avatar.pose[0]), binding.scale],
                    light_levels: avatar.packed_light(),
                    bounce: avatar.bounce,
                    glow_bounce: avatar.glow_bounce,
                    tint: avatar.tint,
                    offsets,
                    first_person_offset,
                });
            }
            self.ranges[asset_id] = first..self.instances.len() as u32;
        }
        self.actors.retain(|_, actor| actor.seen == self.frame);
        self.gpu
            .set(queue, &self.instances, &self.joints, &self.parts);
    }
    pub(super) fn enable_motion(&mut self, enabled: bool) {
        self.gpu.motion.enable(enabled);
    }
    pub(super) fn prepare_motion(&self, queue: &wgpu::Queue, frame: &super::motion::Frame) {
        self.gpu.motion.prepare(queue, frame);
    }
    pub(super) fn submitted(&mut self) {
        self.gpu.motion.submitted();
    }
    pub(super) fn draw_motion(&self, pass: &mut wgpu::RenderPass<'_>, camera: &wgpu::BindGroup) {
        self.gpu.draw_motion(pass, camera, &self.ranges);
    }
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        camera: &wgpu::BindGroup,
        shadow: bool,
    ) -> usize {
        self.gpu.draw(pass, camera, &self.ranges, shadow)
    }
}
