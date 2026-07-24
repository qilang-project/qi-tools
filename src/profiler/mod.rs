//! qiprof — Qi 语言性能分析工具
//!
//! 编译 Qi 源文件（带 QI_PROF=1 插桩），运行并捕获函数级 CPU 剖析数据，
//! 支持多次运行统计（min/max/avg/p50/p95）、终端火焰图、双文件对比。

pub mod compare;
pub mod flame;
pub mod parser;
pub mod report;
pub mod runner;
pub mod stats;

pub use parser::FunctionProfile;
pub use runner::RunResult;
pub use stats::AggregatedProfile;
