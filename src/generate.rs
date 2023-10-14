//! # Generate Random (Multi-Objective) MaxSAT Instances

use std::ops::Range;

use clap::crate_name;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rustsat::{
    clause,
    instances::fio::dimacs,
    types::{Clause, Lit, Var},
};

use crate::config::{InstConfig, LayerType};

const MAX_CL_LEN: u32 = 20;

type Cl = (Option<(u8, usize)>, Clause);

/// Generator for random multi-objective MaxSAT instances
pub struct MoGenerator {
    rng: ChaCha8Rng,
    seed: Option<u64>,
    objs: u8,
    layers: Vec<Layer>,
    arity: Vec<u32>,
    soft: Vec<u8>,
    eqs: u32,
    ands: u32,
    xors3: u32,
    xors4: u32,
    n_soft_left: Vec<u32>,
    weight_range: Range<usize>,
    weight_sum: usize,
    state: LineType,
    next_free_var: Var,
    buffer: Vec<Cl>,
}

impl MoGenerator {
    pub fn new(config: InstConfig) -> Self {
        let mut gen = Self {
            rng: if let Some(seed) = config.seed {
                ChaCha8Rng::seed_from_u64(seed)
            } else {
                ChaCha8Rng::from_entropy()
            },
            seed: config.seed,
            objs: 0,
            layers: vec![],
            arity: vec![],
            soft: vec![],
            eqs: 0,
            ands: 0,
            xors3: 0,
            xors4: 0,
            n_soft_left: vec![],
            weight_range: 0..0,
            weight_sum: 0,
            state: Default::default(),
            next_free_var: Var::new(0),
            buffer: vec![],
        };
        gen.init(config);
        gen
    }

    fn init(&mut self, config: InstConfig) {
        // generate layers
        let max_width = self.rng.gen_range(match config.layer_type {
            LayerType::Tiny => 5..=10,
            LayerType::Small => 10..=20,
            LayerType::Regular => 10..=70,
        });
        self.layers = vec![Layer::default(); self.rng.gen_range(config.layers())];
        for idx in 0..self.layers.len() {
            let width = self.rng.gen_range(match config.layer_type {
                LayerType::Tiny => 5..=max_width,
                _ => 10..=max_width,
            });
            let range = if idx > 0 {
                let first = self.layers[idx - 1].range.end;
                first..first + width + 1
            } else {
                0..width + 1
            };
            let width_plus_last = if idx > 0 {
                width + self.layers[idx - 1].range.end - self.layers[idx - 1].range.start
            } else {
                width
            };
            let n_clauses = (self.rng.gen_range(100..=250) * width_plus_last) / 100;
            let soft = if n_clauses > 4 * width_plus_last {
                self.rng.gen_range(1..=self.objs)
            } else {
                0
            };
            let unused: Vec<Lit> = range
                .clone()
                .flat_map(|idx| (0..=1).map(move |neg| Lit::new(idx, neg > 0)))
                .collect();
            self.layers[idx] = Layer {
                range,
                n_clauses,
                soft,
                unused,
            };
        }
        // generate counts
        if self.rng.gen_bool(1. / 3.) {
            self.eqs = self.rng.gen_range(0..=31);
        }
        if self.rng.gen_bool(1. / 2.) {
            self.ands = self.rng.gen_range(0..=31);
        }
        if self.rng.gen_bool(1. / 4.) {
            self.xors3 = self.rng.gen_range(0..=16);
        }
        if self.rng.gen_bool(1. / 5.) {
            self.xors4 = self.rng.gen_range(0..=12);
        }
        self.objs = self.rng.gen_range(config.objs());
        self.weight_range = match self.rng.gen_range(1..=5) {
            1 => 1..2,
            2 => 1..self.rng.gen_range(3..=33),
            3 => 1..self.rng.gen_range(34..=257),
            4 => 1..self.rng.gen_range(258..=65536),
            _ => {
                if self.rng.gen_bool(1. / 5.) {
                    1..self.rng.gen_range((1 << 32) + 1..(1 << 63))
                } else {
                    1..self.rng.gen_range(65537..=(1 << 32) + 1)
                }
            }
        };
        self.arity = vec![0; self.ands as usize];
        let width_plus_last = if self.layers.len() > 1 {
            self.layers[self.layers.len() - 1].range.end
                - self.layers[self.layers.len() - 1].range.start
                + self.layers[self.layers.len() - 2].range.end
                - self.layers[self.layers.len() - 2].range.start
        } else {
            self.layers[0].range.end - self.layers[0].range.start
        };
        let max_arity = std::cmp::min(MAX_CL_LEN, width_plus_last / 2);
        for arity in &mut self.arity {
            *arity = self.rng.gen_range(2..=max_arity);
        }
        self.soft = vec![0; (self.ands + self.eqs + self.xors3 + self.xors4) as usize];
        if self.objs > 0 {
            let all_soft = self.rng.gen_bool(1. / 10.);
            for s in &mut self.soft {
                if all_soft || self.rng.gen_bool(1. / 5.) {
                    *s = self.rng.gen_range(1..=self.objs);
                }
            }
        }
        self.next_free_var = Var::new(self.layers[self.layers.len() - 1].range.end);
        self.n_soft_left = self.n_softs();
    }

