//! 文件树模型：目录展开 / 折叠、扁平可见行、markdown 过滤。
//!
//! 设计取舍（防 bug）：
//! - 符号链接一律视为叶子（不递归进入），天然避免循环引用；
//! - 读不到的目录标记为不可读并视为空，绝不 panic；
//! - 排序固定为「目录优先 + 名称大小写不敏感」，与读取顺序无关。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 树上的一个节点。
#[derive(Clone, Debug)]
struct TreeNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    expanded: bool,
    readable: bool,
    children: Vec<TreeNode>,
}

/// 侧边栏实际渲染的一行（拍平后的可见节点）。
#[derive(Clone, Debug)]
pub struct TreeRow {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub depth: usize,
    /// 缩进引导线，如 `"│  ├─ "`。
    pub prefix: String,
}

pub struct FileTree {
    pub root: PathBuf,
    nodes: Vec<TreeNode>,
    pub rows: Vec<TreeRow>,
    pub cursor: usize,
    /// false = 只显示目录 + markdown 文件；true = 显示全部（含隐藏文件）。
    pub show_all: bool,
}

/// 哪些扩展名算 markdown（大小写不敏感）。
fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md") | Some("markdown") | Some("mdown") | Some("mkdn") | Some("mkd")
    )
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn sort_nodes(nodes: &mut [TreeNode]) {
    nodes.sort_by(|a, b| {
        // 目录优先，随后按名称大小写不敏感排序，最后按原始名称兜底保证稳定。
        (b.is_dir as u8)
            .cmp(&(a.is_dir as u8))
            .then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// 读一层目录。单个条目出错只跳过该条目；整个目录读失败返回空 + 不可读。
fn read_children(dir: &Path, show_all: bool) -> (Vec<TreeNode>, bool) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (Vec::new(), false),
    };
    let mut nodes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // 软链接：只看链接本身，不跟随，避免循环。
        let ft = match std::fs::symlink_metadata(&path) {
            Ok(m) => m.file_type(),
            Err(_) => continue,
        };
        if ft.is_symlink() {
            if !show_all && (is_hidden(&name) || !is_markdown(&path)) {
                continue;
            }
            nodes.push(TreeNode {
                path,
                name,
                is_dir: false,
                expanded: false,
                readable: true,
                children: Vec::new(),
            });
            continue;
        }
        let is_dir = ft.is_dir();
        if is_dir {
            if !show_all && is_hidden(&name) {
                continue;
            }
            nodes.push(TreeNode {
                path,
                name,
                is_dir: true,
                expanded: false,
                readable: true,
                children: Vec::new(),
            });
        } else {
            if !show_all && (is_hidden(&name) || !is_markdown(&path)) {
                continue;
            }
            nodes.push(TreeNode {
                path,
                name,
                is_dir: false,
                expanded: false,
                readable: true,
                children: Vec::new(),
            });
        }
    }
    sort_nodes(&mut nodes);
    (nodes, true)
}

impl FileTree {
    /// 以 `root` 为根建树。`preselect` 指向打开后默认选中的文件。
    pub fn build(root: &Path, preselect: Option<&Path>) -> Self {
        let mut tree = Self {
            root: root.to_path_buf(),
            nodes: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            show_all: false,
        };
        tree.reload();
        if let Some(sel) = preselect {
            tree.expand_to(sel);
            if let Some(i) = tree.rows.iter().position(|r| r.path == sel) {
                tree.cursor = i;
            }
        }
        tree
    }

    /// 重新扫描磁盘。保留已展开的目录与当前选中（按路径匹配）。
    pub fn reload(&mut self) {
        let expanded = self.expanded_paths();
        let selected = self.rows.get(self.cursor).map(|r| r.path.clone());
        let (nodes, _) = read_children(&self.root, self.show_all);
        self.nodes = nodes;
        self.apply_expanded(&expanded);
        self.rebuild_rows();
        if let Some(sel) = selected
            && let Some(i) = self.rows.iter().position(|r| r.path == sel)
        {
            self.cursor = i;
        }
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
    }

