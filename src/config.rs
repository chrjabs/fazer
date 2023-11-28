//! # Fuzzer Configuration

use std::ops::RangeInclusive;

use futures::executor::{ThreadPool, ThreadPoolBuilder};
use rustsat::types::RsHashMap;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub execution: Option<ExecConfig>,
    pub instances: Option<InstConfig>,
    pub solvers: Option<RsHashMap<String, SolverConfig>>,
    pub minimization: Option<MinimizeConfig>,
}

pub struct FuzzConfig {
    pub pool: Option<ThreadPool>,
    pub instances: InstConfig,
    pub solvers: RsHashMap<String, SolverConfig>,
    pub minimization: Option<MinimizeConfig>,
}

impl TryFrom<Config> for FuzzConfig {
    type Error = &'static str;

    fn try_from(value: Config) -> Result<Self, Self::Error> {
        if value.execution.is_none() {
            return Err("missing execution block in config");
        }
        if value.instances.is_none() {
            return Err("missing instances block in config");
        }
        if value.solvers.is_none() {
            return Err("missing solvers block in config");
        }
        if value.instances.is_none() {
            return Err("missing solvers block in config");
        }
        Ok(FuzzConfig {
            pool: value.execution.unwrap().into(),
            instances: value.instances.unwrap(),
            solvers: value.solvers.unwrap(),
            minimization: value.minimization,
        })
    }
}

pub struct EvalConfig {
    pub pool: Option<ThreadPool>,
    pub solvers: RsHashMap<String, SolverConfig>,
}

impl TryFrom<Config> for EvalConfig {
    type Error = &'static str;

    fn try_from(value: Config) -> Result<Self, Self::Error> {
        if value.solvers.is_none() {
            return Err("missing solvers block in config");
        }
        if value.execution.is_none() {
            return Err("missing execution block in config");
        }
        Ok(EvalConfig {
            pool: value.execution.unwrap().into(),
            solvers: value.solvers.unwrap(),
        })
    }
}

#[derive(Deserialize)]
pub struct ExecConfig {
    pub n_workers: u8,
}

