use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;
use std::slice::from_raw_parts;
use std::str;

use cgmath::{Deg, InnerSpace, Matrix, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::{self, Context, HasContext};
use glutin::{ContextBuilder, ElementState, Event, EventsLoop, GlContext, GlWindow, KeyboardInput, MouseScrollDelta, VirtualKeyCode, WindowBuilder, WindowEvent};
use glutin::dpi::LogicalSize;
use ldraw::{Vector3, Vector4, Matrix3, Matrix4};
use ldraw::color::MaterialRegistry;
use ldraw::library::{
    load_files, scan_ldraw_directory, PartCache, PartDirectoryNative, ResolutionMap,
};
use ldraw::parser::{parse_color_definition, parse_multipart_document};
use ldraw_renderer::geometry::{BakedModel, EdgeBuffer, ModelBuilder};

fn bake(colors: &MaterialRegistry, directory: Rc<RefCell<PartDirectoryNative>>, path: &str) -> (BakedModel, EdgeBuffer) {
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

    let mut builder = ModelBuilder::new(&colors, &resolution);
    builder.traverse(&&document.body, Matrix4::identity(), true, false);
    let model = builder.bake();
    let normals = builder.visualize_normals(2.0);

    drop(builder);
    drop(resolution);
    drop(document);

    println!("Collected {} entries", cache.borrow_mut().collect());

    (model, normals)
}

fn compile_shader<T: HasContext>(gl: &T, src: &str, ty: u32) -> T::Shader {
    let shader;
    unsafe {
        shader = gl.create_shader(ty).unwrap();
        // Attempt to compile the shader
        gl.shader_source(shader, src);
        gl.compile_shader(shader);

        // Get the compile status
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            panic!(log);
        }
    }
    shader
}

#[derive(Debug)]
struct Program<T: HasContext> {
    pub vs: T::Shader,
    pub fs: T::Shader,
    pub program: T::Program,
}

fn compile_program<T: HasContext>(gl: &T, vs_text: &str, fs_text: &str) -> Program<T> {
    let vs = compile_shader(gl, vs_text, glow::VERTEX_SHADER);
    let fs = compile_shader(gl, fs_text, glow::FRAGMENT_SHADER);

    unsafe {
        let program = gl.create_program().unwrap();
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!(gl.get_program_info_log(program));
        }

        Program { vs, fs, program }
    }
}

fn set_up_context(gl: &Context) {
    unsafe {
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.cull_face(glow::BACK);
        gl.enable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.depth_func(glow::LEQUAL);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
    }
}

fn inv_mat3(src: &Matrix4) -> Matrix3 {
    let a00 = src[0][0];
    let a01 = src[0][1];
    let a02 = src[0][2];
    let a10 = src[1][0];
    let a11 = src[1][1];
    let a12 = src[1][2];
    let a20 = src[2][0];
    let a21 = src[2][1];
    let a22 = src[2][2];

    let b01 = a22*a11 - a12*a21;
    let b11 = -a22*a10 + a12*a20;
    let b21 = a21*a10 - a11*a20;

    let det = a00*b01 + a01*b11 + a02*b21;
    if det == 0.0 {
        panic!("Fuck!");
    }
    let id = 1.0 / det;

    Matrix3::new(
        b01*id, (-a22*a01 + a02*a21)*id, (a12*a01 - a02*a11)*id,
        b11*id, (a22*a00 - a02*a20)*id, (-a12*a00 + a02*a10)*id,
        b21*id, (-a21*a00 + a01*a20)*id, (a11*a00 - a01*a10)*id)
}

fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

