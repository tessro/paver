use anyhow::Result;
use clap::Parser;
use paver::cli::{Cli, Command, ConfigCommand, DocType, PromptOutputFormat};
use paver::commands::check::{self, CheckArgs};
use paver::commands::config;
use paver::commands::index;
use paver::commands::init;
use paver::commands::new::{self, NewArgs};
use paver::commands::prompt::{OutputFormat, PromptOptions, generate_prompt};
use paver::templates::TemplateType;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init(args) => {
            init::run(init::InitArgs {
                docs_root: args.docs_root,
                hooks: args.hooks,
                force: args.force,
                working_dir: None,
            })?;
        }
        Command::Check {
            paths,
            format,
            strict,
        } => {
            check::execute(CheckArgs {
                paths,
                format,
                strict,
            })?;
        }
        Command::New {
            doc_type,
            name,
            output,
        } => {
            new::execute(NewArgs {
                doc_type: doc_type.into(),
                name,
                output,
            })?;
        }
        Command::Prompt {
            doc_type,
            name,
            update,
            context,
            output,
        } => {
            let options = PromptOptions {
                doc_type: match doc_type {
                    DocType::Component => TemplateType::Component,
                    DocType::Runbook => TemplateType::Runbook,
                    DocType::Adr => TemplateType::Adr,
                },
                name,
                update_path: update.map(|p| p.to_string_lossy().to_string()),
                context_paths: context
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                output_format: match output {
                    PromptOutputFormat::Text => OutputFormat::Text,
                    PromptOutputFormat::Json => OutputFormat::Json,
                },
            };

            let prompt = generate_prompt(&options)?;
            print!("{}", prompt);
        }
        Command::Hooks => {
            println!("paver hooks: not yet implemented");
        }
        Command::Config(cmd) => match cmd {
            ConfigCommand::Get { key } => {
                config::get(&key)?;
            }
            ConfigCommand::Set { key, value } => {
                config::set(&key, &value)?;
            }
            ConfigCommand::List => {
                config::list()?;
            }
            ConfigCommand::Path => {
                config::path()?;
            }
        },
        Command::Index { output, update } => {
            index::run(&output, update)?;
        }
    }

    Ok(())
}
