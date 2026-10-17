//! 入口：参数解析 → 终端初始化 → 事件循环 → 退出清理。
//!
//! 健壮性：panic 钩子保证任何崩溃都先恢复终端（否则用户终端会“花掉”）。

mod app;
mod cli;
mod lang;
mod markdown;
mod syntax;
mod theme;
mod tree;
mod ui;

use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::{App, InputMode};
use crate::cli::Args;
use crate::lang::{Lang, load_saved_lang};
use crate::theme::{THEMES, load_saved_theme, theme_index_by_name, theme_names};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // 语言优先级：命令行 > 上次保存 > 默认英文。
    let named_lang = args.lang.as_deref().and_then(Lang::from_code);
    let lang = named_lang.unwrap_or_else(load_saved_lang);
    if args.lang.is_some() && named_lang.is_none() {
        eprintln!("{}", lang.unknown_lang(args.lang.as_deref().unwrap_or("")));
    }
    if named_lang.is_some() {
        crate::lang::save_lang(lang);
    }
    let t = lang.texts();

    if args.list_themes {
        println!("{}", lang.list_themes_header());
        for name in theme_names() {
            println!("  {name}");
        }
        return Ok(());
    }

    // 路径归一：文件 → 以父目录为根并预选中；目录 → 以它为根。
    let (root, preselect): (PathBuf, Option<PathBuf>) = resolve_path(&args.path, lang)?;

    // 主题优先级：命令行 > 上次保存 > 默认首个。
    let named_idx = args.theme.as_deref().and_then(theme_index_by_name);
    let theme_idx = named_idx
        .or_else(|| load_saved_theme().as_deref().and_then(theme_index_by_name))
        .unwrap_or(0);
    if args.theme.is_some() && named_idx.is_none() {
        eprintln!(
            "{}",
            lang.unknown_theme(
                args.theme.as_deref().unwrap_or(""),
                &theme_names().join(" / ")
            )
        );
    }
    if named_idx.is_some() {
        crate::theme::save_theme(THEMES[theme_idx].name);
    }

    install_panic_hook();
    let mut terminal = init_terminal().context(t.err_term_init)?;
    let mut app = App::new(&root, preselect.as_deref(), theme_idx);
    app.lang = lang;

    loop {
        terminal
            .draw(|f| ui::render(f, &mut app))
            .context(t.err_draw)?;
        // 搜索态显示输入光标，其余时间隐藏
        if app.input == InputMode::Search {
            let _ = terminal.show_cursor();
        } else {
            let _ = terminal.hide_cursor();
        }
        if app.should_quit {
            break;
        }
        if event::poll(Duration::from_millis(200)).context(t.err_poll)? {
            match event::read().context(t.err_read_ev)? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    app.on_key(key.code, key.modifiers);
                }
                Event::Mouse(mouse) => {
                    app.on_mouse(mouse.kind, mouse.column, mouse.row);
                }
                // Resize 什么都不用做：下一帧按新尺寸重排 + 重换行
                _ => {}
            }
        }
    }

    restore_terminal();
    Ok(())
}

/// 解析启动路径。返回（根目录，预选中文件）。
fn resolve_path(path: &PathBuf, lang: Lang) -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| lang.path_missing(&path.display().to_string()))?;
    if meta.file_type().is_symlink() || meta.is_file() {
        let abs = path.canonicalize().unwrap_or_else(|_| path.clone());
        let parent = abs
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Ok((parent, Some(abs)))
    } else if meta.is_dir() {
        Ok((path.clone(), None))
    } else {
        bail!("{}", lang.path_unsupported(&path.display().to_string()));
    }
}

fn init_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    crossterm::execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout())).map_err(anyhow::Error::from)
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let _ = crossterm::execute!(stdout(), crossterm::cursor::Show);
}

/// 先恢复终端，再沿用默认行为打印 panic 信息。
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default(info);
    }));
}
