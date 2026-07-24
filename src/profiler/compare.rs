//! 双文件/双配置对比模式。

use super::parser::FunctionProfile;
use serde::Serialize;

/// 函数级对比结果。
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDiff {
    pub name: String,
    /// 基准运行耗时（ms）
    pub base_ms: f64,
    /// 对比运行耗时（ms）
    pub target_ms: f64,
    /// 变化百分比（正数 = 变慢，负数 = 变快）
    pub change_pct: f64,
    /// 绝对差值（ms）
    pub delta_ms: f64,
}

/// 整体对比结果。
#[derive(Debug, Clone, Serialize)]
pub struct CompareResult {
    pub base_label: String,
    pub target_label: String,
    /// 函数级对比（按 |变化| 降序）
    pub diffs: Vec<FunctionDiff>,
    /// 基准总运行时间
    pub base_total_ms: f64,
    /// 对比总运行时间
    pub target_total_ms: f64,
    /// 整体变化百分比
    pub overall_change_pct: f64,
}

/// 对比两组剖析数据。
pub fn compare(
    base: &[FunctionProfile],
    target: &[FunctionProfile],
    base_label: &str,
    target_label: &str,
) -> CompareResult {
    let mut diffs = Vec::new();

    // 基准侧的函数
    for bf in base {
        let target_ms = target
            .iter()
            .find(|tf| tf.name == bf.name)
            .map(|tf| tf.total_ms)
            .unwrap_or(0.0);

        let delta = target_ms - bf.total_ms;
        let change_pct = if bf.total_ms > 0.001 {
            delta / bf.total_ms * 100.0
        } else {
            0.0
        };

        diffs.push(FunctionDiff {
            name: bf.name.clone(),
            base_ms: bf.total_ms,
            target_ms,
            change_pct,
            delta_ms: delta,
        });
    }

    // 只在 target 侧新增的函数
    for tf in target {
        if !base.iter().any(|bf| bf.name == tf.name) {
            diffs.push(FunctionDiff {
                name: tf.name.clone(),
                base_ms: 0.0,
                target_ms: tf.total_ms,
                change_pct: 100.0, // 新增
                delta_ms: tf.total_ms,
            });
        }
    }

    // 按 |变化| 降序
    diffs.sort_by(|a, b| {
        b.delta_ms
            .abs()
            .partial_cmp(&a.delta_ms.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let base_total = base.iter().map(|f| f.total_ms).fold(0.0f64, f64::max);
    let target_total = target.iter().map(|f| f.total_ms).fold(0.0f64, f64::max);
    let overall_change = if base_total > 0.001 {
        (target_total - base_total) / base_total * 100.0
    } else {
        0.0
    };

    CompareResult {
        base_label: base_label.to_string(),
        target_label: target_label.to_string(),
        diffs,
        base_total_ms: base_total,
        target_total_ms: target_total,
        overall_change_pct: overall_change,
    }
}

/// 渲染对比结果为终端文本。
pub fn render_compare(result: &CompareResult, color: bool) -> String {
    let mut out = String::new();

    let (green, red, reset, bold, dim) = if color {
        ("\x1b[32m", "\x1b[31m", "\x1b[0m", "\x1b[1m", "\x1b[2m")
    } else {
        ("", "", "", "", "")
    };

    out.push_str(&format!(
        "  {}对比{}: {} vs {}\n",
        bold, reset, result.base_label, result.target_label
    ));

    // 整体
    let arrow = if result.overall_change_pct > 1.0 {
        format!("{}+{:.1}% 变慢{}", red, result.overall_change_pct, reset)
    } else if result.overall_change_pct < -1.0 {
        format!("{}{:.1}% 变快{}", green, result.overall_change_pct, reset)
    } else {
        format!("{}~0% 持平{}", dim, reset)
    };
    out.push_str(&format!(
        "  整体: {:.3}ms -> {:.3}ms ({})\n\n",
        result.base_total_ms, result.target_total_ms, arrow
    ));

    // 表头
    out.push_str(&format!(
        "  {:<20} {:>10} {:>10} {:>10} {:>8}\n",
        "函数", "基准(ms)", "对比(ms)", "差值(ms)", "变化%"
    ));
    out.push_str(&format!("  {}\n", "-".repeat(62)));

    for d in result.diffs.iter().take(15) {
        let change_str = if d.change_pct > 5.0 {
            format!("{}+{:.1}%{}", red, d.change_pct, reset)
        } else if d.change_pct < -5.0 {
            format!("{}{:.1}%{}", green, d.change_pct, reset)
        } else {
            format!("{:.1}%", d.change_pct)
        };

        let delta_str = if d.delta_ms > 0.0 {
            format!("+{:.3}", d.delta_ms)
        } else {
            format!("{:.3}", d.delta_ms)
        };

        // 截断过长的函数名
        let name = if d.name.chars().count() > 18 {
            let mut s: String = d.name.chars().take(16).collect();
            s.push_str("..");
            s
        } else {
            d.name.clone()
        };

        out.push_str(&format!(
            "  {:<20} {:>10.3} {:>10.3} {:>10} {:>8}\n",
            name, d.base_ms, d.target_ms, delta_str, change_str
        ));
    }

    if result.diffs.len() > 15 {
        out.push_str(&format!("  ... 省略 {} 个函数\n", result.diffs.len() - 15));
    }

    out
}
