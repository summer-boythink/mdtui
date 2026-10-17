//! 多主题系统：6 套精心调色的深色 / 浅色主题，`t` / `T` 切换，选择自动持久化。

use ratatui::style::Color;
use std::path::PathBuf;

/// 把 0xRRGGBB 转成终端 RGB 颜色（const，可用于静态主题表）。
const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// 一套主题负责 UI 全部配色。新增主题只需在这里加一行静态配置。
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    // ---- 界面 ----
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub focus: Color,
    pub accent: Color,
    pub header_bg: Color,
    pub header_fg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub warn: Color,
    // ---- Markdown ----
    pub h1: Color,
    pub h2: Color,
    pub h3: Color,
    pub bold: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub code_lang: Color,
    pub quote_bar: Color,
    pub quote_fg: Color,
    pub link: Color,
    pub link_url: Color,
    pub list: Color,
    pub hr: Color,
    pub table_head: Color,
    // ---- 文件树 ----
    pub dir: Color,
    pub file_md: Color,
    pub file_other: Color,
    pub sel_bg: Color,
    pub sel_fg: Color,
    // ---- 搜索 ----
    pub match_bg: Color,
    pub match_fg: Color,
    pub match_cur_bg: Color,
    pub match_cur_fg: Color,
}

pub const THEMES: [Theme; 6] = [
    Theme {
        name: "Tokyo Night",
        bg: rgb(0x1a1b26),
        fg: rgb(0xc0caf5),
        muted: rgb(0x565f89),
        border: rgb(0x3b4261),
        focus: rgb(0x7aa2f7),
        accent: rgb(0x7aa2f7),
        header_bg: rgb(0x24283b),
        header_fg: rgb(0xc0caf5),
        status_bg: rgb(0x16161e),
        status_fg: rgb(0x9aa5ce),
        warn: rgb(0xe0af68),
        h1: rgb(0x7aa2f7),
        h2: rgb(0xbb9af7),
        h3: rgb(0x7dcfff),
        bold: rgb(0xc0caf5),
        code_fg: rgb(0xc0caf5),
        code_bg: rgb(0x24283b),
        code_lang: rgb(0x565f89),
        quote_bar: rgb(0x7aa2f7),
        quote_fg: rgb(0x9aa5ce),
        link: rgb(0x7dcfff),
        link_url: rgb(0x565f89),
        list: rgb(0x7aa2f7),
        hr: rgb(0x3b4261),
        table_head: rgb(0x7aa2f7),
        dir: rgb(0x7aa2f7),
        file_md: rgb(0xc0caf5),
        file_other: rgb(0x565f89),
        sel_bg: rgb(0x33467c),
        sel_fg: rgb(0xc0caf5),
        match_bg: rgb(0xe0af68),
        match_fg: rgb(0x1a1b26),
        match_cur_bg: rgb(0xbb9af7),
        match_cur_fg: rgb(0x1a1b26),
    },
    Theme {
        name: "Nord",
        bg: rgb(0x2e3440),
        fg: rgb(0xeceff4),
        muted: rgb(0x616e88),
        border: rgb(0x4c566a),
        focus: rgb(0x88c0d0),
        accent: rgb(0x88c0d0),
        header_bg: rgb(0x3b4252),
        header_fg: rgb(0xeceff4),
        status_bg: rgb(0x232731),
        status_fg: rgb(0xd8dee9),
        warn: rgb(0xebcb8b),
        h1: rgb(0x88c0d0),
        h2: rgb(0x81a1c1),
        h3: rgb(0x8fbcbb),
        bold: rgb(0xeceff4),
        code_fg: rgb(0xeceff4),
        code_bg: rgb(0x3b4252),
        code_lang: rgb(0x616e88),
        quote_bar: rgb(0x88c0d0),
        quote_fg: rgb(0xd8dee9),
        link: rgb(0x88c0d0),
        link_url: rgb(0x616e88),
        list: rgb(0x88c0d0),
        hr: rgb(0x4c566a),
        table_head: rgb(0x8fbcbb),
        dir: rgb(0x81a1c1),
        file_md: rgb(0xeceff4),
        file_other: rgb(0x616e88),
        sel_bg: rgb(0x434c5e),
        sel_fg: rgb(0xeceff4),
        match_bg: rgb(0xebcb8b),
        match_fg: rgb(0x2e3440),
        match_cur_bg: rgb(0x88c0d0),
        match_cur_fg: rgb(0x2e3440),
    },
    Theme {
        name: "Dracula",
        bg: rgb(0x282a36),
        fg: rgb(0xf8f8f2),
        muted: rgb(0x6272a4),
        border: rgb(0x44475a),
        focus: rgb(0xbd93f9),
        accent: rgb(0xbd93f9),
        header_bg: rgb(0x343746),
        header_fg: rgb(0xf8f8f2),
        status_bg: rgb(0x21222c),
        status_fg: rgb(0xf8f8f2),
        warn: rgb(0xffb86c),
        h1: rgb(0xbd93f9),
        h2: rgb(0xff79c6),
        h3: rgb(0x8be9fd),
        bold: rgb(0xf8f8f2),
        code_fg: rgb(0xf8f8f2),
        code_bg: rgb(0x343746),
        code_lang: rgb(0x6272a4),
        quote_bar: rgb(0xbd93f9),
        quote_fg: rgb(0xbfbecb),
        link: rgb(0x8be9fd),
        link_url: rgb(0x6272a4),
        list: rgb(0xff79c6),
        hr: rgb(0x44475a),
        table_head: rgb(0x50fa7b),
        dir: rgb(0x8be9fd),
        file_md: rgb(0xf8f8f2),
        file_other: rgb(0x6272a4),
        sel_bg: rgb(0x44475a),
        sel_fg: rgb(0xf8f8f2),
        match_bg: rgb(0xf1fa8c),
        match_fg: rgb(0x282a36),
        match_cur_bg: rgb(0xff79c6),
        match_cur_fg: rgb(0x282a36),
    },
    Theme {
        name: "Gruvbox Dark",
        bg: rgb(0x282828),
        fg: rgb(0xebdbb2),
        muted: rgb(0x928374),
        border: rgb(0x504945),
        focus: rgb(0xfabd2f),
        accent: rgb(0xfabd2f),
        header_bg: rgb(0x3c3836),
        header_fg: rgb(0xebdbb2),
        status_bg: rgb(0x1d2021),
        status_fg: rgb(0xd5c4a1),
        warn: rgb(0xfb4934),
        h1: rgb(0xfabd2f),
        h2: rgb(0xfe8019),
        h3: rgb(0x8ec07c),
        bold: rgb(0xebdbb2),
        code_fg: rgb(0xebdbb2),
        code_bg: rgb(0x3c3836),
        code_lang: rgb(0x928374),
        quote_bar: rgb(0xfabd2f),
        quote_fg: rgb(0xd5c4a1),
        link: rgb(0x83a598),
        link_url: rgb(0x928374),
        list: rgb(0xfe8019),
        hr: rgb(0x504945),
        table_head: rgb(0x8ec07c),
        dir: rgb(0x83a598),
        file_md: rgb(0xebdbb2),
        file_other: rgb(0x928374),
        sel_bg: rgb(0x504945),
        sel_fg: rgb(0xebdbb2),
        match_bg: rgb(0xfabd2f),
        match_fg: rgb(0x282828),
        match_cur_bg: rgb(0xfe8019),
        match_cur_fg: rgb(0x282828),
    },
    Theme {
        name: "GitHub Light",
        bg: rgb(0xffffff),
        fg: rgb(0x1f2328),
        muted: rgb(0x59636e),
        border: rgb(0xd1d9e0),
        focus: rgb(0x0969da),
        accent: rgb(0x0969da),
        header_bg: rgb(0xf6f8fa),
        header_fg: rgb(0x1f2328),
        status_bg: rgb(0x1f2328),
        status_fg: rgb(0xffffff),
        warn: rgb(0xcf222e),
        h1: rgb(0x0969da),
        h2: rgb(0x8250df),
        h3: rgb(0x1a7f37),
        bold: rgb(0x1f2328),
        code_fg: rgb(0x1f2328),
        code_bg: rgb(0xf6f8fa),
        code_lang: rgb(0x59636e),
        quote_bar: rgb(0xd1d9e0),
        quote_fg: rgb(0x59636e),
        link: rgb(0x0969da),
        link_url: rgb(0x59636e),
        list: rgb(0x8250df),
        hr: rgb(0xd1d9e0),
        table_head: rgb(0x1f2328),
        dir: rgb(0x0969da),
        file_md: rgb(0x1f2328),
        file_other: rgb(0x59636e),
        sel_bg: rgb(0x0969da),
        sel_fg: rgb(0xffffff),
        match_bg: rgb(0xfff8c5),
        match_fg: rgb(0x1f2328),
        match_cur_bg: rgb(0x0969da),
        match_cur_fg: rgb(0xffffff),
    },
    Theme {
        name: "Solarized Light",
        bg: rgb(0xfdf6e3),
        fg: rgb(0x657b83),
        muted: rgb(0x93a1a1),
        border: rgb(0xd9cfb0),
        focus: rgb(0x268bd2),
        accent: rgb(0x268bd2),
        header_bg: rgb(0xeee8d5),
        header_fg: rgb(0x586e75),
        status_bg: rgb(0x073642),
        status_fg: rgb(0xeee8d5),
        warn: rgb(0xdc322f),
        h1: rgb(0x268bd2),
        h2: rgb(0x2aa198),
        h3: rgb(0x859900),
        bold: rgb(0x586e75),
        code_fg: rgb(0x586e75),
        code_bg: rgb(0xeee8d5),
        code_lang: rgb(0x93a1a1),
        quote_bar: rgb(0x268bd2),
        quote_fg: rgb(0x839496),
        link: rgb(0x268bd2),
        link_url: rgb(0x93a1a1),
        list: rgb(0xcb4b16),
        hr: rgb(0xd9cfb0),
        table_head: rgb(0x859900),
        dir: rgb(0x268bd2),
        file_md: rgb(0x586e75),
        file_other: rgb(0x93a1a1),
        sel_bg: rgb(0x268bd2),
        sel_fg: rgb(0xfdf6e3),
        match_bg: rgb(0xb58900),
        match_fg: rgb(0xfdf6e3),
        match_cur_bg: rgb(0xcb4b16),
        match_cur_fg: rgb(0xfdf6e3),
    },
];

