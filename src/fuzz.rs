//! # Fuzzing MO-MaxSAT Solvers

use futures::executor::{self, ThreadPool};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rustsat::{instances::MultiOptInstance, types::RsHashMap};

use crate::{
    config::{InstConfig, SolverConfig},
    eval,
    gen::MoGenerator,
    Problem,
};

#[derive(Default, Debug)]
pub struct FuzzResult {
    by_inst: RsHashMap<u64, Vec<(String, Problem)>>,
    by_solver: RsHashMap<String, Vec<(u64, Problem)>>,
}

impl FuzzResult {
    fn instance_results(&mut self, inst_seed: u64, problems: Vec<(String, Problem)>) {
        for (slv, prob) in &problems {
            match self.by_solver.get_mut(slv) {
                Some(probs) => probs.push((inst_seed, *prob)),
                None => {
                    self.by_solver.insert(slv.clone(), vec![(inst_seed, *prob)]);
                }
            }
        }
        self.by_inst.insert(inst_seed, problems);
    }

    pub fn n_problems(&self) -> usize {
        self.by_inst
            .iter()
            .fold(0, |cnt, (_, probs)| cnt + probs.len())
    }

    pub fn n_solver_problems(&self, solver: &str) -> usize {
        match self.by_solver.get(solver) {
            Some(probs) => probs.len(),
            None => 0,
        }
    }

    pub fn n_instance_problems(&self, inst_seed: u64) -> usize {
        match self.by_inst.get(&inst_seed) {
            Some(probs) => probs.len(),
            None => 0,
        }
    }

    pub fn instance_problems(&self) -> impl Iterator<Item = (&u64, &Vec<(String, Problem)>)> {
        self.by_inst.iter()
    }

    pub fn solver_problems(&self) -> impl Iterator<Item = (&String, &Vec<(u64, Problem)>)> {
        self.by_solver.iter()
    }
}

pub fn fuzz(
    mut config: InstConfig,
    solvers: &RsHashMap<String, SolverConfig>,
    pool: Option<ThreadPool>,
) -> (usize, FuzzResult) {
    let mut rng = match config.seed {
        Some(seed) => ChaCha8Rng::seed_from_u64(seed),
        None => ChaCha8Rng::from_entropy(),
    };
    let mut results = FuzzResult::default();
    let mut tested = 0;
    for _ in 0..5 {
        loop {
            config.seed = Some(rng.gen());
            if !results.by_inst.contains_key(&config.seed.unwrap()) {
                break;
            }
        }
        let inst: MultiOptInstance = MultiOptInstance::from_iter(MoGenerator::new(config.clone()));
        let probs = executor::block_on(eval::compare(inst.clone(), solvers, pool.clone()));
        if !probs.is_empty() {
            results.instance_results(config.seed.unwrap(), probs);
            inst.to_dimacs_path(format!("buggy-{}.mcnf", config.seed.unwrap()))
                .expect("failed to write instance");
        }
        tested += 1;
    }
    (tested, results)
}
