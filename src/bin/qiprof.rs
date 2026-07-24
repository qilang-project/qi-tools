//! qiprof — Qi 语言性能分析工具
//!
//! 用法示例：
//!   qiprof 程序.qi                         # 单次运行，终端报告
//!   qiprof 程序.qi -n 5                    # 5 次运行，min/max/avg/p95 统计
//!   qiprof 程序.qi --json                  # JSON 输出（适合 CI）
//!   qiprof 程序.qi --no-flame              # 不显示火焰图
//!   qiprof 程序.qi --timeout 30 -- 参数1   # 透传运行参数，限时 30s
//!   qiprof 基准.qi 对比.qi                 # 双文件对比模式

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process;

use clap::Parser;

use qi_tools::profiler::{compare, parser, report, runner, stats};

/// qiprof — Qi 语言性能分析工具
#[derive(Parser)]
#[command(name = "qiprof")]
#[command(about = "Qi 语言性能分析工具 | Performance profiler for Qi programs")]
#[command(version)]
#[command(
    long_about = "编译并运行 Qi 程序，采集函数级 CPU 剖析数据（基于 QI_PROF 编译期插桩）。\n\
                  支持多次运行统计（min/max/avg/p50/p95）、终端火焰图、双文件对比。"
)]
struct Cli {
    /// 被分析的 Qi 源文件（必填）
    #[arg(value_name = "文件")]
    file: PathBuf,

    /// 对比用的第二个 Qi 源文件（启用对比模式）
    #[arg(value_name = "对比文件")]
    compare_file: Option<PathBuf>,

    /// 重复运行次数（>1 时输出统计摘要）
    #[arg(short = 'n', long, default_value = "1", value_name = "次数")]
    runs: usize,

    /// 单次运行超时（秒，0 = 不限）
    #[arg(long, default_value = "60", value_name = "秒")]
    timeout: u64,

    /// 优化级别（none|basic|standard|maximum）
    #[arg(short = 'O', long, value_name = "级别")]
    optimization: Option<String>,

    /// qi 编译器可执行文件路径（默认 PATH 中的 qi）
    #[arg(long, default_value = "qi", value_name = "路径")]
    qi_bin: PathBuf,

    /// 不显示火焰图
    #[arg(long)]
    no_flame: bool,

    /// 输出 JSON 格式结果（适合 CI）
    #[arg(long)]
    json: bool,

    /// 关闭 ANSI 颜色输出
    #[arg(long)]
    no_color: bool,

    /// 终端宽度（影响火焰图宽度，默认自动检测）
    #[arg(long, value_name = "列")]
    width: Option<usize>,

    /// 火焰图最多显示函数数量
    #[arg(long, default_value = "15", value_name = "数量")]
    top: usize,

    /// 透传给被测程序的运行参数（置于 -- 之后）
    #[arg(trailing_var_arg = true, value_name = "参数")]
    args: Vec<String>,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    // 颜色：--no-color / NO_COLOR 环境变量 / 非 TTY 时关闭
    let color =
        !cli.no_color && std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal();

    // 终端宽度
    let term_width = cli.width.or_else(term_width).unwrap_or(80);

    let run_cfg = runner::RunConfig {
        qi_bin: cli.qi_bin.clone(),
        args: cli.args.clone(),
        timeout_secs: cli.timeout,
        optimization: cli.optimization.clone(),
        quiet_stdout: false,
    };

    // ── 对比模式 ──
    if let Some(ref cmp_file) = cli.compare_file {
        run_compare_mode(&cli, &run_cfg, cmp_file, color, term_width);
        return;
    }

    // ── 单次 / 多次运行 ──
    if cli.runs == 1 {
        run_single_mode(&cli, &run_cfg, color, term_width);
    } else {
        run_multi_mode(&cli, &run_cfg, color, term_width);
    }
}

// ─────────────────────────── 单次运行 ───────────────────────────

