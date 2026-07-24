//! 多次运行的统计聚合（min/max/avg/p50/p95/stddev）。

use super::parser::SingleRunProfile;
use serde::Serialize;

/// 单项指标的统计摘要。
#[derive(Debug, Clone, Serialize)]
pub struct Stat {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub median: f64,
    pub p95: f64,
    pub stddev: f64,
}

/// 单个函数的聚合统计。
#[derive(Debug, Clone, Serialize)]
pub struct FunctionStat {
    pub name: String,
    pub calls: Stat,
    pub total_ms: Stat,
    pub avg_us: Stat,
    pub pct: Stat,
}

/// 整体聚合结果。
#[derive(Debug, Clone, Serialize)]
pub struct AggregatedProfile {
    /// 运行次数
    pub runs: usize,
    /// 编译耗时统计
    pub compile_ms: Stat,
    /// 各函数统计（按中位总耗时降序）
    pub functions: Vec<FunctionStat>,
}

/// 对一组 SingleRunProfile 做聚合。
pub fn aggregate(profiles: &[SingleRunProfile]) -> AggregatedProfile {
    let runs = profiles.len();

    // 编译耗时
    let compile_times: Vec<f64> = profiles.iter().filter_map(|p| p.compile_ms).collect();
    let compile_ms = compute_stat(&compile_times);

    // 收集所有出现过的函数名
    let mut all_names: Vec<String> = Vec::new();
    for p in profiles {
        for f in &p.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }
    }

    // 每个函数的多次样本
    let mut functions: Vec<FunctionStat> = Vec::new();
    for name in &all_names {
        let mut calls_v = Vec::new();
        let mut total_v = Vec::new();
        let mut avg_v = Vec::new();
        let mut pct_v = Vec::new();

        for p in profiles {
            if let Some(f) = p.functions.iter().find(|f| &f.name == name) {
                calls_v.push(f.calls as f64);
                total_v.push(f.total_ms);
                avg_v.push(f.avg_us);
                pct_v.push(f.pct);
            }
        }

        functions.push(FunctionStat {
            name: name.clone(),
            calls: compute_stat(&calls_v),
            total_ms: compute_stat(&total_v),
            avg_us: compute_stat(&avg_v),
            pct: compute_stat(&pct_v),
        });
    }

    // 按中位总耗时降序
    functions.sort_by(|a, b| {
        b.total_ms
            .median
            .partial_cmp(&a.total_ms.median)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    AggregatedProfile {
        runs,
        compile_ms,
        functions,
    }
}

fn compute_stat(values: &[f64]) -> Stat {
    if values.is_empty() {
        return Stat {
            min: 0.0,
            max: 0.0,
            avg: 0.0,
            median: 0.0,
            p95: 0.0,
            stddev: 0.0,
        };
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let avg = sum / n as f64;
    let variance = sorted.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / n as f64;

    Stat {
        min: sorted[0],
        max: sorted[n - 1],
        avg,
        median: percentile(&sorted, 50.0),
        p95: percentile(&sorted, 95.0),
        stddev: variance.sqrt(),
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = idx - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_stat() {
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let s = compute_stat(&values);
        assert!((s.min - 10.0).abs() < 0.01);
        assert!((s.max - 50.0).abs() < 0.01);
        assert!((s.avg - 30.0).abs() < 0.01);
        assert!((s.median - 30.0).abs() < 0.01);
    }
}