    fn n_clauses(&self) -> u32 {
        let n_cl = self.layers.iter().fold(0, |cnt, l| cnt + l.n_clauses);
        let n_cl = self.arity.iter().fold(n_cl, |cnt, a| cnt + a + 1);
        let mut n_cl = self.soft.iter().fold(n_cl, |cnt, &s| {
            if s > 0 {
                return cnt + 1;
            }
            cnt
        });
        n_cl += 2 * self.eqs;
        n_cl += 4 * self.xors3;
        n_cl += 8 * self.xors4;
        n_cl
    }

    fn n_softs(&self) -> Vec<u32> {
        let mut cnt = vec![0; self.objs as usize];
        self.layers.iter().for_each(|l| {
            if l.soft > 0 {
                cnt[(l.soft - 1) as usize] += 1;
            }
        });
        self.soft.iter().for_each(|&s| {
            if s > 0 {
                cnt[(s - 1) as usize] += 1
            }
        });
        cnt
    }

    /// Draws a new random weight and ensures that the weight sum does not exceed `2^63-1``
    fn weight(&mut self, oidx: u8) -> usize {
        self.n_soft_left[oidx as usize] -= 1;
        let mut weight = self.rng.gen_range(self.weight_range.clone());
        if weight as u64 + self.n_soft_left[oidx as usize] as u64
            >= u64::MAX - self.weight_sum as u64
        {
            // maxed out weight, only unit weight from now on
            weight = (u64::MAX - 1) as usize
                - self.weight_sum
                - self.n_soft_left[oidx as usize] as usize;
            self.weight_range = 1..2;
        }
        weight
    }

    fn header_line(&self, id: u8) -> String {
        match id {
            0 => format!("Generated by {}", crate_name!()),
            1 => {
                if let Some(seed) = self.seed {
                    format!("seed {}", seed)
                } else {
                    format!("seeded by entropy")
                }
            }
            2 => format!("{} objectives", self.objs),
            3 => format!("weight range {:?}", self.weight_range),
            4 => format!("{} clauses", self.n_clauses()),
            5 => format!("{:?} soft clauses", self.n_softs()),
            6 => format!("equalitites {}", self.eqs),
            7 => format!("ands {}", self.ands),
            8 => format!("xors3 {}", self.xors3),
            _ => format!("xors4 {}", self.xors4),
        }
    }

    fn layer_desc(&self, idx: u8) -> String {
        let layer = &self.layers[idx as usize];
        format!(
            "c layer[{}] = {:?} n_cl={}",
            idx, layer.range, layer.n_clauses
        )
    }

