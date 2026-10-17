mod args;
mod commands;
mod doctor;
mod init;
mod inventory;
mod output;
mod render;

use clap::Parser;

pub fn main() {
    let args = args::Args::parse();
    let result = match args.command {
        Some(command) => commands::run(args::Cli::new(args.global, command)),
        None => commands::bare(args.global),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("parcela: {error:#}");
            std::process::exit(1);
        }
    }
}
