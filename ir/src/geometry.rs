use ldraw::{Vector2, Vector3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoundingBox2 {
    pub min: Vector2,
    pub max: Vector2,
}

impl BoundingBox2 {
    pub fn zero() -> Self {
        BoundingBox2 {
            min: Vector2::new(0.0, 0.0),
            max: Vector2::new(0.0, 0.0),
        }
    }

    pub fn new(a: &Vector2, b: &Vector2) -> Self {
        let (min_x, max_x) = if a.x > b.x { (b.x, a.x) } else { (a.x, b.x) };
        let (min_y, max_y) = if a.y > b.y { (b.y, a.y) } else { (a.y, b.y) };

        BoundingBox2 {
            min: Vector2::new(min_x, min_y),
            max: Vector2::new(max_x, max_y),
        }
    }

    pub fn len_x(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn len_y(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn is_null(&self) -> bool {
        self.min.x == 0.0 && self.min.y == 0.0 && self.max.x == 0.0 && self.max.y == 0.0
    }

    pub fn update_point(&mut self, v: &Vector2) {
        if self.is_null() {
            self.min = *v;
            self.max = *v;
        } else {
            if self.min.x > v.x {
                self.min.x = v.x;
            }
            if self.min.y > v.y {
                self.min.y = v.y;
            }
            if self.max.x < v.x {
                self.max.x = v.x;
            }
            if self.max.y < v.y {
                self.max.y = v.y;
            }
        }
    }

    pub fn update(&mut self, bb: &BoundingBox2) {
        self.update_point(&bb.min);
        self.update_point(&bb.max);
    }

    pub fn center(&self) -> Vector2 {
        (self.min + self.max) * 0.5
    }

    pub fn points(&self) -> [Vector2; 4] {
        [
            Vector2::new(self.min.x, self.min.y),
            Vector2::new(self.min.x, self.max.y),
            Vector2::new(self.max.x, self.min.y),
            Vector2::new(self.max.x, self.max.y),
        ]
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoundingBox3 {
    pub min: Vector3,
    pub max: Vector3,
}

impl BoundingBox3 {
    pub fn zero() -> Self {
        BoundingBox3 {
            min: Vector3::new(0.0, 0.0, 0.0),
            max: Vector3::new(0.0, 0.0, 0.0),
        }
    }

    pub fn new(a: &Vector3, b: &Vector3) -> Self {
        let (min_x, max_x) = if a.x > b.x { (b.x, a.x) } else { (a.x, b.x) };
        let (min_y, max_y) = if a.y > b.y { (b.y, a.y) } else { (a.y, b.y) };
        let (min_z, max_z) = if a.z > b.z { (b.z, a.z) } else { (a.z, b.z) };

        BoundingBox3 {
            min: Vector3::new(min_x, min_y, min_z),
            max: Vector3::new(max_x, max_y, max_z),
        }
    }

    pub fn len_x(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn len_y(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn len_z(&self) -> f32 {
        self.max.z - self.min.z
    }

    pub fn is_null(&self) -> bool {
        self.min.x == 0.0
            && self.min.y == 0.0
            && self.min.z == 0.0
            && self.max.x == 0.0
            && self.max.y == 0.0
            && self.max.z == 0.0
    }

    pub fn update_point(&mut self, v: &Vector3) {
        if self.is_null() {
            self.min = *v;
            self.max = *v;
        } else {
            if self.min.x > v.x {
                self.min.x = v.x;
            }
            if self.min.y > v.y {
                self.min.y = v.y;
            }
            if self.min.z > v.z {
                self.min.z = v.z;
            }
            if self.max.x < v.x {
                self.max.x = v.x;
            }
            if self.max.y < v.y {
                self.max.y = v.y;
            }
            if self.max.z < v.z {
                self.max.z = v.z;
            }
        }
    }

    pub fn update(&mut self, bb: &BoundingBox3) {
        self.update_point(&bb.min);
        self.update_point(&bb.max);
    }

    pub fn center(&self) -> Vector3 {
        (self.min + self.max) * 0.5
    }

    pub fn points(&self) -> [Vector3; 8] {
        [
            Vector3::new(self.min.x, self.min.y, self.min.z),
            Vector3::new(self.min.x, self.min.y, self.max.z),
            Vector3::new(self.min.x, self.max.y, self.min.z),
            Vector3::new(self.min.x, self.max.y, self.max.z),
            Vector3::new(self.max.x, self.min.y, self.min.z),
            Vector3::new(self.max.x, self.min.y, self.max.z),
            Vector3::new(self.max.x, self.max.y, self.min.z),
            Vector3::new(self.max.x, self.max.y, self.max.z),
        ]
    }
}
