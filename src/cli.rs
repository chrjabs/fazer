//! # Command Line Interface

use std::{fmt, fs, io::Write, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use rustsat::{
    instances::{fio::opb, MultiOptInstance},
    types::RsHashSet,
};
use termcolor::{BufferWriter, Color, ColorSpec, WriteColor};

use crate::{
    config::{Config, EvalConfig, FuzzConfig, InstConfig},
    Problem,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    #[arg(short, long)]
    seed: Option<u64>,
    #[command(flatten)]
    color: concolor_clap::Color,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a random instance
    #[clap(visible_alias = "gen")]
    Generate {
        #[command(flatten)]
        inst: InstGenArgs,
        #[command(flatten)]
        config: ConfigArgs,
    },
    /// Minimize a faulty instance
    #[clap(visible_alias = "min")]
    Minimize {
        #[command(flatten)]
        solvers: SolverArgs,
        #[command(flatten)]
        config: ConfigArgs,
        #[command(flatten)]
        inst: InstArgs,
    },
    /// Evaluates a set of solvers on an instance
    #[clap(visible_alias = "eval")]
    Evaluate {
        #[command(flatten)]
        solvers: SolverArgs,
        #[command(flatten)]
        config: ConfigArgs,
        #[command(flatten)]
        inst: InstArgs,
    },
    /// Fuzz a set of solvers
    Fuzz {
        /// The number of worker threads
        #[arg(short = 'j', long)]
        workers: Option<u8>,
        #[command(flatten)]
        inst: InstGenArgs,
        #[command(flatten)]
        solvers: SolverArgs,
        #[command(flatten)]
        config: ConfigArgs,
    },
}

#[derive(Args)]
struct InstGenArgs {
    /// The minimum number of objectives in the generated instance(s)
    #[arg(long)]
    min_objs: Option<u8>,
    /// The maximum number of objectives in the generated instance(s)
    #[arg(long)]
    max_objs: Option<u8>,
    /// The minimum number of layers in the generated instance(s)
    #[arg(long)]
    min_layers: Option<u8>,
    /// The maximum number of layers in the generated instance(s)
    #[arg(long)]
    max_layers: Option<u8>,
}

#[derive(Args)]
struct SolverArgs {
    /// The solvers (by name from the configuration) to run. Using all if none
    /// are given.
    #[arg(short = 's', long)]
    solver: Vec<String>,
}

#[derive(Args)]
struct ConfigArgs {
    /// The fuzzer `.toml` configuration file. For generating instances, the
    /// `execution` and `solvers` sections are optional.
    config_path: PathBuf,
}

#[derive(Args)]
struct InstArgs {
    /// The file format of the input file. With infer, the file format is
    /// inferred from the file extension. Note that generated files will always
    /// be in MCNF format.
    #[arg(long, value_enum, default_value_t = FileFormat::Infer)]
    file_format: FileFormat,
    /// The index in the OPB file to treat as the lowest variable
    #[arg(long, default_value_t = 0)]
    first_var_idx: u32,
    /// The path to the instance file to load. Compressed files with an
    /// extension like `.bz2` or `.gz` can be read.
    instance: PathBuf,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum FileFormat {
    /// Infer the file format from the file extension. `.mcnf`, `.bicnf`,
    /// `.cnf`, `.wcnf` or `.dimacs` are all interpreted as DIMACS files and
    /// `.opb` as an OPB file. All file extensions can also be prepended with
    /// `.bz2` or `.gz` if compression is used.
    Infer,
    /// A DIMACS MCNF file
    Dimacs,
    /// A multi-objective OPB file
    Opb,
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileFormat::Infer => write!(f, "infer"),
            FileFormat::Dimacs => write!(f, "dimacs"),
            FileFormat::Opb => write!(f, "opb"),
        }
    }
}

pub struct Cli {
    stdout: BufferWriter,
    stderr: BufferWriter,
}

pub enum Exec {
    Generate(InstConfig),
    Fuzz(FuzzConfig),
    Evaluate(EvalConfig, MultiOptInstance),
}

#[macro_export]
macro_rules! panic_with_err {
    ($cli:expr) => {
        |e| {
            $cli.error(&format!("{}", e));
            panic!("{}", e)
        }
    };
}

