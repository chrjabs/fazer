//! # Minimizing A Faulty Instance

use rustsat::instances::{MultiOptInstance, Objective, SatInstance};
use scuttle::types::ParetoFront;

use crate::{config::MinimizeConfig, eval, Problem, Solver};

#[derive(Default, Clone)]
struct Instance(Vec<Clause>);

#[derive(Clone)]
struct Clause {
    soft: Option<SoftData>,
    active: bool,
    cl: rustsat::types::Clause,
}

impl Clause {
    fn hard(clause: rustsat::types::Clause) -> Clause {
        Clause {
            soft: None,
            active: true,
            cl: clause,
        }
    }

    fn soft(clause: rustsat::types::Clause, weight: usize, obj: u8) -> Clause {
        Clause {
            soft: Some(SoftData {
                obj,
                val: weight,
                previous: weight,
                lower_bound: 0,
                upper_bound: weight,
            }),
            active: true,
            cl: clause,
        }
    }
}

#[derive(Clone, Copy)]
struct SoftData {
    obj: u8,
    val: usize,
    previous: usize,
    lower_bound: usize,
    upper_bound: usize,
}

enum Modes {
    MinClauses,
    MinLits,
    MinVars,
    Soft2Hard,
    Weight2One,
    WeightBinary,
}

impl Into<MultiOptInstance> for Instance {
    fn into(self) -> MultiOptInstance {
        let mut constr = SatInstance::default();
        let mut objs = vec![];
        self.0.into_iter().for_each(|cl| {
            if !cl.active {
                return;
            }
            if let Some(soft) = cl.soft {
                if soft.obj as usize >= objs.len() {
                    objs.resize((soft.obj + 1).into(), Objective::default());
                }
                objs[soft.obj as usize].add_soft_clause(soft.val, cl.cl);
            } else {
                constr.add_clause(cl.cl);
            }
        });
        MultiOptInstance::compose(constr, objs)
    }
}

impl From<MultiOptInstance> for Instance {
    fn from(value: MultiOptInstance) -> Self {
        let (cnf, objs, _) = value.as_hard_cls_soft_cls();
        let mut inst = Instance::default();
        cnf.into_iter().for_each(|cl| inst.0.push(Clause::hard(cl)));
        objs.into_iter().enumerate().for_each(|(idx, obj)| {
            obj.0
                .into_iter()
                .for_each(|(cl, w)| inst.0.push(Clause::soft(cl, w, idx.try_into().unwrap())))
        });
        inst
    }
}

fn check_instance<S: Solver + for<'a> From<&'a MultiOptInstance>>(
    inst: Instance,
) -> Result<ParetoFront, Problem> {
    let inst: MultiOptInstance = inst.into();
    eval::evaluate::<S>(&inst)
}

pub fn minimize(inst: MultiOptInstance, config: MinimizeConfig) -> MultiOptInstance {
    let inst: Instance = inst.into();
    todo!()
}