    /// 把 `path` 及其所有祖先目录展开（用于启动时定位文件）。
    fn expand_to(&mut self, path: &Path) {
        let mut anc: Vec<PathBuf> = Vec::new();
        let mut cur = path.parent().map(|p| p.to_path_buf());
        while let Some(dir) = cur {
            if dir == self.root || dir.starts_with(&self.root) {
                anc.push(dir.clone());
            }
            if dir == self.root {
                break;
            }
            cur = dir.parent().map(|p| p.to_path_buf());
            if anc.len() > 128 {
                break; // 深度熔断，防异常路径
            }
        }
        let set: HashSet<PathBuf> = anc.into_iter().collect();
        Self::apply_expanded_to(&mut self.nodes, &set, self.show_all);
        self.rebuild_rows();
    }

    fn expanded_paths(&self) -> HashSet<PathBuf> {
        let mut set = HashSet::new();
        fn walk(nodes: &[TreeNode], set: &mut HashSet<PathBuf>) {
            for n in nodes {
                if n.is_dir && n.expanded {
                    set.insert(n.path.clone());
                    walk(&n.children, set);
                }
            }
        }
        walk(&self.nodes, &mut set);
        set
    }

    fn apply_expanded(&mut self, set: &HashSet<PathBuf>) {
        Self::apply_expanded_to(&mut self.nodes, set, self.show_all);
    }

    fn apply_expanded_to(nodes: &mut [TreeNode], set: &HashSet<PathBuf>, show_all: bool) {
        for n in nodes {
            if n.is_dir && set.contains(&n.path) {
                n.expanded = true;
                let (children, _) = read_children(&n.path, show_all);
                n.children = children;
                Self::apply_expanded_to(&mut n.children, set, show_all);
            }
        }
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        fn walk(nodes: &[TreeNode], depth: usize, base: &str, rows: &mut Vec<TreeRow>) {
            let n = nodes.len();
            for (i, node) in nodes.iter().enumerate() {
                let last = i + 1 == n;
                let joint = if depth == 0 {
                    String::new()
                } else if last {
                    format!("{base}└─ ")
                } else {
                    format!("{base}├─ ")
                };
                rows.push(TreeRow {
                    path: node.path.clone(),
                    name: node.name.clone(),
                    is_dir: node.is_dir,
                    expanded: node.expanded,
                    depth,
                    prefix: joint,
                });
                if node.is_dir && node.expanded {
                    let child_base = if depth == 0 {
                        String::new()
                    } else if last {
                        format!("{base}   ")
                    } else {
                        format!("{base}│  ")
                    };
                    walk(&node.children, depth + 1, &child_base, rows);
                }
            }
        }
        walk(&self.nodes, 0, "", &mut rows);
        self.rows = rows;
    }

