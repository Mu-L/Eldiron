use crate::prelude::*;
use theframework::prelude::{Uuid, Vec2, Vec3};

fn default_visible() -> bool {
    true
}

fn default_chunk_size() -> i32 {
    512
}

fn default_operation() -> String {
    "draw".to_string()
}

fn default_brush() -> String {
    "material".to_string()
}

fn default_brush_shape() -> String {
    "solid".to_string()
}

fn default_material() -> String {
    "default".to_string()
}

fn default_finish() -> String {
    "natural".to_string()
}

fn default_material_id() -> u8 {
    0
}

fn default_material_mode() -> String {
    "coat".to_string()
}

fn default_clip() -> String {
    "object".to_string()
}

fn default_size() -> f32 {
    1.0
}

fn default_opacity() -> f32 {
    1.0
}

fn default_color() -> [u8; 4] {
    [132, 132, 128, 255]
}

fn default_palette_indices() -> Vec<u16> {
    Vec::new()
}

fn default_palette_colors() -> Vec<[u8; 4]> {
    Vec::new()
}

fn default_pattern_kind() -> String {
    "brick".to_string()
}

fn default_pattern_scale() -> f32 {
    1.0
}

fn default_pattern_mortar() -> f32 {
    0.08
}

fn default_pattern_detail() -> f32 {
    0.65
}

fn default_pattern_variation() -> f32 {
    0.6
}

fn default_stamp_density() -> f32 {
    0.6
}

fn default_stamp_size_jitter() -> f32 {
    0.25
}

fn default_stamp_rotation_jitter() -> f32 {
    1.0
}

fn default_stamp_variant() -> String {
    "wildflowers".to_string()
}

fn default_revision() -> u64 {
    0
}

fn default_order() -> u64 {
    0
}

pub const ISO_PAINT_BAKED_CHUNK_SIZE: i32 = 64;
pub const ISO_PAINT_BAKED_PIXELS_PER_UV: f32 = 128.0;
pub const ISO_PAINT_NO_SURFACE_DEPTH: f32 = -1.0;

fn deserialize_surface_depth<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = <Option<Vec<Option<f32>>> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(values
        .unwrap_or_default()
        .into_iter()
        .map(|depth| match depth {
            Some(depth) if depth.is_finite() && depth >= 0.0 => depth,
            _ => ISO_PAINT_NO_SURFACE_DEPTH,
        })
        .collect())
}

/// Stable reference to the scene element under an Iso Paint point.
///
/// The paint remains authored in fixed isometric screen space. This optional
/// metadata is only for later sorting, masking, picking, and scene-aware tools.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IsoPaintOwner {
    Unknown(u32),
    Vertex(u32),
    Linedef(u32),
    Sector(u32),
    Character(u32),
    Item(u32),
    Light(u32),
    ItemLight(u32),
    Triangle(u32),
    Terrain { x: i32, z: i32 },
    GeometryObject(Uuid),
    Hole { sector_id: u32, hole_id: u32 },
    Gizmo(u32),
}

impl IsoPaintOwner {
    pub fn same_paint_object(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::GeometryObject(a), Self::GeometryObject(b)) => a == b,
            (Self::Sector(a), Self::Sector(b)) => a == b,
            (Self::Terrain { .. }, Self::Terrain { .. }) => true,
            (Self::Character(a), Self::Character(b)) => a == b,
            (Self::Item(a), Self::Item(b)) => a == b,
            (Self::Triangle(a), Self::Triangle(b)) => a == b,
            _ => self == other,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintPoint {
    pub screen: [i32; 2],
    pub world: Option<[f32; 3]>,
    #[serde(default)]
    pub surface_uv: Option<[f32; 2]>,
    #[serde(default)]
    pub surface_normal: Option<[f32; 3]>,
    #[serde(default)]
    pub camera_scale: Option<f32>,
    #[serde(default)]
    pub viewport_size: Option<[i32; 2]>,
    pub owner: Option<IsoPaintOwner>,
}

impl IsoPaintPoint {
    pub fn new(screen: [i32; 2], world: Option<Vec3<f32>>, owner: Option<IsoPaintOwner>) -> Self {
        Self {
            screen,
            world: world.map(|p| [p.x, p.y, p.z]),
            surface_uv: None,
            surface_normal: None,
            camera_scale: None,
            viewport_size: None,
            owner,
        }
    }

