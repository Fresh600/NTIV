use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "ntiv",
    version,
    about = "Navigate Windows at the speed of keys"
)]
pub struct Cli {
    /// Directory to browse, or a file to reveal and preview
    pub path: Option<PathBuf>,
}
