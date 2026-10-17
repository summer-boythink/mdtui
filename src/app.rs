//! 应用状态：焦点、主题、滚动、搜索、提示消息，以及全部按键行为。

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::ListState;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::markdown::{
    RenderedDoc, WrappedLine, apply_search, render_markdown, render_text, wrap_doc,
};
use crate::theme::{self, THEMES, Theme};
use crate::tree::FileTree;

/// 焦点在哪个窗格。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Sidebar,
    Preview,
    Outline,
}

/// 底部输入状态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    Normal,
    Search,
}

/// 清洗标题纯文本：居中补的空格与 `#` 前缀都去掉，只留标题本身。
fn clean_heading(text: &str) -> String {
    text.trim_start_matches(['#', ' ']).trim().to_string()
}

/// 太大的文件不直接预览（防卡死），只给提示。
const MAX_PREVIEW_BYTES: u64 = 5 * 1024 * 1024;

pub struct SearchState {
    pub query: Vec<char>,
    pub cursor: usize,
    /// 命中所在的展示行号。
    pub matches: Vec<usize>,
    /// 当前高亮第几个命中。
    pub current: usize,
}

impl SearchState {
    fn new() -> Self {
        Self {
            query: Vec::new(),
            cursor: 0,
            matches: Vec::new(),
            current: 0,
        }
    }
    fn text(&self) -> String {
        self.query.iter().collect()
    }
    fn clear(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.matches.clear();
        self.current = 0;
    }
}

pub struct App {
    pub tree: FileTree,
    pub focus: Focus,
    pub input: InputMode,
    pub show_help: bool,
    pub sidebar_visible: bool,
    pub sidebar_width: u16,
    pub theme_idx: usize,
    pub doc: RenderedDoc,
    /// 换行缓存（只在内容 / 宽度变化时重算）。
    wrapped: Vec<WrappedLine>,
    wrap_width: usize,
    doc_version: u64,
    wrapped_version: u64,
    /// 搜索高亮后的展示行缓存。
    shown: Vec<Line<'static>>,
    shown_key: (u64, usize, String, Option<usize>),
    pub scroll: usize,
    pub help_scroll: usize,
    /// 预览区自动换行；关闭后长行直接裁剪。
    pub wrap_enabled: bool,
    pub current_file: Option<PathBuf>,
    pub file_notice: Option<String>,
    pub message: Option<(String, Instant)>,
    pub search: SearchState,
    pub should_quit: bool,
    /// render 回填的可视高度（用于滚动钳制）。
    pub view_h: usize,
    pub help_h: usize,
    /// render 回填的窗格区域（鼠标命中判定用）。
    pub side_area: Rect,
    pub prev_area: Rect,
    /// 侧栏 List 的持久选中态（含滚动 offset，点击换算行号靠它）。
    pub side_state: ListState,
    /// 标题索引：（展示行号，level，标题文本），跳转与当前位置用。
    pub headings: Vec<(usize, u8, String)>,
    /// 右侧大纲栏（默认关闭，`o` 打开）。
    pub outline_visible: bool,
    pub outline_width: u16,
    pub outline_area: Rect,
    pub outline_state: ListState,
    pub outline_cursor: usize,
}

impl App {
    pub fn new(root: &Path, preselect: Option<&Path>, theme_idx: usize) -> Self {
        let mut app = Self {
            tree: FileTree::build(root, preselect),
            focus: if preselect.is_some() {
                Focus::Preview
            } else {
                Focus::Sidebar
            },
            input: InputMode::Normal,
            show_help: false,
            sidebar_visible: true,
            sidebar_width: 30,
            theme_idx,
            doc: render_text("", &THEMES[theme_idx]),
            wrapped: Vec::new(),
            wrap_width: usize::MAX,
            doc_version: 0,
            wrapped_version: u64::MAX,
            shown: Vec::new(),
            shown_key: (u64::MAX, 0, String::new(), None),
            scroll: 0,
            help_scroll: 0,
            wrap_enabled: true,
            current_file: None,
            file_notice: None,
            message: None,
            search: SearchState::new(),
            should_quit: false,
            view_h: 20,
            help_h: 20,
            side_area: Rect::default(),
            prev_area: Rect::default(),
            side_state: ListState::default(),
            headings: Vec::new(),
            outline_visible: false,
            outline_width: 26,
            outline_area: Rect::default(),
            outline_state: ListState::default(),
            outline_cursor: 0,
        };
        if let Some(path) = preselect.map(|p| p.to_path_buf()) {
            app.open_file(&path);
        }
        app
    }

