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
        f.render_widget(Paragraph::new(app.txt().term_too_small), area);
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
    let right = app.lang.theme_label(theme.name);
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
    let mut constraints = vec![];
    if app.sidebar_visible {
        constraints.push(Constraint::Length(app.sidebar_width));
    }
    constraints.push(Constraint::Min(10));
    if app.outline_visible {
        constraints.push(Constraint::Length(app.outline_width));
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    let mut i = 0;
    if app.sidebar_visible {
        app.side_area = cols[i];
        render_sidebar(f, app, theme, cols[i]);
        i += 1;
    } else {
        app.side_area = ratatui::layout::Rect::default();
    }
    app.prev_area = cols[i];
    render_preview(f, app, theme, cols[i]);
    i += 1;
    if app.outline_visible {
        app.outline_area = cols[i];
        render_outline(f, app, theme, cols[i]);
    } else {
        app.outline_area = ratatui::layout::Rect::default();
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
            app.txt().empty_dir,
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

/// 右侧大纲栏：当前文档的标题结构 + 正在读的位置高亮，点击即跳。
fn render_outline(f: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let t = *theme;
    let focused = app.focus == Focus::Outline;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { t.focus } else { t.border }))
        .title(Span::styled(
            app.txt().outline_open_title,
            Style::default().fg(if focused { t.focus } else { t.muted }),
        ))
        .style(Style::default().bg(t.bg));

    let inner_h = (area.height as usize).saturating_sub(2).max(1);
    let items: Vec<ListItem> = if app.current_file.is_none() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            app.txt().no_file_outline,
            Style::default().fg(t.muted),
        )]))]
    } else if app.headings.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            app.txt().no_headings,
            Style::default().fg(t.muted),
        )]))]
    } else {
        // 焦点不在大纲时，光标跟随正文位置（滚轮/键盘/搜索/跳转都会同步）
        if !focused && let Some(idx) = app.current_section_idx() {
            app.outline_cursor = idx;
        }
        app.outline_cursor = app.outline_cursor.min(app.headings.len().saturating_sub(1));
        app.headings
            .iter()
            .map(|(_, level, text)| {
                let indent = "  ".repeat((level.saturating_sub(1)).min(4) as usize);
                let style = if *level <= 1 {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.fg)
                };
                ListItem::new(Line::from(vec![Span::styled(
                    format!("{indent}{text}"),
                    style,
                )]))
            })
            .collect()
    };
    let rows = app.headings.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(t.sel_bg).fg(t.sel_fg))
        .highlight_symbol("");
    let mut state = std::mem::take(&mut app.outline_state);
    let mut offset = state.offset().min(rows.saturating_sub(inner_h));
    let cursor = app.outline_cursor.min(rows.saturating_sub(1));
    if cursor < offset {
        offset = cursor;
    } else if cursor >= offset + inner_h {
        offset = cursor + 1 - inner_h;
    }
    *state.offset_mut() = offset;
    state.select(if rows == 0 { None } else { Some(cursor) });
    f.render_stateful_widget(list, area, &mut state);
    app.outline_state = state;
}

