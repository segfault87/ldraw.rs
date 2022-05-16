use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    vec::Vec
};

use cgmath::SquareMatrix;
use glow::HasContext;
use itertools::izip;
use ldraw::{
    color::{ColorReference, Material},
    document::{Document, MultipartDocument},
    Matrix4, PartAlias, Vector4,
};
use ldraw_ir::{
    geometry::BoundingBox3,
    model::{Object, ObjectGroup, ObjectInstance, Model},
};
use uuid::Uuid;

use crate::utils::cast_as_bytes;

pub struct DisplayItemBuilder {
    name: PartAlias,
    matrices: Vec<Matrix4>,
    colors: Vec<ColorReference>,
}

impl DisplayItemBuilder {
    pub fn new(name: PartAlias) -> Self {
        DisplayItemBuilder {
            name,
            matrices: vec![],
            colors: vec![],
        }
    }
}

pub struct InstanceBuffer<GL: HasContext> {
    gl: Rc<GL>,

    pub count: usize,

    pub model_view_matrices: Vec<Matrix4>,
    pub materials: Vec<Material>,
    pub colors: Vec<Vector4>,
    pub edge_colors: Vec<Vector4>,

    pub model_view_matrices_buffer: Option<GL::Buffer>,
    pub color_buffer: Option<GL::Buffer>,
    pub edge_color_buffer: Option<GL::Buffer>,

    modified: bool,
}

impl<GL: HasContext> InstanceBuffer<GL> {
    pub fn new(gl: Rc<GL>) -> Self {
        InstanceBuffer {
            gl,

            count: 0,

            model_view_matrices: vec![],
            materials: vec![],
            colors: vec![],
            edge_colors: vec![],

            model_view_matrices_buffer: None,
            color_buffer: None,
            edge_color_buffer: None,

            modified: false,
        }
    }

    pub fn calculate_bounding_box(&self, bounding_box: &BoundingBox3) -> Option<BoundingBox3> {
        let mut bb = BoundingBox3::zero();

        for matrix in self.model_view_matrices.iter() {
            for point in bounding_box.points() {
                let transformed = matrix * point.extend(1.0);
                bb.update_point(&transformed.truncate());
            }
        }

        if bb.is_null() {
            None
        } else {
            Some(bb)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    // FIXME: Current method refreshes the whole buffer when changes occur.
    // Additional optimizations could be made (if performance is not sufficient enough)
    pub fn update_buffer(&mut self) {
        if !self.modified {
            return;
        }

        let gl = &self.gl;

        if self.model_view_matrices.is_empty() {
            self.model_view_matrices_buffer = None;
        } else {
            if self.model_view_matrices_buffer.is_none() {
                self.model_view_matrices_buffer = unsafe { gl.create_buffer().ok() };
            }

            let mut buffer = Vec::<f32>::new();
            self.model_view_matrices
                .iter()
                .for_each(|e| buffer.extend(AsRef::<[f32; 16]>::as_ref(e)));

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.model_view_matrices_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    cast_as_bytes(buffer.as_ref()),
                    glow::DYNAMIC_DRAW,
                );
            }
        }

        if self.colors.is_empty() {
            self.color_buffer = None;
        } else {
            if self.color_buffer.is_none() {
                self.color_buffer = unsafe { gl.create_buffer().ok() };
            }

            let mut buffer = Vec::<f32>::new();
            self.colors
                .iter()
                .for_each(|e| buffer.extend(AsRef::<[f32; 4]>::as_ref(e)));

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.color_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    cast_as_bytes(buffer.as_ref()),
                    glow::DYNAMIC_DRAW,
                );
            }
        }

        if self.edge_colors.is_empty() {
            self.edge_color_buffer = None;
        } else {
            if self.edge_color_buffer.is_none() {
                self.edge_color_buffer = unsafe { gl.create_buffer().ok() };
            }

            let mut buffer = Vec::<f32>::new();
            self.edge_colors
                .iter()
                .for_each(|e| buffer.extend(AsRef::<[f32; 4]>::as_ref(e)));

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.edge_color_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    cast_as_bytes(buffer.as_ref()),
                    glow::DYNAMIC_DRAW,
                );
            }
        }

        self.modified = false;
    }
}

