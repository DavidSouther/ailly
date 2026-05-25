use std::path::PathBuf;
use std::process::ExitCode;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;
use ailly_two::cli::run::RunArgs;
use ailly_two::cli::run::run as run_cmd;
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
    /// Fill blank assistant turns by calling the model.
    Run {
        /// Conversation file or run directory.
        target: PathBuf,
    },
    /// Score conversations against an evaluation suite.
    Eval {
        /// Suite name (resolves to `<project>/evals/<suite>.yaml`).
        suite: String,
        /// Conversation file or run directory to evaluate.
        #[arg(long = "over")]
        over: PathBuf,
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
        Command::Run { target } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            match rt.block_on(run_cmd(RunArgs {
                project: cli.project,
                target,
            })) {
                Ok(_outcome) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Eval { suite, over } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            match rt.block_on(eval_run(EvalCmdArgs {
                project: cli.project,
                suite,
                over,
            })) {
                Ok(outcome) => {
                    println!("{}", outcome.report_path.display());
                    if outcome.assertions_failed + outcome.assertions_malformed > 0 {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
