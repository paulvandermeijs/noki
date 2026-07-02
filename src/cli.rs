use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

/// Capture and browse version-controlled notes.
#[derive(Parser, Debug)]
#[command(name = "noki", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Skip the editor and store piped input directly
    #[arg(short = 'n', long)]
    pub no_edit: bool,

    /// The notes repository to use
    #[arg(long, global = true)]
    pub repository: Option<String>,

    #[command(flatten)]
    pub verbose: Verbosity,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List notes
    #[command(visible_alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show a single note by its path
    Show {
        /// The repository-relative path of the note
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Output the unmodified file contents
        #[arg(long)]
        raw: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_ls_alias() {
        let cli = Cli::parse_from(["noki", "ls"]);
        assert!(matches!(cli.command, Some(Commands::List { json: false })));
    }

    #[test]
    fn parses_show_with_flags() {
        let cli = Cli::parse_from(["noki", "show", "a/b.md", "--json"]);
        match cli.command {
            Some(Commands::Show { path, json, raw }) => {
                assert_eq!(path, "a/b.md");
                assert!(json);
                assert!(!raw);
            }
            _ => panic!("expected show"),
        }
    }

    #[test]
    fn default_command_is_none() {
        let cli = Cli::parse_from(["noki", "--no-edit"]);
        assert!(cli.command.is_none());
        assert!(cli.no_edit);
    }
}
