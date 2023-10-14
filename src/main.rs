//! # Fazer - Multi-Objective MaxSAT Fuzzing

use std::io;

use cli::{Cli, Exec};
use generate::MoGenerator;
use rustsat::instances::fio::dimacs;

mod cli;
mod config;
mod generate;

fn main() {
    let (cli, exec) = Cli::init();

    match exec {
        Exec::Generate(config) => dimacs::write_mcnf(&mut io::stdout(), MoGenerator::new(config))
            .unwrap_or_else(panic_with_err!(&cli)),
        Exec::Fuzz(_) => todo!(),
    }
}
