//! Markdown 解析与渲染：pulldown-cmark 事件 → 带样式的逻辑行 → 终端换行。
//!
//! 换行是自研的（不依赖 textwrap），原因：中文连续文本没有空格，
//! 通用英文换行算法会溢出；自研实现按显示宽度贪心填充，CJK 可任意断行、
//! 西文优先断空格，且全程保留 Span 样式不断裂。

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

/// 逻辑行的种类：影响换行阶段的处理。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LineKind {
    /// 普通文本：按宽度换行。
    Normal,
    /// 标题：按宽度换行；H1 在换行阶段居中。level 1-6 供大纲缩进用。
    Heading { level: u8 },
    /// 代码行：换行并把每段补齐到整宽（展示整块背景）。
    Code,
    /// 表格行：绝不换行（保住列对齐），超宽由组件裁剪。
    NoWrap,
    /// 分隔线：换行时展开为整宽的 `─`。
    Hr,
    /// H1 下的强调线：整宽 `─`，用主题 accent 色。
    AccentRule,
}

struct LogicalLine {
    line: Line<'static>,
    kind: LineKind,
    plain: String,
}

/// 一篇文档的渲染结果（与终端宽度无关，可缓存）。
pub struct RenderedDoc {
    logical: Vec<LogicalLine>,
    /// 首个 H1 文本，用作标题栏。
    pub title: Option<String>,
}

/// 带样式的字符流：换行算法的中间表示。
type StyledChars = Vec<(char, Style)>;

/// 换行后的一行：样式行 + 纯文本（搜索用）+ 是否标题首段（标题跳转用）。
pub struct WrappedLine {
    pub line: Line<'static>,
    pub text: String,
    pub heading: bool,
    /// 标题 level（1-6），非标题为 0。
    pub level: u8,
}

// ---------- 解析 ----------

struct ListCtx {
    ordered: bool,
    next: u64,
}

struct ItemCtx {
    /// 祖先条目留下的缩进。
    outer: String,
    /// 本层基础缩进（每层 2 空格）。
    base: String,
    /// 本条 marker（无序 `• ` / 有序 `1. ` / 任务 `[x] `），懒生成。
    marker: Option<Vec<Span<'static>>>,
    marker_plain: Option<String>,
    /// 延续行缩进（marker 等宽空格），懒生成。
    indent: Option<String>,
    first: bool,
    ordered_num: Option<u64>,
}

struct Cell {
    spans: Vec<Span<'static>>,
    plain: String,
}

struct TableBuild {
    aligns: Vec<Alignment>,
    head: Vec<Vec<Cell>>,
    body: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    in_head: bool,
}

enum InlineMod {
    Bold,
    Italic,
    Strike,
    Heading(HeadingLevel),
    Link,
}

struct Builder<'t> {
    theme: &'t Theme,
    out: Vec<LogicalLine>,
    cur: Vec<Span<'static>>,
    cur_plain: String,
    inline: Vec<InlineMod>,
    lists: Vec<ListCtx>,
    items: Vec<ItemCtx>,
    quote_depth: usize,
    in_code: bool,
    code_lines: Vec<String>,
    code_lang: String,
    pending_prefix: Vec<Span<'static>>,
    pending_plain: String,
    heading: Option<HeadingLevel>,
    heading_first: bool,
    heading_text: String,
    title: Option<String>,
    table: Option<TableBuild>,
    cell: Option<Cell>,
    link_text: Option<String>,
    link_dest: Option<String>,
    image_alt: Option<String>,
    image_dest: Option<String>,
}

impl<'t> Builder<'t> {
    fn new(theme: &'t Theme) -> Self {
        Self {
            theme,
            out: Vec::new(),
            cur: Vec::new(),
            cur_plain: String::new(),
            inline: Vec::new(),
            lists: Vec::new(),
            items: Vec::new(),
            quote_depth: 0,
            in_code: false,
            code_lines: Vec::new(),
            code_lang: String::new(),
            pending_prefix: Vec::new(),
            pending_plain: String::new(),
            heading: None,
            heading_first: false,
            heading_text: String::new(),
            title: None,
            table: None,
            cell: None,
            link_text: None,
            link_dest: None,
            image_alt: None,
            image_dest: None,
        }
    }

    /// 当前内联样式：基础色（引用/普通）+ 修饰栈。
    /// 代码块文本走 code_lines 累积、高亮，不经过这里。
    fn cur_style(&self) -> Style {
        let t = self.theme;
        let mut s = if self.quote_depth > 0 {
            Style::default().fg(t.quote_fg)
        } else {
            Style::default().fg(t.fg)
        };
        for m in &self.inline {
            match m {
                InlineMod::Bold => {
                    s = s.fg(t.bold).add_modifier(Modifier::BOLD);
                }
                InlineMod::Italic => {
                    s = s.add_modifier(Modifier::ITALIC);
                }
                InlineMod::Strike => {
                    s = s.add_modifier(Modifier::CROSSED_OUT);
                }
                InlineMod::Heading(level) => {
                    let c = match level {
                        HeadingLevel::H1 => t.h1,
                        HeadingLevel::H2 => t.h2,
                        _ => t.h3,
                    };
                    s = s.fg(c).add_modifier(Modifier::BOLD);
                }
                InlineMod::Link => {
                    s = s.fg(t.link).add_modifier(Modifier::UNDERLINED);
                }
            }
        }
        s
    }

