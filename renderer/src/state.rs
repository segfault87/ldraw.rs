use std::{collections::HashMap, mem::replace, rc::Rc, vec::Vec};

use cgmath::{prelude::*, Deg, Ortho, PerspectiveFov, Point3, Rad, SquareMatrix};
use glow::HasContext;
use image::{load_from_memory_with_format, ImageFormat};
use ldraw::{color::Color, Matrix3, Matrix4, PartAlias, Vector2, Vector3, Vector4};
use ldraw_ir::geometry::{BoundingBox2, BoundingBox3};

use crate::{
    display_list::{DisplayItem, DisplayList},
    part::Part,
    shader::{DefaultProgramInstancingKind, ProgramManager},
    utils::derive_normal_matrix,
};

pub struct ProjectionData {
    pub projection: Matrix4,
    pub model_matrix: Vec<Matrix4>,
    pub model_view: Matrix4,
    pub normal_matrix: Matrix3,
    pub view_matrix: Matrix4,
    pub orthographic: bool,
}

impl Default for ProjectionData {
    fn default() -> Self {
        ProjectionData {
            projection: Matrix4::identity(),
            model_matrix: vec![Matrix4::identity()],
            model_view: Matrix4::identity(),
            normal_matrix: Matrix3::identity(),
            view_matrix: Matrix4::identity(),
            orthographic: false,
        }
    }
}

impl ProjectionData {
    pub fn derive_normal_matrix(&self) -> Matrix3 {
        derive_normal_matrix(&self.model_view)
    }

    pub fn update_projection_matrix(&mut self, proj: &Matrix4) {
        self.projection = *proj;
    }

    fn update_model_view_and_normal_matrix(&mut self) {
        self.model_view = self.view_matrix * self.model_matrix.last().unwrap();
        self.normal_matrix = derive_normal_matrix(&self.model_view);
    }

    pub fn update_view_matrix(&mut self, camera: &Matrix4) {
        self.view_matrix = *camera;
        self.update_model_view_and_normal_matrix();
    }

    pub fn push_model_matrix(&mut self, m: &Matrix4) {
        let top = *self.model_matrix.last().unwrap();
        let transformed = m * top;
        self.model_matrix.push(transformed);
        self.update_model_view_and_normal_matrix();
    }

    pub fn pop_model_matrix(&mut self) {
        if self.model_matrix.len() > 1 {
            self.model_matrix.pop();
            self.update_model_view_and_normal_matrix();
        }
    }

    pub fn derive_projected_bounding_box_2d(&self, bb: &BoundingBox3) -> BoundingBox2 {
        let matrix = self.view_matrix * self.model_matrix.last().unwrap();

        let mut pbb = BoundingBox2::zero();
        for point in bb.points() {
            let p = matrix * point.extend(1.0);
            pbb.update_point(&Vector2::new(p.x, p.y));
        }

        pbb
    }
}

#[derive(Debug)]
pub struct ShadingData {
    pub diffuse: Vector3,
    pub emissive: Vector3,
    pub roughness: f32,
    pub metalness: f32,
    pub opacity: f32,
}

impl Default for ShadingData {
    fn default() -> Self {
        ShadingData {
            diffuse: Vector3::new(1.0, 1.0, 1.0),
            emissive: Vector3::zero(),
            roughness: 0.3,
            metalness: 0.0,
            opacity: 1.0,
        }
    }
}

pub struct PerspectiveCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub fov: Deg<f32>,
}

impl PerspectiveCamera {
    pub fn new(position: Point3<f32>, look_at: Point3<f32>, fov: Deg<f32>) -> Self {
        PerspectiveCamera {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            fov,
        }
    }

    pub fn derive_projection_matrix(&self, width: usize, height: usize) -> Matrix4 {
        let aspect_ratio = width as f32 / height as f32;

        Matrix4::from(PerspectiveFov {
            fovy: Rad::from(self.fov),
            aspect: aspect_ratio,
            near: 10.0,
            far: 100000.0,
        })
    }

