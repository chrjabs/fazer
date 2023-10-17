//! # Command Line Interface

use std::{fs, io::Write, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use termcolor::{BufferWriter, Color, ColorSpec, WriteColor};

use crate::config::{Config, FullConfig, InstConfig};

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
        inst: InstArgs,
        #[command(flatten)]
        config: ConfigArgs,
    },
    /// Fuzz a set of solvers
    Fuzz {
        /// The number of worker threads
        #[arg(short = 'j', long)]
        workers: Option<u8>,
        #[command(flatten)]
        inst: InstArgs,
        #[command(flatten)]
        config: ConfigArgs,
    },
}

#[derive(Args)]
struct InstArgs {
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
struct ConfigArgs {
    /// The fuzzer `.toml` configuration file. For generating instances, the
    /// `execution` and `solvers` sections are optional.
    config_path: PathBuf,
}

pub struct Cli {
    stdout: BufferWriter,
    stderr: BufferWriter,
}

pub enum Exec {
    Generate(InstConfig),
    Fuzz(FullConfig),
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
            let (Command::Generate { inst, config } | Command::Fuzz { inst, config, .. }) =
                &args.command;

            cli.info(&format!("loading config from {:?}", config.config_path));
            let mut config: Config = toml::from_str(
                &fs::read_to_string(config.config_path.clone())
                    .unwrap_or_else(panic_with_err!(cli)),
            )
            .unwrap_or_else(panic_with_err!(cli));
            if let Some(val) = inst.min_objs {
                config.instances.set_min_objs(val);
            }
            if let Some(val) = inst.max_objs {
                config.instances.set_max_objs(val);
            }
            if let Some(val) = inst.min_layers {
                config.instances.set_min_layers(val);
            }
            if let Some(val) = inst.max_layers {
                config.instances.set_max_layers(val);
            }
            config
        };
        let exec = match args.command {
            Command::Generate { .. } => {
                let mut config = config.instances;
                if let Some(val) = args.seed {
                    config.seed = Some(val);
                }
                Exec::Generate(config)
            }
            Command::Fuzz { workers, .. } => {
                let mut config: FullConfig = config.try_into().unwrap_or_else(panic_with_err!(cli));
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
}
