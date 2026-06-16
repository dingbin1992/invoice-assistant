# AGENTS.md — invoice-assistant

Tauri v2 + Preact 桌面应用，用于批量扫描电子发票 PDF、识别字段、按报销类别筛选、合并输出汇总 PDF。

## 构建 & 开发命令

```bash
# 前端开发（仅 Vite，端口 1420）
npm run dev

# Tauri 开发模式（自动启动前端 + Rust 后端）
npm run tauri:dev

# 生产构建（前端 + Rust，输出 NSIS 安装包到 src-tauri/target/release/bundle/）
npm run tauri:build
```

构建需要 MSVC 工具链。`.cargo/config.toml` 和 `_build.bat` 里硬编码了本机 SDK 路径，换机器需修改。

## 关键目录 & 入口

| 路径 | 说明 |
|---|---|
| `src/` | 前端 Preact（JSX），两个入口 |
| `src/main.jsx` | 主窗口入口 → `App.jsx` |
| `src/config.jsx` | 配置窗口入口 → `ConfigApp.jsx` |
| `index.html` / `config.html` | 两个 HTML 入口（Vite multi-page） |
| `src/bridge.js` | `window.__TAURI_INTERNALS__.invoke` 的薄封装 |
| `src-tauri/src/` | Rust 后端 |
| `src-tauri/src/main.rs` | 程序入口，调用 `invoice_assistant_lib::run()` |
| `src-tauri/src/lib.rs` | Tauri Builder，注册所有 command |
| `src-tauri/src/commands/mod.rs` | 所有 `#[tauri::command]` 定义 |
| `src-tauri/src/invoice_parser.rs` | PDF 发票解析（调用 pdftotext + 正则） |
| `src-tauri/src/pdf_merge.rs` | PDF 合并（lopdf，两页合 A4 一页） |
| `src-tauri/src/config_store.rs` | mapping.json / category.json 读写 |
| `config/` | 打包进 `resources` 的默认 mapping.json 和 category.json |
| `src-tauri/poppler/` | 内嵌 pdftotext.exe 及其依赖，打包为 bundle resources |

## 架构要点

- **双窗口应用**：主窗口处理发票导入/筛选/合并；配置窗口（`config.html`）管理 mapping/category。capabilities 里 windows 为 `["main", "config"]`。
- **PDF 解析依赖 Poppler**：`invoice_parser.rs` 通过 `Command` 调用 `pdftotext.exe` 提取文本，再用正则匹配字段。`pdftotext` 的查找顺序：Tauri command 预设路径 → `INVOICE_PDFTOTEXT` 环境变量 → exe 递归上溯 poppler 目录 → cwd 相对路径 → PATH。
- **PDF 合并策略**：每两张发票拼为一页 A4（上下各半页），奇数时末尾单张居中。
- **配置文件**：运行时写入 exe 同级 `config/` 目录（`config_store.rs`），首次启动从 bundle resources 复制。
- **映射表字段**：mapping.json 每条记录必须有 `项目名称`、`通用项目名称`、`大类别`、`报销类别` 四个 key。

## 注意事项

- Vite dev server 固定端口 `1420`（`strictPort: true`），HMR 端口 `1421`。Vite 忽略 `src-tauri/` 的文件变更。
- 前端不使用 `@tauri-apps/api` 的 `invoke`，而是通过 `src/bridge.js` 统一调用 `window.__TAURI_INTERNALS__.invoke`。
- `main.rs` 的 `#![windows_subsystem = "windows"]` 属性不要删除，它阻止 release 模式弹出控制台窗口。
- Rust 测试（`src-tauri/` 下 `cargo test`）依赖 `需求文档/` 目录下有真实 PDF 文件，该目录在 `.gitignore` 中，CI 环境会跳过。
- NSIS 安装脚本 `nsis/installer.nsh` 自定义了中文快捷方式，安装到 `$PROGRAMFILES64\invoice-assistant`。
- 文件编码统一 UTF-8。所有面向用户的文本为中文。
