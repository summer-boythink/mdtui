//! 界面语言：英文默认，`L` 切换中英文，选择自动记住。
//!
//! 纯静态表驱动：新增文案给 [`Texts`] 加字段并填 EN/ZH 两份；
//! 带参数的动态文案走 [`Lang`] 上的同名方法（中英语序不同，硬拼模板易错）。

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Zh,
}

impl Lang {
    pub fn code(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Lang::En),
            "zh" | "cn" | "chinese" | "中文" => Some(Lang::Zh),
            _ => None,
        }
    }

    pub fn texts(&self) -> &'static Texts {
        match self {
            Lang::En => &EN,
            Lang::Zh => &ZH,
        }
    }

    // ---------- 动态文案 ----------

    pub fn theme_label(&self, name: &str) -> String {
        match self {
            Lang::En => format!("Theme {name} (t to switch) "),
            Lang::Zh => format!("主题 {name} (t 切换) "),
        }
    }

    pub fn search_status(&self, q: &str, cur: usize, n: usize) -> String {
        match self {
            Lang::En => format!("/{q}  {cur}/{n}  (Enter to confirm · Esc to cancel)"),
            Lang::Zh => format!("/{q}  {cur}/{n}  (Enter 确认 · Esc 取消)"),
        }
    }

    pub fn pos_status(&self, cur: usize, total: usize, pct: usize) -> String {
        match self {
            Lang::En => format!("Ln {cur}/{total} · {pct}% "),
            Lang::Zh => format!("行 {cur}/{total} · {pct}% "),
        }
    }

    pub fn theme_changed(&self, name: &str) -> String {
        match self {
            Lang::En => format!("Theme: {name}"),
            Lang::Zh => format!("主题：{name}"),
        }
    }

    pub fn lang_changed(&self) -> String {
        match self {
            Lang::En => "Language: English".to_string(),
            Lang::Zh => "语言：中文".to_string(),
        }
    }

    pub fn file_too_big(&self, mb: f64) -> String {
        match self {
            Lang::En => format!("File {mb:.1} MB exceeds preview limit (5 MB), not loaded"),
            Lang::Zh => format!("文件 {mb:.1} MB，超过预览上限（5 MB），未加载"),
        }
    }

    pub fn binary_file(&self) -> String {
        match self {
            Lang::En => "Probably a binary file, cannot preview".to_string(),
            Lang::Zh => "可能是二进制文件，无法预览".to_string(),
        }
    }

    pub fn text_preview(&self) -> String {
        match self {
            Lang::En => "Not Markdown, previewing as plain text".to_string(),
            Lang::Zh => "非 Markdown 文件，按纯文本预览".to_string(),
        }
    }

    pub fn read_failed(&self, e: &str) -> String {
        match self {
            Lang::En => format!("Read failed: {e}"),
            Lang::Zh => format!("读取失败：{e}"),
        }
    }

    pub fn dir_unreadable(&self) -> String {
        match self {
            Lang::En => "Directory unreadable (permission?)".to_string(),
            Lang::Zh => "该目录无法读取（权限不足？）".to_string(),
        }
    }

    pub fn refreshed(&self) -> String {
        match self {
            Lang::En => "Refreshed".to_string(),
            Lang::Zh => "已刷新".to_string(),
        }
    }

    pub fn wrap_on(&self) -> String {
        match self {
            Lang::En => "Word wrap: on".to_string(),
            Lang::Zh => "自动换行：开".to_string(),
        }
    }

    pub fn wrap_off(&self) -> String {
        match self {
            Lang::En => "Word wrap: off (wide lines clipped)".to_string(),
            Lang::Zh => "自动换行：关（超宽裁剪）".to_string(),
        }
    }

    pub fn show_all(&self) -> String {
        match self {
            Lang::En => "Showing all files (incl. hidden/non-md)".to_string(),
            Lang::Zh => "显示全部文件（含隐藏/非 md）".to_string(),
        }
    }

    pub fn show_md_only(&self) -> String {
        match self {
            Lang::En => "Dirs + Markdown only".to_string(),
            Lang::Zh => "只显示目录 + Markdown".to_string(),
        }
    }

    pub fn file_reloaded(&self) -> String {
        match self {
            Lang::En => "File reloaded".to_string(),
            Lang::Zh => "文件已重载".to_string(),
        }
    }

    pub fn no_match(&self) -> String {
        match self {
            Lang::En => "No match".to_string(),
            Lang::Zh => "无匹配".to_string(),
        }
    }

    pub fn matches_n(&self, n: usize) -> String {
        match self {
            Lang::En => format!("{n} matches (n/N to jump)"),
            Lang::Zh => format!("{n} 处匹配（n/N 跳转）"),
        }
    }

    pub fn sidebar_all_title(&self, root: &str) -> String {
        match self {
            Lang::En => format!(" {root} · all "),
            Lang::Zh => format!(" {root} · 全部 "),
        }
    }

    pub fn unknown_theme(&self, got: &str, avail: &str) -> String {
        match self {
            Lang::En => format!("Unknown theme {got:?}, available: {avail}"),
            Lang::Zh => format!("未知主题 {got:?}，可用：{avail}"),
        }
    }

    pub fn list_themes_header(&self) -> String {
        match self {
            Lang::En => "Available themes (`mdtui -t <name>` or press t):".to_string(),
            Lang::Zh => "可用主题（`mdtui -t <名称>` 或按 t 切换）：".to_string(),
        }
    }

    pub fn path_missing(&self, p: &str) -> String {
        match self {
            Lang::En => format!("Path not found: {p}"),
            Lang::Zh => format!("路径不存在：{p}"),
        }
    }

    pub fn path_unsupported(&self, p: &str) -> String {
        match self {
            Lang::En => format!("Unsupported path type: {p}"),
            Lang::Zh => format!("不支持的路径类型：{p}"),
        }
    }

    pub fn unknown_lang(&self, got: &str) -> String {
        match self {
            Lang::En => format!("Unknown language {got:?}, use en or zh"),
            Lang::Zh => format!("未知语言 {got:?}，用 en 或 zh"),
        }
    }
}