    fn layer_clause(&mut self, lidx: u8, cidx: u32) -> Cl {
        debug_assert!((lidx as usize) < self.layers.len());
        debug_assert!(cidx < self.layers[lidx as usize].n_clauses);
        let mut len = 3;
        while len < MAX_CL_LEN
            && len < self.layers[lidx as usize].range.end
            && self.rng.gen_bool(2. / 3.)
        {
            len += 1;
        }
        let layer = &self.layers[lidx as usize];
        let weight = if layer.soft > 0 {
            Some((layer.soft - 1, self.weight(layer.soft - 1)))
        } else {
            None
        };
        let mut mark = vec![];
        let mut cl = Clause::new();
        let mut idx = 0;
        while idx < len {
            let mut l = lidx;
            while l > 0 && self.rng.gen_bool(0.5) {
                l -= 1;
            }
            let layer = &mut self.layers[l as usize];
            let lit = if !layer.unused.is_empty() {
                layer
                    .unused
                    .swap_remove(self.rng.gen_range(0..layer.unused.len()))
            } else {
                Lit::new(
                    self.rng.gen_range(layer.range.clone()),
                    self.rng.gen_bool(0.5),
                )
            };
            if mark.len() <= lit.vidx() {
                mark.resize(lit.vidx() + 1, false);
            }
            if mark[lit.vidx()] {
                continue;
            }
            cl.add(lit);
            mark[lit.vidx()] = true;
            idx += 1;
        }
        (weight, cl)
    }

    fn eq_clauses(&mut self, idx: u32) -> Vec<Cl> {
        debug_assert!(idx < self.eqs);
        let l1 = self.rng.gen_range(0..self.layers.len());
        let l2 = self.rng.gen_range(0..self.layers.len());
        let v1 = self.rng.gen_range(self.layers[l1].range.clone());
        let v2 = self.rng.gen_range(self.layers[l2].range.clone());
        if v1 == v2 {
            return self.eq_clauses(idx);
        }
        let lit1 = Lit::new(v1, self.rng.gen_bool(0.5));
        let lit2 = Lit::new(v2, self.rng.gen_bool(0.5));
        let sidx = idx as usize;
        if self.soft[sidx] > 0 {
            let blit = self.next_free_var.pos_lit();
            self.next_free_var += 1;
            vec![
                (None, clause![blit, lit1, lit2]),
                (None, clause![blit, !lit1, !lit2]),
                (
                    Some((self.soft[sidx] - 1, self.weight(self.soft[sidx] - 1))),
                    clause![!blit],
                ),
            ]
        } else {
            vec![(None, clause![lit1, lit2]), (None, clause![!lit1, !lit2])]
        }
    }

    fn and_clauses(&mut self, idx: u32) -> Vec<Cl> {
        debug_assert!(idx < self.ands);
        let layer = self.rng.gen_range(0..self.layers.len());
        let lhs = Lit::new(
            self.rng.gen_range(self.layers[layer].range.clone()),
            self.rng.gen_bool(0.5),
        );
        let mut mark = vec![false; lhs.vidx() + 1];
        mark[lhs.vidx()] = true;
        let mut cl = clause![lhs];
        let arity = self.arity[idx as usize];
        debug_assert!(arity < MAX_CL_LEN);
        let mut lidx = 0;
        while lidx < arity {
            let layer = self.rng.gen_range(0..self.layers.len());
            let rhs = Lit::new(
                self.rng.gen_range(self.layers[layer].range.clone()),
                self.rng.gen_bool(0.5),
            );
            if rhs.vidx() >= mark.len() {
                mark.resize(rhs.vidx() + 1, false);
            }
            if mark[rhs.vidx()] {
                continue;
            }
            mark[rhs.vidx()] = true;
            cl.add(rhs);
            lidx += 1;
        }
        let sidx = self.eqs as usize + idx as usize;
        if self.soft[sidx] > 0 {
            let blit = self.next_free_var.pos_lit();
            self.next_free_var += 1;
            cl.add(blit);
            let mut cls = vec![(None, cl.clone())];
            cls.extend(cl.drain(1..cl.len()).map(|rhs| (None, clause![!lhs, !rhs])));
            cls.push((
                Some((self.soft[sidx] - 1, self.weight(self.soft[sidx] - 1))),
                clause![!blit],
            ));
            cls
        } else {
            let mut cls = vec![(None, cl.clone())];
            cls.extend(cl.drain(1..).map(|rhs| (None, clause![!lhs, !rhs])));
            cls
        }
    }