impl<GL: HasContext> Drop for InstanceBuffer<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;

        unsafe {
            if let Some(b) = self.model_view_matrices_buffer {
                gl.delete_buffer(b);
            }
            if let Some(b) = self.color_buffer {
                gl.delete_buffer(b);
            }
            if let Some(b) = self.edge_color_buffer {
                gl.delete_buffer(b);
            }
        }
    }
}

pub struct DisplayItem<GL: HasContext> {
    pub part: PartAlias,

    pub opaque: InstanceBuffer<GL>,
    pub translucent: InstanceBuffer<GL>,
}

impl<GL: HasContext> DisplayItem<GL> {
    pub fn new(gl: Rc<GL>, alias: &PartAlias) -> Self {
        DisplayItem {
            part: alias.clone(),

            opaque: InstanceBuffer::new(Rc::clone(&gl)),
            translucent: InstanceBuffer::new(Rc::clone(&gl)),
        }
    }

    /* TODO: This is temporary; should be superseded with sophisticated editor stuffs */
    pub fn update_data(
        &mut self,
        opaque: bool,
        model_view_matrices: &[Matrix4],
        materials: &[Material],
    ) {
        let mut new_model_view_matrices = vec![];
        let mut new_materials = vec![];
        let mut new_colors = vec![];
        let mut new_edge_colors = vec![];
        for (model_view_matrix, material) in izip!(model_view_matrices, materials) {
            new_model_view_matrices.push(*model_view_matrix);
            new_materials.push(material.clone());
            new_colors.push(material.color.into());
            new_edge_colors.push(material.edge.into());
        }

        let buffer = if opaque {
            &mut self.opaque
        } else {
            &mut self.translucent
        };

        buffer.model_view_matrices = new_model_view_matrices;
        buffer.materials = new_materials;
        buffer.colors = new_colors;
        buffer.edge_colors = new_edge_colors;
        buffer.count = model_view_matrices.len();
        buffer.modified = true;
    }

    pub fn add(&mut self, matrix: &Matrix4, material: &Material) {
        let buffer = if material.is_translucent() {
            &mut self.translucent
        } else {
            &mut self.opaque
        };

        buffer.model_view_matrices.push(*matrix);
        buffer.materials.push(material.clone());
        buffer.colors.push(Vector4::from(&material.color));
        buffer.edge_colors.push(Vector4::from(&material.edge));
        buffer.count += 1;
        buffer.modified = true;
    }
}

pub struct DisplayList<GL: HasContext> {
    gl: Rc<GL>,

    pub map: HashMap<PartAlias, DisplayItem<GL>>,
}

impl<GL: HasContext> DisplayList<GL> {
    pub fn count(&self) -> usize {
        let mut count = 0;

        for v in self.map.values() {
            count += v.opaque.count + v.translucent.count;
        }

        count
    }
}

fn build_display_list_multipart<'a, GL: HasContext>(
    gl: Rc<GL>,
    tr: &mut DisplayListTransaction<GL>,
    document: &'a Document,
    matrix: Matrix4,
    material_stack: &mut Vec<Material>,
    parent: &'a MultipartDocument,
) {
    for e in document.iter_refs() {
        if parent.subparts.contains_key(&e.name) {
            material_stack.push(match &e.color {
                ColorReference::Material(m) => m.clone(),
                _ => material_stack.last().unwrap().clone(),
            });

            build_display_list_multipart(
                Rc::clone(&gl),
                tr,
                parent.subparts.get(&e.name).unwrap(),
                matrix * e.matrix,
                material_stack,
                parent,
            );

            material_stack.pop();
        } else {
            let material = match &e.color {
                ColorReference::Material(m) => m,
                _ => material_stack.last().unwrap(),
            };

            tr.add(
                Rc::clone(&gl),
                e.name.clone(),
                matrix * e.matrix,
                material.clone(),
            );
        }
    }
}