macro_rules! is_one_of {
    ($a:expr, $($b:expr),*) => {
        $( $a == $b || )* false
    }
}

impl Cli {
    pub fn init() -> (Self, Exec) {
        let args = CliArgs::parse();
        let cli = Self {
            stdout: BufferWriter::stdout(match args.color.color {
                concolor_clap::ColorChoice::Always => termcolor::ColorChoice::Always,
                concolor_clap::ColorChoice::Never => termcolor::ColorChoice::Never,
                concolor_clap::ColorChoice::Auto => {
                    if atty::is(atty::Stream::Stdout) {
                        termcolor::ColorChoice::Auto
                    } else {
                        termcolor::ColorChoice::Never
                    }
                }
            }),
            stderr: BufferWriter::stderr(match args.color.color {
                concolor_clap::ColorChoice::Always => termcolor::ColorChoice::Always,
                concolor_clap::ColorChoice::Never => termcolor::ColorChoice::Never,
                concolor_clap::ColorChoice::Auto => {
                    if atty::is(atty::Stream::Stderr) {
                        termcolor::ColorChoice::Auto
                    } else {
                        termcolor::ColorChoice::Never
                    }
                }
            }),
        };
        let config = {
            let (Command::Generate { config, .. }
            | Command::Fuzz { config, .. }
            | Command::Minimize { config, .. }
            | Command::Evaluate { config, .. }) = &args.command;

            cli.info(&format!("loading config from {:?}", config.config_path));
            let mut config: Config = toml::from_str(
                &fs::read_to_string(config.config_path.clone())
                    .unwrap_or_else(panic_with_err!(cli)),
            )
            .unwrap_or_else(panic_with_err!(cli));

            if let Command::Generate { inst, .. } | Command::Fuzz { inst, .. } = &args.command {
                if let Some(inst_config) = config.instances.as_mut() {
                    if let Some(val) = inst.min_objs {
                        inst_config.set_min_objs(val);
                    }
                    if let Some(val) = inst.max_objs {
                        inst_config.set_max_objs(val);
                    }
                    if let Some(val) = inst.min_layers {
                        inst_config.set_min_layers(val);
                    }
                    if let Some(val) = inst.max_layers {
                        inst_config.set_max_layers(val);
                    }
                }
            }

            if let Command::Minimize { solvers, .. }
            | Command::Evaluate { solvers, .. }
            | Command::Fuzz { solvers, .. } = &args.command
            {
                if let Some(solver_config) = config.solvers.as_mut() {
                    if !solvers.solver.is_empty() {
                        solvers.solver.iter().for_each(|s| {
                            if !solver_config.contains_key(s) {
                                panic_with_err!(cli)(format!(
                                    "solver {} not found in solver config",
                                    s
                                ))
                            }
                        });
                        let solvers = RsHashSet::from_iter(solvers.solver.clone());
                        solver_config.retain(|s, _| solvers.contains(s));
                    }
                }
            };

            config
        };
        let inst = if let Command::Minimize { inst, .. } | Command::Evaluate { inst, .. } =
            &args.command
        {
            let opb_opts = opb::Options {
                first_var_idx: inst.first_var_idx,
                no_negated_lits: false,
            };
            Some(
                match inst.file_format {
                    FileFormat::Infer => {
                        if let Some(ext) = inst.instance.extension() {
                            let path_without_compr = inst.instance.with_extension("");
                            let ext = if is_one_of!(ext, "gz", "bz2", "xz") {
                                // Strip compression extension
                                match path_without_compr.extension() {
                                    Some(ext) => ext,
                                    None => panic_with_err!(cli)("no file extension"),
                                }
                            } else {
                                ext
                            };
                            if is_one_of!(ext, "mcnf", "bicnf", "wcnf", "cnf", "dimacs") {
                                MultiOptInstance::from_dimacs_path(inst.instance.clone())
                            } else if is_one_of!(ext, "opb") {
                                MultiOptInstance::from_opb_path(inst.instance.clone(), opb_opts)
                            } else {
                                panic_with_err!(cli)(format!("unknown file extension: {:?}", ext))
                            }
                        } else {
                            panic_with_err!(cli)("no file extension")
                        }
                    }
                    FileFormat::Dimacs => MultiOptInstance::from_dimacs_path(inst.instance.clone()),
                    FileFormat::Opb => {
                        MultiOptInstance::from_opb_path(inst.instance.clone(), opb_opts)
                    }
                }
                .unwrap_or_else(panic_with_err!(cli)),
            )
        } else {
            None
        };
        let exec = match args.command {
            Command::Generate { .. } => {
                let mut config: InstConfig = config.try_into().unwrap_or_else(panic_with_err!(cli));
                if let Some(val) = args.seed {
                    config.seed = Some(val);
                }
                Exec::Generate(config)
            }
            Command::Minimize { .. } => todo!(),
            Command::Evaluate { .. } => {
                let config: EvalConfig = config.try_into().unwrap_or_else(panic_with_err!(cli));
                let inst = inst.unwrap();
                Exec::Evaluate(config, inst)
            }
            Command::Fuzz { workers, .. } => {
                let mut config: FuzzConfig = config.try_into().unwrap_or_else(panic_with_err!(cli));
                if let Some(val) = workers {
                    config.execution.n_workers = val;
                }
                Exec::Fuzz(config)
            }
        };
        (cli, exec)
    }