    fn xor3_clauses(&mut self, idx: u32) -> Vec<Cl> {
        debug_assert!(idx < self.xors3);
        let mut lits = [Lit::new(0, false); 3];
        for k in 0..3 {
            let layer = self.rng.gen_range(0..self.layers.len());
            lits[k] = Lit::new(
                self.rng.gen_range(self.layers[layer].range.clone()),
                self.rng.gen_bool(0.5),
            );
        }
        let sidx = self.eqs as usize + self.ands as usize + idx as usize;
        if self.soft[sidx] > 0 {
            let blit = self.next_free_var.pos_lit();
            self.next_free_var += 1;
            vec![
                (None, clause![blit, lits[0], lits[1], lits[2]]),
                (None, clause![blit, lits[0], !lits[1], !lits[2]]),
                (None, clause![blit, !lits[0], lits[1], !lits[2]]),
                (None, clause![blit, !lits[0], !lits[1], lits[2]]),
                (
                    Some((self.soft[sidx] - 1, self.weight(self.soft[sidx] - 1))),
                    clause![!blit],
                ),
            ]
        } else {
            vec![
                (None, clause![lits[0], lits[1], lits[2]]),
                (None, clause![lits[0], !lits[1], !lits[2]]),
                (None, clause![!lits[0], lits[1], !lits[2]]),
                (None, clause![!lits[0], !lits[1], lits[2]]),
            ]
        }
    }

    fn xor4_clauses(&mut self, idx: u32) -> Vec<Cl> {
        debug_assert!(idx < self.xors4);
        let mut lits = [Lit::new(0, false); 4];
        for k in 0..4 {
            let layer = self.rng.gen_range(0..self.layers.len());
            lits[k] = Lit::new(
                self.rng.gen_range(self.layers[layer].range.clone()),
                self.rng.gen_bool(0.5),
            );
        }
        let sidx = self.eqs as usize + self.ands as usize + self.xors3 as usize + idx as usize;
        if self.soft[sidx] > 0 {
            let blit = self.next_free_var.pos_lit();
            self.next_free_var += 1;
            vec![
                (None, clause![blit, lits[0], lits[1], lits[2], lits[3]]),
                (None, clause![blit, lits[0], lits[1], !lits[2], !lits[3]]),
                (None, clause![blit, lits[0], !lits[1], lits[2], !lits[3]]),
                (None, clause![blit, lits[0], !lits[1], !lits[2], lits[3]]),
                (None, clause![blit, !lits[0], lits[1], lits[2], !lits[3]]),
                (None, clause![blit, !lits[0], lits[1], !lits[2], lits[3]]),
                (None, clause![blit, !lits[0], !lits[1], lits[2], lits[3]]),
                (None, clause![blit, !lits[0], !lits[1], !lits[2], !lits[3]]),
                (
                    Some((self.soft[sidx] - 1, self.weight(self.soft[sidx] - 1))),
                    clause![!blit],
                ),
            ]
        } else {
            vec![
                (None, clause![lits[0], lits[1], lits[2]]),
                (None, clause![lits[0], !lits[1], !lits[2]]),
                (None, clause![!lits[0], lits[1], !lits[2]]),
                (None, clause![!lits[0], !lits[1], lits[2]]),
            ]
        }
    }
}

