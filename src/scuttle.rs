//! # Scuttle Configs

use rustsat::instances::MultiOptInstance;
use scuttle::{types::ParetoFront, KernelFunctions, Solve};

use crate::Solver;

pub struct PMin(scuttle::PMinimal);

impl From<&MultiOptInstance> for PMin {
    fn from(value: &MultiOptInstance) -> Self {
        let inst = value.clone();
        let opts = scuttle::options::KernelOptions::default();
        PMin(scuttle::PMinimal::new_defaults(inst, opts).expect("scuttle error"))
    }
}

impl Solver for PMin {
    fn run(&mut self) -> ParetoFront {
        self.0
            .solve(scuttle::Limits::none())
            .expect("scuttle error");
        self.0.pareto_front()
    }
}

pub struct PMinCoreBoosting(scuttle::solver::divcon::SeqDivCon);

impl From<&MultiOptInstance> for PMinCoreBoosting {
    fn from(value: &MultiOptInstance) -> Self {
        let inst = value.clone();
        let opts = scuttle::options::DivConOptions {
            anchor: scuttle::options::DivConAnchor::PMinimal(
                scuttle::options::SubProblemSize::Smaller(0),
            ),
            ..Default::default()
        };
        PMinCoreBoosting(
            scuttle::solver::divcon::SeqDivCon::new_defaults(inst, opts).expect("scuttle error"),
        )
    }
}

impl Solver for PMinCoreBoosting {
    fn run(&mut self) -> ParetoFront {
        self.0
            .solve(scuttle::Limits::none())
            .expect("scuttle error");
        self.0.pareto_front()
    }
}