/// 按名称找主题下标（大小写不敏感）。
pub fn theme_index_by_name(name: &str) -> Option<usize> {
    let want = name.trim().to_ascii_lowercase();
    THEMES
        .iter()
        .position(|t| t.name.to_ascii_lowercase() == want)
}

pub fn theme_names() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.name).collect()
}

fn config_file() -> Option<PathBuf> {
    // MDTUI_THEME_FILE 便于测试与自定义（优先于默认路径）。
    if let Some(p) = std::env::var_os("MDTUI_THEME_FILE") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("mdtui").join("theme"))
}

/// 读上次保存的主题名。读不到就返回 None（静默降级，不打扰用户）。
pub fn load_saved_theme() -> Option<String> {
    let path = config_file()?;
    let s = std::fs::read_to_string(path).ok()?;
    let name = s.lines().next()?.trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// 最佳努力保存主题名：失败也无所谓，直接忽略错误。
pub fn save_theme(name: &str) {
    if let Some(path) = config_file()
        && let Some(parent) = path.parent()
    {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(path, format!("{name}\n"));
    }
}

#[cfg(test)]
pub static THEME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 测试专用：把主题持久化重定向到临时文件，避免污染用户真实配置。
/// 调用方必须先持有 [`THEME_TEST_LOCK`]（环境变量是进程全局的）。
#[cfg(test)]
pub fn sandbox_theme_file(path: &std::path::Path) {
    // edition 2024 下 set/remove_var 是 unsafe；测试里由全局锁串行保护。
    unsafe {
        std::env::set_var("MDTUI_THEME_FILE", path);
    }
}

/// 撤销 [`sandbox_theme_file`]。
#[cfg(test)]
pub fn unsandbox_theme_file() {
    unsafe {
        std::env::remove_var("MDTUI_THEME_FILE");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_unique_and_findable() {
        let names = theme_names();
        assert_eq!(names.len(), THEMES.len());
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            assert!(seen.insert(*n), "duplicate theme name: {n}");
            assert!(theme_index_by_name(n).is_some());
        }
        // 大小写不敏感
        assert_eq!(theme_index_by_name("nord"), theme_index_by_name("Nord"));
        assert!(theme_index_by_name("不存在").is_none());
    }

    #[test]
    fn themes_have_readable_pairs() {
        // 选中态和搜索高亮的前景/背景不能相同，否则看不清
        for t in &THEMES {
            assert_ne!(t.sel_bg, t.sel_fg, "{} sel", t.name);
            assert_ne!(t.match_bg, t.match_fg, "{} match", t.name);
            assert_ne!(t.bg, t.fg, "{} bg/fg", t.name);
        }
    }
}