impl Iterator for MoGenerator {
    type Item = dimacs::McnfLine;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cl) = self.buffer.pop() {
            return Some(map_clause(cl));
        }
        loop {
            match self.state {
                LineType::Header(id) => {
                    if id > 9 {
                        self.state = LineType::LayerDesc(0);
                        continue;
                    }
                    self.state = LineType::Header(id + 1);
                    return Some(dimacs::McnfLine::Comment(self.header_line(id)));
                }
                LineType::LayerDesc(idx) => {
                    if idx as usize >= self.layers.len() {
                        self.state = LineType::LayerCl(0, 0);
                        continue;
                    }
                    self.state = LineType::LayerDesc(idx + 1);
                    return Some(dimacs::McnfLine::Comment(self.layer_desc(idx)));
                }
                LineType::LayerCl(lidx, cidx) => {
                    if lidx as usize >= self.layers.len() {
                        self.state = LineType::EqCl(0);
                        continue;
                    }
                    if cidx >= self.layers[lidx as usize].n_clauses {
                        self.state = LineType::LayerCl(lidx + 1, 0);
                        continue;
                    }
                    self.state = LineType::LayerCl(lidx, cidx + 1);
                    return Some(map_clause(self.layer_clause(lidx, cidx)));
                }
                LineType::EqCl(idx) => {
                    if idx >= self.eqs {
                        self.state = LineType::AndCl(0);
                        continue;
                    }
                    let mut cls = self.eq_clauses(idx);
                    self.buffer.extend(cls.drain(1..));
                    self.state = LineType::EqCl(idx + 1);
                    return Some(map_clause(cls.pop().unwrap()));
                }
                LineType::AndCl(idx) => {
                    if idx >= self.ands {
                        self.state = LineType::Xor3Cl(0);
                        continue;
                    }
                    let mut cls = self.and_clauses(idx);
                    self.buffer.extend(cls.drain(1..));
                    self.state = LineType::AndCl(idx + 1);
                    return Some(map_clause(cls.pop().unwrap()));
                }
                LineType::Xor3Cl(idx) => {
                    if idx >= self.xors3 {
                        self.state = LineType::Xor4Cl(0);
                        continue;
                    }
                    let mut cls = self.xor3_clauses(idx);
                    self.buffer.extend(cls.drain(1..));
                    self.state = LineType::Xor3Cl(idx + 1);
                    return Some(map_clause(cls.pop().unwrap()));
                }
                LineType::Xor4Cl(idx) => {
                    if idx >= self.xors4 {
                        return None;
                    }
                    let mut cls = self.xor4_clauses(idx);
                    self.buffer.extend(cls.drain(1..));
                    self.state = LineType::Xor4Cl(idx + 1);
                    return Some(map_clause(cls.pop().unwrap()));
                }
            }
        }
    }
}

fn map_clause(clause: Cl) -> dimacs::McnfLine {
    match clause.0 {
        Some((o, w)) => dimacs::McnfLine::Soft(clause.1, w, o as usize),
        None => dimacs::McnfLine::Hard(clause.1),
    }
}

#[derive(Default, Clone)]
struct Layer {
    range: Range<u32>,
    n_clauses: u32,
    soft: u8,
    unused: Vec<Lit>,
}

enum LineType {
    Header(u8),
    LayerDesc(u8),
    LayerCl(u8, u32),
    EqCl(u32),
    AndCl(u32),
    Xor3Cl(u32),
    Xor4Cl(u32),
}

impl Default for LineType {
    fn default() -> Self {
        Self::Header(0)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use rustsat::instances::fio::dimacs;

    use crate::config::{InstConfig, LayerType};

    use super::MoGenerator;

    fn gen(seed: u64) {
        let config = InstConfig {
            seed: Some(seed),
            min_objs: 0,
            max_objs: 2,
            min_layers: 2,
            max_layers: 5,
            layer_type: LayerType::Tiny,
        };
        dimacs::write_mcnf(&mut io::stdout(), MoGenerator::new(config)).unwrap();
    }

    #[test]
    fn gen42() {
        gen(42)
    }

    #[test]
    fn gen100() {
        gen(100)
    }

    #[test]
    fn gen2() {
        gen(2)
    }
}
