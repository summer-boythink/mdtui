# mdtui ◈ Terminal Markdown Reader

[中文版](README.zh-CN.md)

Read Markdown elegantly in your terminal: a file-tree sidebar on the left,
rendered preview on the right, and 6 dark/light themes one key away.

```sh
mdtui [PATH] [-t THEME] [--lang en|zh]
```

## Install & Run

```sh
cargo build --release
./target/release/mdtui demo/展示.md   # open a single file
./target/release/mdtui .              # browse a directory
```

## Keys

| Key | Action |
|-----|--------|
| `j/k` `↑/↓` | Move / scroll |
| `h/l` `←/→` `Enter` | Sidebar: collapse / expand & open |
| `Tab` | Cycle sidebar / preview / outline focus |
| `b` / `e` / `Ctrl+B` | Show & focus / hide sidebar |
| `[` `]` | Sidebar narrower / wider |
| `t` / `T` | Next / previous theme (remembered) |
| `L` | Switch UI language EN ⇄ 中文 (remembered, English by default) |
| `a` | Markdown only ⇄ all files |
| `w` | Word wrap on / off |
| `/` `n` `N` | Search / next / previous |
| `{` `}` | Previous / next heading (current section in title bar) |
| `o` | Open / close right outline (off by default, opens focused) |
| `R` / `r` | Refresh all / reload current file |
| Mouse click | Sidebar: open file / expand dir; click preview to focus |
| Wheel | Scroll the pane under cursor |
| `?` | Help page |
| `q` / `Esc` / `Ctrl+C` | Quit |

## Themes

`mdtui --list-themes` to list, `mdtui -t Nord` to pick, `t` to cycle.
The choice is saved to `~/.config/mdtui/theme` and reused next time.

- Dark: Tokyo Night / Nord / Dracula / Gruvbox Dark
- Light: GitHub Light / Solarized Light

## Language

English by default. Press `L` in the app or start with `mdtui --lang zh`.
The choice is saved to `~/.config/mdtui/lang`.

## Behavior

- By default only **directories + Markdown files** are listed
  (`.md/.markdown/.mdown/.mkdn/.mkd`); `a` shows everything incl. hidden files.
- Non-Markdown text is previewed as plain text; binary files and files over
  5 MB are not loaded, with a friendly notice instead.
- Unreadable directories show as empty with a notice; the app never crashes.
- Any panic restores the terminal first, so it never gets "messed up".
- With mouse captured, hold `Shift` for native text selection (standard).
- Rendering details: centered H1 with accent underline, per-level bullets
  (• ◦ ▪ ▸), colored GitHub Alerts, tables with alignment, full-width code
  background with accent gutter + highlighted language label.
- Fenced code blocks get syntax highlighting (syntect under the hood, colors
  mapped to the active theme): keywords / strings / comments / function names /
  numbers each have their color; unknown languages fall back to uniform style.
- Right outline (`o`): document structure + current reading position, click
  to jump, `j/k` to move between sections; wheel scrolls linearly like preview.

## Layout

```text
src/
  main.rs      entry: args → terminal init → event loop → cleanup
  cli.rs       CLI args
  lang.rs      EN/ZH strings + persistence (English by default)
  theme.rs     6 themes + persistence
  tree.rs      file-tree model (expand/filter/sort)
  markdown.rs  parse + render + CJK-friendly wrap + tables + search highlight
  syntax.rs    code highlighting mapped onto our themes
  app.rs       state, keys, search, scroll, mouse
  ui.rs        header / sidebar / preview / outline / footer / help
```
