use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::BufReader;
use std::mem;
use std::path::Path;
use std::ptr;
use std::str;

use cgmath::{Array, Deg, Euler, InnerSpace, Matrix, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use gl;
use gl::types;
use glutin::{ContextBuilder, ElementState, Event, EventsLoop, GlContext, GlWindow, KeyboardInput, MouseScrollDelta, VirtualKeyCode, WindowBuilder, WindowEvent};
use glutin::dpi::LogicalSize;
use ldraw::{Vector3, Vector4, Matrix3, Matrix4};
use ldraw::color::MaterialRegistry;
use ldraw::library::{
    load_files, scan_ldraw_directory, PartCache, PartDirectoryNative, ResolutionMap,
};
use ldraw::parser::{parse_color_definition, parse_multipart_document};
use ldraw_renderer::geometry::{BakedModel, EdgeBuffer, ModelBuilder};

fn bake(colors: &MaterialRegistry, directory: &PartDirectoryNative, path: &str) -> (BakedModel, EdgeBuffer) {
    println!("Parsing document...");
    let document =
        parse_multipart_document(&colors, &mut BufReader::new(File::open(path).unwrap())).unwrap();

    println!("Resolving dependencies...");
    let cache = RefCell::new(PartCache::default());
    let mut resolution = ResolutionMap::new(&directory, &cache);
    resolution.resolve(&&document.body, Some(&document));
    loop {
        let files = match load_files(&colors, &cache, resolution.get_pending()) {
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
    builder.traverse(&mut HashMap::new(), &&document.body, Matrix4::identity(), true, false);
    let model = builder.bake();
    let normals = builder.visualize_normals(2.0);

    drop(builder);
    drop(resolution);
    drop(document);

    println!("Collected {} entries", cache.borrow_mut().collect());

    (model, normals)
}

fn compile_shader(src: &str, ty: types::GLenum) -> types::GLuint {
    let shader;
    unsafe {
        shader = gl::CreateShader(ty);
        // Attempt to compile the shader
        let c_str = CString::new(src.as_bytes()).unwrap();
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), ptr::null());
        gl::CompileShader(shader);

        // Get the compile status
        let mut status = gl::FALSE as types::GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);

        // Fail on error
        if status != (gl::TRUE as types::GLint) {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = Vec::with_capacity(len as usize);
            buf.set_len((len as usize) - 1); // subtract 1 to skip the trailing null character
            gl::GetShaderInfoLog(
                shader,
                len,
                ptr::null_mut(),
                buf.as_mut_ptr() as *mut types::GLchar,
            );
            panic!(
                "{}",
                str::from_utf8(&buf)
                    .ok()
                    .expect("ShaderInfoLog not valid utf8")
            );
        }
    }
    shader
}

struct Program {
    pub vs: types::GLuint,
    pub fs: types::GLuint,
    pub program: types::GLuint,
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program);
            gl::DeleteShader(self.vs);
            gl::DeleteShader(self.fs);
        }
    }
}

fn compile_program(vs_text: &str, fs_text: &str) -> Program {
    let vs = compile_shader(vs_text, gl::VERTEX_SHADER);
    let fs = compile_shader(fs_text, gl::FRAGMENT_SHADER);

    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vs);
        gl::AttachShader(program, fs);
        gl::LinkProgram(program);
        // Get the link status
        let mut status = gl::FALSE as types::GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        // Fail on error
        if status != (gl::TRUE as types::GLint) {
            let mut len: types::GLint = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = Vec::with_capacity(len as usize);
            buf.set_len((len as usize) - 1); // subtract 1 to skip the trailing null character
            gl::GetProgramInfoLog(
                program,
                len,
                ptr::null_mut(),
                buf.as_mut_ptr() as *mut types::GLchar,
            );
            panic!(
                "{}",
                str::from_utf8(&buf)
                    .ok()
                    .expect("ProgramInfoLog not valid utf8")
            );
        }
        Program { vs, fs, program }
    }
}

