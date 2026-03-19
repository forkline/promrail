use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "error"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Trace => write!(f, "trace"),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "prl")]
#[command(about = "Git-native GitOps promotion tool", long_about = None)]
#[command(version)]
#[command(subcommand_required = false)]
#[command(subcommand_precedence_over_arg = false)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, global = true, env = "PROMRAIL_CONFIG")]
    pub config: Option<String>,

    #[arg(short, long, global = true, env = "PROMRAIL_REPO")]
    pub repo: Option<String>,

    #[arg(short, long, global = true, value_enum, default_value = "info")]
    pub log_level: LogLevel,

    // Promote args (used when no subcommand)
    #[arg(
        short = 's',
        long = "source",
        global = true,
        help = "Source environment (uses default_sources if not set)"
    )]
    pub source_vec: Vec<String>,

    #[arg(
        short = 'd',
        long = "dest",
        global = true,
        help = "Destination environment (uses default_dest if not set)"
    )]
    pub dest: Option<String>,

    #[arg(name = "filter", global = true)]
    pub filter_vec: Vec<String>,

    #[arg(long, global = true, help = "Do not delete extra files in destination")]
    pub no_delete: bool,

    #[arg(long, global = true)]
    pub dest_based: bool,

    #[arg(long, global = true)]
    pub dry_run: bool,

    #[arg(
        long,
        global = true,
        help = "Ask for confirmation before applying changes"
    )]
    pub confirm: bool,

    #[arg(long, global = true)]
    pub diff: bool,

    #[arg(long, global = true)]
    pub include_protected: bool,

    #[arg(
        long,
        global = true,
        help = "Allow promotion even with uncommitted changes"
    )]
    pub force: bool,

    #[arg(
        long,
        global = true,
        help = "Allow duplicate files across sources (default: error on duplicates)"
    )]
    pub allow_duplicates: bool,

    #[arg(
        long,
        global = true,
        help = "Only update components that already exist in destination"
    )]
    pub only_existing: bool,

    #[arg(
        long,
        global = true,
        help = "Include files matching .gitignore patterns"
    )]
    pub include_gitignored: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    #[command(about = "Show what would change without applying")]
    Diff {},

    #[command(about = "Copy allowlisted files from source to destination (default)")]
    Promote {},

    #[command(about = "Validate configuration file")]
    Validate {},

    #[command(about = "Version extraction and management")]
    Versions {
        #[command(subcommand)]
        command: VersionsCommands,
    },

    #[command(about = "Snapshot management")]
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommands,
    },

    #[command(about = "Configuration comparison")]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum VersionsCommands {
    #[command(about = "Extract versions from a repository path")]
    Extract {
        #[arg(short = 'p', long)]
        path: String,

        #[arg(short = 'o', long)]
        output: Option<String>,

        #[arg(name = "filter")]
        filter_vec: Vec<String>,
    },

    #[command(about = "Apply versions from a file to a repository")]
    Apply {
        #[arg(short = 'f', long)]
        file: String,

        #[arg(short = 'p', long)]
        path: String,

        #[arg(long, help = "Filter to specific components (comma-separated)")]
        component: Option<String>,

        #[arg(long, help = "Warn on version downgrades")]
        check_conflicts: bool,

        #[arg(long, help = "Create a snapshot before applying")]
        snapshot: bool,

        #[arg(long)]
        dry_run: bool,
    },

    #[command(about = "Compare versions between two repositories")]
    Diff {
        #[arg(short = 's', long)]
        source: String,

        #[arg(short = 'd', long)]
        dest: String,

        #[arg(name = "filter")]
        filter_vec: Vec<String>,
    },

    #[command(about = "Merge versions from multiple sources")]
    Merge {
        #[arg(
            short = 's',
            long,
            help = "Source paths (can be specified multiple times)"
        )]
        source_vec: Vec<String>,

        #[arg(
            short = 'o',
            long,
            help = "Output JSON file path (ignored with --explain)"
        )]
        output: Option<String>,

        #[arg(long, help = "Show human-readable merge summary instead of JSON")]
        explain: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotCommands {
    #[command(about = "List all snapshots")]
    List {
        #[arg(short = 'p', long)]
        path: String,
    },

    #[command(about = "Show snapshot details")]
    Show {
        id: String,

        #[arg(short = 'p', long)]
        path: String,
    },

    #[command(about = "Rollback to a snapshot")]
    Rollback {
        id: String,

        #[arg(short = 'p', long)]
        path: String,
    },

    #[command(about = "Delete a snapshot")]
    Delete {
        id: String,

        #[arg(short = 'p', long)]
        path: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommands {
    #[command(about = "Show configuration reference with all options")]
    Show {},

    #[command(about = "Generate example configuration file")]
    Example {
        #[arg(short, long, help = "Output file path (default: stdout)")]
        output: Option<String>,
    },

    #[command(about = "Compare configuration files between directories")]
    Diff {
        source: String,

        dest: String,

        #[arg(short, long, help = "Filter to specific files (comma-separated)")]
        file: Option<String>,
    },
}
