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
        /// Restrict to the named case(s). Repeatable (`--case a --case b`).
        /// Omitted means every matrix binding, unchanged from today.
        #[arg(long = "case")]
        cases: Vec<String>,
    },
    /// Fill blank assistant turns by calling the model.
    Run {
        /// Conversation file or run directory.
        target: PathBuf,
        /// Restrict to the named case(s). Repeatable (`--case a --case b`).
        /// Omitted means every resolved conversation, unchanged from today.
        #[arg(long = "case")]
        cases: Vec<String>,
    },
    /// Score conversations against an evaluation suite.
    Eval {
        /// Suite name (resolves to `<project>/evals/<suite>.yaml`).
        suite: String,
        /// Conversation file or run directory to evaluate.
        #[arg(long)]
        over: PathBuf,
        /// Restrict to the named case(s). Repeatable (`--case a --case b`).
        /// Omitted means every conversation and suite case, unchanged from
        /// today.
        #[arg(long = "case")]
        cases: Vec<String>,
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
        Command::Assemble { name, cases } => {
            match assemble_run(AssembleArgs {
                project: cli.project,
                name,
                cases,
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
        Command::Run { target, cases } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            match rt.block_on(run_cmd(RunArgs {
                project: cli.project,
                target,
                cases,
            })) {
                Ok(_outcome) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Eval { suite, over, cases } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            match rt.block_on(eval_run(EvalCmdArgs {
                project: cli.project,
                suite,
                over,
                cases,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_case_flag_parses_into_a_vec_for_each_command() {
        let assemble = Cli::parse_from([
            "ailly",
            "assemble",
            "invocation",
            "--case",
            "a",
            "--case",
            "b",
        ]);
        match assemble.command {
            Command::Assemble { name, cases } => {
                assert_eq!(name, "invocation");
                assert_eq!(cases, vec![String::from("a"), String::from("b")]);
            }
            other => panic!("expected Assemble, got {other:?}"),
        }

        let run = Cli::parse_from(["ailly", "run", "runs/id", "--case", "a", "--case", "b"]);
        match run.command {
            Command::Run { target, cases } => {
                assert_eq!(target, PathBuf::from("runs/id"));
                assert_eq!(cases, vec![String::from("a"), String::from("b")]);
            }
            other => panic!("expected Run, got {other:?}"),
        }

        let eval = Cli::parse_from([
            "ailly",
            "eval",
            "invocation",
            "--over",
            "runs/id",
            "--case",
            "a",
            "--case",
            "b",
        ]);
        match eval.command {
            Command::Eval { suite, over, cases } => {
                assert_eq!(suite, "invocation");
                assert_eq!(over, PathBuf::from("runs/id"));
                assert_eq!(cases, vec![String::from("a"), String::from("b")]);
            }
            other => panic!("expected Eval, got {other:?}"),
        }
    }

    #[test]
    fn omitting_case_parses_to_an_empty_vec() {
        let assemble = Cli::parse_from(["ailly", "assemble", "invocation"]);
        match assemble.command {
            Command::Assemble { cases, .. } => assert!(cases.is_empty()),
            other => panic!("expected Assemble, got {other:?}"),
        }
    }

    #[test]
    fn case_flag_combined_with_other_flags_does_not_disturb_their_parsing() {
        let eval = Cli::parse_from([
            "ailly",
            "eval",
            "invocation",
            "--case",
            "x",
            "--over",
            "runs/id",
        ]);
        match eval.command {
            Command::Eval { suite, over, cases } => {
                assert_eq!(suite, "invocation");
                assert_eq!(over, PathBuf::from("runs/id"));
                assert_eq!(cases, vec![String::from("x")]);
            }
            other => panic!("expected Eval, got {other:?}"),
        }
    }
}