    pub fn warning(&self, msg: &str) {
        let mut buffer = self.stderr.buffer();
        buffer
            .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Yellow)))
            .unwrap();
        write!(buffer, "warning").unwrap();
        buffer.reset().unwrap();
        buffer.set_color(ColorSpec::new().set_bold(true)).unwrap();
        write!(buffer, ": ").unwrap();
        buffer.reset().unwrap();
        writeln!(buffer, "{}", msg).unwrap();
        self.stderr.print(&buffer).expect("cannot write to stderr");
    }

    pub fn error(&self, msg: &str) {
        let mut buffer = self.stderr.buffer();
        buffer
            .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Red)))
            .unwrap();
        write!(buffer, "error").unwrap();
        buffer.reset().unwrap();
        buffer.set_color(ColorSpec::new().set_bold(true)).unwrap();
        write!(buffer, ": ").unwrap();
        buffer.reset().unwrap();
        writeln!(buffer, "{}", msg).unwrap();
        self.stderr.print(&buffer).expect("cannot write to stderr");
    }

    pub fn info(&self, msg: &str) {
        let mut buffer = self.stderr.buffer();
        buffer
            .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Blue)))
            .unwrap();
        write!(buffer, "info").unwrap();
        buffer.reset().unwrap();
        buffer.set_color(ColorSpec::new().set_bold(true)).unwrap();
        write!(buffer, ": ").unwrap();
        buffer.reset().unwrap();
        writeln!(buffer, "{}", msg).unwrap();
        self.stderr.print(&buffer).expect("cannot write to stdout");
    }

    pub fn print_problems(&self, problems: &[(String, Problem)]) {
        let (slen, plen) = problems.iter().fold((6, 0), |(slen, plen), (sid, prob)| {
            (
                std::cmp::max(slen, sid.len()),
                std::cmp::max(plen, format!("{}", prob).len()),
            )
        });
        let mut buffer = self.stderr.buffer();
        buffer
            .set_color(ColorSpec::new().set_bold(true).set_dimmed(true))
            .unwrap();
        for _ in 0..slen + plen + 1 {
            write!(buffer, "-").unwrap();
        }
        writeln!(buffer, "").unwrap();
        buffer.reset().unwrap();
        write!(buffer, "Solver").unwrap();
        for _ in 6..slen + 1 {
            write!(buffer, " ").unwrap();
        }
        writeln!(buffer, "Problem").unwrap();
        buffer.set_color(ColorSpec::new().set_dimmed(true)).unwrap();
        for _ in 0..slen + plen + 1 {
            write!(buffer, "-").unwrap();
        }
        writeln!(buffer, "").unwrap();
        buffer.reset().unwrap();
        for (sid, prob) in problems {
            write!(buffer, "{}", sid).unwrap();
            for _ in sid.len()..slen + 1 {
                write!(buffer, " ").unwrap();
            }
            writeln!(buffer, "{}", prob).unwrap();
        }
        buffer
            .set_color(ColorSpec::new().set_bold(true).set_dimmed(true))
            .unwrap();
        for _ in 0..slen + plen + 1 {
            write!(buffer, "-").unwrap();
        }
        writeln!(buffer, "").unwrap();
        self.stderr.print(&buffer).expect("cannot write to stderr");
    }
}