    fn push_span(&mut self, text: String, style: Style) {
        if text.is_empty() {
            return;
        }
        if let Some(cell) = self.cell.as_mut() {
            cell.plain.push_str(&text);
            cell.spans.push(Span::styled(text, style));
            return;
        }
        if self.image_alt.is_some() {
            // 图片 alt 只收集纯文本，End 时统一成型
            if let Some(alt) = self.image_alt.as_mut() {
                alt.push_str(&text);
            }
            return;
        }
        if let Some(link) = self.link_text.as_mut() {
            link.push_str(&text);
        }
        self.cur_plain.push_str(&text);
        self.cur.push(Span::styled(text, style));
    }

    /// 代码块文本累积：首段拼到当前行尾，其余各成新行。
    fn push_code_text(&mut self, text: &str) {
        let clean = text.replace('\t', "    ");
        let mut parts = clean.split('\n');
        if let Some(first) = parts.next() {
            if self.code_lines.is_empty() {
                self.code_lines.push(String::new());
            }
            if let Some(last) = self.code_lines.last_mut() {
                last.push_str(first);
            }
        }
        for part in parts {
            self.code_lines.push(part.to_string());
        }
    }

    fn push_text(&mut self, text: &str) {
        let clean = text.replace('\t', "    ");
        let style = self.cur_style();
        self.push_span(clean, style);
        if self.heading.is_some() {
            self.heading_text.push_str(text);
        }
    }

    /// 把当前行刷出（含引用条、列表 marker、标题前缀、待定前缀）。
    fn flush(&mut self, kind: LineKind) {
        // 标题内的换行也记为标题（供居中与跳转用）。
        let kind = if let Some(level) = self.heading {
            LineKind::Heading { level: level as u8 }
        } else {
            kind
        };
        let t = self.theme;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut plain = String::new();
        // 待定前缀（如脚注定义 `[^1] `）
        spans.append(&mut self.pending_prefix);
        plain.push_str(&self.pending_plain);
        self.pending_plain.clear();
        // 引用条
        for _ in 0..self.quote_depth {
            spans.push(Span::styled("▎ ", Style::default().fg(t.quote_bar)));
            plain.push_str("> ");
        }
        // 列表
        if let Some(item) = self.items.last_mut() {
            let marker = item.marker.take().or_else(|| {
                let level = item.base.len() / 2 + 1;
                let (spans, p) = default_marker(t, item.ordered_num, level);
                item.marker_plain = Some(p);
                Some(spans)
            });
            let indent = item.indent.take().unwrap_or_else(|| {
                let w = item
                    .marker_plain
                    .as_ref()
                    .map(|p| UnicodeWidthStr::width(p.as_str()))
                    .unwrap_or(2);
                " ".repeat(w)
            });
            // 祖先 + 本层基础缩进
            let outer = item.outer.clone();
            let base = item.base.clone();
            plain.push_str(&outer);
            plain.push_str(&base);
            if item.first {
                if let Some(m) = marker {
                    spans.push(Span::raw(format!("{outer}{base}")));
                    spans.extend(m);
                    if let Some(p) = &item.marker_plain {
                        plain.push_str(p);
                    }
                }
                item.first = false;
            } else {
                spans.push(Span::raw(format!("{outer}{base}{indent}")));
                plain.push_str(&indent);
            }
            // 归还（take 出来的要放回去供后续行用）
            // marker 已消费成延续缩进即可，无需归还
        }
        // 标题前缀
        if self.heading.is_some() && self.heading_first {
            if let Some(level) = self.heading {
                let n = level as u8 as usize;
                let hashes = "#".repeat(n) + " ";
                spans.push(Span::styled(hashes.clone(), Style::default().fg(t.muted)));
                plain.push_str(&hashes);
            }
            self.heading_first = false;
        }
        spans.extend(std::mem::take(&mut self.cur));
        plain.push_str(&self.cur_plain);
        self.cur_plain.clear();
        // 块起始处的预刷行常常是全空的（标题/引用/列表/表格开始时），
        // 空 Normal 行直接丢弃，免得文档头顶多出一片空白。
        // （代码块内的空行是 Code kind，必须保留。）
        if kind == LineKind::Normal && plain.is_empty() {
            return;
        }
        // 全空行且无前缀 → 纯空行（折叠逻辑在 blank() 里做）
        self.out.push(LogicalLine {
            line: Line::from(spans),
            kind,
            plain,
        });
    }

    fn blank(&mut self) {
        let is_blank = self
            .out
            .last()
            .is_none_or(|l| l.kind == LineKind::Normal && l.plain.trim().is_empty());
        if !is_blank {
            self.out.push(LogicalLine {
                line: Line::from(""),
                kind: LineKind::Normal,
                plain: String::new(),
            });
        }
    }

