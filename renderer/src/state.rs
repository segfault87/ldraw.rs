use crate::shader::{Bindable, EdgeProgram, ProgramKind, ProgramManager, ShadedProgram};
use crate::GL;

pub struct State<'a, T: GL> {
    program_manager: ProgramManager<T>,
    bound: Option<ProgramKind<'a, T>>,
}

impl<'a, T: GL> State<'a, T> {
    pub fn new(program_manager: ProgramManager<T>) -> State<'a, T> {
        State {
            program_manager,
            bound: None,
        }
    }

    pub fn bind_solid(&'a mut self, bfc_certified: bool) -> &'a ShadedProgram<T> {
        if let Some(e) = &self.bound {
            match (e, bfc_certified) {
                (ProgramKind::Solid(p), true) => return p,
                (ProgramKind::SolidFlat(p), false) => return p,
                (_, _) => {
                    e.unbind();
                }
            }
        }

        if bfc_certified {
            self.bound = Some(ProgramKind::Solid(&self.program_manager.solid));
            &self.program_manager.solid.bind()
        } else {
            self.bound = Some(ProgramKind::SolidFlat(&self.program_manager.solid_flat));
            &self.program_manager.solid_flat.bind()
        }
    }

    pub fn bind_edge(&'a mut self) -> &'a EdgeProgram<T> {
        if let Some(e) = &self.bound {
            if let ProgramKind::Edge(p) = e {
                return p;
            } else {
                e.unbind();
            }
        }

        self.bound = Some(ProgramKind::Edge(&self.program_manager.edge));
        &self.program_manager.edge.bind()
    }
}