/// 全部静态文案。字段名用英文语义，两个实例逐一对照填写。
pub struct Texts {
    pub term_too_small: &'static str,
    pub empty_dir: &'static str,
    pub outline_open_title: &'static str,
    pub no_file_outline: &'static str,
    pub no_headings: &'static str,
    pub help_title: &'static str,
    pub preview_title: &'static str,
    // 欢迎页
    pub welcome_sub: &'static str,
    pub pick_a: &'static str,
    pub in_tree: &'static str,
    pub press: &'static str,
    pub enter_key: &'static str,
    pub to_preview: &'static str,
    pub switch_theme_mid: &'static str,
    pub for_keys: &'static str,
    pub welcome_note: &'static str,
    // 帮助页分区
    pub h_move: &'static str,
    pub h_sidebar: &'static str,
    pub h_preview: &'static str,
    pub h_outline: &'static str,
    pub h_mouse: &'static str,
    pub h_common: &'static str,
    // 帮助条目
    pub d_move: &'static str,
    pub d_topbottom: &'static str,
    pub d_page: &'static str,
    pub d_nextpage: &'static str,
    pub d_open: &'static str,
    pub d_collapse: &'static str,
    pub d_tab: &'static str,
    pub d_sidebar: &'static str,
    pub d_width: &'static str,
    pub d_filter: &'static str,
    pub d_refresh: &'static str,
    pub d_wrap: &'static str,
    pub d_reload: &'static str,
    pub d_search: &'static str,
    pub d_heading: &'static str,
    pub d_theme: &'static str,
    pub d_lang: &'static str,
    pub d_outline_toggle: &'static str,
    pub d_outline_nav: &'static str,
    pub d_outline_back: &'static str,
    pub d_mouse_click: &'static str,
    pub d_mouse_wheel: &'static str,
    pub d_mouse_tip: &'static str,
    pub d_help_close: &'static str,
    pub d_quit: &'static str,
    // 底栏 hint
    pub hint_sidebar: &'static str,
    pub hint_preview: &'static str,
    pub hint_outline: &'static str,
    // 按键标签（中英标点习惯不同）
    pub key_tab: &'static str,
    pub key_toggle: &'static str,
    pub key_click: &'static str,
    pub key_wheel: &'static str,
    pub key_space: &'static str,
    pub key_open: &'static str,
    pub key_outline_go: &'static str,
    // 内部错误上下文
    pub err_term_init: &'static str,
    pub err_draw: &'static str,
    pub err_poll: &'static str,
    pub err_read_ev: &'static str,
}