    fn format_table(&mut self) {
        let Some(tb) = self.table.take() else {
            return;
        };
        let t = self.theme;
        if tb.head.is_empty() && tb.body.is_empty() {
            return;
        }
        let cols = tb
            .head
            .iter()
            .chain(tb.body.iter())
            .map(|r| r.len())
            .max()
            .unwrap_or(0);
        if cols == 0 {
            return;
        }
        let mut widths = vec![0usize; cols];
        for row in tb.head.iter().chain(tb.body.iter()) {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(UnicodeWidthStr::width(cell.plain.as_str()));
            }
        }
        let sep = Span::styled(" │ ", Style::default().fg(t.hr));
        let push_row = |cells: &[Cell], header: bool, out: &mut Vec<LogicalLine>| {
            let mut spans = Vec::new();
            let mut plain = String::new();
            for (i, &col_w) in widths.iter().enumerate() {
                if i > 0 {
                    spans.push(sep.clone());
                    plain.push_str(" | ");
                }
                let align = tb.aligns.get(i).copied().unwrap_or(Alignment::None);
                let (cell_spans, cell_plain) = cells
                    .get(i)
                    .map(|c| (c.spans.clone(), c.plain.clone()))
                    .unwrap_or_default();
                let w = UnicodeWidthStr::width(cell_plain.as_str());
                let pad = col_w.saturating_sub(w);
                let (l, r) = match align {
                    Alignment::Left | Alignment::None => (0, pad),
                    Alignment::Right => (pad, 0),
                    Alignment::Center => (pad / 2, pad - pad / 2),
                };
                if l > 0 {
                    spans.push(Span::raw(" ".repeat(l)));
                    plain.push_str(&" ".repeat(l));
                }
                let mut cell_spans = cell_spans;
                if header {
                    for s in cell_spans.iter_mut() {
                        s.style = s.style.fg(t.table_head).add_modifier(Modifier::BOLD);
                    }
                }
                spans.extend(cell_spans);
                plain.push_str(&cell_plain);
                if r > 0 {
                    spans.push(Span::raw(" ".repeat(r)));
                    plain.push_str(&" ".repeat(r));
                }
            }
            out.push(LogicalLine {
                line: Line::from(spans),
                kind: LineKind::NoWrap,
                plain,
            });
        };
        let mut rows_out = Vec::new();
        for row in &tb.head {
            push_row(row, true, &mut rows_out);
        }
        if !tb.head.is_empty() {
            // 分隔线
            let mut spans = Vec::new();
            let mut plain = String::new();
            for (i, &col_w) in widths.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled("─┼─", Style::default().fg(t.hr)));
                    plain.push_str("-+-");
                }
                spans.push(Span::styled("─".repeat(col_w), Style::default().fg(t.hr)));
                plain.push_str(&"─".repeat(col_w));
            }
            rows_out.push(LogicalLine {
                line: Line::from(spans),
                kind: LineKind::NoWrap,
                plain,
            });
        }
        for row in &tb.body {
            push_row(row, false, &mut rows_out);
        }
        self.out.extend(rows_out);
    }
}

/// GitHub Alert 配色：标签文本 + 主题色。
fn alert_style(t: &Theme, kind: BlockQuoteKind) -> (&'static str, ratatui::style::Color) {
    match kind {
        BlockQuoteKind::Note => ("NOTE", t.link),
        BlockQuoteKind::Tip => ("TIP", t.table_head),
        BlockQuoteKind::Important => ("IMPORTANT", t.h2),
        BlockQuoteKind::Warning => ("WARNING", t.list),
        BlockQuoteKind::Caution => ("CAUTION", t.warn),
    }
}

/// 默认列表 marker：层级越深符号越小（• ◦ ▪ ▸ 循环）。
fn default_marker(
    t: &Theme,
    ordered_num: Option<u64>,
    level: usize,
) -> (Vec<Span<'static>>, String) {
    if let Some(n) = ordered_num {
        let s = format!("{n}. ");
        (
            vec![Span::styled(s.clone(), Style::default().fg(t.list))],
            s,
        )
    } else {
        const BULLETS: [&str; 4] = ["• ", "◦ ", "▪ ", "▸ "];
        let mark = BULLETS[(level.saturating_sub(1)) % BULLETS.len()];
        (
            vec![Span::styled(mark, Style::default().fg(t.list))],
            mark.to_string(),
        )
    }
}

/// 去掉开头的 YAML front matter（`---` 包裹块）。
fn strip_front_matter(src: &str) -> &str {
    let mut lines = src.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    if first.trim_end() != "---" {
        return src;
    }
    let mut consumed = first.len();
    for (i, line) in lines.enumerate() {
        consumed += line.len();
        let trimmed = line.trim_end();
        if (trimmed == "---" || trimmed == "...") && i < 30 {
            return &src[consumed..];
        }
        if i >= 30 {
            break;
        }
    }
    src
}

