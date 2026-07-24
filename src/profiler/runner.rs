//! 编译并运行 Qi 程序，捕获 profiler 输出。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// 单次运行结果。
#[derive(Debug, Clone)]
pub struct RunResult {
    /// 编译耗时（毫秒）
    pub compile_ms: f64,
    /// 程序退出码
    pub exit_code: Option<i32>,
    /// 合并输出（stdout + stderr，含 profiler 报告行）
    pub combined_output: String,
}

/// 运行配置。
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// qi 编译器可执行文件路径
    pub qi_bin: PathBuf,
    /// 透传给被测程序的参数
    pub args: Vec<String>,
    /// 超时秒数（0 = 无限）
    pub timeout_secs: u64,
    /// 优化级别（传给 -O）
    pub optimization: Option<String>,
    /// 仅用于将来扩展
    pub quiet_stdout: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            qi_bin: PathBuf::from("qi"),
            args: Vec::new(),
            timeout_secs: 60,
            optimization: None,
            quiet_stdout: false,
        }
    }
}

/// 编译（带 QI_PROF=1 插桩）并运行，捕获合并输出。
///
/// 流程：
/// 1. `qi compile <文件> -o <tmp>` — 带 QI_PROF=1 让 codegen 注入计时调用
/// 2. 运行 tmp（带 QI_PROF=1 让运行时打报告）
/// 3. 合并 stdout+stderr → combined_output
/// 4. 清理临时文件
pub fn compile_and_run(source: &Path, config: &RunConfig) -> Result<RunResult, String> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("qiprof_tmp");
    let tmp_dir = std::env::temp_dir().join(format!("qiprof_{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let tmp_exe = tmp_dir.join(stem);

    // 1. 编译
    let compile_start = Instant::now();
    let mut compile_cmd = Command::new(&config.qi_bin);
    compile_cmd
        .arg("compile")
        .arg(source)
        .arg("-o")
        .arg(&tmp_exe)
        .env("QI_PROF", "1")
        .env("QI_LINT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(ref opt) = config.optimization {
        compile_cmd.arg("-O").arg(opt);
    }

    let compile_out = compile_cmd
        .output()
        .map_err(|e| format!("启动编译器失败（{}）: {}", config.qi_bin.display(), e))?;
    let compile_ms = compile_start.elapsed().as_secs_f64() * 1000.0;

    if !compile_out.status.success() {
        let stderr = String::from_utf8_lossy(&compile_out.stderr);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!("编译失败:\n{}", stderr));
    }

    // 2. 运行
    let mut run_cmd = Command::new(&tmp_exe);
    run_cmd
        .args(&config.args)
        .env("QI_PROF", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = run_cmd
        .spawn()
        .map_err(|e| format!("启动程序失败: {}", e))?;

    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();

    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(ref mut p) = out_pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let err_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(ref mut p) = err_pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    });

    // 超时
    if config.timeout_secs > 0 {
        let deadline = Instant::now() + std::time::Duration::from_secs(config.timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        terminate_child(&mut child);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&tmp_dir);
                    return Err(format!("等待程序失败: {}", e));
                }
            }
        }
    }

    let status = child.wait().map_err(|e| format!("等待程序失败: {}", e))?;
    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    // 合并
    let mut combined = stdout;
    if !combined.is_empty() && !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str(&stderr);

    // 清理
    let _ = std::fs::remove_file(&tmp_exe);
    let _ = std::fs::remove_file(tmp_exe.with_extension("o"));
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(RunResult {
        compile_ms,
        exit_code: status.code(),
        combined_output: combined,
    })
}

/// 多次运行，返回每次结果。
pub fn run_multiple(
    source: &Path,
    config: &RunConfig,
    count: usize,
) -> Result<Vec<RunResult>, String> {
    let mut results = Vec::with_capacity(count);
    for i in 0..count {
        if count > 1 {
            eprint!("\r  运行 {}/{}... ", i + 1, count);
            let _ = std::io::Write::flush(&mut std::io::stderr());
        }
        results.push(compile_and_run(source, config)?);
    }
    if count > 1 {
        eprintln!();
    }
    Ok(results)
}

/// 优雅终止子进程（Unix: SIGTERM，其他: kill）。
fn terminate_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // POSIX: kill(pid, SIGTERM) 通过 libc 或直接 syscall 都行；
        // 这里用 std::process::Command::kill 的 wrapper 不够（只有 SIGKILL）。
        // 为避免引入 libc 依赖，改用 sh -c kill 最小实现。
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(child.id().to_string())
            .output();
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let _ = child.kill();
}
