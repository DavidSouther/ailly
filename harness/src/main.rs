use std::path::PathBuf;
use std::process::ExitCode;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;
use ailly_two::cli::report::ReportCmdArgs;
use ailly_two::cli::report::ReportCmdOutcome;
use ailly_two::cli::report::ReportMode;
use ailly_two::cli::report::run as report_run;
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
        #[arg(long)]
        over: PathBuf,
    },
    /// Summarise one eval run, or compare two runs side-by-side.
    Report {
        /// First (or only) run ID. With one ID, produces a single-run summary.
        run_id_a: String,
        /// Second run ID. When provided, produces a two-arm comparison report.
        run_id_b: Option<String>,
        /// Display label for arm A (defaults to "arm-a").
        #[arg(long)]
        label_a: Option<String>,
        /// Display label for arm B (defaults to "arm-b").
        #[arg(long)]
        label_b: Option<String>,
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
                    if outcome.has_failures() {
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
        Command::Report {
            run_id_a,
            run_id_b,
            label_a,
            label_b,
        } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            let mode = if let Some(id_b) = run_id_b {
                ReportMode::Comparison {
                    run_id_a,
                    run_id_b: id_b,
                }
            } else {
                ReportMode::Single { run_id: run_id_a }
            };
            match rt.block_on(report_run(ReportCmdArgs {
                project: cli.project,
                mode,
                label_a,
                label_b,
            })) {
                Ok(outcome) => {
                    match outcome {
                        ReportCmdOutcome::Single(s) => println!("{}", s.report_md.display()),
                        ReportCmdOutcome::Comparison(c) => {
                            println!("{}", c.comparison_json.display());
                        }
                    }
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
