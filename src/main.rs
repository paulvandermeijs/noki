use clap::Parser;
use noki::cli::{Cli, Commands};
use noki::{commands, config, vcs};

fn main() {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let code = match run(cli) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let config = config::load(cli.repository)?;
    let backend = vcs::open_backend(&config)?;

    match cli.command {
        Some(Commands::List { json }) => commands::list::run(backend.as_ref(), json),
        Some(Commands::Show { path, json, raw }) => {
            commands::show::run(backend.as_ref(), &path, json, raw)
        }
        None => commands::create::run(backend.as_ref(), &config, cli.no_edit),
    }
}
