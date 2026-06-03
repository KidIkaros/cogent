#![deny(clippy::all)]

use clap::Parser;

mod audit;
mod check_runners;
mod checks_cmd;
mod cli;
mod commands;
mod config;
mod diff;
mod dispatcher;
mod doctor;
mod history;
mod hooks;
mod progress;
mod report;
mod report_formatters;
mod serve;
mod types;
mod watch;

use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let exit_code = dispatcher::dispatch(cli.command);
    std::process::exit(exit_code);
}
