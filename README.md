# qi-tools

Qi语言开发工具集

## 工具列表

### qifmt - Qi代码格式化工具

用于格式化Qi语言源代码的命令行工具。

#### 安装

```bash
cargo build --release
```

#### 使用方法

```bash
# 格式化单个文件
qifmt 文件.qi

# 递归格式化目录中的所有.qi文件
qifmt -r 目录/

# 只检查格式，不修改文件
qifmt --check 文件.qi

# 显示格式化差异
qifmt --diff 文件.qi

# 详细输出
qifmt -v 文件.qi

# 静默模式
qifmt -q 文件.qi
```

#### 选项

- `-r, --recursive` - 递归处理目录
- `--check` - 只检查格式，不修改文件
- `--diff` - 显示格式化差异
- `-v, --verbose` - 详细输出
- `-q, --quiet` - 静默模式
- `--config <FILE>` - 指定配置文件路径
- `--format <FORMAT>` - 输出格式 (text, json)

## 开发

### 构建

```bash
cargo build
```

### 测试

```bash
cargo test
```

### 发布

```bash
cargo build --release
```

## 项目结构

```
qi-tools/
├── src/
│   ├── lib.rs           # 库入口
│   ├── formatter/       # 格式化模块
│   │   ├── mod.rs      # 格式化器核心
│   │   ├── config.rs   # 配置管理
│   │   └── writer.rs   # 代码输出
│   └── bin/
│       └── qifmt.rs    # qifmt命令行工具
├── Cargo.toml
└── README.md
```

## License

MIT