pub const EN: Texts = Texts {
    term_too_small: "Terminal too small, please enlarge",
    empty_dir: "  (empty)",
    outline_open_title: " Outline (o to close) ",
    no_file_outline: "Open a doc for outline",
    no_headings: "No headings here",
    help_title: " Help (? to close) ",
    preview_title: " Preview ",
    welcome_sub: "     Markdown reader in your terminal",
    pick_a: "Pick a file with ",
    in_tree: " in the left tree",
    press: "Press ",
    enter_key: "Enter",
    to_preview: " to preview",
    switch_theme_mid: " to switch theme, ",
    for_keys: " for all keys",
    welcome_note: "Only dirs + Markdown listed by default; a for all files; mouse supported",
    h_move: " Navigate",
    h_sidebar: " Sidebar",
    h_preview: " Preview",
    h_outline: " Outline (right, off by default)",
    h_mouse: " Mouse",
    h_common: " General",
    d_move: "Move up/down (tree / doc / this page)",
    d_topbottom: "Top / bottom",
    d_page: "Page scroll (preview & help)",
    d_nextpage: "Next page",
    d_open: "Expand dir / open file",
    d_collapse: "Collapse dir / go to parent",
    d_tab: "Switch focus (wheel follows mouse, no need)",
    d_sidebar: "Show & focus / hide sidebar",
    d_width: "Sidebar narrower / wider",
    d_filter: "Markdown only ⇄ all files",
    d_refresh: "Refresh tree and current file",
    d_wrap: "Word wrap on / off",
    d_reload: "Reload current file",
    d_search: "Search / next / previous",
    d_heading: "Previous / next heading",
    d_theme: "Next / previous theme (remembered)",
    d_lang: "Switch UI language (remembered)",
    d_outline_toggle: "Open / close outline (opens focused)",
    d_outline_nav: "Move in outline and jump",
    d_outline_back: "Back to preview from outline",
    d_mouse_click: "Sidebar: click file to open / dir to expand; click preview to focus",
    d_mouse_wheel: "Scroll the pane under cursor (this page in help)",
    d_mouse_tip: "Tip: hold Shift for native text selection while mouse is captured",
    d_help_close: "Close this help",
    d_quit: "Quit",
    hint_sidebar: " j/k move · l/Enter open · o outline · Tab switch · ? help · q quit",
    hint_preview: " j/k scroll · / search · b/e tree · o outline · ? help · q quit",
    hint_outline: " j/k jump · Enter go · h back · o close · ? help · q quit",
    key_tab: "Tab · click",
    key_toggle: "b/e (Ctrl+B)",
    key_click: "Left click",
    key_wheel: "Wheel",
    key_space: "Space",
    key_open: "l/→/Enter",
    key_outline_go: "j/k · Enter",
    err_term_init: "failed to init terminal",
    err_draw: "draw failed",
    err_poll: "event poll failed",
    err_read_ev: "event read failed",
};

