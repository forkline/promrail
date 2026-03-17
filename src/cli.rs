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
#[command(name = "promrail")]
#[command(about = "Git-native GitOps promotion tool", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true, env = "PROMRAIL_CONFIG")]
    pub config: Option<String>,

    #[arg(short, long, global = true, env = "PROMRAIL_REPO")]
    pub repo: Option<String>,

    #[arg(short, long, global = true, value_enum, default_value = "info")]
    pub log_level: LogLevel,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Show what would change without applying")]
    Diff {
        #[arg(short = 's', long)]
        source: String,

        #[arg(short = 'd', long)]
        dest: String,

        #[arg(name = "filter")]
        filter_vec: Vec<String>,

        #[arg(long, help = "Do not delete extra files in destination")]
        no_delete: bool,

        #[arg(long)]
        dest_based: bool,

        #[arg(long)]
        include_protected: bool,
    },

    #[command(about = "Copy allowlisted files from source to destination")]
    Promote {
        #[arg(short = 's', long)]
        source: String,

        #[arg(short = 'd', long)]
        dest: String,

        #[arg(name = "filter")]
        filter_vec: Vec<String>,

        #[arg(long, help = "Do not delete extra files in destination")]
        no_delete: bool,

        #[arg(long)]
        dest_based: bool,

        #[arg(long)]
        dry_run: bool,

        #[arg(short, long)]
        yes: bool,

        #[arg(long)]
        diff: bool,

        #[arg(long)]
        include_protected: bool,
    },

    #[command(about = "Validate configuration file")]
    Validate {},

    #[command(about = "Extract versions from a repository")]
    Versions {
        #[command(subcommand)]
        command: VersionsCommands,
    },
}

#[derive(Debug, Subcommand)]
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
}