fn main_loop(model: &BakedModel, normals: &EdgeBuffer) {
    let mut evloop = EventsLoop::new();
    let window = WindowBuilder::new()
        .with_dimensions(LogicalSize::new(1600.0, 1200.0))
        .with_title("ldraw.rs demo");
    let context = ContextBuilder::new()
        .with_multisampling(4);
    let gl_window = GlWindow::new(window, context, &evloop).unwrap();

    unsafe { gl_window.make_current() }.unwrap();

    let gl = Context::from_loader_function(|s| {
        gl_window.get_proc_address(s) as *const _
    });
    set_up_context(&gl);

    let edge_program = compile_program(
        &gl,
        str::from_utf8(include_bytes!("../shaders/edge.vs")).unwrap(),
        str::from_utf8(include_bytes!("../shaders/edge.fs")).unwrap()
    );
    let default_program = compile_program(
        &gl,
        str::from_utf8(include_bytes!("../shaders/default.vs")).unwrap(),
        str::from_utf8(include_bytes!("../shaders/default.fs")).unwrap()
    );

    let projection = Matrix4::from(PerspectiveFov {
        fovy: Rad::from(Deg(45.0)),
        aspect: 1024.0 / 768.0,
        near: 1.0,
        far: 100000.0,
    });
    let mut model_view: Matrix4;

    let vao_mesh: <Context as HasContext>::VertexArray;
    let vbo_mesh_vertices: Option<<Context as HasContext>::Buffer>;
    let vbo_mesh_normals: Option<<Context as HasContext>::Buffer>;
    let vao_edge: <Context as HasContext>::VertexArray;
    let vbo_edge_vertices: Option<<Context as HasContext>::Buffer>;
    let vbo_edge_colors: Option<<Context as HasContext>::Buffer>;
    let vbo_normal_vertices: Option<<Context as HasContext>::Buffer>;
    let vbo_normal_colors: Option<<Context as HasContext>::Buffer>;

    let ueprojection;
    let uemodelview;
    let ueviewmatrix;
    let aeposition;
    let aecolor;

    let umprojection;
    let ummodelview;
    let umviewmatrix;
    let umnormalmatrix;
    let umcolor;
    let umisbfccertified;
    let umlightcolor;
    let umlightdirection;
    let amposition;
    let amnormal;

    let mut draw_normals = false;

    unsafe {
        vao_mesh = gl.create_vertex_array().unwrap();
        gl.bind_vertex_array(Some(vao_mesh));
        vbo_mesh_vertices = Some(gl.create_buffer().unwrap());
        gl.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_vertices);
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            cast_as_bytes(model.mesh.vertices.as_ref()),
            glow::STATIC_DRAW
        );
        vbo_mesh_normals = Some(gl.create_buffer().unwrap());
        gl.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_normals);
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            cast_as_bytes(model.mesh.normals.as_ref()),
            glow::STATIC_DRAW
        );

        vao_edge = gl.create_vertex_array().unwrap();
        gl.bind_vertex_array(Some(vao_edge));
        vbo_edge_vertices = Some(gl.create_buffer().unwrap());
        vbo_edge_colors = Some(gl.create_buffer().unwrap());
        vbo_normal_vertices = Some(gl.create_buffer().unwrap());
        vbo_normal_colors = Some(gl.create_buffer().unwrap());
        if model.edges.len() > 0 {
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.edges.vertices.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.edges.colors.as_ref()),
                glow::STATIC_DRAW
            );
        }

        if normals.len() > 0 {
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_normal_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(normals.vertices.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_normal_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(normals.colors.as_ref()),
                glow::STATIC_DRAW
            );
        }

        ueprojection = gl.get_uniform_location(edge_program.program, "projection");
        uemodelview = gl.get_uniform_location(edge_program.program, "modelView");
        ueviewmatrix = gl.get_uniform_location(edge_program.program, "viewMatrix");
        aeposition = gl.get_attrib_location(edge_program.program, "position");
        aecolor = gl.get_attrib_location(edge_program.program, "color");
        umprojection = gl.get_uniform_location(default_program.program, "projection");
        ummodelview = gl.get_uniform_location(default_program.program, "modelView");
        umviewmatrix = gl.get_uniform_location(default_program.program, "viewMatrix");
        umnormalmatrix = gl.get_uniform_location(default_program.program, "normalMatrix");
        umcolor = gl.get_uniform_location(default_program.program, "color");
        umisbfccertified = gl.get_uniform_location(default_program.program, "isBfcCertified");
        umlightcolor = gl.get_uniform_location(default_program.program, "lightColor");
        umlightdirection = gl.get_uniform_location(default_program.program, "lightDirection");
        amposition = gl.get_attrib_location(default_program.program, "position");
        amnormal = gl.get_attrib_location(default_program.program, "normal");
    }

    let center = Point3::new(0.0, 0.0, 0.0);
    let mut rad = 500.0;
    let mut deg = Deg(0.0);
    let mut closed = false;

    let mesh_index = model.mesh_index.clone();

    let mut drawingorder = model.mesh_index.0.keys().collect::<Vec<_>>();
    drawingorder.sort();

    println!("{:?} {:?}", ueprojection, umprojection);
    println!("{:?}", aeposition);
    println!("{:?}", aecolor);
    println!("{:?}", amposition);
    println!("{:?}", amnormal);
    
    while !closed {
        let view =
            Matrix4::look_at(Point3::new(0.0 + center.x, -rad / 5.0 * 2.0 + center.y, rad + center.z),
                             center,
                             Vector3::new(0.0, -1.0, 0.0));
        let viewinv = view.invert().unwrap();

        let rotation = Quaternion::from_angle_y(deg);
        model_view = Matrix4::from(rotation);

        deg += Deg(1.5);

        let lightcolor = Vector4::new(1.0, 1.0, 1.0, 1.0);
        let lightdirection = Vector4::new(0.0, -0.5, 0.7, 1.0).normalize();
        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(default_program.program));

            gl.enable_vertex_attrib_array(amposition as u32);
            gl.enable_vertex_attrib_array(amnormal as u32);

            let model_view = view * model_view;

            gl.enable(glow::DEPTH_TEST);

            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_vertices);
            gl.vertex_attrib_pointer_f32(
                amposition as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_normals);
            gl.vertex_attrib_pointer_f32(
                amnormal as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0
            );

            gl.uniform_matrix_4_f32_slice(umprojection, false, projection.as_ref());
            gl.uniform_matrix_4_f32_slice(ummodelview, false, model_view.as_ref());
            gl.uniform_matrix_4_f32_slice(umviewmatrix, false, viewinv.as_ref());
            let normal_matrix = inv_mat3(&model_view).transpose();
            gl.uniform_matrix_3_f32_slice(umnormalmatrix, false, normal_matrix.as_ref());

            gl.uniform_4_f32_slice(umlightcolor, lightcolor.as_ref());
            gl.uniform_4_f32_slice(umlightdirection, lightdirection.as_ref());

            for order in drawingorder.iter() {
                let index = mesh_index.0[order];
                
                if let Some(mat) = order.color_ref.get_material() {
                    let color = Vector4::from(mat.color);
                    gl.uniform_4_f32_slice(umcolor, color.as_ref());
                }
                if order.bfc {
                    gl.enable(glow::CULL_FACE);
                    gl.uniform_1_i32(umisbfccertified, 1);
                } else {
                    gl.disable(glow::CULL_FACE);
                    gl.uniform_1_i32(umisbfccertified, 0);
                }
                if order.color_ref.is_material() && order.color_ref.get_material().unwrap().is_semi_transparent() {
                    gl.enable(glow::BLEND);
                } else {
                    gl.disable(glow::BLEND);
                }

                gl.draw_arrays(glow::TRIANGLES, (index.0 / 3) as i32, ((index.1 - index.0) / 3) as i32);
            }

            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);

            gl.disable_vertex_attrib_array(amposition as u32);
            gl.disable_vertex_attrib_array(amnormal as u32);

            gl.use_program(Some(edge_program.program));

            gl.enable_vertex_attrib_array(aeposition as u32);
            gl.enable_vertex_attrib_array(aecolor as u32);

            gl.uniform_matrix_4_f32_slice(ueprojection, false, projection.as_ref());
            gl.uniform_matrix_4_f32_slice(uemodelview, false, model_view.as_ref());
            gl.uniform_matrix_4_f32_slice(ueviewmatrix, false, viewinv.as_ref());

            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_vertices);
            gl.vertex_attrib_pointer_f32(
                aeposition as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_colors);
            gl.vertex_attrib_pointer_f32(
                aecolor as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0
            );

            gl.draw_arrays(glow::LINES, 0, (model.edges.len()) as i32);

            if draw_normals {
                gl.disable(glow::DEPTH_TEST);
                
                gl.bind_buffer(glow::ARRAY_BUFFER, vbo_normal_vertices);
                gl.vertex_attrib_pointer_f32(
                    aeposition as u32,
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0
                );
                gl.bind_buffer(glow::ARRAY_BUFFER, vbo_normal_colors);
                gl.vertex_attrib_pointer_f32(
                    aecolor as u32,
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0
                );
                
                gl.draw_arrays(glow::LINES, 0, (normals.len()) as i32);
            }

            gl.disable_vertex_attrib_array(aeposition as u32);
            gl.disable_vertex_attrib_array(aecolor as u32);

            gl.flush();
        }

        gl_window.swap_buffers().unwrap();

        evloop.poll_events(|event| {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => {
                        closed = true;
                    },
                    WindowEvent::MouseWheel { delta, .. } => {
                        match delta {
                            MouseScrollDelta::LineDelta(_, y) => {
                                rad -= y * 30.0;
                            },
                            MouseScrollDelta::PixelDelta(lp) => {
                                rad -= lp.y as f32 * 2.0;
                            },
                        }
                    },
                    WindowEvent::KeyboardInput { input: KeyboardInput { virtual_keycode: keycode, state: ElementState::Pressed, .. }, .. } => {
                        match keycode {
                            Some(VirtualKeyCode::N) => {
                                draw_normals = !draw_normals;
                            },
                            _ => (),
                        };
                    },
                    _ => (),
                }
            }
        });
    }

    unsafe {
        gl.delete_program(edge_program.program);
        gl.delete_shader(edge_program.vs);
        gl.delete_shader(edge_program.fs);
        gl.delete_program(default_program.program);
        gl.delete_shader(default_program.vs);
        gl.delete_shader(default_program.fs);
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

    let (baked, normals) = bake(&colors, directory, &ldrpath);

    main_loop(&baked, &normals);
}
