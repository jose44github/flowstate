use std::path::PathBuf;

use clap::Parser;

use debateprocessor::{
  file_search::{default_global_search_root, run_db8_search_cli},
  run_standalone, write_demo_document,
};

/// Command line arguments for the standalone rich text processor.
///
/// `clap`'s derive API turns this struct into a parser: it generates
/// `--help`/`-h`, validates input, and fills in defaults for us. The full
/// editor can use the library directly without going through this CLI.
#[derive(Parser)]
#[command(name = "debateprocessor", about = "A rich-text editor for debate documents.")]
struct Cli {
  /// Optional path to the `.db8` document to open.
  #[arg(value_name = "PATH")]
  path: Option<PathBuf>,

  /// Write a freshly generated demo document to `data/demo.db8` and exit.
  /// Mutually exclusive with providing a `PATH`.
  #[arg(long, conflicts_with = "path")]
  write_demo_db8: bool,

  /// Run the temporary terminal-only global `.db8` file search.
  #[arg(long, conflicts_with_all = ["path", "write_demo_db8"])]
  test_db8_search: bool,

  /// Root directory for `--test-db8-search`. Defaults to the user's home directory.
  #[arg(long, value_name = "DIR", requires = "test_db8_search")]
  search_root: Option<PathBuf>,
}

fn main() {
  let cli = Cli::parse();

  if cli.write_demo_db8 {
    write_demo_document().expect("failed to write data/demo.db8");
    return;
  }

  if cli.test_db8_search {
    match run_db8_search_cli(cli.search_root.unwrap_or_else(default_global_search_root)) {
      Ok(Some(path)) => run_standalone(Some(path)),
      Ok(None) => {},
      Err(error) => {
        eprintln!("file search failed: {error}");
        std::process::exit(1);
      },
    }
    return;
  }

  run_standalone(cli.path);
}