fn run_single_mode(cli: &Cli, cfg: &runner::RunConfig, color: bool, width: usize) {
    if !cli.json {
        status("编译并运行...", color);
    }

    let result = match runner::compile_and_run(&cli.file, cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("错误: {}", e);
            process::exit(1);
        }
    };

    let mut profile = parser::parse_profiler_output(&result.combined_output);
    profile.compile_ms = Some(result.compile_ms);
    profile.exit_code = result.exit_code;

    // 打印程序自身输出（不含 profiler 报告行，用淡色区分）
    if !profile.stdout.trim().is_empty() && !cli.json {
        let (dim, reset) = if color {
            ("\x1b[2m", "\x1b[0m")
        } else {
            ("", "")
        };
        eprintln!("{}── 程序输出 ──{}", dim, reset);
        for line in profile.stdout.lines() {
            eprintln!("{}{}{}", dim, line, reset);
        }
        eprintln!("{}─────────────{}", dim, reset);
    }

    if cli.json {
        report::print_json_single(&profile);
    } else {
        let flame_cfg = flame_config(cli, color, width);
        report::print_single_with_flame(
            &profile,
            &cli.file.display().to_string(),
            color,
            &flame_cfg,
        );
    }

    if let Some(code) = result.exit_code {
        if code != 0 {
            process::exit(code);
        }
    }
}

// ─────────────────────────── 多次运行 ───────────────────────────

fn run_multi_mode(cli: &Cli, cfg: &runner::RunConfig, color: bool, width: usize) {
    if !cli.json {
        status(&format!("编译并运行 {} 次...", cli.runs), color);
    }

    let results = match runner::run_multiple(&cli.file, cfg, cli.runs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("错误: {}", e);
            process::exit(1);
        }
    };

    let profiles: Vec<parser::SingleRunProfile> = results
        .iter()
        .map(|r| {
            let mut p = parser::parse_profiler_output(&r.combined_output);
            p.compile_ms = Some(r.compile_ms);
            p.exit_code = r.exit_code;
            p
        })
        .collect();

    let agg = stats::aggregate(&profiles);

    if cli.json {
        report::print_json_aggregated(&agg);
    } else {
        let flame_cfg = flame_config(cli, color, width);
        report::print_aggregated_with_flame(
            &agg,
            &cli.file.display().to_string(),
            color,
            &flame_cfg,
        );
    }
}

// ─────────────────────────── 对比模式 ───────────────────────────

fn run_compare_mode(
    cli: &Cli,
    cfg: &runner::RunConfig,
    cmp_file: &PathBuf,
    color: bool,
    _width: usize,
) {
    let runs = cli.runs.max(1);
    if !cli.json {
        status(&format!("编译并运行两个文件（各 {} 次）...", runs), color);
    }

    // 基准
    let base_results = match runner::run_multiple(&cli.file, cfg, runs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("基准运行失败: {}", e);
            process::exit(1);
        }
    };

    // 对比
    let target_results = match runner::run_multiple(cmp_file, cfg, runs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("对比运行失败: {}", e);
            process::exit(1);
        }
    };

    let base_profile = consolidate(base_results);
    let target_profile = consolidate(target_results);

    let cmp_result = compare::compare(
        &base_profile.functions,
        &target_profile.functions,
        &cli.file.display().to_string(),
        &cmp_file.display().to_string(),
    );

    if cli.json {
        report::print_json_compare(&cmp_result);
    } else {
        report::print_compare(&cmp_result, color);
    }
}

// ─────────────────────────── 辅助 ───────────────────────────────

/// 多次结果 → 取中位数代表值（用于对比）。
fn consolidate(results: Vec<runner::RunResult>) -> parser::SingleRunProfile {
    let profiles: Vec<_> = results
        .iter()
        .map(|r| {
            let mut p = parser::parse_profiler_output(&r.combined_output);
            p.compile_ms = Some(r.compile_ms);
            p
        })
        .collect();
    if profiles.len() == 1 {
        return profiles.into_iter().next().unwrap();
    }
    let agg = stats::aggregate(&profiles);
    parser::SingleRunProfile {
        compile_ms: Some(agg.compile_ms.median),
        exit_code: Some(0),
        functions: agg
            .functions
            .iter()
            .map(|f| parser::FunctionProfile {
                name: f.name.clone(),
                calls: f.calls.median as u64,
                total_ms: f.total_ms.median,
                avg_us: f.avg_us.median,
                pct: f.pct.median,
            })
            .collect(),
        stdout: String::new(),
    }
}

fn flame_config(cli: &Cli, color: bool, width: usize) -> qi_tools::profiler::flame::FlameConfig {
    qi_tools::profiler::flame::FlameConfig {
        width: width.saturating_sub(2),
        max_rows: cli.top,
        color,
        show_flame: !cli.no_flame,
    }
}

fn status(msg: &str, color: bool) {
    let (dim, reset) = if color {
        ("\x1b[2m", "\x1b[0m")
    } else {
        ("", "")
    };
    eprintln!("{}[qiprof] {}{}", dim, msg, reset);
}

fn term_width() -> Option<usize> {
    std::env::var("COLUMNS").ok().and_then(|s| s.parse().ok())
}