pub const ZH: Texts = Texts {
    term_too_small: "窗口太小，请放大终端后使用",
    empty_dir: "  (空目录)",
    outline_open_title: " 大纲 (o 关闭) ",
    no_file_outline: "打开文档后显示",
    no_headings: "本文无标题",
    help_title: " 帮助 (? 关闭) ",
    preview_title: " 预览 ",
    welcome_sub: "     终端里的 Markdown 阅读器",
    pick_a: "在左侧文件树用 ",
    in_tree: " 选择文件",
    press: "按 ",
    enter_key: "回车",
    to_preview: " 打开预览",
    switch_theme_mid: " 切换主题，按 ",
    for_keys: " 查看全部按键",
    welcome_note: "默认只列出目录与 Markdown；按 a 查看全部文件；鼠标点选滚轮均可",
    h_move: " 移动",
    h_sidebar: " 侧边栏",
    h_preview: " 预览",
    h_outline: " 大纲（右侧，默认关闭）",
    h_mouse: " 鼠标",
    h_common: " 通用",
    d_move: "上下移动（侧栏 / 文档 / 本页通用）",
    d_topbottom: "顶部 / 底部",
    d_page: "翻页（预览与帮助）",
    d_nextpage: "下一页",
    d_open: "展开目录 / 打开文件",
    d_collapse: "收起目录 / 回到父目录",
    d_tab: "切换侧栏 / 预览焦点（滚轮指哪滚哪，不用切）",
    d_sidebar: "显示并聚焦 / 隐藏侧栏",
    d_width: "侧栏变窄 / 变宽",
    d_filter: "仅 Markdown ⇄ 显示全部文件",
    d_refresh: "刷新文件树与当前文件",
    d_wrap: "自动换行开 / 关",
    d_reload: "重载当前文件",
    d_search: "搜索 / 下一个 / 上一个",
    d_heading: "上一个 / 下一个标题",
    d_theme: "下一个 / 上一个主题（自动记住）",
    d_lang: "切换界面语言（自动记住）",
    d_outline_toggle: "打开 / 关闭大纲（打开即聚焦）",
    d_outline_nav: "大纲内移动并跳转章节",
    d_outline_back: "从大纲回到预览",
    d_mouse_click: "侧栏：选文件即打开 / 点目录即展开；点预览切焦点",
    d_mouse_wheel: "所在窗格滚动（帮助页里滚本页）",
    d_mouse_tip: "提示：鼠标接管后，终端原生框选复制请按住 Shift",
    d_help_close: "关闭本帮助",
    d_quit: "退出",
    hint_sidebar: " j/k 移动 · l/回车 打开 · o 大纲 · Tab 切换 · ? 帮助 · q 退出",
    hint_preview: " j/k 滚动 · / 搜索 · b/e 侧栏 · o 大纲 · ? 帮助 · q 退出",
    hint_outline: " j/k 跳章节 · 回车 定位 · h 回正文 · o 关闭 · ? 帮助 · q 退出",
    key_tab: "Tab · 点击",
    key_toggle: "b/e（Ctrl+B）",
    key_click: "左键点击",
    key_wheel: "滚轮",
    key_space: "空格",
    key_open: "l/→/回车",
    key_outline_go: "j/k · 回车",
    err_term_init: "终端初始化失败",
    err_draw: "绘制失败",
    err_poll: "事件轮询失败",
    err_read_ev: "读取事件失败",
};

fn config_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MDTUI_LANG_FILE") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("mdtui").join("lang"))
}

/// 读上次保存的语言，默认英文。
pub fn load_saved_lang() -> Lang {
    let path = match config_file() {
        Some(p) => p,
        None => return Lang::En,
    };
    let s = std::fs::read_to_string(path).unwrap_or_default();
    Lang::from_code(s.lines().next().unwrap_or("").trim()).unwrap_or(Lang::En)
}

/// 最佳努力保存语言，失败静默忽略。
pub fn save_lang(lang: Lang) {
    if let Some(path) = config_file()
        && let Some(parent) = path.parent()
    {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(path, format!("{}\n", lang.code()));
    }
}

/// 测试专用：把语言持久化重定向到临时文件，避免污染用户真实配置。
/// 调用方必须先持有主题模块的 [`crate::theme::THEME_TEST_LOCK`]
///（环境变量是进程全局的，跨测试用同一把锁串行）。
#[cfg(test)]
pub fn sandbox_lang_file(path: &std::path::Path) {
    unsafe {
        std::env::set_var("MDTUI_LANG_FILE", path);
    }
}

/// 撤销 [`sandbox_lang_file`]。
#[cfg(test)]
pub fn unsandbox_lang_file() {
    unsafe {
        std::env::remove_var("MDTUI_LANG_FILE");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_roundtrip() {
        assert_eq!(Lang::from_code("en"), Some(Lang::En));
        assert_eq!(Lang::from_code("ZH"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("中文"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("xx"), None);
        assert_eq!(Lang::En.code(), "en");
    }

    #[test]
    fn texts_filled_both_langs() {
        // 关键文案中英都必须非空
        for t in [&EN, &ZH] {
            for s in [
                t.term_too_small,
                t.empty_dir,
                t.outline_open_title,
                t.no_headings,
                t.help_title,
                t.preview_title,
                t.welcome_sub,
                t.welcome_note,
                t.h_move,
                t.h_sidebar,
                t.h_preview,
                t.h_outline,
                t.h_mouse,
                t.h_common,
                t.d_move,
                t.d_search,
                t.d_quit,
                t.d_lang,
                t.hint_sidebar,
                t.hint_preview,
                t.hint_outline,
            ] {
                assert!(!s.is_empty());
            }
        }
    }
}
