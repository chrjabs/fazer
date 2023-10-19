//! # Fazzer - Multi-Objective MaxSAT Fuzzing

use std::{any::Any, fmt, io, process::ExitCode};

use ::scuttle::types::ParetoFront;
use cli::{Cli, Exec};
use gen::MoGenerator;
use rustsat::instances::fio::dimacs;

mod cli;
mod config;
mod eval;
mod gen;
mod min;

// Solver configs
mod scuttle;

trait Solver {
    /// Run the solver and get the discovered Pareto front
    fn run(&mut self) -> ParetoFront;
}

pub enum Problem {
    /// The solver panicked with the given cause
    Panic(Box<dyn Any + Send + 'static>),
    /// Solution is not a solution to the constraints. The parameters are the
    /// index of the non-dominated point and the index of the solution.
    UnsatSol(usize, usize),
    /// Solution does not match the cost of the non-dominated point. The
    /// parameters are the index of the non-dominated point and the index of
    /// the solution.
    CostMismatch(usize, usize),
    /// Repeated point in Pareto front. The parameter is the index of the
    /// repeated points in the Pareto front.
    Repeated(usize, usize),
    /// The returned Pareto front is not non-dominated. The parameter is the
    /// index of the dominated point in the Pareto front.
    SelfDominated(usize),
    /// A returned non-dominated point is dominated by a solution found by
    /// another solver. The parameters are the index of the non-dominated point.
    OtherDominated(usize),
    /// The returned Pareto front is shorter than a valid Pareto front returned
    /// by another solver.
    Short,
    /// The solver returned a non-dominated point with a wrong number of
    /// objective values.
    WrongDimension(usize),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Panic(cause) => write!(f, "panicked"),
            Problem::UnsatSol(ndi, si) => {
                write!(f, "unsat solution (non-dom: {}, sol: {})", ndi, si)
            }
            Problem::CostMismatch(ndi, si) => {
                write!(f, "cost mismatch (non-dom: {}, sol: {})", ndi, si)
            }
            Problem::Repeated(ndi, si) => {
                write!(f, "repeated solution (non-dom: {}, sol: {}", ndi, si)
            }
            Problem::SelfDominated(ndi) => write!(f, "dominated by self (non-dom: {})", ndi),
            Problem::OtherDominated(ndi) => write!(f, "dominated by other (non-dom: {})", ndi),
            Problem::Short => write!(f, "pareto front too short"),
            Problem::WrongDimension(ndi) => {
                write!(f, "point with wrong dimension (non-dom: {})", ndi)
            }
        }
    }
}

fn main() -> ExitCode {
    let (cli, exec) = Cli::init();

    match exec {
        Exec::Generate(config) => dimacs::write_mcnf(&mut io::stdout(), MoGenerator::new(config))
            .unwrap_or_else(panic_with_err!(&cli)),
        Exec::Fuzz(_) => todo!(),
        Exec::Evaluate(solvers, inst) => {
            cli.info(&format!("evaluating {:?}", solvers.keys().collect::<Vec<_>>()));
            let problems = eval::compare(inst, &solvers);
            if !problems.is_empty() {
                cli.print_problems(&problems);
                return ExitCode::from(1);
            }
            cli.info("no problems found");
        }
    }
    ExitCode::from(0)
}
