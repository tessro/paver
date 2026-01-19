use anyhow::Result;
use clap::Parser;
use paver::cli::{Cli, Command, ConfigCommand};
use paver::commands::config;
use paver::commands::new::{self, NewArgs};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            println!("paver init: not yet implemented");
        }
        Command::Check => {
            println!("paver check: not yet implemented");
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
        Command::Prompt => {
            println!("paver prompt: not yet implemented");
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
        Command::Index => {
            println!("paver index: not yet implemented");
        }
    }

    Ok(())
}
