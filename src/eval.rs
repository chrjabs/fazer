//! # Evaluating An Instance With a Solver

use futures::{
    channel::{mpsc, oneshot},
    executor::{self, ThreadPool},
    StreamExt,
};
use rustsat::{encodings::pb::DynamicPolyWatchdog, instances::MultiOptInstance, types::RsHashMap};
use scuttle::types::ParetoFront;

use crate::{
    config::{ScuttleConfig, SolverConfig},
    Problem, Solver,
};

pub fn evaluate<S: Solver + From<MultiOptInstance>>(
    inst: MultiOptInstance,
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
    pool: Option<ThreadPool>,
) -> Vec<(String, Problem)> {
    let (mut tx_prob, rx_prob) = mpsc::channel::<(String, Problem)>(solvers.len());
    let (mut tx_pf, rx_pf) = mpsc::channel::<(String, ParetoFront)>(solvers.len());

    let fut_problems = async {
        for (sid, sconf) in solvers {
            let sid = sid.clone();
            let sconf = sconf.clone();
            let inst = inst.clone();
            let mut pf_tx = tx_pf.clone();
            let mut prob_tx = tx_prob.clone();
            let fut_tx_result = async move {
                let res = match sconf {
                    SolverConfig::Scuttle(conf) => match conf {
                        ScuttleConfig::PMinimal => evaluate::<crate::scuttle::PMin>(inst),
                        ScuttleConfig::CoreBoostedPMinimal => {
                            evaluate::<crate::scuttle::PMinCoreBoosting>(inst)
                        }
                        ScuttleConfig::BiOptSatGte => evaluate::<crate::scuttle::BiOptSat>(inst),
                        ScuttleConfig::BiOptSatDpw => {
                            evaluate::<crate::scuttle::BiOptSat<DynamicPolyWatchdog>>(inst)
                        }
                        ScuttleConfig::LowerBounding => {
                            evaluate::<crate::scuttle::LowerBounding>(inst)
                        }
                    },
                };
                match res {
                    Ok(pf) => pf_tx
                        .try_send((sid, pf))
                        .expect("failed to send pareto front"),
                    Err(prob) => prob_tx
                        .try_send((sid, prob))
                        .expect("failed to send problem"),
                }
            };
            if let Some(ref pool) = pool {
                pool.spawn_ok(fut_tx_result);
            } else {
                fut_tx_result.await;
            }
        }
        tx_pf.disconnect();

        let nobjs = inst.n_objectives();
        let future_pfs = rx_pf
            .filter(|(sid, pf)| {
                filter_pf(
                    sid.clone(),
                    pf.clone(),
                    inst.clone(),
                    pool.clone(),
                    tx_prob.clone(),
                )
            })
            .collect();
        let pfs: Vec<_> = future_pfs.await;
        compare_pfs(pfs, nobjs, pool, tx_prob);

        let fut_problems = rx_prob.collect();
        fut_problems.await
    };
    executor::block_on(fut_problems)
}

async fn filter_pf(
    sid: String,
    pf: ParetoFront,
    inst: MultiOptInstance,
    pool: Option<ThreadPool>,
    mut tx_prob: mpsc::Sender<(String, Problem)>,
) -> bool {
    let (tx_filt, rx_filt) = oneshot::channel::<bool>();
    let future_prob = async move {
        match check_pf(&pf, &inst) {
            Ok(_) => tx_filt.send(true).expect("failed to send filter"),
            Err(prob) => {
                tx_prob
                    .try_send((sid.clone(), prob))
                    .expect("failed to send problem");
                tx_filt.send(false).expect("failed to send filter");
            }
        }
    };
    if let Some(pool) = pool {
        pool.spawn_ok(future_prob);
    } else {
        future_prob.await;
    }
    rx_filt.await.expect("error receiving filter")
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

fn check_pf(pf: &ParetoFront, inst: &MultiOptInstance) -> Result<(), Problem> {
    // Check solutions
    for (ndom_idx, ndom) in pf.iter().enumerate() {
        if ndom.costs().len() != inst.n_objectives() {
            return Err(Problem::WrongDimension(ndom_idx));
        }
        for (sol_idx, sol) in ndom.iter().enumerate() {
            match inst.cost(sol) {
                Some(cost) => {
                    if &cost != ndom.costs() {
                        return Err(Problem::CostMismatch(ndom_idx, sol_idx));
                    }
                }
                None => {
                    return Err(Problem::UnsatSol(ndom_idx, sol_idx));
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
                    return Err(Problem::SelfDominated(idx2));
                }
                Relation::SecondDominates => {
                    return Err(Problem::SelfDominated(idx1));
                }
                Relation::Equal => return Err(Problem::Repeated(idx1, idx2)),
            }
        }
    }
    Ok(())
}

/// Assumes that the Pareto fronts have already been individually checked
async fn compare_pfs(
    mut pfs: Vec<(String, ParetoFront)>,
    nobjs: usize,
    pool: Option<ThreadPool>,
    mut tx_prob: mpsc::Sender<(String, Problem)>,
) {
    // Check lengths
    let max_pf_len = pfs
        .iter()
        .fold(0, |max, (_, pf)| std::cmp::max(max, pf.len()));
    pfs.retain(|(sid, pf)| {
        if pf.len() == max_pf_len {
            return true;
        }
        tx_prob
            .try_send((sid.clone(), Problem::Short))
            .expect("failed to send problem");
        false
    });
    if pfs.len() <= 1 || pfs[0].1.is_empty() {
        return;
    }
    // Build joint non-dominated set and compare
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
                        tx_prob
                            .try_send((sid.clone(), Problem::OtherDominated(ndom_idx)))
                            .expect("failed to send problem");
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
        return;
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
        let mut prob_tx = tx_prob.clone();
        let non_dom_set = non_dom_set.clone();
        let future_prob = async move {
            for (ndom_idx, ndom) in pf.iter().enumerate() {
                for idx in (0..non_dom_set.len()).step_by(nobjs) {
                    match check_relation(ndom.costs(), &non_dom_set[idx..idx + nobjs]) {
                        Relation::Incomparable => (),
                        Relation::FirstDominates => panic!("should never happen"),
                        Relation::SecondDominates => {
                            prob_tx
                                .try_send((sid, Problem::OtherDominated(ndom_idx)))
                                .expect("failed to send problem");
                            return;
                        }
                        Relation::Equal => (),
                    }
                }
            }
        };
        if let Some(ref pool) = pool {
            pool.spawn_ok(future_prob);
        } else {
            future_prob.await;
        }
    }
}
