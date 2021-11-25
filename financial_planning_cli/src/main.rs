use std::path::PathBuf;

use anyhow::{Context, Result};
use structopt::StructOpt;

mod input;
mod output;

#[derive(Debug, StructOpt)]
struct RunOpts {
    /// How to display the output of the model
    #[structopt(subcommand)]
    output_format: output::OutputType,
}

#[derive(Debug, StructOpt)]
struct PrintOpts {}

#[derive(Debug, StructOpt)]
enum Cmd {
    /// Run a model and generate the output
    Run(RunOpts),
    /// Print the loaded/configured model but don't run it
    Print,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opts {
    /// The path to your top level plan file
    #[structopt(parse(from_os_str))]
    plan_file: PathBuf,

    #[structopt(subcommand)]
    cmd: Cmd,
}

fn main() -> Result<()> {
    let opt = Opts::from_args();

    let config = input::read_configs(&opt.plan_file).context("Failed to load configs")?;

    match opt.cmd {
        Cmd::Run(cmd_opts) => {
            let (range, mut model) = config
                .build_model()
                .context("Failed to build model from configs")?;
            let out = model.run(range).context("failed to run model")?;
            cmd_opts
                .output_format
                .output(out)
                .context("failed to display model output")
        }
        Cmd::Print => {
            println!("{:#?}", config);
            let (range, model) = config
                .build_model()
                .context("Failed to build model from configs")?;
            println!("{:#?}", model);
            println!("{:#?}", range);
            Ok(())
        }
    }
}