    pub fn theme(&self) -> Theme {
        THEMES[self.theme_idx % THEMES.len()]
    }

    pub fn set_theme(&mut self, idx: usize, persist: bool) {
        self.theme_idx = idx % THEMES.len();
        let name = self.theme().name;
        if persist {
            theme::save_theme(name);
        }
        // 主题只影响颜色，不影响换行；展示缓存里颜色已 baked，需失效。
        self.shown_key.0 = u64::MAX;
        // 文档需用新主题重渲染
        if let Some(path) = self.current_file.clone() {
            self.open_file(&path);
        }
        self.notify(format!("主题：{}", name));
    }

    pub fn cycle_theme(&mut self, delta: isize) {
        let n = THEMES.len() as isize;
        let next = (self.theme_idx as isize + delta).rem_euclid(n) as usize;
        self.set_theme(next, true);
    }

    pub fn notify(&mut self, msg: String) {
        self.message = Some((msg, Instant::now()));
    }

    pub fn clear_expired_message(&mut self) {
        if self
            .message
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs() >= 3)
        {
            self.message = None;
        }
    }

    // ---------- 文件 ----------

    /// 打开文件：md 渲染，非文本/超大则友好提示。永不 panic。
    pub fn open_file(&mut self, path: &Path) {
        self.current_file = Some(path.to_path_buf());
        self.file_notice = None;
        self.scroll = 0;
        self.search.clear();
        let theme = self.theme();
        match std::fs::metadata(path) {
            Ok(md) if md.len() > MAX_PREVIEW_BYTES => {
                self.file_notice = Some(format!(
                    "文件 {:.1} MB，超过预览上限（5 MB），未加载",
                    md.len() as f64 / 1048576.0
                ));
                self.doc = render_text("", &theme);
            }
            Ok(_) => match std::fs::read(path) {
                Ok(bytes) => {
                    if bytes.contains(&0) {
                        self.file_notice = Some("可能是二进制文件，无法预览".to_string());
                        self.doc = render_text("", &theme);
                    } else {
                        let text = String::from_utf8_lossy(&bytes);
                        let is_md = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                            matches!(
                                e.to_ascii_lowercase().as_str(),
                                "md" | "markdown" | "mdown" | "mkdn" | "mkd"
                            )
                        });
                        self.doc = if is_md {
                            render_markdown(&text, &theme)
                        } else {
                            self.file_notice = Some("非 Markdown 文件，按纯文本预览".to_string());
                            render_text(&text, &theme)
                        };
                    }
                }
                Err(e) => {
                    self.file_notice = Some(format!("读取失败：{e}"));
                    self.doc = render_text("", &theme);
                }
            },
            Err(e) => {
                self.file_notice = Some(format!("读取失败：{e}"));
                self.doc = render_text("", &theme);
            }
        }
        self.doc_version += 1;
    }

    /// 打开侧边栏当前选中（目录则展开，文件则预览）。
    pub fn open_selected(&mut self) {
        let Some(row) = self.tree.selected().cloned() else {
            return;
        };
        if row.is_dir {
            self.tree.toggle_expand();
            if self.tree.selected_unreadable() {
                self.notify("该目录无法读取（权限不足？）".to_string());
            }
        } else {
            self.open_file(&row.path);
            self.focus = Focus::Preview;
        }
    }

    pub fn reload_all(&mut self) {
        self.tree.reload();
        if let Some(path) = self.current_file.clone() {
            self.open_file(&path);
        }
        self.notify("已刷新".to_string());
    }

    // ---------- 换行 / 展示缓存 ----------

    /// 保证换行缓存与当前宽度同步。`width` 为预览区内部宽度。
    pub fn ensure_wrapped(&mut self, width: usize) {
        let width = if self.wrap_enabled {
            width
        } else {
            usize::MAX / 2
        };
        if self.wrapped_version != self.doc_version || self.wrap_width != width {
            self.wrapped = wrap_doc(&self.doc, width.min(1000), &self.theme());
            self.wrap_width = width;
            self.wrapped_version = self.doc_version;
            self.shown_key.0 = u64::MAX; // 展示缓存失效
            // 重建标题索引
            self.headings = self
                .wrapped
                .iter()
                .enumerate()
                .filter(|(_, w)| w.heading)
                .map(|(i, w)| (i, w.level.max(1), clean_heading(&w.text)))
                .filter(|(_, _, s)| !s.is_empty())
                .collect();
            self.outline_cursor = self
                .outline_cursor
                .min(self.headings.len().saturating_sub(1));
        }
    }

    /// 展示行（含搜索高亮），带缓存。
    pub fn shown_lines(&mut self) -> &[Line<'static>] {
        let q = self.search.text();
        // 当前高亮的是第几个命中（apply_search 按同样顺序编号，全局序号一致）
        let cur = if q.is_empty() || self.search.current >= self.search.matches.len() {
            None
        } else {
            Some(self.search.current)
        };
        let key = (self.wrapped_version, self.wrap_width, q.clone(), cur);
        if self.shown_key != key {
            self.shown = apply_search(&self.wrapped, &q, cur, &self.theme());
            self.shown_key = key;
        }
        &self.shown
    }

    /// 重算搜索命中（换行结果变化 / 查询变化时调用）。
    pub fn recompute_search(&mut self) {
        let q = self.search.text();
        self.search.matches.clear();
        self.search.current = 0;
        if q.is_empty() {
            self.shown_key.0 = u64::MAX;
            return;
        }
        let ascii = q.is_ascii();
        let lowered = if ascii {
            q.to_ascii_lowercase()
        } else {
            q.clone()
        };
        for (i, w) in self.wrapped.iter().enumerate() {
            let hay = if ascii {
                w.text.to_ascii_lowercase()
            } else {
                w.text.clone()
            };
            if hay.contains(&lowered) {
                self.search.matches.push(i);
            }
        }
        self.shown_key.0 = u64::MAX;
    }

    pub fn search_jump(&mut self, delta: isize) {
        if self.search.matches.is_empty() {
            return;
        }
        let n = self.search.matches.len() as isize;
        self.search.current = (self.search.current as isize + delta).rem_euclid(n) as usize;
        let line = self.search.matches[self.search.current];
        // 尽量让命中行居中
        self.scroll = line.saturating_sub(self.view_h / 2);
        self.clamp_scroll();
        self.shown_key.0 = u64::MAX;
    }

    // ---------- 标题跳转 ----------

    /// 跳到上/下一个标题（delta>0 向下）。没有标题时静默无操作。
    pub fn jump_heading(&mut self, delta: isize) {
        if self.headings.is_empty() {
            return;
        }
        let target = if delta > 0 {
            self.headings
                .iter()
                .find(|(i, ..)| *i > self.scroll)
                .map(|(i, ..)| *i)
        } else {
            self.headings
                .iter()
                .rev()
                .find(|(i, ..)| *i < self.scroll)
                .map(|(i, ..)| *i)
        };
        if let Some(i) = target {
            self.scroll = i;
            self.clamp_scroll();
        } else if delta < 0 {
            self.scroll = 0;
        }
    }

    /// 当前滚动位置所处的章节标题（标题栏展示用）。
    pub fn current_section(&self) -> Option<&str> {
        self.headings
            .iter()
            .rev()
            .find(|(i, ..)| *i <= self.scroll)
            .map(|(_, _, s)| s.as_str())
    }

    /// 当前章节在大纲里的下标（大纲高亮用）。
    pub fn current_section_idx(&self) -> Option<usize> {
        self.headings.iter().rposition(|(i, ..)| *i <= self.scroll)
    }

    /// 跳到大纲第 idx 个标题。
    pub fn jump_to_outline(&mut self, idx: usize) {
        if let Some((line, ..)) = self.headings.get(idx) {
            self.scroll = *line;
            self.outline_cursor = idx;
            self.clamp_scroll();
        }
    }

    // ---------- 鼠标 ----------

    fn hit(area: &Rect, col: u16, row: u16) -> bool {
        area.width > 0
            && area.height > 0
            && col >= area.x
            && col < area.x.saturating_add(area.width)
            && row >= area.y
            && row < area.y.saturating_add(area.height)
    }

    pub fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        match kind {
            MouseEventKind::ScrollDown => self.mouse_scroll(1, col, row),
            MouseEventKind::ScrollUp => self.mouse_scroll(-1, col, row),
            MouseEventKind::Down(MouseButton::Left) => self.mouse_click(col, row),
            _ => {}
        }
    }

    fn mouse_scroll(&mut self, dir: isize, col: u16, row: u16) {
        const STEP: isize = 3;
        if self.show_help {
            if dir > 0 {
                self.help_scroll += self.help_h.max(1) / 2;
            } else {
                self.help_scroll = self.help_scroll.saturating_sub(self.help_h.max(1) / 2);
            }
            return;
        }
        if Self::hit(&self.side_area, col, row) {
            self.focus = Focus::Sidebar;
            self.tree.move_cursor(STEP * dir);
        } else if Self::hit(&self.outline_area, col, row) {
            // 大纲栏滚轮：正文线性滚动（与预览一致，不意外跳章节）
            self.focus = Focus::Outline;
            self.scroll_preview(STEP * dir);
        } else if Self::hit(&self.prev_area, col, row) {
            self.focus = Focus::Preview;
            self.scroll_preview(STEP * dir);
        }
    }

    fn mouse_click(&mut self, col: u16, row: u16) {
        if self.show_help {
            return; // 帮助页只滚不动，保持简单可预期
        }
        if Self::hit(&self.side_area, col, row) {
            self.focus = Focus::Sidebar;
            // 边框占 1 行：inner 行号 = 点击行 - 顶边框
            let inner = row.saturating_sub(self.side_area.y + 1) as usize;
            let inner_h = (self.side_area.height as usize).saturating_sub(2);
            if inner >= inner_h {
                return; // 点在边框上
            }
            let idx = self.side_state.offset() + inner;
            if idx >= self.tree.rows.len() {
                return; // 点在空白处
            }
            self.tree.cursor = idx;
            self.open_selected();
        } else if Self::hit(&self.outline_area, col, row) {
            self.focus = Focus::Outline;
            let inner = row.saturating_sub(self.outline_area.y + 1) as usize;
            let inner_h = (self.outline_area.height as usize).saturating_sub(2);
            if inner >= inner_h {
                return;
            }
            let idx = self.outline_state.offset() + inner;
            if idx >= self.headings.len() {
                return;
            }
            self.jump_to_outline(idx);
        } else if Self::hit(&self.prev_area, col, row) {
            self.focus = Focus::Preview;
        }
    }

    // ---------- 滚动 ----------

    pub fn total_lines(&self) -> usize {
        // 展示行与换行结果恒等长（搜索只改样式不增删行），
        // 用 wrapped 才能在渲染前也正确钳制。
        self.wrapped.len()
    }

    pub fn clamp_scroll(&mut self) {
        let max = self.total_lines().saturating_sub(self.view_h.max(1));
        self.scroll = self.scroll.min(max);
    }

    pub fn scroll_preview(&mut self, delta: isize) {
        if self.wrapped.is_empty() {
            return;
        }
        let max = self.total_lines().saturating_sub(self.view_h.max(1)) as isize;
        self.scroll = ((self.scroll as isize + delta).clamp(0, max.max(0))) as usize;
    }

    // ---------- 按键 ----------

    pub fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if mods.contains(KeyModifiers::CONTROL) {
            match code {
                KeyCode::Char('c') => self.should_quit = true,
                KeyCode::Char('b') => self.toggle_sidebar(),
                _ => {}
            }
            return;
        }
        // 搜索输入态：几乎所有键都吃掉
        if self.input == InputMode::Search {
            self.search_key(code, mods);
            return;
        }
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                self.help_scroll = 0;
            }
            KeyCode::Tab => self.cycle_focus(1),
            KeyCode::BackTab => self.cycle_focus(-1),
            KeyCode::Char('b') => self.toggle_sidebar(),
            KeyCode::Char('e') => self.toggle_sidebar(), // Explorer：与 b 同义，好按
            KeyCode::Char('t') => self.cycle_theme(1),
            KeyCode::Char('T') => self.cycle_theme(-1),
            KeyCode::Char('[') => self.adjust_sidebar(-2),
            KeyCode::Char(']') => self.adjust_sidebar(2),
            KeyCode::Char('w') => {
                self.wrap_enabled = !self.wrap_enabled;
                self.wrapped_version = u64::MAX; // 强制重算
                self.scroll = 0;
                self.notify(if self.wrap_enabled {
                    "自动换行：开".to_string()
                } else {
                    "自动换行：关（超宽裁剪）".to_string()
                });
            }
            KeyCode::Char('a') => {
                self.tree.toggle_show_all();
                self.notify(if self.tree.show_all {
                    "显示全部文件（含隐藏/非 md）".to_string()
                } else {
                    "只显示目录 + Markdown".to_string()
                });
            }
            KeyCode::Char('R') => self.reload_all(),
            KeyCode::Char('r') => {
                if let Some(path) = self.current_file.clone() {
                    self.open_file(&path);
                    self.notify("文件已重载".to_string());
                }
            }
            KeyCode::Char('/') => {
                self.input = InputMode::Search;
                self.search.clear();
            }
            KeyCode::Char('{') => self.jump_heading(-1),
            KeyCode::Char('}') => self.jump_heading(1),
            KeyCode::Char('o') => self.toggle_outline(),
            KeyCode::Char('n') => self.search_jump(1),
            KeyCode::Char('N') => self.search_jump(-1),
            _ => self.pane_key(code, mods),
        }
    }

    fn pane_key(&mut self, code: KeyCode, _mods: KeyModifiers) {
        // 帮助页：只滚动
        if self.show_help {
            match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.help_scroll = self.help_scroll.saturating_add(1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                KeyCode::Char('d') | KeyCode::PageDown | KeyCode::Char(' ') => {
                    self.help_scroll += self.help_h.max(1);
                }
                KeyCode::Char('u') | KeyCode::PageUp => {
                    self.help_scroll = self.help_scroll.saturating_sub(self.help_h.max(1));
                }
                KeyCode::Char('g') | KeyCode::Home => self.help_scroll = 0,
                KeyCode::Char('G') | KeyCode::End => self.help_scroll = usize::MAX / 2,
                _ => {}
            }
            return;
        }
        match self.focus {
            Focus::Sidebar => match code {
                KeyCode::Char('j') | KeyCode::Down => self.tree.move_cursor(1),
                KeyCode::Char('k') | KeyCode::Up => self.tree.move_cursor(-1),
                KeyCode::Char('g') | KeyCode::Home => self.tree.go_top(),
                KeyCode::Char('G') | KeyCode::End => self.tree.go_bottom(),
                KeyCode::PageDown => self.tree.page(false, 10),
                KeyCode::PageUp => self.tree.page(true, 10),
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.open_selected(),
                KeyCode::Char('h') | KeyCode::Left => self.tree.collapse_or_parent(),
                _ => {}
            },
            Focus::Preview => match code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll_preview(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_preview(-1),
                KeyCode::Char('d') | KeyCode::PageDown | KeyCode::Char(' ') => {
                    let h = self.view_h.max(1) as isize;
                    self.scroll_preview(h)
                }
                KeyCode::Char('u') | KeyCode::PageUp => {
                    let h = self.view_h.max(1) as isize;
                    self.scroll_preview(-h)
                }
                KeyCode::Char('g') | KeyCode::Home => self.scroll_preview(isize::MIN / 2),
                KeyCode::Char('G') | KeyCode::End => self.scroll_preview(isize::MAX / 2),
                _ => {}
            },
            Focus::Outline => match code {
                KeyCode::Char('j') | KeyCode::Down => self.move_outline(1),
                KeyCode::Char('k') | KeyCode::Up => self.move_outline(-1),
                KeyCode::Char('d') | KeyCode::PageDown => self.move_outline(5),
                KeyCode::Char('u') | KeyCode::PageUp => self.move_outline(-5),
                KeyCode::Char('g') | KeyCode::Home => {
                    self.outline_cursor = 0;
                    self.jump_to_outline(0);
                }
                KeyCode::Char('G') | KeyCode::End => {
                    let last = self.headings.len().saturating_sub(1);
                    self.outline_cursor = last;
                    self.jump_to_outline(last);
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    self.jump_to_outline(self.outline_cursor);
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Preview,
                _ => {}
            },
        }
    }

    fn search_key(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Esc => {
                self.input = InputMode::Normal;
                self.search.clear();
                self.shown_key.0 = u64::MAX;
            }
            KeyCode::Enter => {
                self.input = InputMode::Normal;
                if self.search.matches.is_empty() {
                    self.notify("无匹配".to_string());
                } else {
                    self.search_jump(0);
                    let n = self.search.matches.len();
                    self.notify(format!("{n} 处匹配（n/N 跳转）"));
                }
            }
            KeyCode::Backspace => {
                if self.search.cursor > 0 {
                    self.search.cursor -= 1;
                    self.search.query.remove(self.search.cursor);
                    self.live_search();
                } else if self.search.query.is_empty() {
                    self.input = InputMode::Normal;
                }
            }
            KeyCode::Left => {
                self.search.cursor = self.search.cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                self.search.cursor = (self.search.cursor + 1).min(self.search.query.len());
            }
            KeyCode::Char(c) => {
                self.search.query.insert(self.search.cursor, c);
                self.search.cursor += 1;
                self.live_search();
            }
            _ => {}
        }
    }

    /// 输入过程中实时重算并跳到首个命中（防抖就不做了，文本量下足够快）。
    fn live_search(&mut self) {
        self.recompute_search();
        if !self.search.matches.is_empty() {
            let line = self.search.matches[0];
            self.scroll = line.saturating_sub(self.view_h / 2);
            self.clamp_scroll();
        }
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        if self.sidebar_visible {
            // 显示即聚焦：切到文件树就是为了操作它，省一次 Tab
            self.focus = Focus::Sidebar;
        } else if self.focus == Focus::Sidebar {
            self.focus = Focus::Preview;
        }
    }

    fn toggle_outline(&mut self) {
        self.outline_visible = !self.outline_visible;
        if self.outline_visible {
            self.focus = Focus::Outline;
        } else if self.focus == Focus::Outline {
            self.focus = Focus::Preview;
        }
    }

    /// Tab / Shift+Tab 在可见窗格间轮转焦点（隐藏的窗格自动跳过）。
    fn cycle_focus(&mut self, dir: isize) {
        let mut panes = vec![];
        if self.sidebar_visible {
            panes.push(Focus::Sidebar);
        }
        panes.push(Focus::Preview);
        if self.outline_visible {
            panes.push(Focus::Outline);
        }
        let cur = panes.iter().position(|&f| f == self.focus).unwrap_or(0) as isize;
        let next = (cur + dir).rem_euclid(panes.len() as isize) as usize;
        self.focus = panes[next];
    }

    /// 大纲光标移动并同步跳转正文。
    fn move_outline(&mut self, delta: isize) {
        if self.headings.is_empty() {
            return;
        }
        let n = self.headings.len() as isize;
        self.outline_cursor = (self.outline_cursor as isize + delta).clamp(0, n - 1) as usize;
        self.jump_to_outline(self.outline_cursor);
    }

    fn adjust_sidebar(&mut self, delta: i16) {
        let w = (self.sidebar_width as i16 + delta).clamp(15, 60) as u16;
        self.sidebar_width = w;
        if !self.sidebar_visible {
            self.sidebar_visible = true;
            self.focus = Focus::Sidebar;
        }
    }

    /// 侧边栏标题：根目录名 + 过滤状态。
    pub fn sidebar_title(&self) -> String {
        let root = self
            .tree
            .root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.tree.root.to_string_lossy().into_owned());
        if self.tree.show_all {
            format!(" {root} · 全部 ")
        } else {
            format!(" {root} ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdtui-app-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("readme.md"), "# Hi\n\nhello world\n").unwrap();
        fs::write(dir.join("code.txt"), "plain text\n").unwrap();
        dir
    }

    #[test]
    fn open_and_search_flow() {
        let dir = sample_project("flow");
        let mut app = App::new(&dir, None, 0);
        let md = dir.join("readme.md");
        app.open_file(&md);
        assert_eq!(app.doc.title.as_deref(), Some("Hi"));
        app.ensure_wrapped(40);
        assert!(!app.wrapped.is_empty());
        // 搜索
        for c in "hello".chars() {
            app.search.query.push(c);
        }
        app.search.cursor = 5;
        app.recompute_search();
        assert!(!app.search.matches.is_empty());
        app.search_jump(1);
        let _ = app.shown_lines();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn binary_and_missing_are_friendly() {
        let dir = sample_project("bin");
        let mut app = App::new(&dir, None, 0);
        let bin = dir.join("a.bin");
        fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        app.open_file(&bin);
        assert!(app.file_notice.is_some());
        app.open_file(&dir.join("nope.md"));
        assert!(app.file_notice.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn heading_jump_moves_scroll() {
        let dir = sample_project("headings");
        std::fs::write(dir.join("doc.md"), "# A\n\nbody\n\n## B\n\nmore\n\n# C\n").unwrap();
        let mut app = App::new(&dir, None, 0);
        app.open_file(&dir.join("doc.md"));
        app.ensure_wrapped(40);
        app.view_h = 5; // 无真实渲染时手动给可视高度，否则钳制恒为 0
        assert!(app.headings.len() >= 3, "found: {:?}", app.headings);
        assert_eq!(app.scroll, 0);
        app.jump_heading(1);
        assert!(app.scroll > 0, "moved to next heading");
        let second = app.scroll;
        app.jump_heading(1);
        assert!(app.scroll >= second, "keeps going down");
        app.jump_heading(-1);
        assert!(app.scroll <= second, "goes back up");
        assert!(app.current_section().is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mouse_click_and_wheel() {
        use crossterm::event::{MouseButton, MouseEventKind};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::layout::Rect;

        let dir = sample_project("mouse");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("note.md"), "# N\n").unwrap();
        // 空行分隔：Markdown 单换行只是段内软折行，必须用空行才能成 100 个段落
        let long = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        std::fs::write(dir.join("a.md"), format!("# A\n\n{long}\n")).unwrap();

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut app = App::new(&dir, None, 0);
        terminal.draw(|f| crate::ui::render(f, &mut app)).unwrap();
        assert!(app.side_area.width > 0 && app.prev_area.width > 0);

        // rows: [sub, readme.md?..] 实际：sub(dir) / a.md / code.txt? show_all=false → code.txt 不可见
        // 展开 sub，再点 note.md
        app.tree.cursor = 0;
        app.open_selected(); // sub 在 cursor 0（目录优先）
        terminal.draw(|f| crate::ui::render(f, &mut app)).unwrap();
        let y_note = app.side_area.y + 1 + 1; // sub(0) note(1)
        let x = app.side_area.x + 2;
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), x, y_note);
        assert!(
            app.current_file
                .as_ref()
                .is_some_and(|p| p.ends_with("note.md"))
        );
        assert_eq!(app.focus, Focus::Preview);

        // 点目录行 → 收起
        let y_sub = app.side_area.y + 1;
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), x, y_sub);
        assert!(!app.tree.rows.iter().any(|r| r.name == "note.md"));
        // 再点 → 展开
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), x, y_sub);
        assert!(app.tree.rows.iter().any(|r| r.name == "note.md"));

        // 滚轮：预览区向下两格 = 6 行
        app.open_file(&dir.join("a.md"));
        terminal.draw(|f| crate::ui::render(f, &mut app)).unwrap();
        let px = app.prev_area.x + 2;
        let py = app.prev_area.y + 2;
        app.on_mouse(MouseEventKind::ScrollDown, px, py);
        eprintln!("after 1 scroll={}", app.scroll);
        app.on_mouse(MouseEventKind::ScrollDown, px, py);
        assert_eq!(app.scroll, 6);
        app.on_mouse(MouseEventKind::ScrollUp, px, py);
        assert_eq!(app.scroll, 3);

        // 滚轮在侧栏 → 移动光标
        let before = app.tree.cursor;
        app.on_mouse(
            MouseEventKind::ScrollDown,
            app.side_area.x + 1,
            app.side_area.y + 1,
        );
        assert!(app.tree.cursor >= before);

        // 点 header 空白 → 无事发生
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), 0, 0);
        // 点预览 → 焦点切预览
        app.focus = Focus::Sidebar;
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), px, py);
        assert_eq!(app.focus, Focus::Preview);

        // 区域外坐标不炸
        app.on_mouse(MouseEventKind::Down(MouseButton::Left), 200, 200);
        app.side_area = Rect::default();
        app.prev_area = Rect::default();
        app.on_mouse(MouseEventKind::ScrollDown, 5, 5);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn outline_toggle_and_navigate() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let dir = sample_project("outline");
        std::fs::write(
            dir.join("doc.md"),
            "# A\n\nt\n\n## B\n\nt\n\n### C\n\nt\n\n# D\n",
        )
        .unwrap();
        let mut app = App::new(&dir, None, 0);
        // 默认关闭
        assert!(!app.outline_visible);
        app.open_file(&dir.join("doc.md"));
        app.ensure_wrapped(40);
        app.view_h = 5;
        // 索引带 level
        let levels: Vec<u8> = app.headings.iter().map(|(_, l, _)| *l).collect();
        assert_eq!(levels, vec![1, 2, 3, 1]);
        // o 打开并聚焦
        app.on_key(KeyCode::Char('o'), KeyModifiers::NONE);
        assert!(app.outline_visible);
        assert_eq!(app.focus, Focus::Outline);
        // 大纲内 j 跳到 B
        app.on_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.outline_cursor, 1);
        assert!(app.scroll > 0);
        // k 回到 A
        app.on_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.outline_cursor, 0);
        assert_eq!(app.scroll, 0);
        // G 到最后
        app.on_key(KeyCode::Char('G'), KeyModifiers::NONE);
        assert_eq!(app.outline_cursor, 3);
        // h 回预览
        app.on_key(KeyCode::Char('h'), KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Preview);
        // Tab 在三个可见窗格间轮转
        app.on_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Outline);
        app.on_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Sidebar);
        app.on_key(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Outline);
        // o 关闭回预览
        app.focus = Focus::Outline;
        app.on_key(KeyCode::Char('o'), KeyModifiers::NONE);
        assert!(!app.outline_visible);
        assert_eq!(app.focus, Focus::Preview);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidebar_toggle_keys() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let dir = sample_project("sidebar-keys");
        let mut app = App::new(&dir, None, 0);
        assert!(app.sidebar_visible);
        // b 隐藏
        app.on_key(KeyCode::Char('b'), KeyModifiers::NONE);
        assert!(!app.sidebar_visible);
        assert_eq!(app.focus, Focus::Preview);
        // e 显示并聚焦
        app.on_key(KeyCode::Char('e'), KeyModifiers::NONE);
        assert!(app.sidebar_visible);
        assert_eq!(app.focus, Focus::Sidebar);
        // Ctrl+B 同样可用
        app.on_key(KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(!app.sidebar_visible);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_cycle_wraps() {
        use crate::theme::{THEME_TEST_LOCK, sandbox_theme_file, unsandbox_theme_file};
        let _guard = THEME_TEST_LOCK.lock().unwrap();
        let dir = sample_project("theme");
        sandbox_theme_file(&dir.join("themefile"));
        let mut app = App::new(&dir, None, 0);
        let n = THEMES.len();
        app.cycle_theme(-1);
        assert_eq!(app.theme_idx, n - 1);
        app.cycle_theme(1);
        assert_eq!(app.theme_idx, 0);
        // 持久化真的落盘了，且下次能读回
        assert_eq!(
            crate::theme::load_saved_theme().as_deref(),
            Some(THEMES[0].name)
        );
        unsandbox_theme_file();
        let _ = fs::remove_dir_all(&dir);
    }
}
