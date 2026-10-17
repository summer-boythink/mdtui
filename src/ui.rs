//! 界面渲染：顶栏标题 / 侧边文件树 / 预览 / 底栏状态，全部在此组装。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, InputMode};
use crate::theme::Theme;

pub fn render(f: &mut Frame, app: &mut App) {
    app.clear_expired_message();
    let theme = app.theme();
    let area = f.area();
    if area.width < 20 || area.height < 6 {
        f.render_widget(Paragraph::new("窗口太小，请放大终端后使用"), area);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    render_header(f, app, &theme, rows[0]);
    render_body(f, app, &theme, rows[1]);
    render_footer(f, app, &theme, rows[2]);
}

fn render_header(f: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    let file = app
        .current_file
        .as_ref()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "--".to_string());
    let left = format!(" ◈ mdtui   {} ", file);
    let right = format!("主题 {} (t 切换) ", theme.name);
    let w = area.width as usize;
    let pad = w.saturating_sub(
        UnicodeWidthStr::width(left.as_str()) + UnicodeWidthStr::width(right.as_str()),
    );
    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme.header_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, Style::default().fg(theme.header_fg)),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.header_bg)),
        area,
    );
}

fn render_body(f: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    if app.sidebar_visible {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(app.sidebar_width), Constraint::Min(10)])
            .split(area);
        app.side_area = cols[0];
        app.prev_area = cols[1];
        render_sidebar(f, app, theme, cols[0]);
        render_preview(f, app, theme, cols[1]);
    } else {
        app.side_area = ratatui::layout::Rect::default();
        app.prev_area = area;
        render_preview(f, app, theme, area);
    }
}

fn sidebar_block(app: &App, theme: &Theme, title: String) -> Block<'static> {
    let focused = app.focus == Focus::Sidebar;
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.focus } else { theme.border }))
        .title(Span::styled(
            title,
            Style::default().fg(if focused { theme.focus } else { theme.muted }),
        ))
        .style(Style::default().bg(theme.bg))
}

