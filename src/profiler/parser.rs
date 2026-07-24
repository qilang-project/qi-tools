//! 解析 qi-runtime profiler 报告输出为结构化数据。

use serde::Serialize;

/// 单个函数的剖析数据（一次运行）。
#[derive(Debug, Clone, Serialize)]
pub struct FunctionProfile {
    /// 函数名（中文原名）
    pub name: String,
    /// 调用次数
    pub calls: u64,
    /// 总耗时（毫秒, wall-inclusive）
    pub total_ms: f64,
    /// 每次均耗时（微秒）
    pub avg_us: f64,
    /// 占比（以最大值为 100% 基准）
    pub pct: f64,
}

/// 一次运行的完整剖析结果。
#[derive(Debug, Clone, Serialize)]
pub struct SingleRunProfile {
    /// 编译耗时（毫秒）
    pub compile_ms: Option<f64>,
    /// 程序退出码
    pub exit_code: Option<i32>,
    /// 各函数剖析数据（按总耗时降序）
    pub functions: Vec<FunctionProfile>,
    /// 程序标准输出（不含 profiler 报告）
    pub stdout: String,
}

/// 从合并的 stdout+stderr 文本中解析 profiler 报告。
///
/// 报告格式由 qi-runtime profiler.rs 生成：
/// ```text
/// === Qi Profiler (wall-inclusive) ===
/// 函数                         调用次数      总耗时(ms)      每次(µs)     占比%
/// 入口                                1        42.300       42300.000   100.0%
/// 斐波那契                         177      41.800         236.158    98.8%
/// === (占比以最大值为 100% 基准；wall-inclusive 含子调用与插桩开销) ===
/// ```
pub fn parse_profiler_output(output: &str) -> SingleRunProfile {
    let mut functions = Vec::new();
    let mut in_table = false;
    let mut stdout_lines = Vec::new();

    for line in output.lines() {
        if line.contains("=== Qi Profiler") {
            in_table = true;
            continue;
        }
        if !in_table {
            // 非报告行属于程序输出（排除 [qi-rc] 等运行时诊断行）
            if !line.starts_with("[qi-") {
                stdout_lines.push(line);
            }
            continue;
        }
        // 报告尾行
        if line.starts_with("=== (") {
            in_table = false;
            continue;
        }
        // 跳过表头
        if line.contains("占比%") || line.contains("函数") {
            continue;
        }
        // 数据行：末尾 4 列 = 调用次数 总耗时ms 每次µs 占比%；其余为函数名。
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.len() < 5 {
            continue;
        }
        let n = toks.len();
        let pct: f64 = toks[n - 1].trim_end_matches('%').parse().unwrap_or(0.0);
        let avg_us: f64 = toks[n - 2].parse().unwrap_or(0.0);
        let total_ms: f64 = toks[n - 3].parse().unwrap_or(0.0);
        let calls: u64 = toks[n - 4].parse().unwrap_or(0);
        let name = toks[..n - 4].join(" ");
        functions.push(FunctionProfile {
            name,
            calls,
            total_ms,
            avg_us,
            pct,
        });
    }

    SingleRunProfile {
        compile_ms: None,
        exit_code: None,
        functions,
        stdout: stdout_lines.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_report() {
        let output = r#"你好世界
=== Qi Profiler (wall-inclusive) ===
函数                         调用次数      总耗时(ms)      每次(µs)     占比%
入口                                1       42.300       42300.000   100.0%
斐波那契                         177       41.800         236.158    98.8%
=== (占比以最大值为 100% 基准；wall-inclusive 含子调用与插桩开销) ===
"#;
        let profile = parse_profiler_output(output);
        assert_eq!(profile.functions.len(), 2);
        assert_eq!(profile.functions[0].name, "入口");
        assert_eq!(profile.functions[0].calls, 1);
        assert!((profile.functions[0].total_ms - 42.3).abs() < 0.01);
        assert_eq!(profile.functions[1].name, "斐波那契");
        assert_eq!(profile.functions[1].calls, 177);
        assert!(profile.stdout.contains("你好世界"));
    }
}
