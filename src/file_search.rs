use std::path::{Path, PathBuf};

use fff_search::{FFFMode, FilePicker, FilePickerOptions, FuzzySearchOptions, PaginationArgs, QueryParser};

const DB8_FILTER: &str = "*.db8";

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