fn render_sidebar(f: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let t = *theme;
    let block = sidebar_block(app, theme, app.sidebar_title());
    let items: Vec<ListItem> = if app.tree.rows.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "  (空目录)",
            Style::default().fg(t.muted),
        )]))]
    } else {
        app.tree
            .rows
            .iter()
            .map(|r| {
                let mut spans = vec![Span::styled(r.prefix.clone(), Style::default().fg(t.muted))];
                if r.is_dir {
                    let icon = if r.expanded { "▾ " } else { "▸ " };
                    spans.push(Span::styled(
                        icon,
                        Style::default().fg(t.dir).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        r.name.clone(),
                        Style::default().fg(t.dir).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::raw("  "));
                    let is_md = r
                        .path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| {
                            matches!(
                                e.to_ascii_lowercase().as_str(),
                                "md" | "markdown" | "mdown" | "mkdn" | "mkd"
                            )
                        });
                    spans.push(Span::styled(
                        r.name.clone(),
                        Style::default().fg(if is_md { t.file_md } else { t.file_other }),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(t.sel_bg).fg(t.sel_fg))
        .highlight_symbol("");
    // 持久选中态 + 保证光标可见（之前每帧新建 state，offset 恒 0，长树会滚丢光标）
    let inner_h = (area.height as usize).saturating_sub(2).max(1);
    let rows = app.tree.rows.len();
    let cursor = app.tree.cursor.min(rows.saturating_sub(1));
    app.tree.cursor = cursor;
    let mut state = std::mem::take(&mut app.side_state);
    let mut offset = state.offset().min(rows.saturating_sub(inner_h));
    if cursor < offset {
        offset = cursor;
    } else if cursor >= offset + inner_h {
        offset = cursor + 1 - inner_h;
    }
    *state.offset_mut() = offset;
    state.select(if rows == 0 { None } else { Some(cursor) });
    f.render_stateful_widget(list, area, &mut state);
    app.side_state = state;
}

fn preview_title(app: &App) -> String {
    if app.show_help {
        return " 帮助 (? 关闭) ".to_string();
    }
    match &app.current_file {
        Some(p) => {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            // 标题栏顺带显示当前章节，读长文不迷路
            match app.current_section() {
                Some(sec) if !sec.is_empty() => format!(" {name} · §{sec} "),
                _ => match app.doc.title.as_ref() {
                    Some(doc_title) => format!(" {name} · {doc_title} "),
                    None => format!(" {name} "),
                },
            }
        }
        None => " 预览 ".to_string(),
    }
}

fn render_preview(f: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let t = *theme;
    let focused = app.focus == Focus::Preview;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused && !app.show_help {
            t.focus
        } else {
            t.border
        }))
        .title(Span::styled(
            preview_title(app),
            Style::default().fg(if focused { t.focus } else { t.muted }),
        ))
        .style(Style::default().bg(t.bg));

    let inner_w = (area.width as usize).saturating_sub(2);
    let inner_h = (area.height as usize).saturating_sub(2);
    if inner_w < 4 || inner_h < 1 {
        f.render_widget(block, area);
        return;
    }

    if app.show_help {
        app.help_h = inner_h;
        let lines = help_lines(&t);
        let max = lines.len().saturating_sub(inner_h);
        app.help_scroll = app.help_scroll.min(max);
        let para = Paragraph::new(lines)
            .block(block)
            .scroll((app.help_scroll as u16, 0));
        f.render_widget(para, area);
        return;
    }

    if app.current_file.is_none() {
        let lines = welcome_lines(&t);
        f.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    app.view_h = inner_h;
    app.ensure_wrapped(inner_w);
    app.clamp_scroll();
    let scroll = app.scroll;
    let lines = app.shown_lines().to_vec();
    let total = lines.len();
    let para = Paragraph::new(lines)
        .block(block)
        .scroll((scroll as u16, 0));
    f.render_widget(para, area);

    // 右侧滚动条
    if total > inner_h && inner_w >= 6 {
        let mut state = ScrollbarState::new(total.saturating_sub(1)).position(scroll);
        let bar_area = ratatui::layout::Rect {
            x: area.x + area.width.saturating_sub(2),
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(t.accent))
                .track_style(Style::default().fg(t.border)),
            bar_area,
            &mut state,
        );
    }
}

fn welcome_lines(t: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "            ◈ mdtui",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "     终端里的 Markdown 阅读器",
            Style::default().fg(t.muted),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1. ", Style::default().fg(t.accent)),
            Span::styled("在左侧文件树用 ", Style::default().fg(t.fg)),
            Span::styled(
                "j / k",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 选择文件", Style::default().fg(t.fg)),
        ]),
        Line::from(vec![
            Span::styled("  2. ", Style::default().fg(t.accent)),
            Span::styled("按 ", Style::default().fg(t.fg)),
            Span::styled(
                "回车",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 打开预览", Style::default().fg(t.fg)),
        ]),
        Line::from(vec![
            Span::styled("  3. ", Style::default().fg(t.accent)),
            Span::styled("按 ", Style::default().fg(t.fg)),
            Span::styled(
                "t",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 切换主题，按 ", Style::default().fg(t.fg)),
            Span::styled(
                "?",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 查看全部按键", Style::default().fg(t.fg)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  默认只列出目录与 Markdown；按 a 查看全部文件；鼠标点选滚轮均可",
            Style::default().fg(t.muted),
        )]),
    ]
}

fn help_lines(t: &Theme) -> Vec<Line<'static>> {
    let h = |s: &str| {
        Line::from(vec![Span::styled(
            s.to_string(),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )])
    };
    let k = |keys: &str, desc: &str| {
        Line::from(vec![
            Span::styled(
                format!("  {keys:<16}"),
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(t.fg)),
        ])
    };
    vec![
        h(" 移动"),
        k("j/k · ↑/↓", "上下移动（侧栏 / 文档 / 本页通用）"),
        k("g/G · Home/End", "顶部 / 底部"),
        k("d/u · PgDn/PgUp", "翻页（预览与帮助）"),
        k("空格", "下一页"),
        Line::from(""),
        h(" 侧边栏"),
        k("l/→/回车", "展开目录 / 打开文件"),
        k("h/←", "收起目录 / 回到父目录"),
        k("Tab · 点击", "切换侧栏 / 预览焦点（滚轮指哪滚哪，不用切）"),
        k("b/e（Ctrl+B）", "显示并聚焦 / 隐藏侧栏"),
        k("[ · ]", "侧栏变窄 / 变宽"),
        k("a", "仅 Markdown ⇄ 显示全部文件"),
        k("R", "刷新文件树与当前文件"),
        Line::from(""),
        h(" 预览"),
        k("w", "自动换行开 / 关"),
        k("r", "重载当前文件"),
        k("/ · n/N", "搜索 / 下一个 / 上一个"),
        k("{ · }", "上一个 / 下一个标题"),
        k("t · T", "下一个 / 上一个主题（自动记住）"),
        Line::from(""),
        h(" 鼠标"),
        k(
            "左键点击",
            "侧栏：选文件即打开 / 点目录即展开；点预览切焦点",
        ),
        k("滚轮", "所在窗格滚动（帮助页里滚本页）"),
        Line::from(vec![Span::styled(
            "  提示：鼠标接管后，终端原生框选复制请按住 Shift",
            Style::default().fg(t.muted),
        )]),
        Line::from(""),
        h(" 通用"),
        k("? · Esc", "关闭本帮助"),
        k("q · Ctrl+C", "退出"),
    ]
}

fn render_footer(f: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let t = *theme;
    let w = area.width as usize;
    // 左侧：搜索框 > 临时消息 > 文件提示 > 情境按键 hint
    let mut left_spans: Vec<Span> = Vec::new();
    let left_text: String;
    if app.input == InputMode::Search {
        let q: String = app.search.query.iter().collect();
        let n = app.search.matches.len();
        let cur = if n == 0 { 0 } else { app.search.current + 1 };
        left_text = format!("/{q}  {cur}/{n}  (Enter 确认 · Esc 取消)");
        left_spans.push(Span::styled(
            left_text.clone(),
            Style::default().fg(t.status_fg),
        ));
    } else if let Some((msg, _)) = &app.message {
        left_text = format!(" {msg}");
        left_spans.push(Span::styled(
            left_text.clone(),
            Style::default().fg(t.status_fg),
        ));
    } else if let Some(notice) = &app.file_notice {
        left_text = format!(" ⚠ {notice}");
        left_spans.push(Span::styled(" ⚠ ".to_string(), Style::default().fg(t.warn)));
        left_spans.push(Span::styled(
            notice.to_string(),
            Style::default().fg(t.status_fg),
        ));
    } else {
        left_text = match app.focus {
            Focus::Sidebar => {
                " j/k 移动 · l/回车 打开 · h 收起 · Tab 预览 · ? 帮助 · q 退出".to_string()
            }
            Focus::Preview => {
                " j/k/d/u/g/G 滚动 · / 搜索 · t 主题 · b/e 侧栏 · ? 帮助 · q 退出".to_string()
            }
        };
        left_spans.push(Span::styled(
            left_text.clone(),
            Style::default().fg(t.status_fg),
        ));
    }
    let right: String = if app.show_help {
        String::new()
    } else if app.current_file.is_some() {
        let total = app.total_lines();
        let pct = if total == 0 {
            100
        } else {
            (app.scroll + app.view_h.min(total)) * 100 / total.max(1)
        };
        format!(
            "行 {}/{} · {}% ",
            app.scroll + 1,
            total.max(1),
            pct.min(100)
        )
    } else {
        format!("{} ", t.name)
    };
    let pad = w.saturating_sub(
        UnicodeWidthStr::width(left_text.as_str()) + UnicodeWidthStr::width(right.as_str()),
    );
    let mut spans = left_spans;
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(right, Style::default().fg(t.status_fg)));
    let line = Line::from(spans);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(t.status_bg)),
        area,
    );

    // 搜索时显示输入光标
    if app.input == InputMode::Search {
        let prefix: String = app.search.query[..app.search.cursor.min(app.search.query.len())]
            .iter()
            .collect();
        let x = 1 + UnicodeWidthStr::width(("/".to_string() + &prefix).as_str());
        f.set_cursor_position((
            area.x + (x as u16).min(area.width.saturating_sub(1)),
            area.y,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_without_panic() {
        let dir = std::env::temp_dir().join(format!("mdtui-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "# A\n\nhello\n").unwrap();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(&dir, None, 0);
        app.open_file(&dir.join("a.md"));
        terminal.draw(|f| render(f, &mut app)).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();
        assert!(content.contains("mdtui"), "header missing");
        assert!(content.contains('A'), "doc missing");
        // 帮助页
        app.show_help = true;
        terminal.draw(|f| render(f, &mut app)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 脚本化会话：按真实用户顺序敲一遍全部功能键，每步都渲染，
    /// 任何越界 / 空态 / 缓存失效导致的 panic 都会在这里炸出来。
    #[test]
    fn scripted_session_no_panic() {
        use crate::theme::{THEME_TEST_LOCK, sandbox_theme_file, unsandbox_theme_file};

        let _guard = THEME_TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("mdtui-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        std::fs::write(dir.join("a.md"), "# A\n\nhello mdtui\n\n- x\n- y\n").unwrap();
        std::fs::write(dir.join("sub").join("note.md"), "# Note\n\nworld\n").unwrap();
        std::fs::write(dir.join("code.txt"), "plain\n").unwrap();
        sandbox_theme_file(&dir.join("themefile"));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(&dir, None, 0);
        let none = KeyModifiers::NONE;
        let step = |app: &mut App, terminal: &mut Terminal<TestBackend>, code: KeyCode| {
            app.on_key(code, none);
            terminal.draw(|f| render(f, app)).unwrap();
        };

        use KeyCode::*;
        // rows 初始：empty / sub / a.md（目录优先）
        step(&mut app, &mut terminal, Enter); // 展开 empty（空目录）
        step(&mut app, &mut terminal, Down); // sub
        step(&mut app, &mut terminal, Enter); // 展开 sub
        step(&mut app, &mut terminal, Down); // note.md
        step(&mut app, &mut terminal, Enter); // 打开文件 → 焦点切预览
        assert!(app.current_file.is_some());
        // 预览滚动全套
        for k in [
            Char('j'),
            Char('k'),
            Down,
            Up,
            Char('d'),
            Char('u'),
            PageDown,
            PageUp,
            Home,
            End,
            Char('g'),
            Char('G'),
            Char(' '),
        ] {
            step(&mut app, &mut terminal, k);
        }
        // 主题轮一圈 + 回绕
        for _ in 0..7 {
            step(&mut app, &mut terminal, Char('t'));
        }
        step(&mut app, &mut terminal, Char('T'));
        // 侧栏显隐 + 宽度 + 换行 + 过滤 + 刷新
        step(&mut app, &mut terminal, Char('b'));
        step(&mut app, &mut terminal, Tab);
        step(&mut app, &mut terminal, Char('j'));
        step(&mut app, &mut terminal, Char('b'));
        step(&mut app, &mut terminal, Char('['));
        step(&mut app, &mut terminal, Char(']'));
        step(&mut app, &mut terminal, Char('w'));
        step(&mut app, &mut terminal, Char('w'));
        step(&mut app, &mut terminal, Char('a'));
        step(&mut app, &mut terminal, Char('a'));
        step(&mut app, &mut terminal, Char('R'));
        step(&mut app, &mut terminal, Char('r'));
        // 搜索：输入 → 退格 → 确认 → 跳转 → 新搜索 → 取消
        step(&mut app, &mut terminal, Char('/'));
        step(&mut app, &mut terminal, Char('m'));
        step(&mut app, &mut terminal, Char('d'));
        step(&mut app, &mut terminal, Backspace);
        step(&mut app, &mut terminal, Enter);
        step(&mut app, &mut terminal, Char('n'));
        step(&mut app, &mut terminal, Char('N'));
        step(&mut app, &mut terminal, Char('/'));
        step(&mut app, &mut terminal, Esc);
        // 标题跳转
        step(&mut app, &mut terminal, Char('}'));
        step(&mut app, &mut terminal, Char('{'));
        // 鼠标：滚轮 + 点击（坐标按当前布局算）
        {
            use crossterm::event::{MouseButton, MouseEventKind};
            let px = app.prev_area.x + 2;
            let py = app.prev_area.y + 2;
            app.on_mouse(MouseEventKind::ScrollDown, px, py);
            terminal.draw(|f| render(f, &mut app)).unwrap();
            app.on_mouse(MouseEventKind::ScrollUp, px, py);
            terminal.draw(|f| render(f, &mut app)).unwrap();
            app.on_mouse(
                MouseEventKind::Down(MouseButton::Left),
                app.side_area.x + 2,
                app.side_area.y + 1,
            );
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
        // 帮助页滚动后关闭
        step(&mut app, &mut terminal, Char('?'));
        for k in [Char('j'), Char('d'), Char('u'), Char('G'), Char('g')] {
            step(&mut app, &mut terminal, k);
        }
        step(&mut app, &mut terminal, Char('?'));
        // 侧栏焦点下的移动
        step(&mut app, &mut terminal, Tab);
        for k in [
            Char('j'),
            Char('k'),
            Char('g'),
            Char('G'),
            PageDown,
            PageUp,
            Home,
            End,
        ] {
            step(&mut app, &mut terminal, k);
        }
        // 小窗口也不炸
        terminal.draw(|f| render(f, &mut app)).unwrap();
        assert!(app.scroll <= app.total_lines().max(1));
        assert!(app.theme_idx < crate::theme::THEMES.len());
        // 退出
        step(&mut app, &mut terminal, Char('q'));
        assert!(app.should_quit);

        unsandbox_theme_file();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