impl From<ExecConfig> for Option<ThreadPool> {
    fn from(value: ExecConfig) -> Self {
        if value.n_workers > 1 {
            let mut builder = ThreadPoolBuilder::new();
            builder.pool_size(value.n_workers.into());
            Some(builder.create().expect("error creating thread pool"))
        } else {
            None
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct InstConfig {
    pub seed: Option<u64>,
    pub objectives: U8Range,
    layers: U8Range,
    layer_width: U8RandomMaxRange,
    layer_clauses: U8DivRange,
    equalities: U8ProbRange,
    ands: U8ProbRange,
    xors3: U8ProbRange,
    xors4: U8ProbRange,
    max_weight: Vec<U64Range>,
}

impl InstConfig {
    pub fn objs(&self) -> RangeInclusive<u8> {
        self.objectives.min..=self.objectives.max
    }
    pub fn layers(&self) -> RangeInclusive<usize> {
        self.layers.min.into()..=self.layers.max.into()
    }
    pub fn max_layer_width(&self) -> RangeInclusive<u32> {
        self.layer_width.max.min.into()..=self.layer_width.max.max.into()
    }
    pub fn min_layer_width(&self) -> u32 {
        self.layer_width.min.into()
    }
    pub fn layer_clauses(&self) -> RangeInclusive<u32> {
        self.layer_clauses.min.into()..=self.layer_clauses.max.into()
    }
    pub fn layer_clauses_div(&self) -> u32 {
        self.layer_clauses.div.into()
    }
    pub fn eqs_range(&self) -> RangeInclusive<u32> {
        self.equalities.min.into()..=self.equalities.max.into()
    }
    pub fn eqs_nonzero_prob(&self) -> f64 {
        1. - self.equalities.zero_prob
    }
    pub fn ands_range(&self) -> RangeInclusive<u32> {
        self.ands.min.into()..=self.ands.max.into()
    }
    pub fn ands_nonzero_prob(&self) -> f64 {
        1. - self.ands.zero_prob
    }
    pub fn xors3_range(&self) -> RangeInclusive<u32> {
        self.xors3.min.into()..=self.xors3.max.into()
    }
    pub fn xors3_nonzero_prob(&self) -> f64 {
        1. - self.xors3.zero_prob
    }
    pub fn xors4_range(&self) -> RangeInclusive<u32> {
        self.xors4.min.into()..=self.xors4.max.into()
    }
    pub fn xors4_nonzero_prob(&self) -> f64 {
        1. - self.xors4.zero_prob
    }
    pub fn max_weight_variants(&self) -> usize {
        self.max_weight.len()
    }
    pub fn max_weight(&self, variant: usize) -> RangeInclusive<u64> {
        self.max_weight[variant].min..=self.max_weight[variant].max
    }
    pub fn set_max_objs(&mut self, max_objs: u8) {
        self.objectives.max = max_objs
    }
    pub fn set_min_objs(&mut self, min_objs: u8) {
        self.objectives.min = min_objs
    }
    pub fn set_max_layers(&mut self, max_layers: u8) {
        self.layers.max = max_layers
    }
    pub fn set_min_layers(&mut self, min_layers: u8) {
        self.layers.min = min_layers
    }
}

impl TryFrom<Config> for InstConfig {
    type Error = &'static str;

    fn try_from(value: Config) -> Result<Self, Self::Error> {
        if value.instances.is_none() {
            return Err("missing instances block");
        }
        Ok(value.instances.unwrap())
    }
}

/// A range to draw random values from
#[derive(Deserialize, Clone, Copy)]
pub struct U8Range {
    min: u8,
    max: u8,
}

/// A range to draw random values from
#[derive(Deserialize, Clone, Copy)]
pub struct U64Range {
    min: u64,
    max: u64,
}

/// A range with a random max value
#[derive(Deserialize, Clone, Copy)]
pub struct U8RandomMaxRange {
    min: u8,
    max: U8Range,
}

/// A value that is zero with a certain probability and drawn from a range
/// otherwise
#[derive(Deserialize, Clone, Copy)]
pub struct U8ProbRange {
    zero_prob: f64,
    min: u8,
    max: u8,
}

/// A random value range with a divisor associated with it
#[derive(Deserialize, Clone, Copy)]
pub struct U8DivRange {
    min: u8,
    max: u8,
    div: u8,
}

#[derive(Deserialize)]
pub struct MinimizeConfig {
    pub max_rounds: u8,
    pub min_clauses: Option<bool>,
    pub min_literals: Option<bool>,
    pub min_variables: Option<bool>,
    pub soft_to_hard: Option<bool>,
    pub remove_objectives: Option<bool>,
    pub weight_to_one: Option<bool>,
    pub weight_binary_search: Option<bool>,
    pub shuffle_clauses: Option<bool>,
    pub shuffle_literals: Option<bool>,
    pub rename_variable: Option<bool>,
}

impl TryFrom<Config> for MinimizeConfig {
    type Error = &'static str;

    fn try_from(value: Config) -> Result<Self, Self::Error> {
        if value.minimization.is_none() {
            return Err("missing minimization block");
        }
        Ok(value.minimization.unwrap())
    }
}

#[derive(Deserialize, Clone)]
pub enum SolverConfig {
    Scuttle(ScuttleConfig),
}

#[derive(Deserialize, Clone)]
pub enum ScuttleConfig {
    /// Default p-minimal algorithm
    PMinimal,
    /// Core-boosted p-minimal algorithm
    CoreBoostedPMinimal,
    /// BiOptSat(-SU) with GTE
    BiOptSatGte,
    /// BiOptSat(-SU) with DPW
    BiOptSatDpw,
    /// Lower-bounding algorithm
    LowerBounding,
}
