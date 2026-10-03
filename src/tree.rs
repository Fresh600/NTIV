use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::filesystem::read_child_directories;

#[derive(Debug, Clone)]
pub struct TreeItem {
    pub path: PathBuf,
    pub depth: usize,
    pub expanded: bool,
}

#[derive(Debug)]
pub struct TreeState {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<PathBuf>>,
    pub items: Vec<TreeItem>,
    pub selected: usize,
}

impl TreeState {
    pub fn new(current: &Path) -> Self {
        let current = dunce::canonicalize(current).unwrap_or_else(|_| current.to_path_buf());
        let root = filesystem_root(&current);
        let mut state = Self {
            root,
            expanded: HashSet::new(),
            children: HashMap::new(),
            items: Vec::new(),
            selected: 0,
        };
        state.sync_to_path(&current);
        state
    }

    pub fn sync_to_path(&mut self, path: &Path) {
        let normalized = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let path = normalized.as_path();
        let directory = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };

        let mut chain: Vec<PathBuf> = directory
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(&self.root))
            .map(PathBuf::from)
            .collect();
        chain.reverse();

        for ancestor in chain {
            self.expanded.insert(ancestor.clone());
            self.load_children(&ancestor);
        }
        self.rebuild();
        if let Some(index) = self.items.iter().position(|item| item.path == directory) {
            self.selected = index;
        }
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.items
            .get(self.selected)
            .map(|item| item.path.as_path())
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.items.len() - 1);
    }

    pub fn toggle_selected(&mut self) {
        let Some(path) = self.selected_path().map(PathBuf::from) else {
            return;
        };
        if self.expanded.remove(&path) {
            self.rebuild();
            self.select_path(&path);
            return;
        }
        self.load_children(&path);
        self.expanded.insert(path.clone());
        self.rebuild();
        self.select_path(&path);
    }

    pub fn collapse_or_parent(&mut self) {
        let Some(path) = self.selected_path().map(PathBuf::from) else {
            return;
        };
        if self.expanded.remove(&path) {
            self.rebuild();
            self.select_path(&path);
        } else if let Some(parent) = path.parent().map(PathBuf::from) {
            self.select_path(&parent);
        }
    }

    fn select_path(&mut self, path: &Path) {
        if let Some(index) = self.items.iter().position(|item| item.path == path) {
            self.selected = index;
        }
    }

    fn load_children(&mut self, path: &Path) {
        if self.children.contains_key(path) {
            return;
        }
        let children = read_child_directories(path).unwrap_or_default();
        self.children.insert(path.to_path_buf(), children);
    }

    fn rebuild(&mut self) {
        self.items.clear();
        self.push_branch(self.root.clone(), 0);
        if !self.items.is_empty() {
            self.selected = self.selected.min(self.items.len() - 1);
        }
    }

    fn push_branch(&mut self, path: PathBuf, depth: usize) {
        let expanded = self.expanded.contains(&path);
        self.items.push(TreeItem {
            path: path.clone(),
            depth,
            expanded,
        });
        if expanded {
            let children = self.children.get(&path).cloned().unwrap_or_default();
            for child in children {
                self.push_branch(child, depth + 1);
            }
        }
    }
}

fn filesystem_root(path: &Path) -> PathBuf {
    let mut root = path.to_path_buf();
    while let Some(parent) = root.parent() {
        if parent.as_os_str().is_empty() || parent == root {
            break;
        }
        root = parent.to_path_buf();
    }
    root
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn tree_reveals_current_path_lazily() {
        let temp = tempdir().unwrap();
        let child = temp.path().join("child");
        let nested = child.join("nested");
        fs::create_dir_all(&nested).unwrap();

        let tree = TreeState::new(&nested);
        let nested = dunce::canonicalize(nested).unwrap();
        assert!(tree.items.iter().any(|item| item.path == nested));
        assert_eq!(tree.selected_path(), Some(nested.as_path()));
    }
}
