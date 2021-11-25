use std::path::PathBuf;

use anyhow::{Context, Result};
use structopt::StructOpt;

mod input;
mod output;

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opts {
    /// The path to your top level plan file
    #[structopt(parse(from_os_str))]
    plan_file: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opts::from_args();

    let config = input::read_configs(&opt.plan_file).context("Failed to load configs")?;
    println!("{:#?}", config);

    let (range, mut model) = config
        .build_model()
        .context("Failed to build model from configs")?;
    println!("{:#?}", model);
    let out = model.run(range);
    println!("{:#?}", out);
    Ok(())
}
