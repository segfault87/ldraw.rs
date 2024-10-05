const MAX_ITERATIONS: i32 = 10;

pub enum GpuUpdateResult<M> {
    Modified,
    NotModified,
    AdditionalMutations { modified: bool, mutations: Vec<M> },
}

impl<M> From<bool> for GpuUpdateResult<M> {
    fn from(value: bool) -> Self {
        if value {
            Self::Modified
        } else {
            Self::NotModified
        }
    }
}

pub trait GpuUpdate {
    type Mutator;

    fn mutate(&mut self, mutator: Self::Mutator) -> GpuUpdateResult<Self::Mutator>;
    fn handle_gpu_update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);
}

#[derive(Debug)]
pub struct Entity<I> {
    inner: I,
    modified: bool,
}

impl<I> std::ops::Deref for Entity<I> {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<I: GpuUpdate> Entity<I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            modified: true,
        }
    }

    pub fn get(&self) -> &I {
        &self.inner
    }

    fn mutate_inner(&mut self, mutator: I::Mutator, iteration: i32) -> bool {
        if iteration >= MAX_ITERATIONS {
            println!("Nested mutations more than {MAX_ITERATIONS} depths are not allowed");
            false
        } else {
            match self.inner.mutate(mutator) {
                GpuUpdateResult::Modified => true,
                GpuUpdateResult::NotModified => false,
                GpuUpdateResult::AdditionalMutations {
                    mut modified,
                    mutations,
                } => {
                    for mutator in mutations {
                        if self.mutate_inner(mutator, iteration + 1) {
                            modified = true;
                        }
                    }
                    modified
                }
            }
        }
    }

    pub fn mutate(&mut self, mutator: I::Mutator) -> bool {
        if self.mutate_inner(mutator, 0) {
            self.modified = true;
            true
        } else {
            false
        }
    }

    pub fn mutate_all<T: Iterator<Item = I::Mutator>>(&mut self, mutators: T) -> bool {
        let mut modified = false;
        for mutator in mutators {
            if self.mutate_inner(mutator, 0) {
                modified = true;
            }
        }

        if modified {
            self.modified = true;
        }
        modified
    }

    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        if self.modified {
            self.inner.handle_gpu_update(device, queue);
            self.modified = false;
            true
        } else {
            false
        }
    }
}

impl<I: GpuUpdate> From<I> for Entity<I> {
    fn from(value: I) -> Self {
        Self {
            inner: value,
            modified: true,
        }
    }
}