    pub fn derive_view_matrix(&self) -> Matrix4 {
        Matrix4::look_at_rh(self.position, self.look_at, self.up)
    }
}

#[derive(Clone, Debug)]
pub enum OrthographicViewBounds {
    BoundingBox3(BoundingBox3),
    BoundingBox2(BoundingBox2),
    Radius(f32),
    None,
}

pub struct OrthographicCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
}

impl OrthographicCamera {
    pub fn new(position: Point3<f32>, look_at: Point3<f32>) -> Self {
        OrthographicCamera {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
        }
    }

    pub fn new_isometric(center: Point3<f32>) -> Self {
        let sin = Deg(45.0f32).sin() * 10000.0;
        let siny = Deg(35.264f32).sin() * 10000.0;
        let position = Point3::new(center.x + sin, center.y - siny, center.z - sin);

        OrthographicCamera {
            position,
            look_at: center,
            up: Vector3::new(0.0, -1.0, 0.0),
        }
    }

    pub fn derive_projection_matrix(&self, view_bounds: &BoundingBox2) -> Matrix4 {
        Matrix4::from(Ortho {
            left: view_bounds.min.x,
            right: view_bounds.max.x,
            top: view_bounds.max.y,
            bottom: view_bounds.min.y,
            near: 0.1,
            far: 100000.0,
        })
    }

    pub fn derive_view_matrix(&self) -> Matrix4 {
        Matrix4::look_at_rh(self.position, self.look_at, self.up)
    }
}

#[derive(Debug)]
pub struct RenderingStats {
    pub triangles: usize,
    pub lines: usize,
    pub optional_lines: usize,

    pub distinct_parts: usize,
    pub parts: usize,
    
    pub draw_calls: usize,
    pub instanced_draw_calls: usize,
}

impl Default for RenderingStats {
    fn default() -> Self {
        RenderingStats {
            triangles: 0,
            lines: 0,
            optional_lines: 0,
            distinct_parts: 0,
            parts: 0,
            draw_calls: 0,
            instanced_draw_calls: 0,
        }
    }
}

pub struct RenderingContext<GL: HasContext> {
    gl: Rc<GL>,

    pub program_manager: ProgramManager<GL>,
    width: u32,
    height: u32,

    pub projection_data: ProjectionData,
    pub shading_data: ShadingData,
    pub rendering_stats: RenderingStats,

    envmap: Option<GL::Texture>,
}

fn load_envmap() -> Vec<u8> {
    let image =
        load_from_memory_with_format(include_bytes!("../assets/cubemap.png"), ImageFormat::Png)
            .unwrap();
    let rgba = image.to_rgba8();
    rgba.into_raw()
}

impl<GL: HasContext> RenderingContext<GL> {
    pub fn new(gl: Rc<GL>, program_manager: ProgramManager<GL>) -> Self {
        let envmap = unsafe {
            let envmap = match gl.create_texture() {
                Ok(e) => Some(e),
                Err(msg) => {
                    println!("Failed creating envmap texture: {}", msg);
                    None
                }
            };
            gl.bind_texture(glow::TEXTURE_2D, envmap);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                768,
                768,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(&load_envmap()),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );

            envmap
        };

