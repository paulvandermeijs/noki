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

    /// Open or create today's daily note (path from note.daily_filename)
    #[arg(short = 'd', long)]
    pub daily: bool,

    /// Set the note title (overrides the title derived from the content)
    #[arg(short = 't', long)]
    pub title: Option<String>,

    /// Add a label to the note; repeat to add several
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

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
    /// Edit an existing note in your editor
    Edit {
        /// The repository-relative path of the note
        path: String,
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
    fn parses_edit_command() {
        let cli = Cli::parse_from(["noki", "edit", "a/b.md"]);
        match cli.command {
            Some(Commands::Edit { path }) => assert_eq!(path, "a/b.md"),
            _ => panic!("expected edit"),
        }
    }

    #[test]
    fn default_command_is_none() {
        let cli = Cli::parse_from(["noki", "--no-edit"]);
        assert!(cli.command.is_none());
        assert!(cli.no_edit);
    }

    #[test]
    fn parses_title_and_repeated_labels() {
        let cli = Cli::parse_from([
            "noki", "--title", "My title", "--label", "a", "--label", "b",
        ]);
        assert_eq!(cli.title.as_deref(), Some("My title"));
        assert_eq!(cli.labels, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parses_short_title_and_label_flags() {
        let cli = Cli::parse_from(["noki", "-t", "T", "-l", "x", "-l", "y"]);
        assert_eq!(cli.title.as_deref(), Some("T"));
        assert_eq!(cli.labels, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn title_and_labels_default_empty() {
        let cli = Cli::parse_from(["noki"]);
        assert!(cli.title.is_none());
        assert!(cli.labels.is_empty());
    }

    #[test]
    fn parses_daily_flag() {
        let cli = Cli::parse_from(["noki", "--daily"]);
        assert!(cli.daily);
        assert!(cli.command.is_none());
    }

    #[test]
    fn daily_defaults_to_false() {
        let cli = Cli::parse_from(["noki"]);
        assert!(!cli.daily);
    }
}
