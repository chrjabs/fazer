//! # Evaluating An Instance With a Solver

use rustsat::{instances::MultiOptInstance, types::RsHashMap};
use scuttle::types::ParetoFront;

use crate::{
    config::{ScuttleConfig, SolverConfig},
    Problem, Solver,
};

pub fn evaluate<S: Solver + for<'a> From<&'a MultiOptInstance>>(
    inst: &MultiOptInstance,
) -> Result<ParetoFront, Problem> {
    std::panic::catch_unwind(|| {
        let mut solver = S::from(inst);
        solver.run()
    })
    .map_err(Problem::Panic)
}

pub fn compare(
    inst: MultiOptInstance,
    solvers: &RsHashMap<String, SolverConfig>,
) -> Vec<(String, Problem)> {
    let mut problems = vec![];
    let mut pfs = vec![];
    for (sid, sconf) in solvers {
        let res = match sconf {
            SolverConfig::Scuttle(conf) => match conf {
                ScuttleConfig::PMinimal => evaluate::<crate::scuttle::PMin>(&inst),
                ScuttleConfig::CoreBoostedPMinimal => {
                    evaluate::<crate::scuttle::PMinCoreBoosting>(&inst)
                }
                ScuttleConfig::BiOptSatGte => todo!(),
                ScuttleConfig::BiOptSatDpw => todo!(),
                ScuttleConfig::LowerBounding => todo!(),
            },
        };
        match res {
            Ok(pf) => pfs.push((sid.clone(), pf)),
            Err(prob) => problems.push((sid.clone(), prob)),
        }
    }
    problems.extend(compare_pfs(pfs, &inst));
    problems
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Relation {
    Incomparable,
    FirstDominates,
    SecondDominates,
    Equal,
}

fn check_relation(c1: &[isize], c2: &[isize]) -> Relation {
    let mut dom = Relation::Equal;
    for (c1, c2) in c1.iter().zip(c2.iter()) {
        if c1 < c2 {
            if dom == Relation::SecondDominates {
                return Relation::Incomparable;
            }
            dom = Relation::FirstDominates;
        } else if c2 < c1 {
            if dom == Relation::FirstDominates {
                return Relation::Incomparable;
            }
            dom = Relation::SecondDominates;
        }
    }
    dom
}

fn compare_pfs(
    mut pfs: Vec<(String, ParetoFront)>,
    inst: &MultiOptInstance,
) -> Vec<(String, Problem)> {
    let mut problems = vec![];
    pfs.retain(|(sid, pf)| {
        // Check solutions
        for (ndom_idx, ndom) in pf.iter().enumerate() {
            if ndom.costs().len() != inst.n_objectives() {
                problems.push((sid.clone(), Problem::WrongDimension(ndom_idx)));
                return false;
            }
            for (sol_idx, sol) in ndom.iter().enumerate() {
                match inst.cost(sol) {
                    Some(cost) => {
                        if &cost != ndom.costs() {
                            problems.push((sid.clone(), Problem::CostMismatch(ndom_idx, sol_idx)));
                            return false;
                        }
                    }
                    None => {
                        problems.push((sid.clone(), Problem::UnsatSol(ndom_idx, sol_idx)));
                        return false;
                    }
                }
            }
        }
        // Check non-dominance
        for idx1 in 0..pf.len() - 1 {
            for idx2 in idx1 + 1..pf.len() {
                match check_relation(pf[idx1].costs(), pf[idx2].costs()) {
                    Relation::Incomparable => continue,
                    Relation::FirstDominates => {
                        problems.push((sid.clone(), Problem::SelfDominated(idx2)))
                    }
                    Relation::SecondDominates => {
                        problems.push((sid.clone(), Problem::SelfDominated(idx1)))
                    }
                    Relation::Equal => problems.push((sid.clone(), Problem::Repeated(idx1, idx2))),
                }
                return false;
            }
        }
        true
    });
    // Check lengths
    let max_pf_len = pfs
        .iter()
        .fold(0, |max, (_, pf)| std::cmp::max(max, pf.len()));
    pfs.retain(|(sid, pf)| {
        if pf.len() == max_pf_len {
            return true;
        }
        problems.push((sid.clone(), Problem::Short));
        false
    });
    if pfs.len() <= 1 || pfs[0].1.is_empty() {
        return problems;
    }
    // Build joint non-dominated set and compare
    let nobjs = inst.n_objectives();
    let mut non_dom_set = vec![0; pfs[0].1.len() * nobjs];
    for (idx, ndom) in pfs[0].1.iter().enumerate() {
        non_dom_set[idx * nobjs..(idx + 1) * nobjs].copy_from_slice(ndom.costs());
    }
    pfs.retain(|(sid, pf)| {
        'ndoms: for (ndom_idx, ndom) in pf.iter().enumerate() {
            let mut append = true;
            for idx in (0..non_dom_set.len()).step_by(nobjs) {
                match check_relation(ndom.costs(), &non_dom_set[idx..idx + nobjs]) {
                    Relation::Incomparable => (),
                    Relation::FirstDominates => {
                        non_dom_set[idx..idx + nobjs].copy_from_slice(ndom.costs());
                        append = false;
                    }
                    Relation::SecondDominates => {
                        problems.push((sid.clone(), Problem::OtherDominated(ndom_idx)));
                        return false;
                    }
                    Relation::Equal => continue 'ndoms,
                }
            }
            if append {
                non_dom_set.resize(non_dom_set.len() + nobjs, 0);
                let len = non_dom_set.len();
                non_dom_set[len - nobjs..len].copy_from_slice(ndom.costs());
            }
        }
        true
    });
    assert!(!pfs.is_empty());
    if pfs.len() <= 1 {
        return problems;
    }
    // Deduplicate non-dominated set
    let mut idx1 = 0;
    while idx1 < non_dom_set.len() - nobjs {
        let mut idx2 = idx1 + nobjs;
        while idx2 < non_dom_set.len() {
            if non_dom_set[idx1..idx1 + nobjs] == non_dom_set[idx2..idx2 + nobjs] {
                non_dom_set.drain(idx2..idx2 + nobjs);
                continue;
            }
            idx2 += nobjs;
        }
        idx1 += nobjs;
    }
    // Check remaining Pareto fronts against joint non-dominated set
    'solvers: for (sid, pf) in pfs {
        for (ndom_idx, ndom) in pf.iter().enumerate() {
            for idx in (0..non_dom_set.len()).step_by(nobjs) {
                match check_relation(ndom.costs(), &non_dom_set[idx..idx + nobjs]) {
                    Relation::Incomparable => (),
                    Relation::FirstDominates => panic!("should never happen"),
                    Relation::SecondDominates => {
                        problems.push((sid.clone(), Problem::OtherDominated(ndom_idx)));
                        continue 'solvers;
                    }
                    Relation::Equal => (),
                }
            }
        }
    }
    problems
}
