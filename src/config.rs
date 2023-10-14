//! # Fuzzer Configuration

use std::{collections::HashMap, ops::RangeInclusive};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub execution: Option<ExecConfig>,
    pub instances: InstConfig,
    pub solvers: Option<HashMap<String, SolverConfig>>,
}

pub struct FullConfig {
    pub execution: ExecConfig,
    pub instances: InstConfig,
    pub solvers: HashMap<String, SolverConfig>,
}

impl TryFrom<Config> for FullConfig {
    type Error = &'static str;

    fn try_from(value: Config) -> Result<Self, Self::Error> {
        if value.execution.is_none() {
            return Err("missing execution block in config");
        }
        if value.solvers.is_none() {
            return Err("missing solvers block in config");
        }
        Ok(FullConfig {
            execution: value.execution.unwrap(),
            instances: value.instances,
            solvers: value.solvers.unwrap(),
        })
    }
}

#[derive(Deserialize)]
pub struct ExecConfig {
    pub n_workers: u8,
}

#[derive(Deserialize)]
pub struct InstConfig {
    pub seed: Option<u64>,
    pub min_objs: u8,
    pub max_objs: u8,
    pub min_layers: u8,
    pub max_layers: u8,
    pub layer_type: LayerType,
}

impl InstConfig {
    pub fn objs(&self) -> RangeInclusive<u8> {
        self.min_objs.into()..=self.max_objs.into()
    }
    pub fn layers(&self) -> RangeInclusive<usize> {
        self.min_layers.into()..=self.max_layers.into()
    }
}

#[derive(Deserialize)]
pub enum LayerType {
    Tiny,
    Small,
    Regular,
}

#[derive(Deserialize)]
pub enum SolverConfig {
    Scuttle(ScuttleConfig),
}

#[derive(Deserialize)]
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
