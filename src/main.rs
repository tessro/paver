use anyhow::Result;
use clap::Parser;
use pave::cli::{
    AdoptOutputFormat, Cli, Command, ConfigCommand, DocType, HooksCommand, MigrateOutputFormat,
    PromptOutputFormat,
};
use pave::commands::adopt::{self, AdoptArgs};
use pave::commands::build;
use pave::commands::changed::{self, ChangedArgs};
use pave::commands::check::{self, CheckArgs};
use pave::commands::config;
use pave::commands::coverage::{self, CoverageArgs};
use pave::commands::doctor::{self, DoctorArgs};
use pave::commands::hooks;
use pave::commands::index;
use pave::commands::init;
use pave::commands::lint::{self, LintArgs};
use pave::commands::migrate::{self, MigrateArgs};
use pave::commands::new::{self, NewArgs};
use pave::commands::prompt::{OutputFormat, PromptOptions, generate_prompt};
use pave::commands::status::{self, StatusArgs};
use pave::commands::verify::{self, VerifyArgs};
use pave::templates::TemplateType;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Adopt {
            path,
            format,
            suggest_config,
            dry_run,
        } => {
            adopt::execute(AdoptArgs {
                path,
                format: match format {
                    AdoptOutputFormat::Text => adopt::AdoptOutputFormat::Text,
                    AdoptOutputFormat::Json => adopt::AdoptOutputFormat::Json,
                },
                suggest_config,
                dry_run,
            })?;
        }
        Command::Init(args) => {
            init::run(init::InitArgs {
                docs_root: args.docs_root,
                skip_hooks: args.skip_hooks,
                force: args.force,
                working_dir: None,
            })?;
        }
        Command::Check {
            paths,
            format,
            strict,
            gradual,
            changed,
            base,
        } => {
            check::execute(CheckArgs {
                paths,
                format,
                strict,
                gradual,
                changed,
                base,
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
        Command::Hooks(cmd) => match cmd {
            HooksCommand::Install {
                hook,
                force,
                verify,
            } => {
                // Use --verify flag if specified, otherwise check config
                let run_verify = verify
                    || pave::config::PaveConfig::load(pave::config::CONFIG_FILENAME)
                        .map(|c| c.hooks.run_verify)
                        .unwrap_or(false);
                hooks::install(hook, force, run_verify)?;
            }
            HooksCommand::Uninstall { hook } => {
                hooks::uninstall(hook)?;
            }
        },
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
        Command::Changed {
            base,
            format,
            strict,
        } => {
            changed::execute(ChangedArgs {
                base,
                format,
                strict,
            })?;
        }
        Command::Verify {
            paths,
            format,
            report,
            timeout,
            keep_going,
        } => {
            verify::execute(VerifyArgs {
                paths,
                format,
                report,
                timeout,
                keep_going,
            })?;
        }
        Command::Build { output } => {
            build::execute(build::BuildArgs { output })?;
        }
        Command::Coverage {
            path,
            format,
            threshold,
            include,
            exclude,
        } => {
            coverage::execute(CoverageArgs {
                path,
                format,
                threshold,
                include,
                exclude,
            })?;
        }
        Command::Lint {
            paths,
            format,
            fix,
            rules,
            external_links,
        } => {
            lint::execute(LintArgs {
                paths,
                format,
                fix,
                rules,
                external_links,
            })?;
        }
        Command::Doctor { paths, format } => {
            doctor::execute(DoctorArgs { paths, format })?;
        }
        Command::Status {
            paths,
            format,
            changed,
            base,
        } => {
            status::execute(StatusArgs {
                paths,
                format,
                changed,
                base,
            })?;
        }
        Command::Migrate {
            path,
            format,
            dry_run,
            sections,
            interactive,
            backup,
        } => {
            migrate::execute(MigrateArgs {
                path,
                format: match format {
                    MigrateOutputFormat::Text => migrate::MigrateOutputFormat::Text,
                    MigrateOutputFormat::Json => migrate::MigrateOutputFormat::Json,
                },
                dry_run,
                sections,
                interactive,
                backup,
            })?;
        }
    }

    Ok(())
}
