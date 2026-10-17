//! 命令行参数：`mdtui [PATH] [-t 主题] [--list-themes]`。

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mdtui",
    version,
    about = "终端 Markdown 阅读器：文件树侧边栏 + 优雅渲染 + 自由切换主题"
)]
pub struct Args {
    /// 打开的文件或目录（默认当前目录）
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// 初始主题（不填则用上次的）。可用 `--list-themes` 查看
    #[arg(short, long, value_name = "THEME")]
    pub theme: Option<String>,

    /// 列出所有可用主题并退出
    #[arg(long)]
    pub list_themes: bool,
}
