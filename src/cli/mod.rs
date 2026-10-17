mod args;
mod commands;
mod output;
mod render;

use clap::Parser;

pub fn main() {
    match commands::run(args::Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("parcela: {error:#}");
            std::process::exit(1);
        }
    }
}