pub fn render_markdown(src: &str, theme: &Theme) -> RenderedDoc {
    let src = strip_front_matter(src);
    let mut b = Builder::new(theme);
    let parser = Parser::new_ext(src, Options::all());
    let t = theme;

    for ev in parser {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    b.flush(LineKind::Normal);
                    b.heading = Some(level);
                    b.heading_first = true;
                    b.heading_text.clear();
                    b.inline.push(InlineMod::Heading(level));
                }
                Tag::BlockQuote(kind) => {
                    b.flush(LineKind::Normal);
                    // GitHub Alerts（> [!NOTE] 等）：标记行已被解析器吃掉，
                    // 这里补一条彩色小标题。
                    if let Some(k) = kind {
                        let (label, color) = alert_style(t, k);
                        let mut spans = Vec::new();
                        for _ in 0..=b.quote_depth {
                            spans.push(Span::styled("▎ ", Style::default().fg(t.quote_bar)));
                        }
                        spans.push(Span::styled(
                            label.to_string(),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ));
                        b.out.push(LogicalLine {
                            line: Line::from(spans),
                            kind: LineKind::Normal,
                            plain: format!("> {label}"),
                        });
                    }
                    b.quote_depth += 1;
                }
                Tag::CodeBlock(kind) => {
                    b.flush(LineKind::Normal);
                    b.in_code = true;
                    b.code_lines = vec![String::new()];
                    b.code_lang = match &kind {
                        CodeBlockKind::Fenced(s) => s.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    // 代码块顶栏：色条 + ``` + 高亮的语言名，全行统一底色
                    let bar = || Span::styled("│ ", Style::default().fg(t.accent).bg(t.code_bg));
                    let fence = Style::default().fg(t.code_lang).bg(t.code_bg);
                    let mut fence_spans = vec![bar(), Span::styled("```", fence)];
                    if !b.code_lang.is_empty() {
                        fence_spans.push(Span::styled(
                            b.code_lang.clone(),
                            Style::default()
                                .fg(t.accent)
                                .bg(t.code_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                    let label = if b.code_lang.is_empty() {
                        "```".to_string()
                    } else {
                        format!("```{}", b.code_lang)
                    };
                    b.out.push(LogicalLine {
                        line: Line::from(fence_spans),
                        kind: LineKind::Code,
                        plain: format!("│ {label}"),
                    });
                }
                Tag::HtmlBlock => {}
                Tag::List(start) => {
                    b.flush(LineKind::Normal);
                    b.lists.push(ListCtx {
                        ordered: start.is_some(),
                        next: start.unwrap_or(1),
                    });
                }
                Tag::Item => {
                    b.flush(LineKind::Normal);
                    let depth = b.lists.len();
                    let ordered_num = b.lists.last_mut().and_then(|l| {
                        if l.ordered {
                            let n = l.next;
                            l.next += 1;
                            Some(n)
                        } else {
                            None
                        }
                    });
                    let outer: String = b
                        .items
                        .last()
                        .map(|p| {
                            format!(
                                "{}{}{}",
                                p.outer,
                                p.base,
                                p.indent.clone().unwrap_or_default()
                            )
                        })
                        .unwrap_or_default();
                    b.items.push(ItemCtx {
                        outer,
                        base: "  ".repeat(depth.saturating_sub(1)),
                        marker: None,
                        marker_plain: None,
                        indent: None,
                        first: true,
                        ordered_num,
                    });
                }
                Tag::FootnoteDefinition(label) => {
                    b.flush(LineKind::Normal);
                    let s = format!("[^{label}] ");
                    b.pending_prefix
                        .push(Span::styled(s.clone(), Style::default().fg(t.accent)));
                    b.pending_plain.push_str(&s);
                }
                Tag::Table(aligns) => {
                    b.flush(LineKind::Normal);
                    b.table = Some(TableBuild {
                        aligns,
                        head: Vec::new(),
                        body: Vec::new(),
                        row: Vec::new(),
                        in_head: false,
                    });
                }
                Tag::TableHead => {
                    if let Some(tb) = b.table.as_mut() {
                        tb.in_head = true;
                    }
                }
                Tag::TableRow => {}
                Tag::TableCell => {
                    b.cell = Some(Cell {
                        spans: Vec::new(),
                        plain: String::new(),
                    });
                }
                Tag::Emphasis => b.inline.push(InlineMod::Italic),
                Tag::Strong => b.inline.push(InlineMod::Bold),
                Tag::Strikethrough => b.inline.push(InlineMod::Strike),
                Tag::Link { dest_url, .. } => {
                    b.link_text = Some(String::new());
                    b.link_dest = Some(dest_url.into_string());
                    b.inline.push(InlineMod::Link);
                }
                Tag::Image { dest_url, .. } => {
                    b.image_alt = Some(String::new());
                    b.image_dest = Some(dest_url.into_string());
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    b.flush(LineKind::Normal);
                    b.blank();
                }
                TagEnd::Heading(_) => {
                    b.flush(LineKind::Normal);
                    let h1 = matches!(b.heading, Some(HeadingLevel::H1));
                    if b.title.is_none() && !b.heading_text.trim().is_empty() {
                        // 只有 H1 才当标题：检查记录的 level
                        if h1 {
                            b.title = Some(b.heading_text.trim().to_string());
                        }
                    }
                    b.inline.pop();
                    b.heading = None;
                    if h1 {
                        // H1 自带一条 accent 下划线，更有“大标题”感
                        b.out.push(LogicalLine {
                            line: Line::from(""),
                            kind: LineKind::AccentRule,
                            plain: String::new(),
                        });
                    }
                    b.blank();
                }
                TagEnd::BlockQuote(_) => {
                    b.flush(LineKind::Normal);
                    b.quote_depth = b.quote_depth.saturating_sub(1);
                    b.blank();
                }
                TagEnd::CodeBlock => {
                    b.in_code = false;
                    // 末尾空行是文本尾换行符的副产品，去掉更干净
                    while b.code_lines.last().is_some_and(|l| l.is_empty()) {
                        b.code_lines.pop();
                    }
                    let lang = std::mem::take(&mut b.code_lang);
                    let lines = std::mem::take(&mut b.code_lines);
                    // 每行加左侧色条（样式与纯文本同步，搜索高亮不错位）
                    let bar_style = Style::default().fg(t.accent).bg(t.code_bg);
                    for (spans, text) in crate::syntax::highlight_code_lines(t, &lang, &lines) {
                        let mut spans = spans;
                        spans.insert(0, Span::styled("│ ", bar_style));
                        b.out.push(LogicalLine {
                            line: Line::from(spans),
                            kind: LineKind::Code,
                            plain: format!("│ {text}"),
                        });
                    }
                    b.out.push(LogicalLine {
                        line: Line::from(vec![
                            Span::styled("│ ", bar_style),
                            Span::styled("```", Style::default().fg(t.code_lang).bg(t.code_bg)),
                        ]),
                        kind: LineKind::Code,
                        plain: "│ ```".to_string(),
                    });
                    b.blank();
                }
                TagEnd::HtmlBlock => {
                    b.flush(LineKind::Normal);
                }
                TagEnd::List(_) => {
                    b.lists.pop();
                    b.flush(LineKind::Normal);
                    if b.lists.is_empty() {
                        b.blank();
                    }
                }
                TagEnd::Item => {
                    b.flush(LineKind::Normal);
                    b.items.pop();
                }
                TagEnd::FootnoteDefinition => {
                    b.flush(LineKind::Normal);
                    b.blank();
                }
                TagEnd::Table => {
                    b.format_table();
                    b.blank();
                }
                TagEnd::TableHead => {
                    // 注意：表头行没有 TableRow 包裹，cells 直接挂在 TableHead 下，
                    // 必须在这里收行，否则会漏进第一个 body 行。
                    if let Some(tb) = b.table.as_mut() {
                        let row = std::mem::take(&mut tb.row);
                        if !row.is_empty() {
                            tb.head.push(row);
                        }
                        tb.in_head = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(tb) = b.table.as_mut() {
                        let row = std::mem::take(&mut tb.row);
                        if tb.in_head {
                            tb.head.push(row);
                        } else {
                            tb.body.push(row);
                        }
                    }
                }
                TagEnd::TableCell => {
                    if let Some(cell) = b.cell.take()
                        && let Some(tb) = b.table.as_mut()
                    {
                        tb.row.push(cell);
                    }
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    b.inline.pop();
                }
                TagEnd::Link => {
                    b.inline.pop();
                    let text = b.link_text.take().unwrap_or_default();
                    let dest = b.link_dest.take().unwrap_or_default();
                    if !dest.is_empty() && dest != text {
                        b.push_span(format!(" ({dest})"), Style::default().fg(t.link_url));
                    }
                }
                TagEnd::Image => {
                    let alt = b.image_alt.take().unwrap_or_default();
                    let dest = b.image_dest.take().unwrap_or_default();
                    b.push_span("[image: ".to_string(), Style::default().fg(t.muted));
                    b.push_span(alt, Style::default().fg(t.fg));
                    b.push_span("]".to_string(), Style::default().fg(t.muted));
                    if !dest.is_empty() {
                        b.push_span(format!(" ({dest})"), Style::default().fg(t.link_url));
                    }
                }
                _ => {}
            },
            Event::Text(s) => {
                if b.in_code {
                    // 代码块文本只累积，End 时统一高亮（跨行注释等需要整块上下文）
                    b.push_code_text(&s);
                } else {
                    b.push_text(&s);
                }
            }
            Event::Code(s) => {
                let style = Style::default().fg(t.code_fg).bg(t.code_bg);
                b.push_span(s.into_string(), style);
            }
            Event::Html(s) => {
                let style = Style::default().fg(t.muted);
                for (i, part) in s.split('\n').enumerate() {
                    if i > 0 {
                        b.flush(LineKind::Normal);
                    }
                    if !part.is_empty() {
                        b.push_span(part.to_string(), style);
                    }
                }
            }
            Event::InlineHtml(s) => {
                b.push_span(s.into_string(), Style::default().fg(t.muted));
            }
            Event::InlineMath(s) => {
                b.push_span(format!("${s}$"), Style::default().fg(t.accent));
            }
            Event::DisplayMath(s) => {
                b.flush(LineKind::Normal);
                b.push_span(format!("$${s}$$"), Style::default().fg(t.accent));
                b.flush(LineKind::Normal);
                b.blank();
            }
            Event::FootnoteReference(label) => {
                b.push_span(format!("[^{label}]"), Style::default().fg(t.accent));
            }
            Event::SoftBreak => {
                if b.in_code {
                    b.code_lines.push(String::new());
                } else {
                    b.push_span(" ".to_string(), b.cur_style());
                }
            }
            Event::HardBreak => {
                if b.in_code {
                    b.code_lines.push(String::new());
                } else {
                    b.flush(LineKind::Normal);
                }
            }
            Event::Rule => {
                b.flush(LineKind::Normal);
                b.out.push(LogicalLine {
                    line: Line::from(""),
                    kind: LineKind::Hr,
                    plain: String::new(),
                });
                b.blank();
            }
            Event::TaskListMarker(checked) => {
                let (mark, _): (String, bool) = if checked {
                    ("[x] ".to_string(), true)
                } else {
                    ("[ ] ".to_string(), false)
                };
                if let Some(item) = b.items.last_mut() {
                    item.marker = Some(vec![Span::styled(
                        mark.clone(),
                        Style::default().fg(t.list).add_modifier(Modifier::BOLD),
                    )]);
                    item.marker_plain = Some(mark);
                }
            }
        }
    }
    b.flush(LineKind::Normal);
    RenderedDoc {
        logical: b.out,
        title: b.title,
    }
}

/// 纯文本预览（非 markdown 文件）：原样成行，不断语义，只做终端换行。
pub fn render_text(src: &str, theme: &Theme) -> RenderedDoc {
    let _ = theme;
    let logical = src
        .replace('\t', "    ")
        .split('\n')
        .map(|l| LogicalLine {
            line: Line::from(l.to_string()),
            kind: LineKind::Normal,
            plain: l.to_string(),
        })
        .collect();
    RenderedDoc {
        logical,
        title: None,
    }
}

// ---------- 换行 ----------

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// 把一个样式行按显示宽度切成多段。`trim_leading` 为 true 时延续行去前导空格
///（普通文本）；代码保持缩进所以传 false。
fn wrap_spans(
    line: &Line<'static>,
    width: usize,
    trim_leading: bool,
) -> Vec<(Line<'static>, String)> {
    // 拍平成 (char, Style)
    let mut chars: Vec<(char, Style)> = Vec::new();
    for span in &line.spans {
        for ch in span.content.chars() {
            chars.push((ch, span.style));
        }
    }
    if chars.is_empty() {
        return vec![(Line::from(""), String::new())];
    }
    let mut out = Vec::new();
    let mut buf: StyledChars = Vec::new();
    let mut buf_w = 0usize;
    let mut last_space: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        let (ch, st) = chars[i];
        let w = char_width(ch);
        if buf_w + w > width && !buf.is_empty() {
            // 需要断行：优先在最后一个空格处断
            let (emit, mut rest): (StyledChars, StyledChars) = if let Some(sp) = last_space {
                let r = buf.split_off(sp + 1);
                let e = std::mem::take(&mut buf);
                // 去掉行尾空格本身
                let mut e = e;
                e.pop();
                (e, r)
            } else {
                (std::mem::take(&mut buf), Vec::new())
            };
            if trim_leading {
                while rest.first().is_some_and(|(c, _)| *c == ' ') {
                    rest.remove(0);
                }
            }
            out.push(build_line(emit));
            buf = rest;
            buf_w = buf.iter().map(|(c, _)| char_width(*c)).sum();
            last_space = buf.iter().rposition(|(c, _)| *c == ' ');
            // 不消费当前字符，重新判断
            continue;
        }
        if ch == ' ' {
            last_space = Some(buf.len());
        }
        buf_w += w;
        buf.push((ch, st));
        i += 1;
    }
    if !buf.is_empty() || out.is_empty() {
        out.push(build_line(buf));
    }
    out
}

fn build_line(buf: StyledChars) -> (Line<'static>, String) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();
    let mut cur_style: Option<Style> = None;
    let mut cur_text = String::new();
    let flush =
        |spans: &mut Vec<Span<'static>>, cur_text: &mut String, cur_style: &mut Option<Style>| {
            if !cur_text.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(cur_text),
                    cur_style.unwrap_or_default(),
                ));
                *cur_style = None;
            }
        };
    for (ch, st) in buf {
        if cur_style != Some(st) {
            flush(&mut spans, &mut cur_text, &mut cur_style);
            cur_style = Some(st);
        }
        cur_text.push(ch);
        plain.push(ch);
    }
    flush(&mut spans, &mut cur_text, &mut cur_style);
    (Line::from(spans), plain)
}

