//! qi-bindgen —— 解析 C 头文件，自动生成 Qi 的 `外部` 声明。
//! Generate Qi `外部` (extern C) declarations from a C header file.
//!
//! 用法 / Usage:
//!   qi-bindgen math.h --库 m -o math.qi
//!   qi-bindgen string.h --库 c --前缀 str
//!   qi-bindgen openssl/sha.h --库 crypto --前缀 SHA -I/opt/homebrew/include

use clap::Parser;
use qi_tools::bindgen::{generate, parse};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "qi-bindgen")]
#[command(
    about = "从 C 头文件生成 Qi 外部声明 / Generate Qi `外部` bindings from a C header",
    long_about = "解析 C 头文件（用 libclang），把可映射的 C 函数原型翻译成 Qi 的 \
                  `外部 \"库名\" { 函数 名(...): 类型; }` 声明。\n\
                  Parse a C header with libclang and emit Qi `外部` extern-C declarations.\n\n\
                  类型映射 / Type mapping:\n\
                  \x20 int/long/size_t/enum/...  -> 整数\n\
                  \x20 float/double              -> 浮点数\n\
                  \x20 _Bool                     -> 布尔\n\
                  \x20 const char* / char* (参数) -> 字符串\n\
                  \x20 void (返回)               -> 空\n\n\
                  不可映射（char* 返回 / 指针 / 结构体 / 函数指针 / 变参）的函数会被\n\
                  跳过并生成注释，保证输出文件可编译。\n\
                  Unmappable functions are skipped as comments so the output still compiles."
)]
#[command(version)]
struct Cli {
    /// C 头文件路径 / Path to the C header file
    #[arg(value_name = "HEADER")]
    header: PathBuf,

    /// 链接的库名，生成 `外部 "库名"` 并链接期 -l库名 / Library name for `外部 "<lib>"`
    #[arg(long = "库", value_name = "LIB")]
    lib: String,

    /// 输出 .qi 文件（缺省打印到标准输出）/ Output .qi file (defaults to stdout)
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output: Option<PathBuf>,

    /// 只导出以该前缀开头的函数（如 SSL_ / av_ / SHA）/ Only export functions with this prefix
    #[arg(long = "前缀", value_name = "PREFIX")]
    prefix: Option<String>,

    /// 透传给 clang 的额外参数，可重复（如 -I/path）/ Extra clang args, e.g. -I/path (repeatable)
    #[arg(short = 'I', value_name = "DIR")]
    includes: Vec<String>,

    /// 其它透传给 clang 的原始参数 / Additional raw clang args
    #[arg(long = "clang-arg", value_name = "ARG")]
    clang_args: Vec<String>,
}

fn main() {
    // 默认 libclang 路径（可被环境变量覆盖）。
    if std::env::var_os("LIBCLANG_PATH").is_none() {
        for cand in [
            "/opt/homebrew/opt/llvm@21/lib",
            "/opt/homebrew/opt/llvm/lib",
            "/usr/local/opt/llvm/lib",
            "/usr/lib/llvm-21/lib",
        ] {
            if std::path::Path::new(cand).exists() {
                std::env::set_var("LIBCLANG_PATH", cand);
                break;
            }
        }
    }

    let cli = Cli::parse();

    if !cli.header.exists() {
        eprintln!("错误 / error: 头文件不存在: {:?}", cli.header);
        process::exit(1);
    }

    // 组装 clang 参数：-I 展开成 clang 的 -I 形式。
    let mut clang_args: Vec<String> = Vec::new();
    for inc in &cli.includes {
        clang_args.push(format!("-I{}", inc));
    }
    clang_args.extend(cli.clang_args.iter().cloned());

    let opts = parse::ParseOptions {
        prefix: cli.prefix.clone(),
        clang_args,
    };

    let result = match parse::parse_header(&cli.header, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("错误 / error: {}", e);
            process::exit(1);
        }
    };

    let header_str = cli.header.to_string_lossy();
    let content = generate::render(&header_str, &cli.lib, cli.prefix.as_deref(), &result);

    match &cli.output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &content) {
                eprintln!("错误 / error: 写入 {:?} 失败: {}", path, e);
                process::exit(1);
            }
            eprintln!(
                "已生成 / generated: {:?}（可映射 {} 个，跳过 {} 个）",
                path,
                result.mapped.len(),
                result.skipped.len()
            );
        }
        None => {
            print!("{}", content);
            eprintln!(
                "（可映射 {} 个，跳过 {} 个）",
                result.mapped.len(),
                result.skipped.len()
            );
        }
    }
}