fn preview_title(app: &App) -> String {
    if app.show_help {
        return app.txt().help_title.to_string();
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
        None => app.txt().preview_title.to_string(),
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
        let lines = help_lines(app);
        let max = lines.len().saturating_sub(inner_h);
        app.help_scroll = app.help_scroll.min(max);
        let para = Paragraph::new(lines)
            .block(block)
            .scroll((app.help_scroll as u16, 0));
        f.render_widget(para, area);
        return;
    }

    if app.current_file.is_none() {
        let lines = welcome_lines(app);
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

fn welcome_lines(app: &App) -> Vec<Line<'static>> {
    let t = app.theme();
    let x = app.txt();
    vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "            ◈ mdtui",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            x.welcome_sub,
            Style::default().fg(t.muted),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1. ", Style::default().fg(t.accent)),
            Span::styled(x.pick_a, Style::default().fg(t.fg)),
            Span::styled(
                "j / k",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(x.in_tree, Style::default().fg(t.fg)),
        ]),
        Line::from(vec![
            Span::styled("  2. ", Style::default().fg(t.accent)),
            Span::styled(x.press, Style::default().fg(t.fg)),
            Span::styled(
                x.enter_key,
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(x.to_preview, Style::default().fg(t.fg)),
        ]),
        Line::from(vec![
            Span::styled("  3. ", Style::default().fg(t.accent)),
            Span::styled(x.press, Style::default().fg(t.fg)),
            Span::styled(
                "t",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(x.switch_theme_mid, Style::default().fg(t.fg)),
            Span::styled(
                "?",
                Style::default().fg(t.bold).add_modifier(Modifier::BOLD),
            ),
            Span::styled(x.for_keys, Style::default().fg(t.fg)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            x.welcome_note,
            Style::default().fg(t.muted),
        )]),
    ]
}

fn help_lines(app: &App) -> Vec<Line<'static>> {
    let t = app.theme();
    let x = app.txt();
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
        h(x.h_move),
        k("j/k · ↑/↓", x.d_move),
        k("g/G · Home/End", x.d_topbottom),
        k("d/u · PgDn/PgUp", x.d_page),
        k(x.key_space, x.d_nextpage),
        Line::from(""),
        h(x.h_sidebar),
        k(x.key_open, x.d_open),
        k("h/←", x.d_collapse),
        k(x.key_tab, x.d_tab),
        k(x.key_toggle, x.d_sidebar),
        k("[ · ]", x.d_width),
        k("a", x.d_filter),
        k("R", x.d_refresh),
        Line::from(""),
        h(x.h_preview),
        k("w", x.d_wrap),
        k("r", x.d_reload),
        k("/ · n/N", x.d_search),
        k("{ · }", x.d_heading),
        k("t · T", x.d_theme),
        k("L", x.d_lang),
        Line::from(""),
        h(x.h_outline),
        k("o", x.d_outline_toggle),
        k(x.key_outline_go, x.d_outline_nav),
        k("h", x.d_outline_back),
        Line::from(""),
        h(x.h_mouse),
        k(x.key_click, x.d_mouse_click),
        k(x.key_wheel, x.d_mouse_wheel),
        Line::from(vec![Span::styled(
            x.d_mouse_tip,
            Style::default().fg(t.muted),
        )]),
        Line::from(""),
        h(x.h_common),
        k("? · Esc", x.d_help_close),
        k("q · Ctrl+C", x.d_quit),
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
        left_text = app.lang.search_status(&q, cur, n);
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
            Focus::Sidebar => app.txt().hint_sidebar.to_string(),
            Focus::Preview => app.txt().hint_preview.to_string(),
            Focus::Outline => app.txt().hint_outline.to_string(),
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
        app.lang
            .pos_status(app.scroll + 1, total.max(1), pct.min(100))
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

    #[test]
    fn renders_both_langs() {
        use crate::lang::Lang;
        let dir = std::env::temp_dir().join(format!("mdtui-langs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "# A\n\nhello\n").unwrap();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        for lang in [Lang::En, Lang::Zh] {
            let mut app = App::new(&dir, None, 0);
            app.lang = lang;
            // 欢迎页（无文件）+ 帮助页 + 预览页，三种都渲染一遍
            terminal.draw(|f| render(f, &mut app)).unwrap();
            app.show_help = true;
            terminal.draw(|f| render(f, &mut app)).unwrap();
            app.show_help = false;
            app.open_file(&dir.join("a.md"));
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 脚本化会话：按真实用户顺序敲一遍全部功能键，每步都渲染，
    /// 任何越界 / 空态 / 缓存失效导致的 panic 都会在这里炸出来。
    #[test]
    fn scripted_session_no_panic() {
        use crate::lang::{sandbox_lang_file, unsandbox_lang_file};
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
        sandbox_lang_file(&dir.join("langfile"));

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
        // 大纲：默认关闭 → o 打开并聚焦 → 跳转 → 点击 → o 关闭
        assert!(!app.outline_visible);
        step(&mut app, &mut terminal, Char('o'));
        assert!(app.outline_visible);
        step(&mut app, &mut terminal, Char('j'));
        step(&mut app, &mut terminal, Enter);
        {
            use crossterm::event::{MouseButton, MouseEventKind};
            app.on_mouse(
                MouseEventKind::Down(MouseButton::Left),
                app.outline_area.x + 2,
                app.outline_area.y + 1,
            );
            terminal.draw(|f| render(f, &mut app)).unwrap();
            app.on_mouse(
                MouseEventKind::ScrollDown,
                app.outline_area.x + 2,
                app.outline_area.y + 2,
            );
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
        step(&mut app, &mut terminal, Char('o'));
        assert!(!app.outline_visible);
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
        // 语言切换中英各渲染一次
        step(&mut app, &mut terminal, Char('L'));
        step(&mut app, &mut terminal, Char('?'));
        step(&mut app, &mut terminal, Char('?'));
        step(&mut app, &mut terminal, Char('L'));
        // 小窗口也不炸
        terminal.draw(|f| render(f, &mut app)).unwrap();
        assert!(app.scroll <= app.total_lines().max(1));
        assert!(app.theme_idx < crate::theme::THEMES.len());
        // 退出
        step(&mut app, &mut terminal, Char('q'));
        assert!(app.should_quit);

        unsandbox_theme_file();
        unsandbox_lang_file();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
