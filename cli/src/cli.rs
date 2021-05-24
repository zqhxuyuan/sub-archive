use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Cli {
    #[structopt(flatten)]
    pub run: sc_cli::RunCmd,
}
