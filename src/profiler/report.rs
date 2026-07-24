//! 文本与 JSON 报告输出。

use super::compare::CompareResult;
use super::flame::FlameConfig;
use super::parser::SingleRunProfile;
use super::stats::AggregatedProfile;

const LINE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

// ─────────────────────────── 文本报告 ───────────────────────────

/// 单次运行报告。
pub fn print_single_with_flame(
    profile: &SingleRunProfile,
    file: &str,
    color: bool,
    flame_cfg: &FlameConfig,
) {
    let (bold, reset, _dim) = ansi(color);

    println!("{}", LINE);
    println!("  {}Qi 性能分析报告{}", bold, reset);
    println!("  文件: {}", file);
    if let Some(ms) = profile.compile_ms {
        println!("  编译耗时: {:.1}ms", ms);
    }
    if let Some(code) = profile.exit_code {
        println!("  退出码: {}", code);
    }
    println!("{}", LINE);
    println!();

    print_function_table(&profile.functions, 1, color);

    if flame_cfg.show_flame && !profile.functions.is_empty() {
        println!();
        println!("  {}▶ 火焰图 (wall-inclusive){}", bold, reset);
        print!("{}", super::flame::render(&profile.functions, flame_cfg));
    }

    println!();
    println!("{}", LINE);
}

/// 多次运行聚合报告。
pub fn print_aggregated_with_flame(
    agg: &AggregatedProfile,
    file: &str,
    color: bool,
    flame_cfg: &FlameConfig,
) {
    let (bold, reset, dim) = ansi(color);

    println!("{}", LINE);
    println!(
        "  {}Qi 性能分析报告{} {}({} 次运行统计){}",
        bold, reset, dim, agg.runs, reset
    );
    println!("  文件: {}", file);
    println!(
        "  编译耗时: avg {:.1}ms  min {:.1}ms  max {:.1}ms  stddev {:.1}ms",
        agg.compile_ms.avg, agg.compile_ms.min, agg.compile_ms.max, agg.compile_ms.stddev
    );
    println!("{}", LINE);
    println!();

    if agg.functions.is_empty() {
        println!("  {}(无函数剖析数据){}", dim, reset);
    } else {
        println!(
            "  {:<22} {:>8} {:>10} {:>10} {:>10} {:>10}",
            "函数", "调用", "中位(ms)", "最小(ms)", "最大(ms)", "标准差"
        );
        println!("  {}", "-".repeat(74));
        for f in &agg.functions {
            println!(
                "  {:<22} {:>8.0} {:>10.3} {:>10.3} {:>10.3} {:>10.3}",
                truncate_name(&f.name, 20),
                f.calls.median,
                f.total_ms.median,
                f.total_ms.min,
                f.total_ms.max,
                f.total_ms.stddev,
            );
        }
        println!("  {}wall-inclusive；{} 次运行{}", dim, agg.runs, reset);
    }

    if flame_cfg.show_flame && !agg.functions.is_empty() {
        println!();
        println!("  {}▶ 火焰图 (中位数){}", bold, reset);
        print!(
            "{}",
            super::flame::render_aggregated(&agg.functions, flame_cfg)
        );
    }

    println!();
    println!("{}", LINE);
}

/// 对比报告。
pub fn print_compare(result: &CompareResult, color: bool) {
    let (bold, reset, _) = ansi(color);
    println!("{}", LINE);
    println!("  {}Qi 性能对比报告{}", bold, reset);
    println!("{}", LINE);
    println!();
    print!("{}", super::compare::render_compare(result, color));
    println!();
    println!("{}", LINE);
}

// ─────────────────────────── JSON 报告 ───────────────────────────

pub fn print_json_single(profile: &SingleRunProfile) {
    match serde_json::to_string_pretty(profile) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("JSON 序列化失败: {}", e),
    }
}

pub fn print_json_aggregated(agg: &AggregatedProfile) {
    match serde_json::to_string_pretty(agg) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("JSON 序列化失败: {}", e),
    }
}

pub fn print_json_compare(result: &CompareResult) {
    match serde_json::to_string_pretty(result) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("JSON 序列化失败: {}", e),
    }
}

// ─────────────────────────── 工具 ───────────────────────────────

fn print_function_table(functions: &[super::parser::FunctionProfile], _runs: usize, color: bool) {
    let (_, reset, dim) = ansi(color);
    if functions.is_empty() {
        println!(
            "  {}(无函数剖析数据 — 程序过快或未产生函数调用){}",
            dim, reset
        );
        return;
    }
    println!(
        "  {:<22} {:>10} {:>12} {:>12} {:>8}",
        "函数", "调用次数", "总耗时(ms)", "每次(us)", "占比%"
    );
    println!("  {}", "-".repeat(68));
    for f in functions {
        println!(
            "  {:<22} {:>10} {:>12.3} {:>12.3} {:>7.1}%",
            truncate_name(&f.name, 20),
            f.calls,
            f.total_ms,
            f.avg_us,
            f.pct,
        );
    }
    println!(
        "  {}(wall-inclusive: 占比以最大值为 100% 基准){}",
        dim, reset
    );
}

fn ansi(color: bool) -> (&'static str, &'static str, &'static str) {
    if color {
        ("\x1b[1m", "\x1b[0m", "\x1b[2m")
    } else {
        ("", "", "")
    }
}

/// 截断过长函数名（显示宽度）。
fn truncate_name(name: &str, max_display: usize) -> String {
    let mut w = 0usize;
    let mut s = String::new();
    for ch in name.chars() {
        let cw = if ch.is_ascii() { 1usize } else { 2 };
        if w + cw > max_display {
            s.push_str("..");
            break;
        }
        w += cw;
        s.push(ch);
    }
    s
}
