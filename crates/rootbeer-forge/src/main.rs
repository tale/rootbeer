mod command;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "rootbeer-forge",
    version,
    about = "Build and publish Rootbeer packages"
)]
struct Cli {
    #[command(flatten)]
    args: command::Args,
}

fn main() {
    command::run(Cli::parse().args);
}
