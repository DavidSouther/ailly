use std::path::PathBuf;
use std::process::ExitCode;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use clap::Parser;
use clap::Subcommand;

#[derive(Parser, Debug)]
#[command(name = "ailly", about = "Context Window Swiss Army Knife")]
struct Cli {
    /// Project root. Defaults to the current working directory.
    #[arg(short = 'p', long = "project", global = true, default_value = ".")]
    project: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Expand an assembly into one conversation file per matrix binding.
    Assemble {
        /// Assembly name (resolves to `<project>/assemblies/<name>.yaml`).
        name: String,
    },
}

#[expect(clippy::print_stdout, reason = "binary entry point")]
#[expect(clippy::print_stderr, reason = "binary entry point")]
fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Assemble { name } => {
            match assemble_run(AssembleArgs {
                project: cli.project,
                name,
            }) {
                Ok(run_dir) => {
                    println!("{}", run_dir.display());
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