/// 把文档按给定宽度换行。含缓存键无关的纯计算，调用方负责缓存。
pub fn wrap_doc(doc: &RenderedDoc, width: usize, theme: &Theme) -> Vec<WrappedLine> {
    let width = width.max(12);
    let mut out = Vec::new();
    for ll in doc.logical.iter() {
        match ll.kind {
            LineKind::Hr | LineKind::AccentRule => {
                let s = "─".repeat(width);
                let color = if ll.kind == LineKind::AccentRule {
                    theme.accent
                } else {
                    theme.hr
                };
                out.push(WrappedLine {
                    line: Line::from(vec![Span::styled(s.clone(), Style::default().fg(color))]),
                    text: String::new(),
                    heading: false,
                    level: 0,
                });
            }
            LineKind::NoWrap => out.push(WrappedLine {
                line: ll.line.clone(),
                text: ll.plain.clone(),
                heading: false,
                level: 0,
            }),
            LineKind::Code => {
                for (line, text) in wrap_spans(&ll.line, width, false) {
                    // 补齐整宽，展示整块代码背景
                    let w = UnicodeWidthStr::width(text.as_str());
                    let mut line = line;
                    if w < width {
                        line.spans.push(Span::styled(
                            " ".repeat(width - w),
                            Style::default().bg(theme.code_bg),
                        ));
                    }
                    out.push(WrappedLine {
                        line,
                        text,
                        heading: false,
                        level: 0,
                    });
                }
            }
            LineKind::Heading { level } => {
                let h1 = level == 1;
                for (fi, (line, text)) in wrap_spans(&ll.line, width, true).into_iter().enumerate()
                {
                    // H1 居中：左右补空格（样式与文本同步，保证搜索高亮不错位）
                    let (line, text) = if h1 {
                        let w = UnicodeWidthStr::width(text.as_str());
                        let pad = width.saturating_sub(w) / 2;
                        if pad > 0 {
                            let mut line = line;
                            line.spans.insert(0, Span::raw(" ".repeat(pad)));
                            (line, format!("{}{text}", " ".repeat(pad)))
                        } else {
                            (line, text)
                        }
                    } else {
                        (line, text)
                    };
                    out.push(WrappedLine {
                        line,
                        text,
                        heading: fi == 0,
                        level: if fi == 0 { level } else { 0 },
                    });
                }
            }
            LineKind::Normal => {
                for (line, text) in wrap_spans(&ll.line, width, true) {
                    out.push(WrappedLine {
                        line,
                        text,
                        heading: false,
                        level: 0,
                    });
                }
            }
        }
    }
    out
}

