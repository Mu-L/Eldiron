use crate::prelude::*;
use rayon::prelude::*;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use theframework::prelude::*;

pub struct Renderer {
    pub textures: FxHashMap<Uuid, TheRGBATile>,
    pub materials: IndexMap<Uuid, MaterialFXObject>,
    pub tiles: TheFlattenedMap3D<Uuid>,
    pub position: Vec3f,
    pub hover_pos: Option<Vec3i>,

    screen_offset: Vec2i,

    pub canvas: GameCanvas,
}

#[allow(clippy::new_without_default)]
impl Renderer {
    pub fn new() -> Self {
        Self {
            textures: FxHashMap::default(),
            materials: IndexMap::default(),
            tiles: TheFlattenedMap3D::new((0, -1, 0), (80, 2, 80)),
            position: Vec3f::zero(),
            hover_pos: None,

            screen_offset: Vec2i::zero(),

            canvas: GameCanvas::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prerender(
        &mut self,
        _size: Vec2i,
        prerendered: &mut PreRendered,
        region: &Region,
        settings: &mut RegionDrawSettings,
        palette: &ThePalette,
        sender: mpsc::Sender<PreRenderResult>,
    ) -> bool {
        // --
        let mut tiles = vec![];

        // Add all tiles which do not have all samples
        let w = region.width;
        let h = region.height;
        for x in 0..w {
            for y in 0..h {
                let tile = Vec2i::new(x, y);
                if let Some(samples) = prerendered.tile_samples.get(&tile) {
                    if (*samples as i32) < region.pathtracer_samples {
                        tiles.push(tile);
                    }
                } else {
                    tiles.push(tile);
                }
            }
        }

        // Finished ?
        if tiles.is_empty() {
            return false;
        }

        let _start = self.get_time();

        let max_render_distance = 50;
        // TODO: Get Setting from Render Node

        let update = RegionUpdate::default();

        // Fill the code level with the blocking info and collect lights
        let mut level = Level::new(region.width, region.height, settings.time);
        region.fill_code_level(&mut level, &self.textures, &update);

        // Collect the material params
        let mut material_params: FxHashMap<Uuid, Vec<Vec<f32>>> = FxHashMap::default();
        for (id, material) in &self.materials {
            let params = material.load_parameters(&settings.time);
            material_params.insert(*id, params);
        }

        // Collect the geo_object params
        let mut geo_params: FxHashMap<Uuid, Vec<Vec<f32>>> = FxHashMap::default();
        for (id, geo_obj) in &region.geometry {
            let params = geo_obj.load_parameters(&settings.time);
            geo_params.insert(*id, params);
        }

        // Collect the render settings params
        let render_settings_params: Vec<Vec<f32>> = region.regionfx.load_parameters(&settings.time);

        //
        let prerendered_mutex = Arc::new(Mutex::new(prerendered));

        let _start = self.get_time();

        let tile_size = region.tile_size;
        let tile_size_f = region.tile_size as f32;

        tiles.par_iter_mut().for_each(|tile| {
            let mut tile_data = PreRenderedTileData::new(tile_size, tile_size);

            let mut rng = rand::thread_rng();
            let mut tile_is_empty = true;

            for h in 0..tile_size {
                for w in 0..tile_size {
                    // let x = tile.x * tile_size + w;
                    // let y = tile.y * tile_size + h;
                    // let xx = x as f32;
                    // let yy = y as f32;

                    // Pathtracer
                    // Based on GLSL_pathtracer (https://github.com/knightcrawler25/GLSL-PathTracer)
                    //

                    let mut ray = region.regionfx.cam_create_ray(
                        vec2f(w as f32 / tile_size_f, 1.0 - (h as f32 / tile_size_f)),
                        //self.position, //vec3f(0.5, 0.5, 0.5),
                        vec3f(tile.x as f32 + 0.5, 0.0, tile.y as f32 + 0.5),
                        vec2f(tile_size_f, tile_size_f),
                        vec2f(rng.gen(), rng.gen()),
                        &render_settings_params,
                    );

                    // BSDF Pathtracer based on glsl_pathtracer
                    // https://github.com/knightcrawler25/GLSL-PathTracer

                    let mut dist = 0.0;

                    let _radiance = Vec3f::zero();
                    let mut throughput = Vec3f::one();

                    let mut state = BSDFState::default();
                    //let mut light_sample = BSDFLightSampleRec::default();
                    let mut scatter_sample = BSDFScatterSampleRec::default();

                    //let mut alpha = 1.0;

                    // For medium tracking
                    let mut _in_medium = false;
                    let mut _medium_sampled = false;
                    let mut _surface_scatter = false;

                    state.depth = 0;

                    // Choose the light we want to sample for this sample
                    // We choose one random light per sample

                    let mut sample_light_grid_pos: Option<Vec2i> = None;
                    let mut direct_light_sample = Vec3f::zero();
                    let mut possible_lights_to_sample = vec![];

                    for (light_grid, light) in &level.lights {
                        let light_dist = length(Vec2f::from(*light_grid - *tile));

                        // Is light in distance ?
                        if light_dist < light.max_distance {
                            possible_lights_to_sample.push(*light_grid);
                        }
                    }

                    if !possible_lights_to_sample.is_empty() {
                        let index = rng.gen_range(0..possible_lights_to_sample.len());
                        sample_light_grid_pos = Some(possible_lights_to_sample[index]);
                    }

                    for depth in 0..8 {
                        if let Some(hit) = self.prerender_pixel(
                            ray,
                            region,
                            &update,
                            settings,
                            max_render_distance,
                            palette,
                            &material_params,
                            &geo_params,
                            &mut rng,
                        ) {
                            if depth == 0 {
                                dist = hit.distance;
                            }

                            state.depth = depth;

                            state.mat.clone_from(&hit.mat);
                            state.mat.roughness = max(state.mat.roughness, 0.001);
                            // Remapping from clearcoat gloss to roughness
                            state.mat.clearcoat_roughness =
                                lerp(0.1, 0.001, state.mat.clearcoat_roughness);

                            state.hit_dist = hit.distance;
                            state.fhp = hit.hit_point;

                            state.normal = hit.normal;
                            state.ffnormal = if dot(state.normal, ray.d) <= 0.0 {
                                state.normal
                            } else {
                                -state.normal
                            };

                            state.eta = if dot(ray.d, state.normal) < 0.0 {
                                1.0 / state.mat.ior
                            } else {
                                state.mat.ior
                            };

                            onb(state.normal, &mut state.tangent, &mut state.bitangent);

                            let aspect = sqrt(1.0 - state.mat.anisotropic * 0.9);
                            state.mat.ax = max(0.001, state.mat.roughness / aspect);
                            state.mat.ay = max(0.001, state.mat.roughness * aspect);

                            _surface_scatter = true;

                            // Direct light sampling

                            if let Some(light_grid) = sample_light_grid_pos {
                                if let Some(light) = level.lights.get(&light_grid) {
                                    let light_dist = length(Vec2f::from(light_grid - *tile));

                                    if light_dist < light.max_distance {
                                        let mut light_sample = BSDFLightSampleRec::default();
                                        let mut scatter_sample = BSDFScatterSampleRec::default();

                                        let scatter_pos = state.fhp + state.normal * hit.eps;

                                        let light_pos = vec3f(
                                            light_grid.x as f32 + 0.5,
                                            0.6,
                                            light_grid.y as f32 + 0.5,
                                        );

                                        let radius = 0.4;

                                        let l = BSDFLight {
                                            position: light_pos,
                                            emission: light.color * light.strength * 3.0,
                                            radius,
                                            type_: 1.0,
                                            u: Vec3f::zero(),
                                            v: Vec3f::zero(),
                                            area: 4.0 * f32::pi() * radius * radius,
                                        };

                                        sample_sphere_light(
                                            &l,
                                            scatter_pos,
                                            &mut light_sample,
                                            1,
                                            &mut rng,
                                            light.max_distance,
                                        );

                                        let li = light_sample.emission;

                                        if self.shadow_ray(
                                            Ray::new(scatter_pos, light_sample.direction),
                                            Vec3i::from(light_pos),
                                            light,
                                            region,
                                            &update,
                                            settings,
                                            &material_params,
                                            &geo_params,
                                        ) {
                                            scatter_sample.f = disney_eval(
                                                &state,
                                                -ray.d,
                                                state.ffnormal,
                                                light_sample.direction,
                                                &mut scatter_sample.pdf,
                                            );

                                            let mut mis_weight = 1.0;
                                            if l.area > 0.0 {
                                                // No MIS for distant light
                                                mis_weight = power_heuristic(
                                                    light_sample.pdf,
                                                    scatter_sample.pdf,
                                                );
                                            }

                                            if scatter_sample.pdf > 0.0 {
                                                direct_light_sample +=
                                                    (mis_weight * li * scatter_sample.f
                                                        / light_sample.pdf)
                                                        * throughput;
                                            }
                                        }
                                    }
                                }
                            }

                            // Next event estimation
                            //radiance += direct_light * throughput;

                            // Sample BSDF for color and outgoing direction
                            scatter_sample.f = disney_sample(
                                &state,
                                -ray.d,
                                state.ffnormal,
                                &mut scatter_sample.l,
                                &mut scatter_sample.pdf,
                                &mut rng,
                            );
                            if scatter_sample.pdf > 0.0 {
                                throughput *= scatter_sample.f / scatter_sample.pdf;
                            } else {
                                break;
                            }

                            ray.d = scatter_sample.l;
                            ray.o = state.fhp + ray.d * hit.eps;
                        } else {
                            if depth == 0 {
                                throughput = Vec3f::zero();
                            } else {
                                tile_is_empty = false;
                            }
                            break;
                        }
                    }

                    let mut lights: Vec<PreRenderedLight> = vec![];

                    // Store the light sample
                    if let Some(light_grid) = sample_light_grid_pos {
                        if !direct_light_sample.x.is_nan() {
                            lights.push(PreRenderedLight {
                                pos: light_grid,
                                brdf: (
                                    half::f16::from_f32(direct_light_sample.x),
                                    half::f16::from_f32(direct_light_sample.y),
                                    half::f16::from_f32(direct_light_sample.z),
                                ),
                            });
                        }
                    }

                    tile_data.albedo.set_pixel_vec3f(w, h, &throughput);
                    tile_data.distance.set((w, h), half::f16::from_f32(dist));
                    tile_data.lights.set((w, h), lights);

                    // -- End
                }
            }

            let mut prerendered = prerendered_mutex.lock().unwrap();

            let mut sample = 0;
            if let Some(sampled) = prerendered.tile_samples.get(&vec2i(tile.x, tile.y)) {
                sample = *sampled as i32;
            }

            if !tile_is_empty {
                sender
                    .send(PreRenderResult::RenderedRegionTile(
                        region.id,
                        *tile,
                        sample as u16,
                        tile_data,
                    ))
                    .unwrap();
                sample += 1;
            } else {
                sample = region.pathtracer_samples;
            }
            prerendered
                .tile_samples
                .insert(vec2i(tile.x, tile.y), sample as u16);
        });

        let _stop = self.get_time();
        //println!("render time {:?}", _stop - _start);

        false
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn prerender_pixel(
        &self,
        ray: Ray,
        region: &Region,
        update: &RegionUpdate,
        settings: &RegionDrawSettings,
        max_render_distance: i32,
        palette: &ThePalette,
        material_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
        geo_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
        _rng: &mut ThreadRng,
    ) -> Option<Hit> {
        let mut hit = Hit::default();

        #[inline(always)]
        fn equal(l: f32, r: Vec3f) -> Vec3f {
            vec3f(
                if l == r.x { 1.0 } else { 0.0 },
                if l == r.y { 1.0 } else { 0.0 },
                if l == r.z { 1.0 } else { 0.0 },
            )
        }

        let mut has_hit = false;
        //let mut has_heightmap_material = false;

        let dist = 0.0;

        if let Some(terrain_dist) = region.heightmap.raymarch(&ray) {
            let mut terrain_hit = ray.at(terrain_dist);
            let tile_id = vec2i(terrain_hit.x.floor() as i32, terrain_hit.z.floor() as i32);
            let key = Vec3i::new(tile_id.x, terrain_hit.y as i32, tile_id.y);

            let terrain_normal =
                region
                    .heightmap
                    .calculate_normal(terrain_hit.x, terrain_hit.z, 0.001);
            hit.normal = terrain_normal;
            hit.hit_point = terrain_hit;

            let mut geo_ids: Vec<Uuid> = vec![];
            {
                let ro = ray.o;
                let rd = ray.d;

                let mut i = floor(ro);
                let mut dist = 0.0;

                let mut normal = vec3f(0.0, 0.0, 0.0);
                let srd = signum(rd);

                let rdi = 1.0 / (2.0 * rd);

                let mut key: Vec3<i32>;

                for _ii in 0..max_render_distance {
                    key = Vec3i::from(i);

                    if key.y < -1 {
                        break;
                    }

                    if dist > hit.distance {
                        break;
                    }

                    // Collect the hit geo ids which we will process later.
                    if let Some(ids) = region.geometry_areas.get(&vec3i(key.x, 0, key.z)) {
                        for id in ids {
                            if !geo_ids.contains(id) {
                                geo_ids.push(*id);
                            }
                        }
                    }

                    // Raymarch (extruded) materials to see if they intersect.
                    if let Some(mask) = region.heightmap.get_material_mask(key.x, key.z) {
                        //has_heightmap_material = true;
                        let mut h = Hit::default(); //hit.clone();

                        let mut t = dist;
                        for _ in 0..20 {
                            // Max distance a ray can travel in a unit cube
                            if t - dist > 1.732 {
                                break;
                            }

                            let mut p = ray.at(t);
                            let t_dist = region.heightmap.interpolate_height(p.x, p.z);
                            p -= t_dist;

                            if let Some(material_mask) = mask.at_f(vec2f(p.x.fract(), p.z.fract()))
                            {
                                let index = (material_mask[0] - 1) as usize;
                                if let Some((_id, material)) = self.materials.get_index(index) {
                                    let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                                    if let Some(m_params) = material_params.get(&material.id) {
                                        mat_obj_params.clone_from(m_params);
                                    }

                                    //if material.has_geometry_trail() {
                                    let dist = material.get_heightmap_distance_3d(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        &mat_obj_params,
                                    );

                                    if dist < h.eps && dist < h.distance {
                                        hit.clone_from(&h);
                                        hit.hit_point = p;
                                        hit.distance = t;

                                        hit.normal = material.heightmap_normal(
                                            &settings.time,
                                            p,
                                            &mut h,
                                            &mat_obj_params,
                                        );

                                        let f = self.get_uv_face(hit.normal, hit.hit_point);
                                        hit.uv = f.0;
                                        hit.global_uv = vec2f(p.x.floor(), p.z.floor()) + hit.uv;

                                        // hit.global_uv = match f.1 {
                                        //     0 => f.0 + vec2f(p.z, p.y),
                                        //     1 => f.0 + vec2f(p.x, p.z),
                                        //     _ => f.0 + vec2f(p.x, p.y),
                                        // };
                                        hit.pattern_pos = hit.global_uv;
                                        h.clone_from(&hit);
                                        material.compute(
                                            &mut hit,
                                            palette,
                                            &self.textures,
                                            &mat_obj_params,
                                        );

                                        // Overlay material
                                        let index = (material_mask[1] - 1) as usize;
                                        if let Some((_id, material)) =
                                            self.materials.get_index(index)
                                        {
                                            hit.value = 1.0;

                                            let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                                            if let Some(m_params) =
                                                material_params.get(&material.id)
                                            {
                                                mat_obj_params.clone_from(m_params);
                                            }

                                            material.compute(
                                                &mut hit,
                                                palette,
                                                &self.textures,
                                                &mat_obj_params,
                                            );
                                        }

                                        has_hit = true;
                                        break;
                                    }

                                    t += dist;
                                    //}
                                }
                            }
                        }
                    }

                    if let Some(tile) = self.tiles.get((key.x, key.y, key.z)) {
                        if dist > hit.distance {
                            continue;
                        }

                        //let mut uv = vec2f(terrain_hit.x.fract(), terrain_hit.z.fract());
                        let mut uv = self.get_uv_face(normal, ray.at(dist)).0;
                        //pixel = [(uv.x * 255.0) as u8, (uv.y * 255.0) as u8, 0, 255];
                        if let Some(data) = self.textures.get(tile) {
                            let index = settings.anim_counter % data.buffer.len();

                            // TODO apply alpha correctly for WallFX blends
                            let mut alpha: f32 = 1.0;

                            //if key.y == 0 {
                            if let Some(wallfx) = update.wallfx.get(&(tile_id.x, tile_id.y)) {
                                let mut valid = true;
                                let mut xx = 0;
                                let mut yy = 0;
                                let d = (update.server_tick - wallfx.at_tick) as f32
                                    + settings.delta_in_tick
                                    - 1.0;
                                if d < 1.0 {
                                    let t = (d * region.grid_size as f32) as i32;
                                    if wallfx.prev_fx != WallFX::Normal {
                                        wallfx.prev_fx.apply(
                                            &mut xx,
                                            &mut yy,
                                            &mut alpha,
                                            &(region.grid_size - t),
                                            &(1.0 - d),
                                        );
                                    } else {
                                        wallfx.fx.apply(&mut xx, &mut yy, &mut alpha, &t, &d);
                                    }
                                } else if wallfx.fx != WallFX::Normal {
                                    valid = false;
                                }

                                if valid {
                                    uv.x += xx as f32 / region.grid_size as f32;
                                    uv.y += yy as f32 / region.grid_size as f32;
                                } else {
                                    uv = vec2f(-1.0, -1.0);
                                }
                            }
                            //}

                            if !data.billboard {
                                if let Some(p) = data.buffer[index].at_f_vec4f(uv) {
                                    hit.mat.base_color = vec3f(p.x, p.y, p.z);
                                    hit.normal = -hit.normal;
                                    hit.distance = dist;
                                    hit.hit_point = ray.at(dist);
                                    has_hit = true;
                                }
                            } else {
                                let xx = i.x + 0.5;
                                let zz = i.z + 0.5;

                                let plane_pos = vec3f(xx, 0.5, zz);

                                let mut plane_normal = normalize(plane_pos - ray.o);
                                plane_normal.y = 0.0;
                                let denom = dot(plane_normal, ray.d);

                                if denom > 0.0001 {
                                    let t = dot(plane_pos - ray.o, plane_normal) / denom;
                                    if t >= 0.0 {
                                        let hit_pos = ray.at(t);
                                        if (xx - hit_pos.x).abs() <= 0.5
                                            && (zz - hit_pos.z).abs() <= 0.5
                                            && hit_pos.y >= 0.0
                                            && hit_pos.y <= 1.0
                                        {
                                            #[inline(always)]
                                            fn compute_primary(normal: Vec3f) -> Vec3f {
                                                let a = cross(normal, vec3f(1.0, 0.0, 0.0));
                                                let b = cross(normal, vec3f(0.0, 1.0, 0.0));

                                                let max_ab =
                                                    if dot(a, a) < dot(b, b) { b } else { a };

                                                let c = cross(normal, vec3f(0.0, 0.0, 1.0));

                                                normalize(if dot(max_ab, max_ab) < dot(c, c) {
                                                    c
                                                } else {
                                                    max_ab
                                                })
                                            }
                                            let index = settings.anim_counter % data.buffer.len();

                                            let plane_vector_u = compute_primary(plane_normal);
                                            let plane_vector_v = cross(plane_vector_u, ray.d);

                                            let relative = hit_pos - plane_pos;
                                            let u_dot = dot(relative, plane_vector_u);
                                            let v_dot = dot(relative, plane_vector_v);

                                            let u = 0.5 + u_dot;
                                            let v = 0.5 + v_dot;

                                            //println!("{}, {}", u, v);

                                            let x =
                                                (u * data.buffer[index].dim().width as f32) as i32;
                                            let y = ((1.0 - v)
                                                * data.buffer[index].dim().height as f32)
                                                as i32;
                                            if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                                                if c[3] == 255 {
                                                    let col = TheColor::from_u8_array(c).to_vec4f();
                                                    hit.mat.base_color = vec3f(col.x, col.y, col.z);
                                                    hit.distance = t;
                                                    hit.normal = -hit.normal;
                                                    hit.hit_point = ray.at(t);
                                                    has_hit = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // if has_hit {
                    //     break;
                    // }

                    let plain = (1.0 + srd - 2.0 * (ro - i)) * rdi;
                    dist = min(plain.x, min(plain.y, plain.z));
                    normal = equal(dist, plain) * srd;
                    i += normal;
                }
            }

            hit.key = Vec3f::from(key);
            for geo_id in geo_ids {
                let mut h = Hit::default();
                if let Some(geo_obj) = region.geometry.get(&geo_id) {
                    if let Some(geo_obj_params) = geo_params.get(&geo_obj.id) {
                        let material = self.materials.get(&geo_obj.material_id);
                        let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                        if let Some(m_params) = material_params.get(&geo_obj.material_id) {
                            mat_obj_params.clone_from(m_params);
                        }

                        let mut t = 0.0;

                        for _ in 0..20 {
                            // Max distance a ray can travel in a unit cube
                            // if t > 1.732 {
                            //     break;
                            // }

                            let mut p = ray.at(t);
                            let t_dist = region.heightmap.interpolate_height(p.x, p.z);
                            p.y -= t_dist;

                            let d; // = (f32::INFINITY, 0);
                            if let Some(material) = material {
                                d = material.get_distance_3d(
                                    &settings.time,
                                    p,
                                    &mut h,
                                    geo_obj,
                                    geo_obj_params,
                                    &mat_obj_params,
                                );
                            } else {
                                d = MaterialFXObject::default().get_distance_3d(
                                    &settings.time,
                                    p,
                                    &mut h,
                                    geo_obj,
                                    geo_obj_params,
                                    &mat_obj_params,
                                );
                            }

                            //d.0 -= t_dist;

                            if d.0 < h.eps && t < hit.distance {
                                h.hit_point = p;

                                hit.clone_from(&h);
                                if let Some(material) = material {
                                    hit.normal = material.normal(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        geo_obj_params,
                                        &mat_obj_params,
                                    );
                                } else {
                                    hit.normal = MaterialFXObject::default().normal(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        geo_obj_params,
                                        &mat_obj_params,
                                    );
                                }

                                hit.distance = dist + t;
                                hit.mat.base_color = vec3f(0.5, 0.5, 0.5);

                                // if h.extrusion == GeoFXNodeExtrusion::None {
                                //     hit.value = 1.0;
                                //     geo_obj.nodes[d.1].distance_3d(
                                //         &settings.time,
                                //         p,
                                //         &mut Some(&mut hit),
                                //         &geo_obj_params[d.1],
                                //     );
                                // }

                                if let Some(material) = material {
                                    let f = self.get_uv_face(hit.normal, hit.hit_point);
                                    hit.uv = f.0;
                                    hit.global_uv = vec2f(p.x.floor(), p.z.floor()) + hit.uv;
                                    //match f.1 {
                                    //0 => f.0 + vec2f(hit.hit_point.z, hit.hit_point.y),
                                    //1 => f.0 + vec2f(hit.hit_point.x, hit.hit_point.z),
                                    //_ => f.0 + vec2f(hit.hit_point.x, hit.hit_point.y),
                                    //};
                                    material.compute(
                                        &mut hit,
                                        palette,
                                        &self.textures,
                                        &mat_obj_params,
                                    );
                                }

                                has_hit = true;
                            }
                            t += d.0;
                        }
                    }
                }
            }

            // If no hit, we draw the material at index #0
            // if !has_hit && has_heightmap_material {
            //     hit.hit_point = terrain_hit;
            //     hit.normal = terrain_normal;
            //     hit.distance = terrain_dist;

            //     if let Some((id, material)) = self.materials.get_index(0) {
            //         let mut mat_obj_params: Vec<Vec<f32>> = vec![];

            //         if let Some(m_params) = material_params.get(id) {
            //             mat_obj_params.clone_from(m_params);
            //         }

            //         let f = self.get_uv_face(hit.normal, hit.hit_point);
            //         hit.uv = f.0;
            //         hit.global_uv = vec2f(terrain_hit.x.floor(), terrain_hit.z.floor()) + hit.uv;
            //         hit.pattern_pos = hit.global_uv;
            //         hit.two_d = true;
            //         material.compute(&mut hit, palette, &self.textures, &mat_obj_params);

            //         has_hit = true;
            //     }
            // }

            // If no hit, we render the heightmap with its painted materials.
            if false {
                // ! has_hit {
                if let Some(mask) = region.heightmap.get_material_mask(tile_id.x, tile_id.y) {
                    let terrain_uv = vec2f(terrain_hit.x.fract(), terrain_hit.z.fract());

                    if let Some(material_mask) = mask.at_f(terrain_uv) {
                        let index = (material_mask[0] - 1) as usize;
                        if let Some((_id, material)) = self.materials.get_index(index) {
                            let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                            if let Some(m_params) = material_params.get(&material.id) {
                                mat_obj_params.clone_from(m_params);
                            }

                            hit.hit_point = terrain_hit;
                            hit.normal = terrain_normal;

                            if material.has_geometry_trail() {
                                let d = material.get_heightmap_distance_3d(
                                    &settings.time,
                                    vec3f(terrain_hit.x, 0.0, terrain_hit.z), //terrain_hit,
                                    &mut hit,
                                    &mat_obj_params,
                                );

                                let t = terrain_dist + d - 20.8;
                                hit.hit_point = ray.at(t);
                                hit.distance = t;
                                terrain_hit = hit.hit_point;

                                hit.normal = material.heightmap_normal(
                                    &settings.time,
                                    ray.at(d),
                                    &mut hit,
                                    &mat_obj_params,
                                );
                            }

                            //if !material.has_geometry_trail() {

                            let f = self.get_uv_face(hit.normal, hit.hit_point);
                            hit.uv = f.0;
                            hit.global_uv = match f.1 {
                                0 => f.0 + vec2f(terrain_hit.z, terrain_hit.y),
                                1 => f.0 + vec2f(terrain_hit.x, terrain_hit.z),
                                _ => f.0 + vec2f(terrain_hit.x, terrain_hit.y),
                            };
                            hit.pattern_pos = hit.global_uv;
                            material.compute(&mut hit, palette, &self.textures, &mat_obj_params);

                            has_hit = true;
                            //}
                            /*else {
                            //let mut h = hit.clone();
                            let mut t = 0.001;
                            let mut h = Hit::default();

                            for _ in 0..40 {
                                let mut p = ray.at(t);
                                p.y -= region.heightmap.interpolate_height(p.x, p.z);

                                let d = material.get_heightmap_distance_3d(
                                    &settings.time,
                                    p,
                                    &mut h,
                                    &region.heightmap,
                                    &mat_obj_params,
                                );

                                if d < h.eps {
                                    hit.clone_from(&h);

                                    hit.hit_point = p;

                                    let mut h = hit.clone();
                                    hit.normal = material.heightmap_normal(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        &region.heightmap,
                                        &mat_obj_params,
                                    );

                                    hit.distance = t;
                                    hit.mat.base_color = vec3f(0.5, 0.5, 0.5);

                                    let f = self.get_uv_face(hit.normal, hit.hit_point);
                                    hit.uv = f.0;
                                    hit.global_uv = match f.1 {
                                        0 => f.0 + vec2f(p.z, p.y),
                                        1 => f.0 + vec2f(p.x, p.z),
                                        _ => f.0 + vec2f(p.x, p.y),
                                    };
                                    material.compute(
                                        &mut hit,
                                        palette,
                                        &self.textures,
                                        &mat_obj_params,
                                    );

                                    has_hit = true;
                                    break;
                                }
                                t += d;
                            }
                            }*/
                        }

                        // Overlay the 2nd material
                        /*
                        if has_hit {
                            let index = (material_mask[1] - 1) as usize;
                            if let Some((_id, material)) = self.materials.get_index(index) {
                                let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                                if let Some(m_params) = material_params.get(&material.id) {
                                    mat_obj_params.clone_from(m_params);
                                }

                                //let mut h = hit.clone();
                                material.compute(
                                    &mut hit,
                                    palette,
                                    &self.textures,
                                    &mat_obj_params,
                                );
                            }
                            }*/
                    }
                }
            }

            //println!("{}", hit.normal);
        }
        // else {
        //     hit.mat.base_color = vec3f(1.0, 1.0, 1.0);
        //     return Some(hit);
        // }

        if has_hit {
            Some(hit)
        } else {
            None
        }
    }

    /// RENDERED

    #[allow(clippy::too_many_arguments)]
    pub fn rendered(
        &mut self,
        buffer: &mut TheRGBABuffer,
        region: &Region,
        update: &mut RegionUpdate,
        settings: &mut RegionDrawSettings,
        compute_delta: bool,
        palette: &ThePalette,
    ) {
        let _start = self.get_time();

        let width = buffer.dim().width as usize;
        let height = buffer.dim().height as usize;

        let pixels = buffer.pixels_mut();

        let width_f = width as f32;
        let height_f = height as f32;

        let region_height = region.height * region.grid_size;

        if compute_delta {
            update.generate_character_pixel_positions(
                region.grid_size as f32,
                &self.textures,
                vec2i(width as i32, height as i32),
                region_height,
                settings,
            );
        }

        // Fill the code level with the blocking info and collect lights
        let mut level = Level::new(region.width, region.height, settings.time);
        region.fill_code_level(&mut level, &self.textures, update);

        // Collect the render settings params
        let render_settings_params: Vec<Vec<f32>> = region.regionfx.load_parameters(&settings.time);

        let mut start_x = 0;
        let mut start_y = 0;

        // Find the location in the game canvas

        let position = if settings.center_on_character.is_some() {
            settings.center_3d + self.position
        } else {
            self.position
        };

        let ray = region.regionfx.cam_create_ray(
            vec2f(0.5, 0.5),
            position,
            vec2f(width_f, height_f),
            vec2f(0.0, 0.0),
            &render_settings_params,
        );

        //let mut screen_pos = Vec2i::zero();

        let plane_normal = vec3f(0.0, 1.0, 0.0);
        let denom = dot(plane_normal, ray.d);

        if denom.abs() > 0.0001 {
            let t = dot(vec3f(0.0, 0.0, 0.0) - ray.o, plane_normal) / denom;
            if t >= 0.0 {
                let p = ray.o + ray.d * t;
                let screen_pos = region.regionfx.cam_world_to_canvas(region, p);
                start_x = screen_pos.x - width as i32 / 2;
                start_y = screen_pos.y - height as i32 / 2;

                self.screen_offset.x = start_x;
                self.screen_offset.y = start_y;
            }
        }

        // Render loop

        pixels
            .par_rchunks_exact_mut(width * 4)
            .enumerate()
            .for_each(|(j, line)| {
                for (i, pixel) in line.chunks_exact_mut(4).enumerate() {
                    let i = j * width + i;

                    let xx = (i % width) as f32;
                    let yy = (i / width) as f32;

                    let canvas_pos = vec2i(start_x + xx as i32, start_y + (height_f - yy) as i32);
                    let pos = region.regionfx.cam_canvas_to_world(region, canvas_pos);

                    let tile_size_f = region.tile_size as f32;
                    let ray = region.regionfx.cam_create_ray(
                        vec2f(0.5, 0.5),
                        pos,
                        vec2f(tile_size_f, tile_size_f),
                        vec2f(0.0, 0.0),
                        &render_settings_params,
                    );

                    // --

                    pixel.copy_from_slice(&self.rendered_pixel(
                        ray,
                        canvas_pos,
                        region,
                        update,
                        settings,
                        palette,
                        &render_settings_params,
                    ));
                }
            });

        let _stop = self.get_time();
        // println!("render time {:?}", _stop - _start);
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn rendered_pixel(
        &self,
        ray: Ray,
        pos: Vec2i,
        region: &Region,
        update: &RegionUpdate,
        settings: &RegionDrawSettings,
        palette: &ThePalette,
        fx_params: &[Vec<f32>],
    ) -> RGBA {
        let mut color = vec3f(0.0, 0.0, 0.0);
        //let hit_props = Hit::default();

        let rd = ray.d;
        let mut dist = 0.0;
        //let normal = Vec3f::zero();
        let mut hit = false;

        // let tile_pos = vec2i(pos.x / region.tile_size, pos.y / region.tile_size);
        // let pixel_pos = vec2i(pos.x % region.tile_size, pos.y % region.tile_size);

        //if let Some(tile_data) = &region.prerendered.tiles.get(&tile_pos) {
        if let Some(albedo) = &self.canvas.canvas.get_pixel(pos.x, pos.y) {
            let abso = TheColor::from_u8_array(*albedo).to_vec3f();

            // color.x = powf(settings.daylight.x * abso.x, 1.0 / 2.2);
            // color.y = powf(settings.daylight.y * abso.y, 1.0 / 2.2);
            // color.z = powf(settings.daylight.z * abso.z, 1.0 / 2.2);
            color.x = settings.daylight.x * abso.x;
            color.y = settings.daylight.y * abso.y;
            color.z = settings.daylight.z * abso.z;

            if let Some(d) = &self.canvas.distance_canvas.get((pos.x, pos.y)) {
                dist = d.to_f32();
            }

            if let Some(lights) = &self.canvas.lights_canvas.get((pos.x, pos.y)) {
                for light in lights.iter() {
                    fn hash_u32(seed: u32) -> u32 {
                        let mut state = seed;
                        state = (state ^ 61) ^ (state >> 16);
                        state = state.wrapping_add(state << 3);
                        state ^= state >> 4;
                        state = state.wrapping_mul(0x27d4eb2d);
                        state ^= state >> 15;
                        state
                    }

                    fn flicker_value(anim_counter: u32, intensity: f32) -> f32 {
                        let hash = hash_u32(anim_counter);
                        let flicker_value = (hash as f32 / u32::MAX as f32).clamp(0.0, 1.0);

                        let flicker_range = intensity * (flicker_value - 0.5) * 2.0;
                        (1.0 + flicker_range).clamp(0.0, 1.0)
                    }

                    let brdf = vec3f(
                        light.brdf.0.to_f32(),
                        light.brdf.1.to_f32(),
                        light.brdf.2.to_f32(),
                    );

                    let l = brdf * flicker_value(settings.anim_counter as u32, 0.2);
                    color += l;
                }
            }

            hit = true;
        }

        // Test against characters
        for (pos, tile_id, _character_id, _facing) in &update.characters_pixel_pos {
            let xx = pos.x as f32 / region.grid_size as f32 + 0.5;
            let zz = pos.y as f32 / region.grid_size as f32 + 0.5;

            let plane_pos = vec3f(xx, 0.5, zz);

            let mut plane_normal = normalize(plane_pos - ray.o);
            plane_normal.y = 0.0;
            let denom = dot(plane_normal, ray.d);

            if denom > 0.0001 {
                let t = dot(plane_pos - ray.o, plane_normal) / denom;
                if t >= 0.0 && !hit || (hit && t < dist) {
                    let hit_point = ray.at(t);
                    if (xx - hit_point.x).abs() <= 0.5
                        && (zz - hit_point.z).abs() <= 0.5
                        && hit_point.y >= 0.0
                        && hit_point.y <= 1.0
                    {
                        if let Some(data) = self.textures.get(tile_id) {
                            #[inline(always)]
                            fn compute_primary(normal: Vec3f) -> Vec3f {
                                let a = cross(normal, vec3f(1.0, 0.0, 0.0));
                                let b = cross(normal, vec3f(0.0, 1.0, 0.0));

                                let max_ab = if dot(a, a) < dot(b, b) { b } else { a };

                                let c = cross(normal, vec3f(0.0, 0.0, 1.0));

                                normalize(if dot(max_ab, max_ab) < dot(c, c) {
                                    c
                                } else {
                                    max_ab
                                })
                            }
                            let index = settings.anim_counter % data.buffer.len();

                            // let plane_vector_u = normalize(cross(rd, vec3f(1.0, 0.0, 0.0)));
                            // let plane_vector_v = normalize(cross(rd, vec3f(0.0, 1.0, 0.0)));

                            let plane_vector_u = compute_primary(plane_normal);
                            let plane_vector_v = cross(plane_vector_u, rd);

                            let relative = hit_point - plane_pos;
                            let u_dot = dot(relative, plane_vector_u);
                            let v_dot = dot(relative, plane_vector_v);

                            let u = 0.5 + u_dot;
                            let v = 0.5 + v_dot;

                            let x = (u * data.buffer[index].dim().width as f32) as i32;
                            let y = ((1.0 - v) * data.buffer[index].dim().height as f32) as i32;
                            if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                                if c[3] == 255 {
                                    let col = TheColor::from_u8_array(c).to_vec3f();
                                    color = col * settings.daylight;
                                    dist = t;
                                    hit = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        region
            .regionfx
            .fx_3d(region, palette, pos, &mut color, fx_params);

        // Show hover
        if let Some(hover) = self.hover_pos {
            let plane_normal = vec3f(0.0, 1.0, 0.0);
            let denom = dot(plane_normal, ray.d);

            if denom.abs() > 0.0001 {
                let t = dot(vec3f(0.0, 0.0, 0.0) - ray.o, plane_normal) / denom;
                if t >= 0.0 {
                    let hp = Vec3i::from(ray.at(t));
                    if hp == hover {
                        color = TheColor::from_vec3f(color)
                            .mix(&TheColor::white(), 0.5)
                            .to_vec3f();
                    }
                }
            }
        }

        TheColor::from_vec3f(color).to_u8_array()
    }

    /// RENDER

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        buffer: &mut TheRGBABuffer,
        region: &Region,
        update: &mut RegionUpdate,
        settings: &mut RegionDrawSettings,
        compute_delta: bool,
        palette: &ThePalette,
    ) {
        let _start = self.get_time();

        let width = buffer.dim().width as usize;
        let height = buffer.dim().height as usize;

        //let stride = buffer.stride();
        let pixels = buffer.pixels_mut();
        //let height = dim.height;

        let width_f = width as f32;
        let height_f = height as f32;

        let region_height = region.height * region.grid_size;

        let grid_size = region.grid_size as f32;

        if compute_delta {
            update.generate_character_pixel_positions(
                grid_size,
                &self.textures,
                vec2i(width as i32, height as i32),
                region_height,
                settings,
            );
        }

        let max_render_distance = 20;

        // Fill the code level with the blocking info and collect lights
        let mut level = Level::new(region.width, region.height, settings.time);
        region.fill_code_level(&mut level, &self.textures, update);

        // Collect the material params
        let mut material_params: FxHashMap<Uuid, Vec<Vec<f32>>> = FxHashMap::default();
        for (id, material) in &self.materials {
            let params = material.load_parameters(&settings.time);
            material_params.insert(*id, params);
        }

        // Collect the render settings params
        let render_settings_params: Vec<Vec<f32>> = region.regionfx.load_parameters(&settings.time);

        // Collect the geo_object params
        let mut geo_params: FxHashMap<Uuid, Vec<Vec<f32>>> = FxHashMap::default();
        for (id, geo_obj) in &region.geometry {
            let params = geo_obj.load_parameters(&settings.time);
            geo_params.insert(*id, params);
        }

        pixels
            .par_rchunks_exact_mut(width * 4)
            .enumerate()
            .for_each(|(j, line)| {
                for (i, pixel) in line.chunks_exact_mut(4).enumerate() {
                    let i = j * width + i;

                    let xx = (i % width) as f32;
                    let yy = (i / width) as f32;

                    let position = if settings.center_on_character.is_some() {
                        settings.center_3d + self.position
                    } else {
                        self.position
                    };

                    let ray = region.regionfx.cam_create_ray(
                        vec2f(xx / width_f, yy / height_f),
                        position,
                        vec2f(width_f, height_f),
                        vec2f(0.0, 0.0),
                        &render_settings_params,
                    );

                    pixel.copy_from_slice(&self.render_pixel(
                        ray,
                        region,
                        update,
                        settings,
                        &level,
                        max_render_distance,
                        palette,
                        &material_params,
                        &geo_params,
                    ));
                }
            });

        let _stop = self.get_time();
        println!("render time {:?}", _stop - _start);
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn render_pixel(
        &self,
        ray: Ray,
        region: &Region,
        update: &RegionUpdate,
        settings: &RegionDrawSettings,
        level: &Level,
        max_render_distance: i32,
        palette: &ThePalette,
        material_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
        geo_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
    ) -> RGBA {
        let mut hit = Hit::default();

        let mut color = vec4f(0.0, 0.0, 0.0, 1.0);
        let hit_props = Hit::default();

        #[inline(always)]
        fn equal(l: f32, r: Vec3f) -> Vec3f {
            vec3f(
                if l == r.x { 1.0 } else { 0.0 },
                if l == r.y { 1.0 } else { 0.0 },
                if l == r.z { 1.0 } else { 0.0 },
            )
        }

        let ro = ray.o;
        let rd = ray.d;

        let mut i = floor(ro);
        let mut dist = 0.0;

        let mut normal = Vec3f::zero();
        let srd = signum(rd);

        let rdi = 1.0 / (2.0 * rd);

        let mut key: Vec3<i32>;
        let mut has_hit = false;

        for _ii in 0..max_render_distance {
            key = Vec3i::from(i);

            if key.y < -1 {
                break;
            }

            // First person is limited to 1 y
            if
            /*camera_type == CameraType::FirstPerson &&*/
            key.y > 1 {
                break;
            }

            if let Some(geo_ids) = region.geometry_areas.get(&key) {
                hit.key = Vec3f::from(key);
                for geo_id in geo_ids {
                    let mut h = Hit::default();
                    if let Some(geo_obj) = region.geometry.get(geo_id) {
                        let geo_obj_params = geo_obj.load_parameters(&settings.time);
                        let material = self.materials.get(&geo_obj.material_id);
                        let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                        if let Some(m_params) = material_params.get(&geo_obj.material_id) {
                            mat_obj_params.clone_from(m_params);
                        }

                        let lro = ray.at(dist);

                        let r = Ray::new(lro, ray.d);
                        let mut t = 0.01;

                        for _ in 0..20 {
                            // Max distance a ray can travel in a unit cube
                            if t > 1.732 {
                                break;
                            }

                            let p = r.at(t);

                            let d; // = (f32::INFINITY, 0);
                            if let Some(material) = material {
                                d = material.get_distance_3d(
                                    &settings.time,
                                    p,
                                    &mut h,
                                    geo_obj,
                                    &geo_obj_params,
                                    &mat_obj_params,
                                );
                            } else {
                                d = MaterialFXObject::default().get_distance_3d(
                                    &settings.time,
                                    p,
                                    &mut h,
                                    geo_obj,
                                    &geo_obj_params,
                                    &mat_obj_params,
                                );
                            }

                            if d.0.abs() < h.eps && dist + t < hit.distance {
                                hit.clone_from(&h);
                                hit.hit_point = p;

                                if let Some(material) = material {
                                    hit.normal = material.normal(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        &geo_obj_params,
                                        &mat_obj_params,
                                    );
                                } else {
                                    hit.normal = MaterialFXObject::default().normal(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        &geo_obj_params,
                                        &mat_obj_params,
                                    );
                                }

                                hit.distance = dist + t;
                                hit.mat.base_color = vec3f(0.5, 0.5, 0.5);

                                // if h.extrusion == GeoFXNodeExtrusion::None {
                                //     hit.value = 1.0;
                                //     geo_obj.nodes[d.1].distance_3d(
                                //         &settings.time,
                                //         p,
                                //         &mut Some(&mut hit),
                                //         &geo_obj_params[d.1],
                                //     );
                                // }

                                if let Some(material) = material {
                                    let f = self.get_uv_face(hit.normal, hit.hit_point);
                                    hit.uv = f.0;
                                    hit.global_uv = match f.1 {
                                        0 => f.0 + vec2f(i.z, i.y),
                                        1 => f.0 + vec2f(i.x, i.z),
                                        _ => f.0 + vec2f(i.x, i.y),
                                    };
                                    material.compute(
                                        &mut hit,
                                        palette,
                                        &self.textures,
                                        &mat_obj_params,
                                    );
                                }

                                color = vec4f(
                                    hit.mat.base_color.x,
                                    hit.mat.base_color.y,
                                    hit.mat.base_color.y,
                                    1.0,
                                );

                                dist = hit.distance;
                                has_hit = true;
                            }
                            t += d.0;
                        }
                    }
                }
            }
            // Test against world tiles
            if let Some(tile) = self.tiles.get((key.x, key.y, key.z)) {
                let mut uv = self.get_uv_face(normal, ray.at(dist)).0;
                //pixel = [(uv.x * 255.0) as u8, (uv.y * 255.0) as u8, 0, 255];
                if let Some(data) = self.textures.get(tile) {
                    let index = settings.anim_counter % data.buffer.len();

                    // TODO apply alpha correctly for WallFX blends
                    let mut alpha: f32 = 1.0;

                    if key.y == 0 {
                        if let Some(wallfx) = update.wallfx.get(&(key.x, key.z)) {
                            let mut valid = true;
                            let mut xx = 0;
                            let mut yy = 0;
                            let d = (update.server_tick - wallfx.at_tick) as f32
                                + settings.delta_in_tick
                                - 1.0;
                            if d < 1.0 {
                                let t = (d * region.grid_size as f32) as i32;
                                if wallfx.prev_fx != WallFX::Normal {
                                    wallfx.prev_fx.apply(
                                        &mut xx,
                                        &mut yy,
                                        &mut alpha,
                                        &(region.grid_size - t),
                                        &(1.0 - d),
                                    );
                                } else {
                                    wallfx.fx.apply(&mut xx, &mut yy, &mut alpha, &t, &d);
                                }
                            } else if wallfx.fx != WallFX::Normal {
                                valid = false;
                            }

                            if valid {
                                uv.x += xx as f32 / region.grid_size as f32;
                                uv.y += yy as f32 / region.grid_size as f32;
                            } else {
                                uv = vec2f(-1.0, -1.0);
                            }
                        }
                    }

                    if !data.billboard {
                        if let Some(p) = data.buffer[index].at_f_vec4f(uv) {
                            //if p[3] == 255 {
                            color = p;
                            has_hit = true;
                            break;
                            //}
                        }
                    } else {
                        let xx = i.x + 0.5;
                        let zz = i.z + 0.5;

                        let plane_pos = vec3f(xx, 0.5, zz);

                        let mut plane_normal = normalize(plane_pos - ray.o);
                        plane_normal.y = 0.0;
                        let denom = dot(plane_normal, ray.d);

                        if denom > 0.0001 {
                            let t = dot(plane_pos - ray.o, plane_normal) / denom;
                            if t >= 0.0 && !has_hit || (has_hit && t < dist) {
                                let hit_pos = ray.at(t);
                                if (xx - hit_pos.x).abs() <= 0.5
                                    && (zz - hit_pos.z).abs() <= 0.5
                                    && hit_pos.y >= 0.0
                                    && hit_pos.y <= 1.0
                                {
                                    #[inline(always)]
                                    fn compute_primary(normal: Vec3f) -> Vec3f {
                                        let a = cross(normal, vec3f(1.0, 0.0, 0.0));
                                        let b = cross(normal, vec3f(0.0, 1.0, 0.0));

                                        let max_ab = if dot(a, a) < dot(b, b) { b } else { a };

                                        let c = cross(normal, vec3f(0.0, 0.0, 1.0));

                                        normalize(if dot(max_ab, max_ab) < dot(c, c) {
                                            c
                                        } else {
                                            max_ab
                                        })
                                    }
                                    let index = settings.anim_counter % data.buffer.len();

                                    let plane_vector_u = compute_primary(plane_normal);
                                    let plane_vector_v = cross(plane_vector_u, rd);

                                    let relative = hit_pos - plane_pos;
                                    let u_dot = dot(relative, plane_vector_u);
                                    let v_dot = dot(relative, plane_vector_v);

                                    let u = 0.5 + u_dot;
                                    let v = 0.5 + v_dot;

                                    //println!("{}, {}", u, v);

                                    let x = (u * data.buffer[index].dim().width as f32) as i32;
                                    let y =
                                        ((1.0 - v) * data.buffer[index].dim().height as f32) as i32;
                                    if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                                        if c[3] == 255 {
                                            let col = TheColor::from_u8_array(c).to_vec4f();
                                            color = col;
                                            dist = t;
                                            //normal = vec3f(0.0, 0.0, 1.0);
                                            has_hit = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let plain = (1.0 + srd - 2.0 * (ro - i)) * rdi;
            dist = min(plain.x, min(plain.y, plain.z));
            normal = equal(dist, plain) * srd;
            i += normal;
        }

        // Test against characters
        for (pos, tile_id, character_id, _facing) in &update.characters_pixel_pos {
            if
            /*camera_type == CameraType::FirstPerson
            &&*/
            Some(*character_id) == settings.center_on_character {
                // Skip the character itself in first person mode.
                continue;
            }

            let xx = pos.x as f32 / region.grid_size as f32 + 0.5;
            let zz = pos.y as f32 / region.grid_size as f32 + 0.5;

            let plane_pos = vec3f(xx, 0.5, zz);

            let mut plane_normal = normalize(plane_pos - ray.o);
            plane_normal.y = 0.0;
            let denom = dot(plane_normal, ray.d);

            if denom > 0.0001 {
                let t = dot(plane_pos - ray.o, plane_normal) / denom;
                if t >= 0.0 && !has_hit || (has_hit && t < dist) {
                    let hit_point = ray.at(t);
                    if (xx - hit_point.x).abs() <= 0.5
                        && (zz - hit_point.z).abs() <= 0.5
                        && hit_point.y >= 0.0
                        && hit_point.y <= 1.0
                    {
                        if let Some(data) = self.textures.get(tile_id) {
                            #[inline(always)]
                            fn compute_primary(normal: Vec3f) -> Vec3f {
                                let a = cross(normal, vec3f(1.0, 0.0, 0.0));
                                let b = cross(normal, vec3f(0.0, 1.0, 0.0));

                                let max_ab = if dot(a, a) < dot(b, b) { b } else { a };

                                let c = cross(normal, vec3f(0.0, 0.0, 1.0));

                                normalize(if dot(max_ab, max_ab) < dot(c, c) {
                                    c
                                } else {
                                    max_ab
                                })
                            }
                            let index = settings.anim_counter % data.buffer.len();

                            // let plane_vector_u = normalize(cross(rd, vec3f(1.0, 0.0, 0.0)));
                            // let plane_vector_v = normalize(cross(rd, vec3f(0.0, 1.0, 0.0)));

                            let plane_vector_u = compute_primary(plane_normal);
                            let plane_vector_v = cross(plane_vector_u, rd);

                            let relative = hit_point - plane_pos;
                            let u_dot = dot(relative, plane_vector_u);
                            let v_dot = dot(relative, plane_vector_v);

                            let u = 0.5 + u_dot;
                            let v = 0.5 + v_dot;

                            //println!("{}, {}", u, v);

                            let x = (u * data.buffer[index].dim().width as f32) as i32;
                            let y = ((1.0 - v) * data.buffer[index].dim().height as f32) as i32;
                            if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                                if c[3] == 255 {
                                    let col = TheColor::from_u8_array(c).to_vec4f();
                                    color = col;
                                    dist = t;
                                    //normal = vec3f(0.0, 0.0, 1.0);
                                    has_hit = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Light Sampling
        //
        if has_hit {
            let daylight = vec4f(
                settings.daylight.x,
                settings.daylight.y,
                settings.daylight.z,
                1.0,
            );

            if level.lights.is_empty() {
                color *= daylight;
            } else {
                // Sample the lights
                let mut total_light = Vec3f::new(0.0, 0.0, 0.0);
                for (light_grid, light) in &level.lights {
                    let light_pos =
                        vec3f(light_grid.x as f32 + 0.5, 0.8, light_grid.y as f32 + 0.5);
                    let mut light_strength = light.strength;

                    if light.color_type == 1 {
                        light_strength = daylight.x;
                    }

                    let mut ro = ray.at(dist);

                    if light.limiter == 1 && ro.y > light_pos.y {
                        continue;
                    }
                    if light.limiter == 2 && ro.x < light_pos.x {
                        continue;
                    }
                    if light.limiter == 3 && ro.y < light_pos.y {
                        continue;
                    }
                    if light.limiter == 4 && ro.x > light_pos.x {
                        continue;
                    }

                    let light_dir = light_pos - ro;
                    let light_dist = length(light_dir);
                    if light_dist < light.max_distance {
                        ro += light_dir * 0.001;

                        let light_ray = Ray::new(ro, light_dir);

                        /*
                        let intensity = 1.0 - (light_dist / light.max_distance).clamp(0.0, 1.0);
                        //intensity *= if s == 0 { 2.0 } else { 1.0 };
                        let mut light_color =
                            Vec3f::from(intensity * light_strength / light.samples as f32);
                        if light.color_type == 0 {
                            light_color *= light.color
                        }
                        total_light += light_color;
                        */

                        if self.shadow_ray(
                            light_ray,
                            Vec3i::from(light_pos),
                            light,
                            region,
                            update,
                            settings,
                            material_params,
                            geo_params,
                        ) {
                            let c = if settings.pbr {
                                let mut light_color = Vec3f::from(1.5 * light_strength);
                                if light.color_type == 0 {
                                    light_color *= light.color
                                }
                                let roughness = hit_props.mat.roughness;
                                let metallic = hit_props.mat.metallic;
                                let reflectance = 0.0; //hit_props.reflectance;
                                let base_color = vec3f(color.x, color.y, color.z);

                                let f0 = 0.16 * reflectance * reflectance * (1.0 - metallic)
                                    + base_color * metallic;

                                self.compute_pbr_lighting(
                                    light_pos,
                                    light_color,
                                    ro,
                                    abs(normal),
                                    -rd,
                                    base_color,
                                    roughness,
                                    f0,
                                )
                            } else {
                                let mut light_color = Vec3f::from(light_strength);
                                if light.color_type == 0 {
                                    light_color *= light.color
                                }
                                let intensity =
                                    1.0 - (light_dist / light.max_distance).clamp(0.0, 1.0);

                                light_color * intensity
                            };

                            total_light += c;
                        }
                    }
                }

                // color = color * daylight + vec4f(total_light.x, total_light.y, total_light.z, 1.0);

                let min_color = vec4f(
                    color.x * daylight.x,
                    color.y * daylight.y,
                    color.z * daylight.z,
                    1.0,
                );

                color = clamp(
                    color * daylight
                        + color * vec4f(total_light.x, total_light.y, total_light.z, 1.0),
                    min_color,
                    color,
                );
            }
        }

        // if let Some(saturation) = saturation {
        //     let mut hsl = TheColor::from_vec4f(color).as_hsl();
        //     hsl.y *= saturation;
        //     color = TheColor::from_hsl(hsl.x * 360.0, hsl.y.clamp(0.0, 1.0), hsl.z).to_vec4f();
        // }

        // Show hover
        if let Some(hover) = self.hover_pos {
            let plane_normal = vec3f(0.0, 1.0, 0.0);
            let denom = dot(plane_normal, ray.d);

            if denom.abs() > 0.0001 {
                let t = dot(vec3f(0.0, 0.0, 0.0) - ray.o, plane_normal) / denom;
                if t >= 0.0 {
                    let hp = Vec3i::from(ray.at(t));
                    if hp == hover {
                        color = TheColor::from_vec4f(color)
                            .mix(&TheColor::white(), 0.5)
                            .to_vec4f();
                    }
                }
            }
        }

        TheColor::from_vec4f(color).to_u8_array()
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn shadow_ray(
        &self,
        ray: Ray,
        light_pos: Vec3i,
        light: &Light,
        region: &Region,
        _update: &RegionUpdate,
        settings: &RegionDrawSettings,
        material_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
        geo_params: &FxHashMap<Uuid, Vec<Vec<f32>>>,
    ) -> bool {
        #[inline(always)]
        fn equal(l: f32, r: Vec3f) -> Vec3f {
            vec3f(
                if l == r.x { 1.0 } else { 0.0 },
                if l == r.y { 1.0 } else { 0.0 },
                if l == r.z { 1.0 } else { 0.0 },
            )
        }

        let ro = ray.o;
        let rd = ray.d;

        let mut i = floor(ro);

        if Vec3i::from(i) == light_pos {
            return true;
        }

        let mut dist = 0.0;

        let mut normal;
        let srd = signum(rd);

        let rdi = 1.0 / (2.0 * rd);

        let mut key: Vec3<i32>;

        for _ in 0..light.max_distance as i32 {
            key = Vec3i::from(i);

            if key == light_pos {
                return true;
            }

            if key.y < -1 {
                return false;
            }

            if key.y > 1 {
                return false;
            }

            // Test against world tiles
            if let Some(tile) = self.tiles.get((key.x, key.y, key.z)) {
                if let Some(data) = self.textures.get(tile) {
                    if data.blocking {
                        return false;
                    }
                }
            }

            // Test against geometry
            if let Some(geo_ids) = region.geometry_areas.get(&key) {
                for geo_id in geo_ids {
                    let mut h = Hit::default();
                    if let Some(geo_obj) = region.geometry.get(geo_id) {
                        if let Some(geo_obj_params) = geo_params.get(&geo_obj.id) {
                            let material = self.materials.get(&geo_obj.material_id);
                            let mut mat_obj_params: Vec<Vec<f32>> = vec![];

                            if let Some(m_params) = material_params.get(&geo_obj.material_id) {
                                mat_obj_params.clone_from(m_params);
                            }

                            let lro = ray.at(dist);

                            let r = Ray::new(lro, ray.d);
                            let mut t = 0.00;

                            for _ in 0..20 {
                                // Max distance a ray can travel in a unit cube
                                if t > 1.732 {
                                    break;
                                }

                                let p = r.at(t);

                                let d; // = (f32::INFINITY, 0);
                                if let Some(material) = material {
                                    d = material.get_distance_3d(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        geo_obj_params,
                                        &mat_obj_params,
                                    );
                                } else {
                                    d = MaterialFXObject::default().get_distance_3d(
                                        &settings.time,
                                        p,
                                        &mut h,
                                        geo_obj,
                                        geo_obj_params,
                                        &mat_obj_params,
                                    );
                                }

                                if d.0.abs() < 0.15 {
                                    return false;
                                }
                                t += d.0;
                            }
                        }
                    }
                }
            }

            let plain = (1.0 + srd - 2.0 * (ro - i)) * rdi;
            dist = min(plain.x, min(plain.y, plain.z));
            normal = equal(dist, plain) * srd;
            i += normal;
        }

        false
    }

    #[inline(always)]
    pub fn get_uv_face(&self, normal: Vec3f, hp: Vec3f) -> (Vec2f, usize) {
        // Calculate the absolute values of the normal components
        let abs_normal = abs(normal);

        // Determine which face of the cube was hit based on the maximum component of the normal
        let face_index = if abs_normal.x > abs_normal.y {
            if abs_normal.x > abs_normal.z {
                0 // X-axis face
            } else {
                2 // Z-axis face
            }
        } else if abs_normal.y > abs_normal.z {
            1 // Y-axis face
        } else {
            2 // Z-axis face
        };

        // Calculate UV coordinates based on the face
        match face_index {
            0 => (Vec2f::new(frac(hp.z), 1.0 - frac(hp.y)), 0), // X-axis face
            1 => (Vec2f::new(frac(hp.x), frac(hp.z)), 1),       // Y-axis face
            2 => (Vec2f::new(frac(hp.x), 1.0 - frac(hp.y)), 2), // Z-axis face
            _ => (Vec2f::zero(), 0),
        }
    }

    pub fn set_region(&mut self, region: &Region) {
        self.tiles.clear();
        for (pos, tile) in &region.tiles {
            for i in 0..tile.layers.len() {
                if i == 0 {
                    if let Some(tile_uuid) = tile.layers[i] {
                        self.tiles.set((pos.0, -1, pos.1), tile_uuid);
                    }
                } else if i == 1 {
                    if let Some(tile_uuid) = tile.layers[i] {
                        self.tiles.set((pos.0, 0, pos.1), tile_uuid);
                    }
                } else if i == 2 {
                    if let Some(tile_uuid) = tile.layers[i] {
                        self.tiles.set((pos.0, 1, pos.1), tile_uuid);
                    }
                }
            }
        }
    }

    pub fn set_textures(&mut self, tiles: FxHashMap<Uuid, TheRGBATile>) {
        self.textures = tiles;
    }

    pub fn set_position(&mut self, position: Vec3f) {
        self.position = position;
    }

    /// The current camera renders the prerendered tiles into the game canvas.
    pub fn render_canvas(&mut self, region: &Region) {
        region.regionfx.cam_render_canvas(region, &mut self.canvas);
    }

    /*
    /// Create the camera setup.
    pub fn create_camera_setup(
        &mut self,
        region: &Region,
        settings: &mut RegionDrawSettings,
    ) -> (Vec3f, Vec3f, f32, CameraMode, CameraType, f32) {
        let mut position = self.position;
        let mut facing = vec3f(0.0, 0.0, -1.0);
        if settings.center_on_character.is_some() {
            position = settings.center_3d + self.position;
            facing = settings.facing_3d;
        }

        // Get the camera settings

        let camera_type = region.camera_type;
        let mut first_person_height = 0.5;
        let mut top_down_height = 7.0;
        let mut top_down_x_offset = -1.0;
        let mut top_down_z_offset = 1.0;
        let mut first_person_fov = 70.0;
        let top_down_fov = 75.0;
        let tilted_iso_height = 4.0;
        let tilted_iso_fov = 75.0;

        if let Some(v) = region.regionfx.get(
            str!("Camera"),
            str!("First Person FoV"),
            &settings.time,
            TheInterpolation::Linear,
        ) {
            if let Some(value) = v.to_f32() {
                first_person_fov = value;
            }
        }

        if let Some(v) = region.regionfx.get(
            str!("Camera"),
            str!("First Person Height"),
            &settings.time,
            TheInterpolation::Linear,
        ) {
            if let Some(value) = v.to_f32() {
                first_person_height = value;
            }
        }

        if let Some(v) = region.regionfx.get(
            str!("Camera"),
            str!("Top Down Height"),
            &settings.time,
            TheInterpolation::Linear,
        ) {
            if let Some(value) = v.to_f32() {
                top_down_height = value;
            }
        }

        if let Some(v) = region.regionfx.get(
            str!("Camera"),
            str!("Top Down X Offset"),
            &settings.time,
            TheInterpolation::Linear,
        ) {
            if let Some(value) = v.to_f32() {
                top_down_x_offset = value;
            }
        }

        if let Some(v) = region.regionfx.get(
            str!("Camera"),
            str!("Top Down Z Offset"),
            &settings.time,
            TheInterpolation::Linear,
        ) {
            if let Some(value) = v.to_f32() {
                top_down_z_offset = value;
            }
        }

        // Camera

        let mut ro = vec3f(position.x + 0.5, 0.5, position.z + 0.5);
        let rd;
        let fov;
        let mut camera_mode = CameraMode::Pinhole;

        let scale_factor;

        if camera_type == CameraType::TopDown {
            rd = ro;
            ro.y = top_down_height;
            ro.x += top_down_x_offset;
            ro.z += top_down_z_offset;
            fov = top_down_fov;
            camera_mode = CameraMode::Orthogonal;
            scale_factor = top_down_height / 1.5;
        } else if camera_type == CameraType::FirstPerson {
            // First person
            ro.y = first_person_height;
            rd = ro + facing * 2.0;
            fov = first_person_fov;
            scale_factor = 1.0;
        } else {
            // Tilted iso
            rd = ro;
            ro.y = tilted_iso_height;
            ro.z += 1.0;
            fov = tilted_iso_fov;
            camera_mode = CameraMode::Orthogonal;
            scale_factor = tilted_iso_height;
        }

        fn transform_zoom(z: f32, power: f32) -> f32 {
            1.0 + (z - 1.0).powf(power) / 10.0
        }

        (
            ro,
            rd,
            fov / (transform_zoom(region.zoom, 2.0)),
            camera_mode,
            camera_type,
            scale_factor,
        )
        }*/

    /// Gets the current time in milliseconds
    fn get_time(&self) -> u128 {
        let time;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let t = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards");
            time = t.as_millis();
        }
        #[cfg(target_arch = "wasm32")]
        {
            time = web_sys::window().unwrap().performance().unwrap().now() as u128;
        }
        time
    }

    /// Returns the terrain hit position at the given screen coordinate (if any).
    pub fn get_hit_position_at(
        &mut self,
        screen_coord: Vec2i,
        region: &Region,
        _settings: &mut RegionDrawSettings,
        _width: usize,
        _height: usize,
    ) -> Option<(Vec3i, Vec3f)> {
        let screen = vec2i(
            self.screen_offset.x + screen_coord.x,
            self.screen_offset.y + screen_coord.y,
        );
        let p = region.regionfx.cam_canvas_to_world(region, screen);
        Some((
            Vec3i::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32),
            p,
        ))
    }

    fn g1v(&self, dot_nv: f32, k: f32) -> f32 {
        1.0 / (dot_nv * (1.0 - k) + k)
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_pbr_lighting(
        &self,
        light_pos: Vec3f,
        light_color: Vec3f,
        position: Vec3f,
        n: Vec3f,
        v: Vec3f,
        albedo: Vec3f,
        roughness: f32,
        f0: Vec3f,
    ) -> Vec3f {
        let alpha = roughness * roughness;
        let l = normalize(light_pos - position);
        let h = normalize(v + l);

        let dot_nl = clamp(dot(n, l), 0.0, 1.0);
        let dot_nv = clamp(dot(n, v), 0.0, 1.0);
        let dot_nh = clamp(dot(n, h), 0.0, 1.0);
        let dot_lh = clamp(dot(l, h), 0.0, 1.0);

        let alpha_sqr = alpha * alpha;
        let pi = std::f32::consts::PI;
        let denom = dot_nh * dot_nh * (alpha_sqr - 1.0) + 1.0;
        let d = alpha_sqr / (pi * denom * denom);

        let dot_lh_5 = (1.0_f32 - dot_lh).powf(5.0);
        let f = f0 + (1.0 - f0) * dot_lh_5;

        let k = alpha / 2.0;
        let vis = self.g1v(dot_nl, k) * self.g1v(dot_nv, k);

        let specular = d * f * vis;

        let inv_pi = std::f32::consts::FRAC_1_PI;
        let diffuse = albedo * inv_pi;

        (diffuse + specular) * light_color * dot_nl
    }
}

/*
// Alternative to billboarding: Auto face to North / South or East / West
if facing.y.abs() > facing.x.abs() {
    // Character is north / south aligned

    let plane_normal = vec3f(0.0, 0.0, 1.0);
    let denom = dot(plane_normal, ray.d);

    let xx = pos.x as f32 / region.grid_size as f32 + 0.5;
    let zz = pos.y as f32 / region.grid_size as f32 + 0.5;

    if denom.abs() > 0.0001 {
        let t = dot(vec3f(xx, 0.0, zz) - ray.o, plane_normal) / denom;
        if t >= 0.0 && !hit || (hit && t < dist) {
            let hit = ray.at(t);
            if (xx - hit.x).abs() <= 0.5
                && (zz - hit.z).abs() <= 0.5
                && hit.y >= 0.0
                && hit.y <= 1.0
            {
                if let Some(data) = self.textures.get(tile_id) {
                    //color.x = 1.0;
                    let index = settings.anim_counter % data.buffer.len();
                    let x = ((hit.x - xx + 0.5) * data.buffer[index].dim().width as f32)
                        as i32;
                    let y =
                        ((1.0 - hit.y) * data.buffer[index].dim().height as f32) as i32;
                    if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                        if c[3] == 255 {
                            let col = TheColor::from_u8_array(c).to_vec4f();
                            color = col;
                            dist = t;
                            normal = vec3f(0.0, 0.0, 1.0);
                        }
                    }
                }
            }
        }
    }
} else {
    // Character is east / west aligned

    let plane_normal = vec3f(1.0, 0.0, 0.0);
    let denom = dot(plane_normal, ray.d);

    let xx = pos.x as f32 / region.grid_size as f32 + 0.5;
    let zz = pos.y as f32 / region.grid_size as f32 + 0.5;

    if denom.abs() > 0.0001 {
        let t = dot(vec3f(xx, 0.0, zz) - ray.o, plane_normal) / denom;
        if t >= 0.0 && !hit || (hit && t < dist) {
            let hit = ray.at(t);
            if (xx - hit.x).abs() <= 0.5
                && (zz - hit.z).abs() <= 0.5
                && hit.y >= 0.0
                && hit.y <= 1.0
            {
                if let Some(data) = self.textures.get(tile_id) {
                    //color.x = 1.0;
                    let index = settings.anim_counter % data.buffer.len();
                    let x = ((hit.z - zz + 0.5) * data.buffer[index].dim().width as f32)
                        as i32;
                    let y =
                        ((1.0 - hit.y) * data.buffer[index].dim().height as f32) as i32;
                    if let Some(c) = data.buffer[index].at(vec2i(x, y)) {
                        if c[3] == 255 {
                            let col = TheColor::from_u8_array(c).to_vec4f();
                            color = col;
                            dist = t;
                            normal = vec3f(0.0, 0.0, 1.0);
                        }
                    }
                }
            }
        }
    }
    }*/
