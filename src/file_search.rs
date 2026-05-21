use std::{
  io::{self, Write},
  path::{Path, PathBuf},
  time::Instant,
};

use crossterm::{
  cursor,
  event::{self, Event, KeyCode, KeyEventKind},
  execute, queue,
  style::{Attribute, Print, SetAttribute},
  terminal::{self, ClearType},
};
use fff_search::{FFFMode, FilePicker, FilePickerOptions, FuzzySearchOptions, PaginationArgs, QueryParser};

const DB8_FILTER: &str = "*.db8";
const RESULT_LIMIT: usize = 12;

#[derive(Clone, Debug)]
pub struct FileSearchHit {
  pub path: PathBuf,
}

pub struct Db8FileSearch {
  picker: FilePicker,
}

impl Db8FileSearch {
  pub fn new(root: PathBuf) -> anyhow::Result<Self> {
    let root = normalize_search_root(root)?;
    let mut picker = FilePicker::new(FilePickerOptions {
      base_path: root.to_string_lossy().to_string(),
      mode: FFFMode::Ai,
      watch: false,
      ..Default::default()
    })?;
    picker.collect_files()?;
    Ok(Self { picker })
  }

  pub fn root(&self) -> &Path {
    self.picker.base_path()
  }

  pub fn indexed_file_count(&self) -> usize {
    self.picker.live_file_count()
  }

  pub fn search(&self, query: &str, limit: usize) -> Vec<FileSearchHit> {
    search_db8_files(&self.picker, query, limit)
  }
}

pub fn default_global_search_root() -> PathBuf {
  std::env::var_os("HOME")
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn run_db8_search_cli(root: PathBuf) -> anyhow::Result<Option<PathBuf>> {
  let started = Instant::now();
  let search = Db8FileSearch::new(root)?;

  let scan_message = format!(
    "Indexed {} files under {} in {:?}",
    search.indexed_file_count(),
    search.root().display(),
    started.elapsed()
  );

  let mut terminal = SearchTerminal::enter()?;
  let result = interactive_search_loop(&search, &scan_message, &mut terminal.stdout);
  terminal.leave()?;
  result
}

fn normalize_search_root(root: PathBuf) -> anyhow::Result<PathBuf> {
  let root = root.canonicalize().unwrap_or(root);
  if !root.exists() {
    anyhow::bail!("search root does not exist: {}", root.display());
  }
  if root.parent().is_none() {
    anyhow::bail!("fff-search refuses to index the filesystem root; choose a narrower root");
  }
  Ok(root)
}

fn interactive_search_loop(search: &Db8FileSearch, scan_message: &str, stdout: &mut io::Stdout) -> anyhow::Result<Option<PathBuf>> {
  let mut query = String::new();
  let mut selected = 0usize;

  loop {
    let hits = search.search(&query, RESULT_LIMIT);
    if selected >= hits.len() {
      selected = hits.len().saturating_sub(1);
    }
    render_search(stdout, scan_message, &query, &hits, selected)?;

    let Event::Key(key) = event::read()? else {
      continue;
    };
    if key.kind != KeyEventKind::Press {
      continue;
    }
    match key.code {
      KeyCode::Esc => return Ok(None),
      KeyCode::Enter => return Ok(hits.get(selected).map(|hit| hit.path.clone())),
      KeyCode::Backspace => {
        query.pop();
        selected = 0;
      },
      KeyCode::Char(ch) => {
        query.push(ch);
        selected = 0;
      },
      KeyCode::Up => {
        selected = selected.saturating_sub(1);
      },
      KeyCode::Down => {
        if selected + 1 < hits.len() {
          selected += 1;
        }
      },
      _ => {},
    }
  }
}

fn search_db8_files(picker: &FilePicker, typed_query: &str, limit: usize) -> Vec<FileSearchHit> {
  let parser = QueryParser::default();
  let query_text = if typed_query.trim().is_empty() {
    DB8_FILTER.to_string()
  } else {
    format!("{} {}", typed_query.trim(), DB8_FILTER)
  };
  let query = parser.parse(&query_text);
  let results = picker.fuzzy_search(
    &query,
    None,
    FuzzySearchOptions {
      max_threads: 0,
      current_file: None,
      project_path: Some(picker.base_path()),
      pagination: PaginationArgs { offset: 0, limit },
      ..Default::default()
    },
  );

  results
    .items
    .into_iter()
    .filter(|item| is_db8_path(&item.file_name(picker)))
    .map(|item| FileSearchHit {
      path: item.absolute_path(picker, picker.base_path()),
    })
    .collect()
}

fn is_db8_path(path: &str) -> bool {
  Path::new(path)
    .extension()
    .and_then(|extension| extension.to_str())
    .is_some_and(|extension| extension.eq_ignore_ascii_case("db8"))
}

fn render_search(stdout: &mut io::Stdout, scan_message: &str, query: &str, hits: &[FileSearchHit], selected: usize) -> anyhow::Result<()> {
  queue!(
    stdout,
    cursor::MoveTo(0, 0),
    terminal::Clear(ClearType::All),
    Print("DB8 global search\n"),
    Print(scan_message),
    Print("\n\nType to filter, Up/Down to select, Enter to choose, Esc to cancel.\n\n"),
    Print("> "),
    Print(query),
    Print("\n\n")
  )?;

  for (ix, hit) in hits.iter().enumerate() {
    if ix == selected {
      queue!(stdout, SetAttribute(Attribute::Reverse))?;
    }
    queue!(stdout, Print(format!("{}\n", hit.path.display())))?;
    if ix == selected {
      queue!(stdout, SetAttribute(Attribute::Reset))?;
    }
  }

  if hits.is_empty() {
    queue!(stdout, Print("No .db8 files match.\n"))?;
  }

  stdout.flush()?;
  Ok(())
}

struct SearchTerminal {
  stdout: io::Stdout,
}

impl SearchTerminal {
  fn enter() -> anyhow::Result<Self> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
    Ok(Self { stdout })
  }

  fn leave(&mut self) -> anyhow::Result<()> {
    execute!(self.stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    Ok(())
  }
}

impl Drop for SearchTerminal {
  fn drop(&mut self) {
    let _ = execute!(self.stdout, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
  }
}
