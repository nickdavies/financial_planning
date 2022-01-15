use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use structopt::StructOpt;

use financial_planning_lib::input;

mod output;

#[derive(Debug, StructOpt)]
struct RunOpts {
    /// The path to your top level plan file
    #[structopt(parse(from_os_str))]
    plan_file: PathBuf,

    /// How to display the output of the model
    #[structopt(subcommand)]
    output_format: output::OutputType,

}

#[derive(Debug, StructOpt)]
struct PrintOpts {
    /// The path to your top level plan file
    #[structopt(parse(from_os_str))]
    plan_file: PathBuf,
}

#[cfg(feature = "webserver")]
#[derive(Debug, StructOpt)]
struct WebserverOpts {
    /// The authentication file. This is a toml file with the format of
    /// ```
    /// [users]
    /// <username> = <salt>:sha256(<salt><password>)
    /// ```
    ///
    /// This is NOT meant to be production worthy AT ALL. Don't expose
    /// this to the internet, it is here just as the most basic of auth
    /// so you can host it on a LAN.
    #[structopt(long, parse(from_os_str))]
    auth_file: std::path::PathBuf,

    /// The directory to find all the static assets in
    #[structopt(long, parse(from_os_str))]
    static_dir: std::path::PathBuf,

    /// The port to use. 0 will auto assign
    #[structopt(long, default_value = "8080")]
    port: u16,
}

#[derive(Debug, StructOpt)]
enum Cmd {
    /// Run a model and generate the output
    Run(RunOpts),
    /// Print the loaded/configured model but don't run it
    Print(PrintOpts),
    #[cfg(feature = "webserver")]
    Webserver(WebserverOpts),
}

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opts {
    #[structopt(subcommand)]
    cmd: Cmd,
}


pub struct FsFileLoader {}

impl input::FileLoader for FsFileLoader {
    fn load(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path).context(format!("failed to read path {:?}", path))
    }
}

fn main() -> Result<()> {
    let opt = Opts::from_args();

    match opt.cmd {
        Cmd::Run(cmd_opts) => {
            let config = input::read_configs(
                &cmd_opts.plan_file,
                FsFileLoader{}
            ).context("Failed to load configs")?;

            let (range, mut model) = config
                .build_model()
                .context("Failed to build model from configs")?;
            let out = model.run(range.clone()).context("failed to run model")?;
            cmd_opts
                .output_format
                .output(out, &range)
                .context("failed to display model output")
        }
        Cmd::Print(cmd_opts) => {
            let config = input::read_configs(
                &cmd_opts.plan_file,
                FsFileLoader{}
            ).context("Failed to load configs")?;

            println!("{:#?}", config);
            let (range, model) = config
                .build_model()
                .context("Failed to build model from configs")?;
            println!("{:#?}", model);
            println!("{:#?}", range);
            Ok(())
        }
        #[cfg(feature = "webserver")]
        Cmd::Webserver(args) => financial_planning_web::run_server(args.port, args.auth_file.clone(), args.static_dir.clone())
                .context("Failed to start server")
    }
}