    pub fn with_surface_uv(mut self, surface_uv: Option<Vec2<f32>>) -> Self {
        self.surface_uv = surface_uv.map(|uv| [uv.x, uv.y]);
        self
    }

    pub fn with_surface_normal(mut self, surface_normal: Option<Vec3<f32>>) -> Self {
        self.surface_normal = surface_normal.map(|normal| [normal.x, normal.y, normal.z]);
        self
    }

    pub fn with_camera_scale(mut self, camera_scale: Option<f32>) -> Self {
        self.camera_scale = camera_scale;
        self
    }

    pub fn with_viewport_size(mut self, viewport_size: Option<[i32; 2]>) -> Self {
        self.viewport_size = viewport_size;
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintStamp {
    pub id: Uuid,
    #[serde(default = "default_order")]
    pub order: u64,
    pub kind: String,
    pub screen: [i32; 2],
    pub world: Option<[f32; 3]>,
    #[serde(default)]
    pub surface_normal: Option<[f32; 3]>,
    pub owner: Option<IsoPaintOwner>,
    pub sort_depth: f32,
    pub size: f32,
    #[serde(default)]
    pub camera_scale: Option<f32>,
    #[serde(default = "default_stamp_variant")]
    pub variant: String,
    pub rotation: f32,
    pub variation: u32,
    #[serde(default = "default_material_id")]
    pub material_id: u8,
    pub color: [u8; 4],
    #[serde(default = "default_palette_indices")]
    pub palette_indices: Vec<u16>,
    #[serde(default = "default_palette_colors")]
    pub palette_colors: Vec<[u8; 4]>,
    pub opacity: f32,
    pub screen_bounds: [i32; 4],
}

impl IsoPaintStamp {
    pub fn new(
        kind: String,
        point: IsoPaintPoint,
        color: [u8; 4],
        palette_indices: Vec<u16>,
        palette_colors: Vec<[u8; 4]>,
        material_id: u8,
        size: f32,
        opacity: f32,
        variant: String,
        size_jitter: f32,
        rotation_jitter: f32,
    ) -> Self {
        let variation = Self::variation_seed(point.screen, point.world);
        let size_jitter = size_jitter.clamp(0.0, 1.0);
        let size_noise = ((variation >> 16) & 0xff) as f32 / 255.0;
        let size_scale = 1.0 + (size_noise - 0.5) * 2.0 * size_jitter;
        let size = (size * size_scale).max(0.01);
        let radius = (size * 5.0).round().max(5.0) as i32;
        let height = (size * 13.0).round().max(13.0) as i32;
        let rotation = ((variation & 0xffff) as f32 / 65535.0 - 0.5)
            * std::f32::consts::TAU
            * rotation_jitter.clamp(0.0, 1.0);
        Self {
            id: Uuid::new_v4(),
            order: 0,
            kind,
            screen: point.screen,
            world: point.world,
            surface_normal: point.surface_normal,
            owner: point.owner,
            sort_depth: point.screen[1] as f32,
            size,
            camera_scale: point.camera_scale,
            variant: if variant.is_empty() {
                default_stamp_variant()
            } else {
                variant
            },
            rotation,
            variation,
            material_id,
            color,
            palette_indices,
            palette_colors,
            opacity: opacity.clamp(0.0, 1.0),
            screen_bounds: [
                point.screen[0] - radius,
                point.screen[1] - height,
                point.screen[0] + radius,
                point.screen[1] + radius,
            ],
        }
    }

    fn variation_seed(screen: [i32; 2], world: Option<[f32; 3]>) -> u32 {
        let mut value = (screen[0] as u32).wrapping_mul(0x9e37_79b9)
            ^ (screen[1] as u32).wrapping_mul(0x85eb_ca6b);
        if let Some(world) = world {
            for component in world {
                value ^= component.to_bits().wrapping_mul(0xc2b2_ae35);
                value = value.rotate_left(13);
            }
        }
        value ^= value >> 16;
        value = value.wrapping_mul(0x7feb_352d);
        value ^ (value >> 15)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintStroke {
    pub id: Uuid,
    #[serde(default = "default_order")]
    pub order: u64,
    pub operation: String,
    pub brush: String,
    #[serde(default = "default_brush_shape")]
    pub brush_shape: String,
    pub material: String,
    pub finish: String,
    #[serde(default = "default_material_id")]
    pub material_id: u8,
    #[serde(default = "default_material_mode")]
    pub material_mode: String,
    #[serde(default = "default_clip")]
    pub clip: String,
    #[serde(default = "default_color")]
    pub color: [u8; 4],
    #[serde(default = "default_palette_indices")]
    pub palette_indices: Vec<u16>,
    #[serde(default = "default_palette_colors")]
    pub palette_colors: Vec<[u8; 4]>,
    #[serde(default = "default_pattern_kind")]
    pub pattern_kind: String,
    #[serde(default = "default_pattern_scale")]
    pub pattern_scale: f32,
    #[serde(default = "default_pattern_mortar")]
    pub pattern_mortar: f32,
    #[serde(default = "default_pattern_detail")]
    pub pattern_detail: f32,
    #[serde(default = "default_pattern_variation")]
    pub pattern_variation: f32,
    pub size: f32,
    pub opacity: f32,
    pub points: Vec<IsoPaintPoint>,
    pub screen_bounds: [i32; 4],
}

impl IsoPaintStroke {
    pub fn new(
        operation: String,
        brush: String,
        brush_shape: String,
        material: String,
        finish: String,
        material_id: u8,
        material_mode: String,
        clip: String,
        color: [u8; 4],
        palette_indices: Vec<u16>,
        palette_colors: Vec<[u8; 4]>,
        pattern_kind: String,
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
        size: f32,
        opacity: f32,
        first_point: IsoPaintPoint,
    ) -> Self {
        let screen = first_point.screen;
        Self {
            id: Uuid::new_v4(),
            order: 0,
            operation,
            brush,
            brush_shape,
            material,
            finish,
            material_id,
            material_mode,
            clip,
            color,
            palette_indices,
            palette_colors,
            pattern_kind,
            pattern_scale: pattern_scale.clamp(0.25, 4.0),
            pattern_mortar: pattern_mortar.clamp(0.0, 0.4),
            pattern_detail: pattern_detail.clamp(0.0, 1.0),
            pattern_variation: pattern_variation.clamp(0.0, 1.0),
            size: size.max(0.01),
            opacity: opacity.clamp(0.0, 1.0),
            points: vec![first_point],
            screen_bounds: [screen[0], screen[1], screen[0], screen[1]],
        }
    }

    pub fn append_point(&mut self, point: IsoPaintPoint) {
        self.screen_bounds[0] = self.screen_bounds[0].min(point.screen[0]);
        self.screen_bounds[1] = self.screen_bounds[1].min(point.screen[1]);
        self.screen_bounds[2] = self.screen_bounds[2].max(point.screen[0]);
        self.screen_bounds[3] = self.screen_bounds[3].max(point.screen[1]);
        self.points.push(point);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintChunk {
    pub origin: [i32; 2],
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(default)]
    pub stamp_revision: u64,
    pub strokes: Vec<IsoPaintStroke>,
    #[serde(default)]
    pub stamps: Vec<IsoPaintStamp>,
}

impl IsoPaintChunk {
    pub fn new(origin: [i32; 2]) -> Self {
        Self {
            origin,
            revision: 0,
            stamp_revision: 0,
            strokes: Vec::new(),
            stamps: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintBakedChunk {
    pub owner: IsoPaintOwner,
    pub origin: [i32; 2],
    #[serde(default = "default_revision")]
    pub revision: u64,
    pub color_rgba: Vec<u8>,
    pub material_rgba: Vec<u8>,
}

impl IsoPaintBakedChunk {
    pub fn new(owner: IsoPaintOwner, origin: [i32; 2]) -> Self {
        let len = ISO_PAINT_BAKED_CHUNK_SIZE as usize * ISO_PAINT_BAKED_CHUNK_SIZE as usize * 4;
        let mut material_rgba = vec![0_u8; len];
        for pixel in material_rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[254, 0, 0, 0]);
        }
        Self {
            owner,
            origin,
            revision: 0,
            color_rgba: vec![0_u8; len],
            material_rgba,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintScreenChunk {
    pub origin: [i32; 2],
    #[serde(default)]
    pub screen_anchor: Option<[i32; 2]>,
    #[serde(default)]
    pub world_anchor: Option<[f32; 3]>,
    #[serde(default)]
    pub camera_scale: Option<f32>,
    #[serde(default)]
    pub viewport_size: Option<[i32; 2]>,
    #[serde(default)]
    pub source_camera_key: Option<u64>,
    #[serde(default)]
    pub clip_owner: Option<IsoPaintOwner>,
    #[serde(default)]
    pub replace_color: bool,
    #[serde(default = "default_revision")]
    pub revision: u64,
    pub color_rgba: Vec<u8>,
    pub material_rgba: Vec<u8>,
    #[serde(default, deserialize_with = "deserialize_surface_depth")]
    pub surface_depth: Vec<f32>,
    #[serde(default)]
    pub surface_anchor_depth: Option<f32>,
}

impl IsoPaintScreenChunk {
    pub fn new(origin: [i32; 2], size: i32) -> Self {
        let size = size.max(1) as usize;
        let len = size * size * 4;
        let mut material_rgba = vec![0_u8; len];
        for pixel in material_rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[254, 0, 0, 0]);
        }
        Self {
            origin,
            screen_anchor: None,
            world_anchor: None,
            camera_scale: None,
            viewport_size: None,
            source_camera_key: None,
            clip_owner: None,
            replace_color: false,
            revision: 0,
            color_rgba: vec![0_u8; len],
            material_rgba,
            surface_depth: vec![ISO_PAINT_NO_SURFACE_DEPTH; size * size],
            surface_anchor_depth: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IsoPaintLayer {
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: i32,
    #[serde(default)]
    pub chunks: IndexMap<String, IsoPaintChunk>,
    #[serde(default)]
    pub screen_chunks: IndexMap<String, IsoPaintScreenChunk>,
    #[serde(default)]
    pub baked_chunks: IndexMap<String, IsoPaintBakedChunk>,
    #[serde(skip)]
    pub screen_commit_strokes: Vec<Uuid>,
    #[serde(default = "default_operation")]
    pub active_operation: String,
    #[serde(default = "default_brush")]
    pub active_brush: String,
    #[serde(default = "default_brush_shape")]
    pub active_brush_shape: String,
    #[serde(default = "default_material")]
    pub active_material: String,
    #[serde(default = "default_finish")]
    pub active_finish: String,
    #[serde(default = "default_material_id")]
    pub active_material_id: u8,
    #[serde(default = "default_material_mode")]
    pub active_material_mode: String,
    #[serde(default = "default_clip")]
    pub active_clip: String,
    #[serde(default = "default_color")]
    pub active_color: [u8; 4],
    #[serde(default = "default_palette_indices")]
    pub active_palette_indices: Vec<u16>,
    #[serde(default = "default_palette_colors")]
    pub active_palette_colors: Vec<[u8; 4]>,
    #[serde(default = "default_pattern_kind")]
    pub active_pattern_kind: String,
    #[serde(default = "default_pattern_scale")]
    pub active_pattern_scale: f32,
    #[serde(default = "default_pattern_mortar")]
    pub active_pattern_mortar: f32,
    #[serde(default = "default_pattern_detail")]
    pub active_pattern_detail: f32,
    #[serde(default = "default_pattern_variation")]
    pub active_pattern_variation: f32,
    #[serde(default = "default_stamp_density")]
    pub active_stamp_density: f32,
    #[serde(default = "default_stamp_size_jitter")]
    pub active_stamp_size_jitter: f32,
    #[serde(default = "default_stamp_rotation_jitter")]
    pub active_stamp_rotation_jitter: f32,
    #[serde(default = "default_stamp_variant")]
    pub active_stamp_variant: String,
    #[serde(default = "default_size")]
    pub active_size: f32,
    #[serde(default = "default_opacity")]
    pub active_opacity: f32,
}

impl Default for IsoPaintLayer {
    fn default() -> Self {
        Self {
            visible: true,
            chunk_size: default_chunk_size(),
            chunks: IndexMap::default(),
            screen_chunks: IndexMap::default(),
            baked_chunks: IndexMap::default(),
            screen_commit_strokes: Vec::new(),
            active_operation: default_operation(),
            active_brush: default_brush(),
            active_brush_shape: default_brush_shape(),
            active_material: default_material(),
            active_finish: default_finish(),
            active_material_id: default_material_id(),
            active_material_mode: default_material_mode(),
            active_clip: default_clip(),
            active_color: default_color(),
            active_palette_indices: default_palette_indices(),
            active_palette_colors: default_palette_colors(),
            active_pattern_kind: default_pattern_kind(),
            active_pattern_scale: default_pattern_scale(),
            active_pattern_mortar: default_pattern_mortar(),
            active_pattern_detail: default_pattern_detail(),
            active_pattern_variation: default_pattern_variation(),
            active_stamp_density: default_stamp_density(),
            active_stamp_size_jitter: default_stamp_size_jitter(),
            active_stamp_rotation_jitter: default_stamp_rotation_jitter(),
            active_stamp_variant: default_stamp_variant(),
            active_size: default_size(),
            active_opacity: default_opacity(),
        }
    }
}

impl IsoPaintLayer {
    pub fn stroke_first_owner(&self, stroke_id: Uuid) -> Option<IsoPaintOwner> {
        self.chunks
            .values()
            .flat_map(|chunk| &chunk.strokes)
            .find(|stroke| stroke.id == stroke_id)
            .and_then(|stroke| stroke.points.first())
            .and_then(|point| point.owner.clone())
    }

    pub fn set_active_settings(
        &mut self,
        operation: impl Into<String>,
        brush: impl Into<String>,
        brush_shape: impl Into<String>,
        material: impl Into<String>,
        finish: impl Into<String>,
        material_id: u8,
        material_mode: impl Into<String>,
        clip: impl Into<String>,
        color: [u8; 4],
        palette_indices: Vec<u16>,
        palette_colors: Vec<[u8; 4]>,
        pattern_kind: impl Into<String>,
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
        stamp_density: f32,
        stamp_size_jitter: f32,
        stamp_rotation_jitter: f32,
        stamp_variant: impl Into<String>,
        size: f32,
        opacity: f32,
    ) {
        self.active_operation = operation.into();
        self.active_brush = brush.into();
        self.active_brush_shape = brush_shape.into();
        self.active_material = material.into();
        self.active_finish = finish.into();
        self.active_material_id = material_id;
        let material_mode = material_mode.into();
        self.active_material_mode = match material_mode.as_str() {
            "replace" => "replace".to_string(),
            "stamp" => "stamp".to_string(),
            _ => "coat".to_string(),
        };
        self.active_clip = clip.into();
        self.active_color = color;
        self.active_palette_indices = palette_indices;
        self.active_palette_colors = palette_colors;
        self.active_pattern_kind = pattern_kind.into();
        self.active_pattern_scale = pattern_scale.clamp(0.25, 4.0);
        self.active_pattern_mortar = pattern_mortar.clamp(0.0, 0.4);
        self.active_pattern_detail = pattern_detail.clamp(0.0, 1.0);
        self.active_pattern_variation = pattern_variation.clamp(0.0, 1.0);
        self.active_stamp_density = stamp_density.clamp(0.0, 1.0);
        self.active_stamp_size_jitter = stamp_size_jitter.clamp(0.0, 1.0);
        self.active_stamp_rotation_jitter = stamp_rotation_jitter.clamp(0.0, 1.0);
        let stamp_variant = stamp_variant.into();
        self.active_stamp_variant = if stamp_variant.is_empty() {
            default_stamp_variant()
        } else {
            stamp_variant
        };
        self.active_size = size.max(0.01);
        self.active_opacity = opacity.clamp(0.0, 1.0);
    }

    pub fn chunk_origin_for_screen(&self, screen: [i32; 2]) -> [i32; 2] {
        let size = self.chunk_size.max(1);
        [
            screen[0].div_euclid(size) * size,
            screen[1].div_euclid(size) * size,
        ]
    }

    pub fn chunk_key(origin: [i32; 2]) -> String {
        format!("{},{}", origin[0], origin[1])
    }

    pub fn ensure_screen_chunk(&mut self, origin: [i32; 2]) -> &mut IsoPaintScreenChunk {
        let key = Self::chunk_key(origin);
        self.screen_chunks
            .entry(key)
            .or_insert_with(|| IsoPaintScreenChunk::new(origin, self.chunk_size))
    }

    fn next_paint_order(&self) -> u64 {
        self.chunks
            .values()
            .flat_map(|chunk| {
                chunk
                    .strokes
                    .iter()
                    .map(|stroke| stroke.order)
                    .chain(chunk.stamps.iter().map(|stamp| stamp.order))
            })
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    pub fn begin_stroke(&mut self, first_point: IsoPaintPoint) -> Uuid {
        let origin = self.chunk_origin_for_screen(first_point.screen);
        let key = Self::chunk_key(origin);
        let mut stroke = IsoPaintStroke::new(
            self.active_operation.clone(),
            self.active_brush.clone(),
            self.active_brush_shape.clone(),
            self.active_material.clone(),
            self.active_finish.clone(),
            self.active_material_id,
            self.active_material_mode.clone(),
            self.active_clip.clone(),
            self.active_color,
            self.active_palette_indices.clone(),
            self.active_palette_colors.clone(),
            self.active_pattern_kind.clone(),
            self.active_pattern_scale,
            self.active_pattern_mortar,
            self.active_pattern_detail,
            self.active_pattern_variation,
            self.active_size,
            self.active_opacity,
            first_point,
        );
        stroke.order = self.next_paint_order();
        let id = stroke.id;
        let chunk = self
            .chunks
            .entry(key)
            .or_insert_with(|| IsoPaintChunk::new(origin));
        chunk.revision = chunk.revision.wrapping_add(1);
        chunk.strokes.push(stroke);
        id
    }

    pub fn add_stamp(&mut self, point: IsoPaintPoint) -> Uuid {
        let origin = self.chunk_origin_for_screen(point.screen);
        let key = Self::chunk_key(origin);
        let mut stamp = IsoPaintStamp::new(
            self.active_brush.clone(),
            point,
            self.active_color,
            self.active_palette_indices.clone(),
            self.active_palette_colors.clone(),
            self.active_material_id,
            self.active_size,
            self.active_opacity,
            self.active_stamp_variant.clone(),
            self.active_stamp_size_jitter,
            self.active_stamp_rotation_jitter,
        );
        stamp.order = self.next_paint_order();
        let id = stamp.id;
        let chunk = self
            .chunks
            .entry(key)
            .or_insert_with(|| IsoPaintChunk::new(origin));
        chunk.stamp_revision = chunk.stamp_revision.wrapping_add(1);
        chunk.stamps.push(stamp);
        id
    }

    pub fn erase_stamps_near(&mut self, screen: [i32; 2], radius: f32) -> bool {
        self.erase_stamps_near_owner(screen, radius, None)
    }

    pub fn erase_stamps_near_owner(
        &mut self,
        screen: [i32; 2],
        radius: f32,
        owner_filter: Option<&IsoPaintOwner>,
    ) -> bool {
        self.erase_stamps_near_owner_kind(screen, radius, owner_filter, None)
    }

    pub fn erase_stamps_near_owner_kind(
        &mut self,
        screen: [i32; 2],
        radius: f32,
        owner_filter: Option<&IsoPaintOwner>,
        kind_filter: Option<&str>,
    ) -> bool {
        let radius = (radius * 3.0).round().max(3.0) as i32;
        let radius_sq = radius * radius;
        let mut changed = false;
        for chunk in self.chunks.values_mut() {
            let before = chunk.stamps.len();
            chunk.stamps.retain(|stamp| {
                let dx = stamp.screen[0] - screen[0];
                let dy = stamp.screen[1] - screen[1];
                let near = dx * dx + dy * dy <= radius_sq;
                let owner_matches = owner_filter.is_none_or(|owner_filter| {
                    stamp
                        .owner
                        .as_ref()
                        .is_some_and(|owner| owner_filter.same_paint_object(owner))
                });
                let kind_matches = kind_filter.is_none_or(|kind_filter| {
                    stamp.kind == kind_filter
                        || (kind_filter == "grass" && stamp.kind == "grass_stamp")
                });
                !(near && owner_matches && kind_matches)
            });
            if chunk.stamps.len() != before {
                chunk.stamp_revision = chunk.stamp_revision.wrapping_add(1);
                changed = true;
            }
        }
        changed
    }

    pub fn append_point(&mut self, stroke_id: Uuid, point: IsoPaintPoint) -> bool {
        for chunk in self.chunks.values_mut() {
            if let Some(stroke) = chunk
                .strokes
                .iter_mut()
                .find(|stroke| stroke.id == stroke_id)
            {
                if let Some(last) = stroke.points.last() {
                    let min_spacing = (stroke.size * 0.75).round().clamp(1.0, 12.0) as i32;
                    let dx = point.screen[0] - last.screen[0];
                    let dy = point.screen[1] - last.screen[1];
                    if dx * dx + dy * dy < min_spacing * min_spacing {
                        return false;
                    }
                }
                stroke.append_point(point);
                chunk.revision = chunk.revision.wrapping_add(1);
                return true;
            }
        }
        false
    }

    pub fn take_stroke(&mut self, stroke_id: Uuid) -> Option<IsoPaintStroke> {
        for chunk in self.chunks.values_mut() {
            if let Some(index) = chunk
                .strokes
                .iter()
                .position(|stroke| stroke.id == stroke_id)
            {
                chunk.revision = chunk.revision.wrapping_add(1);
                return Some(chunk.strokes.remove(index));
            }
        }
        None
    }

    pub fn mark_stroke_for_screen_commit(&mut self, stroke_id: Uuid) {
        if !self.screen_commit_strokes.contains(&stroke_id) {
            self.screen_commit_strokes.push(stroke_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_origin_uses_floor_division_for_negative_screen_coords() {
        let layer = IsoPaintLayer::default();
        assert_eq!(layer.chunk_origin_for_screen([10, 20]), [0, 0]);
        assert_eq!(layer.chunk_origin_for_screen([-1, -1]), [-512, -512]);
    }

    #[test]
    fn screen_chunks_store_color_and_material_pixels() {
        let mut layer = IsoPaintLayer::default();
        let chunk = layer.ensure_screen_chunk([0, 0]);
        assert_eq!(chunk.color_rgba.len(), 512 * 512 * 4);
        assert_eq!(chunk.material_rgba.len(), 512 * 512 * 4);
        assert_eq!(chunk.surface_depth.len(), 512 * 512);
        assert_eq!(&chunk.color_rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&chunk.material_rgba[0..4], &[254, 0, 0, 0]);
    }

    #[test]
    fn stroke_bounds_expand_when_points_are_appended() {
        let mut layer = IsoPaintLayer::default();
        let id = layer.begin_stroke(IsoPaintPoint::new([10, 12], None, None));
        let initial_revision = layer.chunks.values().next().unwrap().revision;
        assert!(layer.append_point(id, IsoPaintPoint::new([20, 4], None, None)));
        let chunk = layer.chunks.values().next().unwrap();
        assert_eq!(chunk.strokes[0].screen_bounds, [10, 4, 20, 12]);
        assert!(chunk.revision > initial_revision);
    }

    #[test]
    fn stamps_are_stored_separately_from_strokes() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "grass".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.active_color = [35, 120, 45, 255];
        let id = layer.add_stamp(IsoPaintPoint::new(
            [20, 24],
            Some(Vec3::new(1.0, 2.0, 3.0)),
            None,
        ));
        let chunk = layer.chunks.values().next().unwrap();
        assert!(chunk.strokes.is_empty());
        assert_eq!(chunk.stamps.len(), 1);
        assert_eq!(chunk.stamps[0].id, id);
        assert_eq!(chunk.stamps[0].kind, "grass");
        assert_eq!(chunk.stamps[0].world, Some([1.0, 2.0, 3.0]));
        assert!(chunk.stamp_revision > 0);
    }

    #[test]
    fn rubble_stamps_keep_their_brush_kind() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "rubble".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.active_color = [96, 90, 76, 255];
        layer.add_stamp(IsoPaintPoint::new([42, 48], None, None));
        let chunk = layer.chunks.values().next().unwrap();
        assert!(chunk.strokes.is_empty());
        assert_eq!(chunk.stamps.len(), 1);
        assert_eq!(chunk.stamps[0].kind, "rubble");
        assert_eq!(chunk.stamps[0].color, [96, 90, 76, 255]);
    }

    #[test]
    fn stamps_keep_their_material_id() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "leaves".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.active_material_id = 44;
        layer.add_stamp(IsoPaintPoint::new([42, 48], None, None));
        let chunk = layer.chunks.values().next().unwrap();
        assert_eq!(chunk.stamps.len(), 1);
        assert_eq!(chunk.stamps[0].material_id, 44);
    }

    #[test]
    fn flowers_stamps_keep_their_brush_kind() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "flowers".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.active_stamp_variant = "poppies".to_string();
        layer.active_color = [75, 119, 57, 255];
        layer.active_palette_indices = vec![37, 32, 46, 47];
        layer.active_palette_colors = vec![
            [30, 80, 40, 255],
            [220, 40, 50, 255],
            [180, 28, 42, 255],
            [40, 22, 18, 255],
        ];
        layer.add_stamp(IsoPaintPoint::new([50, 56], None, None));
        let chunk = layer.chunks.values().next().unwrap();
        assert_eq!(chunk.stamps.len(), 1);
        assert_eq!(chunk.stamps[0].kind, "flowers");
        assert_eq!(chunk.stamps[0].variant, "poppies");
        assert_eq!(chunk.stamps[0].color, [75, 119, 57, 255]);
        assert_eq!(chunk.stamps[0].palette_indices, vec![37, 32, 46, 47]);
        assert_eq!(chunk.stamps[0].palette_colors[1], [220, 40, 50, 255]);
    }

    #[test]
    fn stamp_erase_can_be_filtered_to_one_owner() {
        let mut layer = IsoPaintLayer::default();
        let owner_a = IsoPaintOwner::Sector(1);
        let owner_b = IsoPaintOwner::Sector(2);
        layer.add_stamp(IsoPaintPoint::new([20, 24], None, Some(owner_a.clone())));
        layer.add_stamp(IsoPaintPoint::new([21, 25], None, Some(owner_b)));

        assert!(layer.erase_stamps_near_owner([20, 24], 1.0, Some(&owner_a)));
        let stamps: Vec<_> = layer
            .chunks
            .values()
            .flat_map(|chunk| &chunk.stamps)
            .collect();
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].owner, Some(IsoPaintOwner::Sector(2)));
    }

    #[test]
    fn stamp_erase_can_be_filtered_to_one_kind() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "rubble".to_string();
        layer.add_stamp(IsoPaintPoint::new([20, 24], None, None));
        layer.active_brush = "leaves".to_string();
        layer.add_stamp(IsoPaintPoint::new([21, 25], None, None));

        assert!(layer.erase_stamps_near_owner_kind([20, 24], 1.0, None, Some("rubble")));
        let stamps: Vec<_> = layer
            .chunks
            .values()
            .flat_map(|chunk| &chunk.stamps)
            .collect();
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].kind, "leaves");
    }

    #[test]
    fn stamp_jitter_settings_affect_new_stamps() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "footprints".to_string();
        layer.active_size = 4.0;
        layer.active_stamp_size_jitter = 0.0;
        layer.active_stamp_rotation_jitter = 0.0;
        layer.add_stamp(IsoPaintPoint::new([42, 48], None, None));
        let stamp = layer
            .chunks
            .values()
            .next()
            .unwrap()
            .stamps
            .first()
            .unwrap();
        assert_eq!(stamp.size, 4.0);
        assert_eq!(stamp.rotation, 0.0);
    }

    #[test]
    fn stamps_remember_placement_camera_scale() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "flowers".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.add_stamp(IsoPaintPoint::new([42, 48], None, None).with_camera_scale(Some(6.0)));
        let stamp = layer
            .chunks
            .values()
            .next()
            .unwrap()
            .stamps
            .first()
            .unwrap();
        assert_eq!(stamp.camera_scale, Some(6.0));
    }

    #[test]
    fn stamp_erase_only_removes_nearby_stamps() {
        let mut layer = IsoPaintLayer::default();
        layer.active_brush = "grass".to_string();
        layer.active_material_mode = "stamp".to_string();
        layer.add_stamp(IsoPaintPoint::new([20, 24], None, None));
        layer.add_stamp(IsoPaintPoint::new([200, 240], None, None));
        assert!(layer.erase_stamps_near([21, 25], 1.0));
        let stamps: Vec<_> = layer
            .chunks
            .values()
            .flat_map(|chunk| &chunk.stamps)
            .collect();
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].screen, [200, 240]);
    }
}
