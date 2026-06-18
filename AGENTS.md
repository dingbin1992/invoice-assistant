# AGENTS.md - 发票助手开发指南

## 项目概述
Tauri 2.x 桌面应用，用于批量处理电子发票 PDF：解析字段、分类、合并输出。

## 技术栈
- **前端**: Preact + Vite (端口 1420)
- **后端**: Rust + Tauri 2.x
- **PDF处理**: Poppler (pdftotext/pdftocairo)，打包在 `src-tauri/poppler/`

## 开发命令

### 前端开发
```bash
npm run dev          # 启动 Vite 开发服务器 (localhost:1420)
```

### Tauri 开发
```bash
npm run tauri:dev    # 启动 Tauri 桌面应用开发模式
```

### 构建发布
```bash
npm run tauri:build  # 构建生产版本
_build.bat           # Windows 构建脚本（设置 VS 编译环境）
```

## 项目结构
```
src/                  # Preact 前端代码
  ├── main.jsx        # 入口
  ├── App.jsx         # 主界面
  ├── ConfigView.jsx  # 报销类别管理
  └── bridge.js       # Tauri IPC 桥接

src-tauri/src/        # Rust 后端
  ├── lib.rs          # 插件注册和命令绑定
  ├── commands/mod.rs # Tauri 命令实现
  ├── invoice_parser.rs # PDF 发票解析
  ├── pdf_merge.rs    # PDF 合并功能
  └── config_store.rs # 配置文件管理
```

## 关键约束

### Poppler 依赖
- 应用运行时需要 `pdftotext.exe` 和 `pdftocairo.exe`
- 路径查找逻辑：从 exe 目录向上递归 5 层，查找 `poppler/Library/bin/` 或 `resources/poppler/Library/bin/`
- 开发时需确保 poppler 二进制文件在正确位置

### Windows 构建
- 需要 Visual Studio Build Tools 和 Windows SDK
- `_build.bat` 设置了编译环境变量
- Release 配置: `opt-level = "s"`, LTO 启用, strip 符号

### 配置存储
- 使用 `tauri-plugin-fs` 访问用户目录
- 配置文件位置: `%APPDATA%/com.monarch.invoice-assistant/`
- 包含 `category.json` (报销类别) 和 `mapping.json` (映射关系)

## 常见问题

### PDF 解析失败
- 检查 poppler 二进制文件是否正确打包
- 查看 `debug_pdf` 命令输出的调试信息

### 前端热更新不工作
- 确保 `vite.config.js` 中 `server.port` 为 1420
- HMR 使用 WebSocket 端口 1421

### 构建失败
- 检查 Rust 工具链: `rustc --version` (需要 1.77+)
- 检查 Node.js 版本
- 清理缓存: `rm -rf src-tauri/target`

## 代码规范
- 文件编码统一: UTF-8
- 前端使用 Preact hooks 模式
- Rust 命令使用 `#[tauri::command]` 宏
- 错误处理: 前端用 toast 提示，后端返回 `Result<T, String>`

## Git 规范
- 默认分支: `main`
- 发布包位置: `releases/` 目录（使用 mv 而非 cp）
- 不提交: `node_modules/`, `src-tauri/target/`, `releases/`