        RenderingContext {
            gl: Rc::clone(&gl),
            program_manager,
            width: 1,
            height: 1,
            projection_data: ProjectionData::default(),
            shading_data: ShadingData::default(),
            rendering_stats: RenderingStats::default(),
            envmap,
        }
    }

    pub fn apply_perspective_camera(&mut self, camera: &PerspectiveCamera) {
        self.projection_data.update_projection_matrix(
            &camera.derive_projection_matrix(self.width as _, self.height as _),
        );
        self.projection_data
            .update_view_matrix(&camera.derive_view_matrix());
        self.projection_data.orthographic = false;
    }

    pub fn apply_orthographic_camera(
        &mut self,
        camera: &OrthographicCamera,
        view_bounds: &OrthographicViewBounds,
    ) -> Option<BoundingBox2> {
        self.projection_data
            .update_view_matrix(&camera.derive_view_matrix());

        let (view_bounding_box, fraction) = match view_bounds {
            OrthographicViewBounds::BoundingBox3(bb) => {
                let transformed_bb = self.projection_data.derive_projected_bounding_box_2d(bb);

                let (adjusted, fraction) = if transformed_bb.len_x() >= transformed_bb.len_y() {
                    let margin = transformed_bb.len_x() * 0.05;
                    let d = (transformed_bb.len_x() - transformed_bb.len_y()) * 0.5;
                    let fd = d / transformed_bb.len_x();

                    (
                        BoundingBox2::new(
                            &Vector2::new(
                                transformed_bb.min.x - margin,
                                transformed_bb.min.y - d - margin,
                            ),
                            &Vector2::new(
                                transformed_bb.max.x + margin,
                                transformed_bb.max.y + d + margin,
                            ),
                        ),
                        BoundingBox2::new(&Vector2::new(0.0, fd), &Vector2::new(1.0, 1.0 - fd)),
                    )
                } else {
                    let margin = transformed_bb.len_x() * 0.05;
                    let d = (transformed_bb.len_y() - transformed_bb.len_x()) * 0.5;
                    let fd = d / transformed_bb.len_y();

                    (
                        BoundingBox2::new(
                            &Vector2::new(
                                transformed_bb.min.x - d - margin,
                                transformed_bb.min.y - margin,
                            ),
                            &Vector2::new(
                                transformed_bb.max.x + d + margin,
                                transformed_bb.max.y + margin,
                            ),
                        ),
                        BoundingBox2::new(&Vector2::new(fd, 0.0), &Vector2::new(1.0 - fd, 1.0)),
                    )
                };

                (adjusted, Some(fraction))
            }
            OrthographicViewBounds::BoundingBox2(bb) => (bb.clone(), None),
            OrthographicViewBounds::Radius(r) => (
                BoundingBox2::new(
                    &Vector2::new(-(r / self.width as f32), -(r / self.height as f32)),
                    &Vector2::new(r / self.width as f32, r / self.height as f32),
                ),
                None,
            ),
            OrthographicViewBounds::None => (
                BoundingBox2::new(
                    &Vector2::new(-(self.width as f32 * 0.125), -(self.height as f32 * 0.125)),
                    &Vector2::new(self.width as f32 * 0.125, self.height as f32 * 0.125),
                ),
                None,
            ),
        };

        self.projection_data
            .update_projection_matrix(&camera.derive_projection_matrix(&view_bounding_box));
        self.projection_data.orthographic = true;

        fraction
    }

    pub fn upload_shading_data(&self) {
        self.program_manager.bind_envmap(&self.envmap);
    }

    pub fn set_initial_state(&self) {
        let gl = &self.gl;
        unsafe {
            gl.clear_color(1.0, 1.0, 1.0, 0.0);
            gl.clear_depth_f32(1.0);
            gl.line_width(1.0);
            gl.cull_face(glow::BACK);
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.depth_func(glow::LEQUAL);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.blend_equation(glow::FUNC_ADD);
            gl.polygon_offset(1.0, 0.0);
            gl.enable(glow::POLYGON_OFFSET_FILL);
        }
    }

    pub fn begin(&mut self) {
        unsafe {
            self.gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
    }

    pub fn end(&mut self) -> RenderingStats {
        unsafe {
            self.gl.flush();
        }
        
        replace(&mut self.rendering_stats, RenderingStats::default())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        unsafe {
            self.gl.viewport(0, 0, width as _, height as _);
        }
    }

    pub fn render_instanced(
        &mut self,
        part: &Part<GL>,
        display_item: &DisplayItem<GL>,
        translucent: bool,
    ) {
        let gl = &self.gl;
        let part_buffer = &part.part;

        let instance_buffer = &display_item.instances;

        if instance_buffer.count == 0 {
            return;
        } else if instance_buffer.count == 1 {
            self.projection_data
                .push_model_matrix(&instance_buffer.model_view_matrices[0]);
            self.render_single_part(part, &instance_buffer.colors[0], translucent);
            self.projection_data.pop_model_matrix();
            return;
        }

        self.rendering_stats.parts += instance_buffer.count;
        self.rendering_stats.distinct_parts += 1;

        if let Some(uncolored_index) = &part_buffer.uncolored_index {
            let program = self
                .program_manager
                .get_default_program(DefaultProgramInstancingKind::InstancedWithColors, true);

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&instance_buffer);
            bind.bind_instanced_color_data(&instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    uncolored_index.start as i32,
                    uncolored_index.span as i32,
                    instance_buffer.count as i32,
                );
            }
            self.rendering_stats.instanced_draw_calls += 1;
            self.rendering_stats.triangles += part.part.uncolored_triangles_count * instance_buffer.count;
        }
        if let Some(uncolored_without_bfc_index) = &part_buffer.uncolored_without_bfc_index {
            let program = self
                .program_manager
                .get_default_program(DefaultProgramInstancingKind::InstancedWithColors, false);

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&instance_buffer);
            bind.bind_instanced_color_data(&instance_buffer);

            unsafe {
                gl.disable(glow::CULL_FACE);
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    uncolored_without_bfc_index.start as i32,
                    uncolored_without_bfc_index.span as i32,
                    instance_buffer.count as i32,
                );
                gl.enable(glow::CULL_FACE);
            }
            self.rendering_stats.instanced_draw_calls += 1;
            self.rendering_stats.triangles += part.part.uncolored_without_bfc_triangles_count * instance_buffer.count;
        }
        let subparts = if translucent {
            &part_buffer.translucent_indices
        } else {
            &part_buffer.opaque_indices
        };
        for (group, indices) in subparts.iter() {
            let program = self
                .program_manager
                .get_default_program(DefaultProgramInstancingKind::Instanced, group.bfc);
            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(instance_buffer);
            let color = match group.color_ref.get_color_rgba() {
                Some(e) => e,
                None => continue,
            };
            bind.bind_non_instanced_color_data(&color);

            unsafe {
                if !group.bfc {
                    gl.disable(glow::CULL_FACE);
                }
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    indices.start as i32,
                    indices.span as i32,
                    instance_buffer.count as i32,
                );
                if !group.bfc {
                    gl.enable(glow::CULL_FACE);
                }
            }
            self.rendering_stats.instanced_draw_calls += 1;
        }
        self.rendering_stats.triangles += if translucent {
            part.part.translucent_triangles_count
        } else {
            part.part.opaque_triangles_count
        } * instance_buffer.count;

        if let Some(edges) = &part_buffer.edges {
            let program = self.program_manager.get_edge_program(true);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(edges);
            bind.bind_instanced_attribs(&instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::LINES,
                    0,
                    edges.length as i32,
                    instance_buffer.count as i32,
                );
            }
            self.rendering_stats.instanced_draw_calls += 1;
            self.rendering_stats.lines += part.part.edges_count * instance_buffer.count;
        }

        if let Some(optional_edges) = &part_buffer.optional_edges {
            let program = self.program_manager.get_optional_edge_program(true);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(optional_edges);
            bind.bind_instanced_attribs(instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::LINES,
                    0,
                    optional_edges.length as i32,
                    instance_buffer.count as i32,
                );
            }
            self.rendering_stats.instanced_draw_calls += 1;
            self.rendering_stats.optional_lines += part.part.optional_edges_count * instance_buffer.count;
        }
    }

    pub fn render_single_part(&mut self, part: &Part<GL>, color: &Color, translucent: bool) {
        let gl = &self.gl;
        let part_buffer = &part.part;

        let color_rgba: Vector4 = color.color.into();
        let edge_color_rgba: Vector4 = color.edge.into();

        self.rendering_stats.parts += 1;
        self.rendering_stats.distinct_parts += 1;

        if color.is_translucent() == translucent {
            if let Some(uncolored_index) = &part_buffer.uncolored_index {
                let program = self
                    .program_manager
                    .get_default_program(DefaultProgramInstancingKind::NonInstanced, true);

                let bind = program.bind(&self.projection_data, &self.shading_data);
                bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
                bind.bind_non_instanced_color_data(&color_rgba);

                unsafe {
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        uncolored_index.start as i32,
                        uncolored_index.span as i32,
                    );
                }
                self.rendering_stats.draw_calls += 1;
                self.rendering_stats.triangles += part.part.uncolored_triangles_count;
            }
            if let Some(uncolored_without_bfc_index) = &part_buffer.uncolored_without_bfc_index {
                let program = self
                    .program_manager
                    .get_default_program(DefaultProgramInstancingKind::NonInstanced, false);

                let bind = program.bind(&self.projection_data, &self.shading_data);
                bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
                bind.bind_non_instanced_color_data(&color_rgba);

                unsafe {
                    gl.disable(glow::CULL_FACE);
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        uncolored_without_bfc_index.start as i32,
                        uncolored_without_bfc_index.span as i32,
                    );
                    gl.enable(glow::CULL_FACE);
                }
                self.rendering_stats.draw_calls += 1;
                self.rendering_stats.triangles += part.part.uncolored_triangles_count;
            }
        }

        let subparts = if translucent {
            &part_buffer.translucent_indices
        } else {
            &part_buffer.opaque_indices
        };
        for (group, indices) in subparts.iter() {
            let color = match group.color_ref.get_color_rgba() {
                Some(e) => e,
                None => continue,
            };

            let program = self
                .program_manager
                .get_default_program(DefaultProgramInstancingKind::NonInstanced, group.bfc);

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(part_buffer.mesh.as_ref().unwrap());
            bind.bind_non_instanced_color_data(&color);

            unsafe {
                if !group.bfc {
                    gl.disable(glow::CULL_FACE);
                }
                gl.draw_arrays(glow::TRIANGLES, indices.start as i32, indices.span as i32);
                if !group.bfc {
                    gl.enable(glow::CULL_FACE);
                }
            }
            self.rendering_stats.draw_calls += 1;
        }
        self.rendering_stats.triangles += if translucent {
            part.part.translucent_triangles_count
        } else {
            part.part.opaque_triangles_count
        };

        if let Some(edges) = &part_buffer.edges {
            let program = self.program_manager.get_edge_program(false);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(edges);
            bind.bind_non_instanced_properties(&color_rgba, &edge_color_rgba);

            unsafe {
                gl.draw_arrays(glow::LINES, 0, edges.length as i32);
            }
            self.rendering_stats.draw_calls += 1;
            self.rendering_stats.lines += part.part.edges_count;
        }

        if let Some(optional_edges) = &part_buffer.optional_edges {
            let program = self.program_manager.get_optional_edge_program(false);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(optional_edges);
            bind.bind_non_instanced_properties(&color_rgba, &edge_color_rgba);

            unsafe {
                gl.draw_arrays(glow::LINES, 0, optional_edges.length as i32);
            }
            self.rendering_stats.draw_calls += 1;
            self.rendering_stats.optional_lines += part.part.optional_edges_count;
        }
    }

    pub fn render_display_list(
        &mut self,
        parts: &HashMap<PartAlias, Part<GL>>,
        display_list: &mut DisplayList<GL>,
        translucent: bool,
    ) {
        let display_items = if translucent {
            &display_list.translucent
        } else {
            &display_list.opaque
        };

        for (alias, object) in display_items.iter() {
            let part = match parts.get(alias) {
                Some(e) => e,
                None => continue,
            };

            self.render_instanced(part, object, translucent);
        }
    }
}

impl<GL: HasContext> Drop for RenderingContext<GL> {
    fn drop(&mut self) {
        unsafe {
            if let Some(e) = self.envmap {
                self.gl.delete_texture(e);
            }
        }
    }
}