fn build_display_list_contents<GL: HasContext>(
    gl: Rc<GL>,
    tr: &mut DisplayListTransaction<GL>,
    object_groups: &HashMap<Uuid, ObjectGroup>,
    exclusion_set: &HashSet<Uuid>,
    objects: &Vec<Object>,
    matrix: Matrix4,
) {
    for object in objects.iter() {
        if exclusion_set.contains(&object.id) {
            continue;
        }
        match &object.data {
            ObjectInstance::Part(part) => {
                let material = match &part.color {
                    ColorReference::Material(m) => Some(m.clone()),
                    _ => None,
                };

                tr.add(
                    Rc::clone(&gl),
                    part.part.clone(),
                    matrix * part.matrix,
                    material.unwrap_or_else(|| Material::default()),
                );
            }
            ObjectInstance::PartGroup(group_instance) => {
                if let Some(group) = object_groups.get(&group_instance.group_id) {
                    build_display_list_contents(
                        Rc::clone(&gl),
                        tr,
                        object_groups,
                        exclusion_set,
                        &group.objects,
                        matrix * group_instance.matrix,
                    );
                }
            }
            _ => {}
        }
    }
}

pub struct DisplayListTransaction<'a, GL: HasContext> {
    list: &'a mut DisplayList<GL>,
    affected_items: HashSet<PartAlias>,
}

impl<'a, GL: HasContext> DisplayListTransaction<'a, GL> {

    pub fn add(&mut self, gl: Rc<GL>, name: PartAlias, matrix: Matrix4, material: Material) {
        let entry = self.list.map
            .entry(name.clone())
            .or_insert_with(|| DisplayItem::new(Rc::clone(&gl), &name));

        entry.add(&matrix, &material);
        self.affected_items.insert(name);
    }

    pub fn clear(&mut self) {
        self.list.map.clear();
    }

    pub fn end(self) {
        for item in self.affected_items.iter() {
            if let Some(entry) = self.list.map.get_mut(item) {
                entry.opaque.update_buffer();
                entry.translucent.update_buffer();
            }
        }
    }

}

impl<GL: HasContext> DisplayList<GL> {
    pub fn new(gl: Rc<GL>) -> Self {
        DisplayList {
            gl,
            map: HashMap::new(),
        }
    }

    pub fn from_multipart_document(gl: Rc<GL>, document: &MultipartDocument) -> Self {
        let mut display_list = DisplayList::new(Rc::clone(&gl));
        let mut material_stack = vec![Material::default()];

        let mut tr = display_list.start_modification();

        build_display_list_multipart(
            gl,
            &mut tr,
            &document.body,
            Matrix4::identity(),
            &mut material_stack,
            document,
        );

        tr.end();

        display_list
    }

    pub fn from_model(gl: Rc<GL>, model: &Model) -> Self {
        let mut display_list = DisplayList::new(Rc::clone(&gl));

        let mut tr = display_list.start_modification();

        build_display_list_contents(
            gl,
            &mut tr,
            &model.object_groups,
            &HashSet::new(),
            &model.objects,
            Matrix4::identity(),
        );

        tr.end();
        
        display_list
    }

    pub fn rebuild(&mut self, model: &Model, group_id: Option<Uuid>, exclusion_set: &HashSet<Uuid>) {
        let gl = Rc::clone(&self.gl);

        let mut tr = self.start_modification();
        tr.clear();

        match group_id {
            Some(id) => {
                if let Some(group) = model.object_groups.get(&id) {
                    build_display_list_contents(
                        gl,
                        &mut tr,
                        &model.object_groups,
                        exclusion_set,
                        &group.objects,
                        Matrix4::identity(),
                    );
                }
            }
            None => {
                build_display_list_contents(
                    gl,
                    &mut tr,
                    &model.object_groups,
                    exclusion_set,
                    &model.objects,
                    Matrix4::identity(),
                );
            }
        }

        tr.end();
    }

    pub fn transact<F: Fn(&mut DisplayListTransaction<GL>)>(&mut self, f: F) {
        let mut transaction = DisplayListTransaction {
            list: self,
            affected_items: HashSet::new(),
        };

        f(&mut transaction);
        transaction.end();
    }

    pub fn start_modification(&mut self) -> DisplayListTransaction<GL> {
        DisplayListTransaction {
            list: self,
            affected_items: HashSet::new(),
        }
    }

}