    fn find_node_mut<'a>(nodes: &'a mut [TreeNode], path: &Path) -> Option<&'a mut TreeNode> {
        for n in nodes.iter_mut() {
            if n.path == path {
                return Some(n);
            }
            if n.is_dir
                && n.expanded
                && let Some(hit) = Self::find_node_mut(&mut n.children, path)
            {
                return Some(hit);
            }
        }
        None
    }

    // ---------- 光标移动 ----------

    pub fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.cursor as isize + delta).clamp(0, len - 1);
        self.cursor = next as usize;
    }

    pub fn go_top(&mut self) {
        self.cursor = 0;
    }

    pub fn go_bottom(&mut self) {
        self.cursor = self.rows.len().saturating_sub(1);
    }

    pub fn page(&mut self, up: bool, size: usize) {
        let step = (size.max(1)) as isize;
        self.move_cursor(if up { -step } else { step });
    }

    // ---------- 展开 / 折叠 / 打开 ----------

    pub fn selected(&self) -> Option<&TreeRow> {
        self.rows.get(self.cursor)
    }

    /// 切换选中目录的展开状态。返回 true 表示是目录（已处理），false 表示是文件。
    pub fn toggle_expand(&mut self) -> bool {
        let Some(row) = self.rows.get(self.cursor).cloned() else {
            return false;
        };
        if !row.is_dir {
            return false;
        }
        if let Some(node) = Self::find_node_mut(&mut self.nodes, &row.path) {
            node.expanded = !node.expanded;
            if node.expanded {
                let (children, readable) = read_children(&node.path, self.show_all);
                node.children = children;
                node.readable = readable;
            } else {
                node.children.clear();
            }
        }
        self.rebuild_rows();
        // 折叠/展开后光标尽量停在原目录上
        if let Some(i) = self.rows.iter().position(|r| r.path == row.path) {
            self.cursor = i;
        }
        true
    }

    /// 收起当前目录；若已收起则跳到父目录。返回跳到的行是否变化（供调用方决定是否加载）。
    pub fn collapse_or_parent(&mut self) {
        let Some(row) = self.rows.get(self.cursor).cloned() else {
            return;
        };
        if row.is_dir && row.expanded {
            self.toggle_expand();
            return;
        }
        // 找父目录行：向前找第一个 depth 更小的目录
        for i in (0..self.cursor).rev() {
            if self.rows[i].depth < row.depth && self.rows[i].is_dir {
                self.cursor = i;
                break;
            }
        }
    }

    pub fn toggle_show_all(&mut self) {
        self.show_all = !self.show_all;
        self.reload();
    }

    /// 当前选中是否为不可读目录（展开后空且不可读时提示用）。
    pub fn selected_unreadable(&self) -> bool {
        let Some(row) = self.selected() else {
            return false;
        };
        if !row.is_dir || !row.expanded {
            return false;
        }
        Self::find_node_mut_readable(&self.nodes, &row.path).is_some_and(|r| !r)
    }

    fn find_node_mut_readable(nodes: &[TreeNode], path: &Path) -> Option<bool> {
        for n in nodes {
            if n.path == path {
                return Some(n.readable);
            }
            if n.is_dir
                && n.expanded
                && let Some(hit) = Self::find_node_mut_readable(&n.children, path)
            {
                return Some(hit);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdtui-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("b.md"), "# B").unwrap();
        fs::write(dir.join("a.txt"), "plain").unwrap();
        fs::write(dir.join(".hidden.md"), "# H").unwrap();
        fs::write(dir.join("sub").join("c.md"), "# C").unwrap();
        dir
    }

    #[test]
    fn default_filters_non_md_and_hidden() {
        let dir = sample_dir("filter");
        let tree = FileTree::build(&dir, None);
        let names: Vec<_> = tree.rows.iter().map(|r| r.name.clone()).collect();
        // 目录 + md 文件可见；txt 与隐藏文件不可见
        assert!(names.contains(&"sub".to_string()));
        assert!(names.contains(&"b.md".to_string()));
        assert!(!names.contains(&"a.txt".to_string()));
        assert!(!names.contains(&".hidden.md".to_string()));
        // 目录排在文件前
        assert!(
            names.iter().position(|n| n == "sub").unwrap()
                < names.iter().position(|n| n == "b.md").unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn show_all_reveals_everything() {
        let dir = sample_dir("all");
        let mut tree = FileTree::build(&dir, None);
        tree.toggle_show_all();
        let names: Vec<_> = tree.rows.iter().map(|r| r.name.clone()).collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&".hidden.md".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_and_collapse() {
        let dir = sample_dir("expand");
        let mut tree = FileTree::build(&dir, None);
        assert_eq!(tree.rows.len(), 2); // sub + b.md
        tree.cursor = 0; // sub
        assert!(tree.toggle_expand());
        assert!(tree.rows.iter().any(|r| r.name == "c.md"));
        tree.collapse_or_parent(); // 已展开 → 收起
        assert!(!tree.rows.iter().any(|r| r.name == "c.md"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_does_not_panic() {
        let tree = FileTree::build(Path::new("/definitely/not/here/mdtui"), None);
        assert!(tree.rows.is_empty());
    }
}
