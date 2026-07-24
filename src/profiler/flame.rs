//! 终端火焰图渲染器。

use super::parser::FunctionProfile;
use super::stats::FunctionStat;

/// 火焰图渲染配置。
pub struct FlameConfig {
    /// 终端宽度（列数）
    pub width: usize,
    /// 最多显示函数数
    pub max_rows: usize,
    /// 是否使用 ANSI 颜色
    pub color: bool,
    /// 是否渲染火焰图（false 时 render 返回空串）
    pub show_flame: bool,
}

impl Default for FlameConfig {
    fn default() -> Self {
        Self {
            width: 78,
            max_rows: 15,
            color: true,
            show_flame: true,
        }
    }
}

// ANSI 颜色渐变：热（红）→ 冷（青蓝）
const COLORS: &[&str] = &[
    "\x1b[91m", // 亮红
    "\x1b[31m", // 红
    "\x1b[33m", // 黄
    "\x1b[93m", // 亮黄
    "\x1b[32m", // 绿
    "\x1b[36m", // 青
    "\x1b[34m", // 蓝
];
const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

/// 渲染单次运行的火焰图。
pub fn render(functions: &[FunctionProfile], cfg: &FlameConfig) -> String {
    if !cfg.show_flame || functions.is_empty() {
        return String::new();
    }
    render_bars(
        functions
            .iter()
            .take(cfg.max_rows)
            .map(|f| (f.name.as_str(), f.total_ms)),
        functions.len(),
        cfg,
    )
}

/// 渲染聚合数据的火焰图（中位数耗时）。
pub fn render_aggregated(functions: &[FunctionStat], cfg: &FlameConfig) -> String {
    if !cfg.show_flame || functions.is_empty() {
        return String::new();
    }
    render_bars(
        functions
            .iter()
            .take(cfg.max_rows)
            .map(|f| (f.name.as_str(), f.total_ms.median)),
        functions.len(),
        cfg,
    )
}

fn render_bars<'a>(
    items: impl Iterator<Item = (&'a str, f64)>,
    total: usize,
    cfg: &FlameConfig,
) -> String {
    let items: Vec<_> = items.collect();
    if items.is_empty() {
        return "  (无函数剖析数据)\n".to_string();
    }

    let max_ms = items
        .iter()
        .map(|(_, ms)| *ms)
        .fold(0.0f64, f64::max)
        .max(0.001);

    // 计算名字列宽（按显示宽度，上限 28）
    let name_w = items
        .iter()
        .map(|(name, _)| display_width(name))
        .max()
        .unwrap_or(8)
        .min(28);

    // 柱子区域 = 总宽 - 名字 - 右侧标注（12 字符）- 间距 2
    let bar_area = cfg.width.saturating_sub(name_w + 14);

    let mut out = String::new();
    for (i, (name, ms)) in items.iter().enumerate() {
        let ratio = ms / max_ms;
        let bar_len = ((ratio * bar_area as f64) as usize).max(if *ms > 0.0 { 1 } else { 0 });

        let (color, reset, dim) = if cfg.color {
            let ci = (i * COLORS.len() / items.len().max(1)).min(COLORS.len() - 1);
            (COLORS[ci], RESET, DIM)
        } else {
            ("", "", "")
        };

        let padded = pad_display(name, name_w);
        out.push_str(&format!(
            "  {} {}{}{} {}{:>8.3}ms{}\n",
            padded,
            color,
            "\u{2588}".repeat(bar_len),
            reset,
            dim,
            ms,
            reset,
        ));
    }

    if total > cfg.max_rows {
        let (dim, reset) = if cfg.color { (DIM, RESET) } else { ("", "") };
        out.push_str(&format!(
            "  {}... 还有 {} 个函数{}  \n",
            dim,
            total - cfg.max_rows,
            reset,
        ));
    }

    out
}

/// 字符串终端显示宽度（中文字 2 列，ASCII 1 列）。
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if c.is_ascii() { 1usize } else { 2 })
        .sum()
}

/// 左侧补空格到指定显示宽度（名字右对齐）。
fn pad_display(s: &str, target: usize) -> String {
    let w = display_width(s);
    if w >= target {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(target - w), s)
    }
}
