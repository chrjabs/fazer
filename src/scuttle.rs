//! # Scuttle Configs

use rustsat::{encodings::pb::DbGte, instances::MultiOptInstance, types::Lit};
use scuttle::{types::ParetoFront, KernelFunctions, Solve};

use crate::Solver;

pub struct PMin(scuttle::PMinimal);

impl From<MultiOptInstance> for PMin {
    fn from(value: MultiOptInstance) -> Self {
        let opts = scuttle::options::KernelOptions::default();
        PMin(scuttle::PMinimal::new_defaults(value, opts).expect("scuttle error"))
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

impl From<MultiOptInstance> for PMinCoreBoosting {
    fn from(value: MultiOptInstance) -> Self {
        let inst = value.clone();
        let opts = scuttle::options::DivConOptions {
            anchor: scuttle::options::DivConAnchor::PMinimal(
                scuttle::options::SubProblemSize::Smaller(0),
            ),
            ..Default::default()
        };
        PMinCoreBoosting(
            scuttle::solver::divcon::SeqDivCon::new_defaults(value, opts).expect("scuttle error"),
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

pub struct BiOptSat<PBE = DbGte>(scuttle::BiOptSat<PBE>);

impl<PBE> From<MultiOptInstance> for BiOptSat<PBE>
where
    PBE: rustsat::encodings::pb::BoundUpperIncremental + FromIterator<(Lit, usize)>,
{
    fn from(value: MultiOptInstance) -> Self {
        let opts = scuttle::options::KernelOptions::default();
        BiOptSat(scuttle::BiOptSat::<PBE>::new_defaults(value, opts).expect("scuttle error"))
    }
}

impl<PBE> Solver for BiOptSat<PBE>
where
    PBE: rustsat::encodings::pb::BoundUpperIncremental,
{
    fn run(&mut self) -> ParetoFront {
        self.0
            .solve(scuttle::Limits::none())
            .expect("scuttle error");
        self.0.pareto_front()
    }
}

pub struct LowerBounding(scuttle::LowerBounding);

impl From<MultiOptInstance> for LowerBounding {
    fn from(value: MultiOptInstance) -> Self {
        let opts = scuttle::options::KernelOptions::default();
        LowerBounding(scuttle::LowerBounding::new_defaults(value, opts).expect("scuttle error"))
    }
}

impl Solver for LowerBounding {
    fn run(&mut self) -> ParetoFront {
        self.0
            .solve(scuttle::Limits::none())
            .expect("scuttle error");
        self.0.pareto_front()
    }
}