// ---------- 搜索高亮 ----------

/// 在换行结果上标出搜索命中。返回展示用行（原 wrapped 不变）。
/// ASCII 查询大小写不敏感；含非 ASCII 时大小写敏感（保字节索引正确）。
pub fn apply_search(
    wrapped: &[WrappedLine],
    query: &str,
    current: Option<usize>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if query.is_empty() {
        return wrapped.iter().map(|w| w.line.clone()).collect();
    }
    let ascii = query.is_ascii();
    let q = if ascii {
        query.to_ascii_lowercase()
    } else {
        query.to_string()
    };
    let match_style = Style::default().bg(theme.match_bg).fg(theme.match_fg);
    let cur_style = Style::default()
        .bg(theme.match_cur_bg)
        .fg(theme.match_cur_fg)
        .add_modifier(Modifier::BOLD);
    let mut match_no = 0usize;
    wrapped
        .iter()
        .map(|w| {
            let hay = if ascii {
                w.text.to_ascii_lowercase()
            } else {
                w.text.clone()
            };
            let ranges: Vec<(usize, usize)> = hay
                .match_indices(q.as_str())
                .map(|(i, m)| (i, i + m.len()))
                .collect();
            if ranges.is_empty() {
                return w.line.clone();
            }
            // 按字节区间切分原 spans（ASCII 查询的命中必在字符边界上）
            let mut cuts: Vec<usize> = Vec::with_capacity(ranges.len() * 2 + 2);
            cuts.push(0);
            for (s, e) in &ranges {
                cuts.push(*s);
                cuts.push(*e);
            }
            cuts.push(w.text.len());
            cuts.sort_unstable();
            cuts.dedup();
            // 把原行按字节切成 (start,end,style) 片
            let mut pieces: Vec<(usize, usize, Style)> = Vec::new();
            let mut off = 0usize;
            for span in &w.line.spans {
                let len = span.content.len();
                pieces.push((off, off + len, span.style));
                off += len;
            }
            let mut new_spans: Vec<Span<'static>> = Vec::new();
            for win in cuts.windows(2) {
                let (s, e) = (win[0], win[1]);
                if s >= e {
                    continue;
                }
                // 该区间是否命中
                let mut hit_idx: Option<usize> = None;
                for (ri, (rs, re)) in ranges.iter().enumerate() {
                    if s >= *rs && e <= *re {
                        hit_idx = Some(match_no + ri);
                        break;
                    }
                }
                let text = w.text[s..e].to_string();
                if let Some(global) = hit_idx {
                    let st = if Some(global) == current {
                        cur_style
                    } else {
                        match_style
                    };
                    new_spans.push(Span::styled(text, st));
                } else {
                    let st = pieces
                        .iter()
                        .find(|(ps, pe, _)| s >= *ps && e <= *pe)
                        .map(|(_, _, st)| *st)
                        .unwrap_or_default();
                    new_spans.push(Span::styled(text, st));
                }
            }
            match_no += ranges.len();
            Line::from(new_spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::THEMES;

    fn theme() -> Theme {
        THEMES[0]
    }

    #[test]
    fn headings_lists_code_quote() {
        let doc = render_markdown("# T\n\n- a\n- b\n\n> q\n\n`c`\n", &theme());
        let all: String = doc
            .logical
            .iter()
            .map(|l| l.plain.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("# T"), "heading keeps hashes: {all}");
        assert!(all.contains("• a"), "list marker: {all}");
        assert!(all.contains("> q"), "quote: {all}");
        assert!(all.contains('c'), "inline code: {all}");
        assert_eq!(doc.title.as_deref(), Some("T"));
    }

    #[test]
    fn table_renders_with_borders() {
        let doc = render_markdown("| A | B |\n|---|---|\n| 1 | 2 |\n", &theme());
        let lines: Vec<String> = doc.logical.iter().map(|l| l.plain.clone()).collect();
        let all = lines.join("\n");
        assert!(all.contains('A') && all.contains('1'), "cells: {all}");
        assert!(all.contains('│') || all.contains('|'), "separator: {all}");
        // 回归：表头与 body 必须分行（表头行没有 TableRow 包裹，易漏收）
        let head = lines.iter().position(|l| l.contains('A')).unwrap();
        let body = lines.iter().position(|l| l.contains('1')).unwrap();
        assert_ne!(head, body, "header and body merged: {all}");
    }

    #[test]
    fn wrap_cjk_by_width() {
        // 连续中文无空格，必须按宽度硬断
        let doc = render_markdown("一二三四五六七八九十", &theme());
        let wrapped = wrap_doc(&doc, 12, &theme());
        assert!(wrapped.len() >= 2, "cjk should wrap: {}", wrapped.len());
        for w in &wrapped {
            assert!(
                UnicodeWidthStr::width(w.text.as_str()) <= 12,
                "overflow: {}",
                w.text
            );
        }
    }

    #[test]
    fn wrap_keeps_styles() {
        let doc = render_markdown("**abcdef** ghijkl mnopqr stuvwx yz0123 456789", &theme());
        let wrapped = wrap_doc(&doc, 12, &theme());
        assert!(wrapped.len() >= 2);
        let bold = wrapped[0]
            .line
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(bold, "first fragment keeps bold");
    }

    #[test]
    fn front_matter_stripped() {
        let doc = render_markdown("---\ntitle: x\n---\n\n# T\n", &theme());
        let all: String = doc
            .logical
            .iter()
            .map(|l| l.plain.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!all.contains("title: x"), "front matter: {all}");
        assert!(all.contains("# T"), "body kept: {all}");
    }

    #[test]
    fn code_block_has_bar_and_lang_label() {
        let doc = render_markdown("```rust\nfn main() {}\n```\n", &theme());
        let plains: Vec<String> = doc.logical.iter().map(|l| l.plain.clone()).collect();
        // 顶栏 / 内容 / 底栏都有色条前缀
        assert!(plains.iter().any(|l| l == "│ ```rust"), "fence: {plains:?}");
        assert!(
            plains.iter().any(|l| l == "│ fn main() {}"),
            "body: {plains:?}"
        );
        assert!(plains.iter().any(|l| l == "│ ```"), "close: {plains:?}");
        // 语言名用 accent 加粗
        let top = doc.logical.iter().find(|l| l.plain == "│ ```rust").unwrap();
        let lang_span = top.line.spans.iter().find(|s| s.content == "rust").unwrap();
        assert_eq!(lang_span.style.fg, Some(theme().accent));
        assert!(lang_span.style.add_modifier.contains(Modifier::BOLD));
        // 色条 span 与纯文本同步（搜高亮不错位的根基）
        for l in &doc.logical {
            let span_text: String = l.line.spans.iter().map(|s| s.content.clone()).collect();
            assert_eq!(span_text, l.plain, "spans/plain in sync");
        }
    }

    #[test]
    fn code_block_blank_line_keeps_bar() {
        let doc = render_markdown("```\na\n\nb\n```\n", &theme());
        let plains: Vec<String> = doc.logical.iter().map(|l| l.plain.clone()).collect();
        assert!(
            plains.contains(&"│ ".to_string()),
            "blank keeps bar: {plains:?}"
        );
    }

    #[test]
    fn search_inside_code_with_bar() {
        let doc = render_markdown("```rust\nfn main() {}\n```\n", &theme());
        let wrapped = wrap_doc(&doc, 60, &theme());
        let shown = apply_search(&wrapped, "main", Some(0), &theme());
        let hit = shown.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.style.bg == Some(theme().match_cur_bg) && s.content.contains("main"))
        });
        assert!(hit, "match highlighted inside code");
    }

    #[test]
    fn bullets_nest_by_level() {
        let doc = render_markdown("- a\n  - b\n    - c\n", &theme());
        let all: String = doc
            .logical
            .iter()
            .map(|l| l.plain.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("• a"), "l1: {all}");
        assert!(all.contains("◦ b"), "l2: {all}");
        assert!(all.contains("▪ c"), "l3: {all}");
    }

    #[test]
    fn h1_centered_and_underlined() {
        let doc = render_markdown("# Hi\n\ntext\n", &theme());
        let wrapped = wrap_doc(&doc, 20, &theme());
        let first = &wrapped[0];
        assert!(first.heading, "first fragment flagged");
        assert!(first.text.starts_with("  "), "centered: {:?}", first.text);
        assert!(first.text.contains("# Hi"));
        // H1 后跟 accent 下划线
        let has_rule = wrapped
            .iter()
            .any(|w| !w.text.contains('#') && w.line.width() == 20);
        assert!(has_rule, "accent rule present");
    }

    #[test]
    fn github_alert_renders_label() {
        let doc = render_markdown("> [!NOTE]\n> hi\n", &theme());
        let all: String = doc
            .logical
            .iter()
            .map(|l| l.plain.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("NOTE"), "label: {all}");
        assert!(all.contains("hi"), "body: {all}");
        assert!(!all.contains("[!NOTE]"), "marker consumed: {all}");
    }

    #[test]
    fn search_highlights_ascii_case_insensitive() {
        let doc = render_markdown("Hello World\nfoo bar\n", &theme());
        let wrapped = wrap_doc(&doc, 40, &theme());
        let shown = apply_search(&wrapped, "hello", None, &theme());
        let styled = shown[0]
            .spans
            .iter()
            .any(|s| s.style.bg == Some(theme().match_bg));
        assert!(styled, "match should be highlighted");
    }
}