fn set_up_context() {
    unsafe {
        gl::ClearColor(1.0, 1.0, 1.0, 1.0);
        gl::CullFace(gl::BACK);
        gl::Enable(gl::CULL_FACE);
        gl::Enable(gl::DEPTH_TEST);
        gl::Enable(gl::BLEND);
        gl::DepthFunc(gl::LEQUAL);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
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

fn main_loop(model: &BakedModel, normals: &EdgeBuffer) {
    let mut evloop = EventsLoop::new();
    let window = WindowBuilder::new()
        .with_dimensions(LogicalSize::new(1024.0, 768.0))
        .with_title("ldraw.rs demo");
    let context = ContextBuilder::new()
        .with_multisampling(4);
    let gl_window = GlWindow::new(window, context, &evloop).unwrap();

    unsafe { gl_window.make_current() }.unwrap();

    gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);
    set_up_context();

    let edge_program = compile_program(str::from_utf8(include_bytes!("../shaders/edge.vs")).unwrap(),
                                       str::from_utf8(include_bytes!("../shaders/edge.fs")).unwrap());
    let default_program = compile_program(str::from_utf8(include_bytes!("../shaders/default.vs")).unwrap(),
                                          str::from_utf8(include_bytes!("../shaders/default.fs")).unwrap());

    let projection
        = Matrix4::from(PerspectiveFov {
        fovy: Rad::from(Deg(45.0)),
        aspect: 1024.0 / 768.0,
        near: 1.0,
        far: 100000.0,
    });
    let mut model_view = Matrix4::identity();

    let mut vao_mesh: types::GLuint = 0;
    let mut vbo_mesh: Vec<types::GLuint> = Vec::new();
    let mut vao_edge: types::GLuint = 0;
    let mut vbo_edge: Vec<types::GLuint> = Vec::new();

    vbo_mesh.resize(model.meshes.len() * 2, 0);
    vbo_edge.resize(4, 0);

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

    let mut vbomap = HashMap::new();

    let mut draw_normals = false;

    unsafe {
        gl::GenVertexArrays(1, &mut vao_mesh);
        gl::BindVertexArray(vao_mesh);
        gl::GenBuffers((model.meshes.len() * 2) as i32, vbo_mesh.as_mut_ptr());
        let mut i = 0;
        for (k, mesh) in model.meshes.iter() {
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_mesh[i]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (mesh.vertices.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&mesh.vertices[0]),
                gl::STATIC_DRAW
            );
            i += 1;
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_mesh[i]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (mesh.normals.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&mesh.normals[0]),
                gl::STATIC_DRAW
            );
            i += 1;

            vbomap.insert(k, (vbo_mesh[i-2], vbo_mesh[i-1]));
        }

        gl::GenVertexArrays(1, &mut vao_edge);
        if model.edges.len() > 0 {
            gl::BindVertexArray(vao_edge);
            gl::GenBuffers(4 as i32, vbo_edge.as_mut_ptr());
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[0]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (model.edges.vertices.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&model.edges.vertices[0]),
                gl::STATIC_DRAW
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[1]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (model.edges.colors.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&model.edges.colors[0]),
                gl::STATIC_DRAW
            );
        }

        if normals.len() > 0 {
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[2]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (normals.vertices.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&normals.vertices[0]),
                gl::STATIC_DRAW
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[3]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (normals.colors.len() * mem::size_of::<f32>()) as types::GLsizeiptr,
                mem::transmute(&normals.colors[0]),
                gl::STATIC_DRAW
            );
        }

        ueprojection = gl::GetUniformLocation(edge_program.program, CString::new("projection").unwrap().as_ptr());
        uemodelview = gl::GetUniformLocation(edge_program.program, CString::new("modelView").unwrap().as_ptr());
        ueviewmatrix = gl::GetUniformLocation(edge_program.program, CString::new("viewMatrix").unwrap().as_ptr());
        aeposition = gl::GetAttribLocation(edge_program.program, CString::new("position").unwrap().as_ptr());
        aecolor = gl::GetAttribLocation(edge_program.program, CString::new("color").unwrap().as_ptr());

        umprojection = gl::GetUniformLocation(default_program.program, CString::new("projection").unwrap().as_ptr());
        ummodelview = gl::GetUniformLocation(default_program.program, CString::new("modelView").unwrap().as_ptr());
        umviewmatrix = gl::GetUniformLocation(default_program.program, CString::new("viewMatrix").unwrap().as_ptr());
        umnormalmatrix = gl::GetUniformLocation(default_program.program, CString::new("normalMatrix").unwrap().as_ptr());
        umcolor = gl::GetUniformLocation(default_program.program, CString::new("color").unwrap().as_ptr());
        umisbfccertified = gl::GetUniformLocation(default_program.program, CString::new("isBfcCertified").unwrap().as_ptr());
        umlightcolor = gl::GetUniformLocation(default_program.program, CString::new("lightColor").unwrap().as_ptr());
        umlightdirection = gl::GetUniformLocation(default_program.program, CString::new("lightDirection").unwrap().as_ptr());
        amposition = gl::GetAttribLocation(default_program.program, CString::new("position").unwrap().as_ptr());
        amnormal = gl::GetAttribLocation(default_program.program, CString::new("normal").unwrap().as_ptr());
    }

    let center = Point3::new(0.0, 0.0, 0.0);
    let mut rad = 500.0;
    let mut deg = Deg(0.0);
    let mut closed = false;

    let mut drawingorder = model.meshes.keys().collect::<Vec<_>>();
    drawingorder.sort();

    println!("{:?}", drawingorder);
    
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
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            gl::UseProgram(default_program.program);

            gl::EnableVertexAttribArray(amposition as types::GLuint);
            gl::EnableVertexAttribArray(amnormal as types::GLuint);

            let model_view = view * model_view;

            gl::Enable(gl::DEPTH_TEST);

            for order in drawingorder.iter() {
                gl::BindBuffer(gl::ARRAY_BUFFER, vbomap[order].0);
                gl::VertexAttribPointer(
                    amposition as types::GLuint,
                    3,
                    gl::FLOAT,
                    gl::FALSE as types::GLboolean,
                    0,
                    ptr::null()
                );
                gl::BindBuffer(gl::ARRAY_BUFFER, vbomap[order].1);
                gl::VertexAttribPointer(
                    amnormal as types::GLuint,
                    3,
                    gl::FLOAT,
                    gl::FALSE as types::GLboolean,
                    0,
                    ptr::null()
                );
                gl::UniformMatrix4fv(umprojection, 1, 0, projection.as_ptr());
                gl::UniformMatrix4fv(ummodelview, 1, 0, model_view.as_ptr());
                gl::UniformMatrix4fv(umviewmatrix, 1, 0, viewinv.as_ptr());
                let normal_matrix = inv_mat3(&model_view).transpose();
                gl::UniformMatrix3fv(umnormalmatrix, 1, 0, normal_matrix.as_ptr());
                if let Some(mat) = order.color_ref.get_material() {
                    let color = Vector4::from(mat.color);
                    gl::Uniform4fv(umcolor, 1, color.as_ptr());
                }
                if order.bfc {
                    gl::Enable(gl::CULL_FACE);
                    gl::Uniform1i(umisbfccertified, 1);
                } else {
                    gl::Disable(gl::CULL_FACE);
                    gl::Uniform1i(umisbfccertified, 0);
                }
                if order.color_ref.is_material() && order.color_ref.get_material().unwrap().is_semi_transparent() {
                    gl::Enable(gl::BLEND);
                } else {
                    gl::Disable(gl::BLEND);
                }
                gl::Uniform4fv(umlightcolor, 1, lightcolor.as_ptr());
                gl::Uniform4fv(umlightdirection, 1, lightdirection.as_ptr());

                gl::DrawArrays(gl::TRIANGLES, 0, model.meshes[order].len() as i32);
            }

            gl::Disable(gl::BLEND);
            gl::Disable(gl::CULL_FACE);

            gl::DisableVertexAttribArray(amposition as types::GLuint);
            gl::DisableVertexAttribArray(amnormal as types::GLuint);
            
            gl::UseProgram(edge_program.program);

            gl::EnableVertexAttribArray(aeposition as types::GLuint);
            gl::EnableVertexAttribArray(aecolor as types::GLuint);

            gl::UniformMatrix4fv(ueprojection, 1, 0, projection.as_ptr());
            gl::UniformMatrix4fv(uemodelview, 1, 0, model_view.as_ptr());
            gl::UniformMatrix4fv(ueviewmatrix, 1, 0, viewinv.as_ptr());

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[0]);
            gl::VertexAttribPointer(
                aeposition as types::GLuint,
                3,
                gl::FLOAT,
                gl::FALSE as types::GLboolean,
                0,
                ptr::null()
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[1]);
            gl::VertexAttribPointer(
                aecolor as types::GLuint,
                3,
                gl::FLOAT,
                gl::FALSE as types::GLboolean,
                0,
                ptr::null()
            );

            gl::DrawArrays(gl::LINES, 0, (model.edges.len()) as i32);

            if draw_normals {
                gl::Disable(gl::DEPTH_TEST);
                
                gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[2]);
                gl::VertexAttribPointer(
                    aeposition as types::GLuint,
                    3,
                    gl::FLOAT,
                    gl::FALSE as types::GLboolean,
                    0,
                    ptr::null()
                );
                gl::BindBuffer(gl::ARRAY_BUFFER, vbo_edge[3]);
                gl::VertexAttribPointer(
                    aecolor as types::GLuint,
                    3,
                    gl::FLOAT,
                    gl::FALSE as types::GLboolean,
                    0,
                    ptr::null()
                );
                
                gl::DrawArrays(gl::LINES, 0, (normals.len()) as i32);
            }

            gl::DisableVertexAttribArray(aeposition as types::GLuint);
            gl::DisableVertexAttribArray(aecolor as types::GLuint);
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
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    println!("Scanning LDraw directory...");
    let directory = scan_ldraw_directory(&ldrawdir).unwrap();

    println!("Loading color definition...");
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().nth(1) {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let (baked, normals) = bake(&colors, &directory, &ldrpath);

    main_loop(&baked, &normals);
}
