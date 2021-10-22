use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;
use std::str;
use std::time::Instant;
use std::vec::Vec;

use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::{self, Context, HasContext};
use glutin::dpi::LogicalSize;
use glutin::{ContextBuilder, Event, EventsLoop, WindowBuilder, WindowEvent};
use ldraw::color::{Material, MaterialRegistry};
use ldraw::color::ColorReference;
use ldraw::library::{
    load_files, scan_ldraw_directory, CacheCollectionStrategy, PartCache, PartDirectoryNative,
    ResolutionMap,
};
use ldraw::parser::{parse_color_definition, parse_multipart_document};
use ldraw::{Vector3, Vector4, Matrix3, Matrix4, PartAlias};
use ldraw_renderer::{
    error::RendererError,
    geometry::{GroupKey, ModelBuilder, NativeBakedModel, OpenGlBakedModel},
    scene::{ProjectionParams, ShadingParams},
    shader::{Bindable, ProgramManager},
};

pub struct TestRenderer<T: HasContext> {
    gl: Rc<T>,

    program_manager: ProgramManager<T>,

    default_material: Material,

    model: OpenGlBakedModel<T>,

    edge_length: i32,
    drawing_order: Vec<GroupKey>,

    center: Point3<f32>,
    radius: f32,
    degrees: Deg<f32>,

    projection_params: ProjectionParams,
    normal_matrix: Matrix3,
    shading_params: ShadingParams,

    time: f32,
}

impl<T: HasContext> TestRenderer<T> {
    pub fn new(
        model: &NativeBakedModel,
        colors: &MaterialRegistry,
        gl: Rc<T>,
    ) -> Result<TestRenderer<T>, RendererError> {
        let program_manager = ProgramManager::new(Rc::clone(&gl))?;

        let default_material = ColorReference::resolve(7, &colors)
            .get_material()
            .unwrap()
            .clone();

        let gl_ = &gl;

        unsafe {
            gl_.clear_color(1.0, 1.0, 1.0, 1.0);
            gl_.cull_face(glow::BACK);
            gl_.enable(glow::CULL_FACE);
            gl_.enable(glow::DEPTH_TEST);
            gl_.enable(glow::BLEND);
            gl_.depth_func(glow::LEQUAL);
            gl_.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl_.line_width(1.0);
        }

        let opengl_model = OpenGlBakedModel::create(Rc::clone(&gl), &model);

        let mut drawing_order = model
            .index
            .0
            .keys()
            .map(|v| v.clone())
            .collect::<Vec<_>>();
        drawing_order.sort();

        let center = Point3::new(0.0, 0.0, 0.0);
        let radius = 500.0;
        let degrees = Deg(0.0);

        Ok(TestRenderer {
            gl: Rc::clone(&gl),

            program_manager,

            default_material,

            model: opengl_model,

            edge_length: model.buffer.edges.len() as i32,
            drawing_order,

            center,
            radius,
            degrees,

            projection_params: ProjectionParams::new(),
            normal_matrix: Matrix3::identity(),
            shading_params: ShadingParams::new(),

            time: 0.0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.projection_params.projection = Matrix4::from(PerspectiveFov {
            fovy: Rad::from(Deg(45.0)),
            aspect: width as f32 / height as f32,
            near: 1.0,
            far: 100000.0,
        });

        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
        }
    }

    pub fn animate(&mut self, time: f32) {
        let delta = time - self.time;

        self.projection_params.view_matrix = Matrix4::look_at(
            Point3::new(
                0.0 + self.center.x,
                -self.radius / 5.0 * 2.0 + self.center.y,
                self.radius + self.center.z,
            ),
            self.center,
            Vector3::new(0.0, -1.0, 0.0),
        );

        self.degrees += Deg(delta * 60.0);
        let rotation = Quaternion::from_angle_y(self.degrees);
        self.projection_params.model_view = Matrix4::from(rotation);
        self.normal_matrix = self.projection_params.calculate_normal_matrix();

        self.time = time;
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);

            for order in self.drawing_order.iter() {
                let color = if let Some(mat) = order.color_ref.get_material() {
                    Vector4::from(mat.color)
                } else {
                    Vector4::from(self.default_material.color)
                };
                let program = if order.bfc {
                    &self.program_manager.solid
                } else {
                    &self.program_manager.solid_flat
                };
                program.bind();
                program.bind_uniforms(&self.projection_params, AsRef::<[f32; 9]>::as_ref(&self.normal_matrix),
                                      &self.shading_params, AsRef::<[f32; 4]>::as_ref(&color));

                self.model.buffer.mesh.bind(&program.attrib_position, &program.attrib_normal);

                if order.color_ref.is_material()
                    && order
                        .color_ref
                        .get_material()
                        .unwrap()
                        .is_semi_transparent()
                {
                    gl.enable(glow::BLEND);
                } else {
                    gl.disable(glow::BLEND);
                }

                let index = self.model.index.0[order];

                gl.draw_arrays(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32);

                program.unbind();
            }

            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);

