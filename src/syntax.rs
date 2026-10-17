//! 代码块语法高亮：syntect 解析词法，配色映射到 mdtui 自己的主题。
//!
//! 刻意不用 syntect 自带主题：自带主题的背景色会和我们的代码块底色打架，
//! 且换一套 mdtui 主题就得换一套高亮。这里只取 syntect 的词法 scope，
//! 前景色全部来自当前 [`Theme`](crate::theme::Theme)，6 套主题天然适配。

use std::str::FromStr;
use std::sync::OnceLock;

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme as SynTheme, ThemeSettings,
};
use syntect::parsing::SyntaxSet;

use crate::theme::Theme;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();

fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syn_color(c: ratatui::style::Color) -> Option<SynColor> {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => Some(SynColor { r, g, b, a: 255 }),
        _ => None,
    }
}

/// 一条高亮规则：scope 选择器 + 前景色 + 修饰。
fn rule(
    selector: &str,
    fg: ratatui::style::Color,
    bold: bool,
    italic: bool,
) -> Option<syntect::highlighting::ThemeItem> {
    let mut font_style = FontStyle::empty();
    if bold {
        font_style |= FontStyle::BOLD;
    }
    if italic {
        font_style |= FontStyle::ITALIC;
    }
    Some(syntect::highlighting::ThemeItem {
        scope: ScopeSelectors::from_str(selector).ok()?,
        style: StyleModifier {
            foreground: syn_color(fg),
            background: None,
            font_style: if font_style.is_empty() {
                None
            } else {
                Some(font_style)
            },
        },
    })
}

/// 按当前主题现配一套 syntect 主题（背景沿用代码块底色，只定前景）。
fn build_theme(t: &Theme) -> SynTheme {
    let rules = [
        rule("comment", t.muted, false, true),
        rule("string", t.h3, false, false),
        rule("keyword", t.accent, true, false),
        rule("storage", t.accent, true, false),
        rule("constant", t.h2, false, false),
        rule("entity.name.function", t.link, false, false),
        rule("entity.name.type", t.list, false, false),
        rule("entity.name.tag", t.accent, false, false),
        rule("invalid", t.warn, true, false),
        rule("punctuation", t.muted, false, false),
    ];
    SynTheme {
        name: Some(t.name.to_string()),
        author: Some("mdtui".to_string()),
        settings: ThemeSettings {
            foreground: syn_color(t.code_fg),
            background: syn_color(t.code_bg),
            ..Default::default()
        },
        scopes: rules.into_iter().flatten().collect(),
    }
}

/// 高亮整个代码块，返回每行的（spans，纯文本）。
/// 语言未知/为空时退化为统一代码样式，绝不报错。
pub fn highlight_code_lines(
    theme: &Theme,
    lang: &str,
    lines: &[String],
) -> Vec<(Vec<Span<'static>>, String)> {
    let uniform = || {
        lines
            .iter()
            .map(|l| {
                let style = Style::default().fg(theme.code_fg).bg(theme.code_bg);
                (vec![Span::styled(l.clone(), style)], l.clone())
            })
            .collect()
    };
    let lang = lang.trim();
    if lang.is_empty() {
        return uniform();
    }
    let syntax = syntaxes()
        .find_syntax_by_token(lang)
        .or_else(|| syntaxes().find_syntax_by_token(&lang.to_ascii_lowercase()));
    let Some(syntax) = syntax else {
        return uniform();
    };
    let syn_theme = build_theme(theme);
    let mut highlighter = HighlightLines::new(syntax, &syn_theme);
    let mut out = Vec::with_capacity(lines.len());
    // 注意：syntect 要求传入的行带尾部换行符，否则行注释状态会泄漏到下一行
    //（如 `# py` 之后整块变注释色）。返回 token 里多出的 `\n` 要剥掉。
    let mut probe = String::new();
    for line in lines {
        probe.clear();
        probe.push_str(line);
        probe.push('\n');
        match highlighter.highlight_line(&probe, syntaxes()) {
            Ok(tokens) => {
                let mut spans = Vec::new();
                let mut plain = String::new();
                let n = tokens.len();
                for (idx, (style, text)) in tokens.iter().enumerate() {
                    let mut text = *text;
                    if idx + 1 == n {
                        text = text.strip_suffix('\n').unwrap_or(text);
                    }
                    if text.is_empty() {
                        continue;
                    }
                    let mut s = Style::default().bg(theme.code_bg);
                    let fg = style.foreground;
                    s = s.fg(ratatui::style::Color::Rgb(fg.r, fg.g, fg.b));
                    if style.font_style.contains(FontStyle::BOLD) {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if style.font_style.contains(FontStyle::ITALIC) {
                        s = s.add_modifier(Modifier::ITALIC);
                    }
                    if style.font_style.contains(FontStyle::UNDERLINE) {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    plain.push_str(text);
                    spans.push(Span::styled(text.to_string(), s));
                }
                out.push((spans, plain));
            }
            // 高亮失败就整行退化为统一样式（行数不变，不错位）
            Err(_) => {
                let style = Style::default().fg(theme.code_fg).bg(theme.code_bg);
                out.push((vec![Span::styled(line.clone(), style)], line.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::THEMES;

    #[test]
    fn rust_code_gets_multiple_colors() {
        let t = THEMES[0];
        let lines = vec![
            "// 注释".to_string(),
            "fn main() {".to_string(),
            "    let s = \"hi\";".to_string(),
            "}".to_string(),
        ];
        let out = highlight_code_lines(&t, "rust", &lines);
        assert_eq!(out.len(), 4);
        // 注释行应带 muted 前景
        let comment_fg = out[0].0.iter().map(|s| s.style.fg).collect::<Vec<_>>();
        assert!(
            comment_fg.contains(&Some(t.muted)),
            "comment colored: {comment_fg:?}"
        );
        // 关键字 fn 应加粗
        let kw_bold = out[1]
            .0
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(kw_bold, "keyword bold");
        // 纯文本与原文一致（高亮不能丢字）
        assert_eq!(out[2].1, "    let s = \"hi\";");
    }

    #[test]
    fn unknown_and_empty_lang_falls_back() {
        let t = THEMES[0];
        let lines = vec!["hello".to_string()];
        for lang in ["", "某种语言", "   "] {
            let out = highlight_code_lines(&t, lang, &lines);
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].1, "hello");
            assert!(
                out[0].0.iter().all(|s| s.style.fg == Some(t.code_fg)),
                "uniform for {lang:?}"
            );
        }
    }

    #[test]
    fn lang_lookup_case_insensitive() {
        let t = THEMES[0];
        let lines = vec!["print('x')".to_string()];
        let out = highlight_code_lines(&t, "Python", &lines);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, "print('x')");
    }
}
