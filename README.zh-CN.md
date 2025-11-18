# TransJLC

<div align="center">
  <img src="docs/image/TransJLC.svg" alt="TransJLC Logo" width="200"/>
</div>

<div align="center">

[![crates.io](https://img.shields.io/crates/v/TransJLC.svg)](https://crates.io/crates/TransJLC)
[![license](https://img.shields.io/github/license/HalfSweet/TransJLC)](https://github.com/HalfSweet/TransJLC/blob/main/LICENSE)
[![release](https://img.shields.io/github/v/release/HalfSweet/TransJLC)](https://github.com/HalfSweet/TransJLC/releases)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/HalfSweet/TransJLC/rust.yml)

</div>

<p align="center">
  <a href="./README.md">English</a> | <a href="./README.zh-CN.md">简体中文</a>
</p>

**TransJLC** 是一个用于将其他 EDA 软件生成的 Gerber 文件转换为与嘉立创 EDA（立创商城的在线编辑器）兼容格式的工具，以方便在嘉立创进行生产。

## ✨ 功能特性

-   自动识别来自常见 EDA 软件（KiCad、Protel、Altium Designer）的 Gerber 文件。
-   将文件重命名以符合嘉立创所需的文件命名规范。
-   可自动将输出文件压缩为 ZIP 归档，便于上传。
-   跨平台支持（Windows、macOS、Linux）。

## 📦 安装

### 通过 crates.io (推荐)

请确保您已安装 Rust 工具链。然后，您可以直接从 crates.io 安装 `TransJLC`：

```bash
cargo install TransJLC
```

### 从源码编译

1.  克隆仓库：
    ```bash
    git clone https://github.com/HalfSweet/TransJLC.git
    ```
2.  进入项目目录：
    ```bash
    cd TransJLC
    ```
3.  以 release 模式构建项目：
    ```bash
    cargo build --release
    ```
    可执行文件将位于 `target/release/TransJLC`。

## 🚀 使用方法

在您的终端中运行该工具，并提供必要的选项。

### 命令行选项

| 选项          | 缩写 | 描述                                                              | 默认值      |
| ------------- | ---- | ----------------------------------------------------------------- | ----------- |
| `--eda`       | `-e` | 指定源 EDA 软件。可选：`auto`, `kicad`, `jlc`, `protel`。           | `auto`      |
| `--path`      | `-p` | 包含 Gerber 文件的目录路径。                                      | `.` (当前目录) |
| `--output_path` | `-o` | 转换后文件保存的路径。                                            | `./output`  |
| `--zip`       | `-z` | 如果设置为 `true`，则会创建输出文件的 ZIP 归档。                  | `false`     |
| `--zip_name`  | `-n` | 生成的 ZIP 文件的名称（不含 `.zip` 扩展名）。                     | `Gerber`    |

### 使用示例

转换位于 `D:\Projects\MyPCB\Gerber` 的 Gerber 文件，将它们保存到 `D:\Projects\MyPCB\Output`，然后创建一个名为 `MyProject.zip` 的 ZIP 文件。

```bash
TransJLC -p="D:\Projects\MyPCB\Gerber" -o="D:\Projects\MyPCB\Output" -z=true -n=MyProject
```

## 🤝 贡献

欢迎各种贡献、问题和功能请求！请随时查看 [issues 页面](https://github.com/HalfSweet/TransJLC/issues)。

## 📄 许可证

该项目采用 Apache-2.0 许可证。详情请参阅 [LICENSE](LICENSE) 文件。

## 版权声明

本项目不建议任何方式进行任何形式商用！其中的代码仅用于学习和研究，禁止用于任何商业目的，也禁止用于伤害深圳嘉立创科技集团股份有限公司。其中`lceda` `立创EDA` `嘉立创EDA` `嘉立创`等均属于深圳嘉立创科技集团股份有限公司所注册商标，请注意使用。