            let program = &self.program_manager.edge;
            program.bind();
            program.bind_uniforms(&self.projection_params);
            self.model.buffer.edges.bind(&program.attrib_position, &program.attrib_colors);

            gl.draw_arrays(glow::LINES, 0, self.edge_length);

            program.unbind();

            gl.flush();
        }
    }
}

fn bake(
    colors: &MaterialRegistry,
    directory: Rc<RefCell<PartDirectoryNative>>,
    path: &str,
) -> NativeBakedModel {
    println!("Parsing document...");
    let document =
        parse_multipart_document(&colors, &mut BufReader::new(File::open(path).unwrap())).unwrap();

    println!("Resolving dependencies...");
    let cache = Rc::new(RefCell::new(PartCache::default()));
    let mut resolution = ResolutionMap::new(directory, Rc::clone(&cache));
    resolution.resolve(&&document.body, Some(&document));
    loop {
        let files = match load_files(&colors, Rc::clone(&cache), resolution.get_pending()) {
            Some(e) => e,
            None => break,
        };
        for key in files {
            let doc = cache.borrow().query(&key).unwrap();
            resolution.update(&key, doc);
        }
    }

    println!("Baking model...");

    let mut builder =
        ModelBuilder::new(&resolution).with_feature(PartAlias::from("stud.dat"));
    builder.traverse(&&document.body, Matrix4::identity(), true, false);
    let model = builder.bake();

    drop(builder);
    drop(resolution);
    drop(document);

    println!(
        "Collected {} entries",
        cache
            .borrow_mut()
            .collect(CacheCollectionStrategy::PartsAndPrimitives)
    );

    model
}

fn set_up_context(gl: &Context) {
    unsafe {
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.line_width(1.0);
        gl.cull_face(glow::BACK);
        gl.enable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.depth_func(glow::LEQUAL);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
    }
}

fn main_loop(model: &NativeBakedModel, colors: &MaterialRegistry) {
    let mut evloop = EventsLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .with_dimensions(LogicalSize::new(1280.0, 720.0));
    let windowed_context = ContextBuilder::new()
        .with_vsync(true)
        .build_windowed(window_builder, &evloop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let gl = unsafe { Context::from_loader_function(|s| windowed_context.get_proc_address(s) as *const _) };
    set_up_context(&gl);

    let gl = Rc::new(gl);

    let mut app = match TestRenderer::new(model, &colors, Rc::clone(&gl)) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    };
    let window = windowed_context.window();
    let size = window.get_inner_size().unwrap();
    let (w, h) = size.to_physical(window.get_hidpi_factor()).into();
    app.resize(w, h);

    println!("Bounding box: {:?}", model.bounding_box);

    let mut closed = false;
    let started = Instant::now();
    while !closed {
        set_up_context(&*gl);

        app.animate(started.elapsed().as_millis() as f32 / 1000.0);
        app.render();

        unsafe {
            (*gl).flush();
        }

        windowed_context.swap_buffers().unwrap();

        evloop.poll_events(|event| {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => {
                        closed = true;
                    }
                    WindowEvent::Resized(size) => {
                        let physical = size.to_physical(window.get_hidpi_factor());
                        windowed_context.resize(physical);
                        let (w, h) = physical.into();
                        app.resize(w, h);
                    }
                    _ => (),
                }
            }
        });
    }
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    println!("Scanning LDraw directory...");
    let directory = Rc::new(RefCell::new(scan_ldraw_directory(&ldrawdir).unwrap()));

    println!("Loading color definition...");
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().nth(1) {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let baked = bake(&colors, directory, &ldrpath);

    main_loop(&baked, &colors);
}
