use std::path::{Path, PathBuf};
use std::process::Command;
use serde::Serialize;

/// 合并结果
#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub total: usize,
    pub output_dir: String,
    pub files: Vec<String>,
}

/// 从安装目录向上递归搜索pdftocairo
fn find_pdftocairo() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for _ in 0..5 {
            for sub in &["poppler/Library/bin/pdftocairo.exe", "resources/poppler/Library/bin/pdftocairo.exe"] {
                let c = dir.join(sub);
                if c.exists() { return Some(c); }
            }
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    None
}

/// 获取Python脚本路径
fn get_merge_script_path() -> Result<String, String> {
    // 优先使用当前目录下的脚本
    let local_script = "merge_pdfs.py";
    if Path::new(local_script).exists() {
        return Ok(local_script.to_string());
    }

    // 从exe所在目录查找
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

        // 1. 直接在exe目录
        let candidate = dir.join("merge_pdfs.py");
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }

        // 2. 在output_py_tools目录
        let output_py_tools_candidate = dir.join("output_py_tools").join("merge_pdfs.py");
        if output_py_tools_candidate.exists() {
            return Ok(output_py_tools_candidate.to_string_lossy().to_string());
        }

        // 3. 向上查找最多3层
        let mut dir = dir;
        for _ in 0..3 {
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
                let candidate = dir.join("merge_pdfs.py");
                if candidate.exists() {
                    return Ok(candidate.to_string_lossy().to_string());
                }
                let output_py_tools_candidate = dir.join("output_py_tools").join("merge_pdfs.py");
                if output_py_tools_candidate.exists() {
                    return Ok(output_py_tools_candidate.to_string_lossy().to_string());
                }
            } else {
                break;
            }
        }
    }

    Err("找不到merge_pdfs.py脚本".to_string())
}

/// 调用Python脚本合并PDF
pub fn merge_pdfs(
    input_files: Vec<String>,
    output_dir: String,
    file_prefix: String,
) -> Result<MergeResult, String> {
    // 确保输出目录存在
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 生成JSON数据
    let json_data = serde_json::to_string_pretty(&input_files).map_err(|e| format!("JSON序列化失败: {}", e))?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let json_path = format!("{}.json", timestamp);

    std::fs::write(&json_path, &json_data)
        .map_err(|e| format!("创建临时JSON失败: {}", e))?;

    // 获取Python脚本路径
    let script_path = get_merge_script_path()?;

    // 设置pdftocairo路径环境变量
    let mut cmd = Command::new("python");
    cmd.arg(&script_path)
       .arg(&json_path)
       .arg(&output_dir)
       .arg(&file_prefix);

    // 如果找到pdftocairo，设置环境变量
    if let Some(pdftocairo_path) = find_pdftocairo() {
        let pdftocairo_dir = pdftocairo_path.parent().unwrap_or(Path::new("."));
        cmd.env("PDFTOCAIRO_PATH", pdftocairo_path.to_string_lossy().to_string());
        // 设置POPPLER_DATADIR
        let poppler_data = pdftocairo_dir.join("../../share/poppler");
        if poppler_data.exists() {
            cmd.env("POPPLER_DATADIR", poppler_data.to_string_lossy().to_string());
        }
    }

    // Windows: 隐藏终端窗口
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output()
        .map_err(|e| format!("调用Python脚本失败: {}", e))?;

    // 清理临时JSON文件
    std::fs::remove_file(&json_path).ok();

    if !output.status.success() {
        // 尝试用不同编码解码错误信息
        let stderr = if let Ok(s) = String::from_utf8(output.stderr.clone()) {
            s
        } else if let Ok(s) = String::from_utf8(output.stderr) {
            s
        } else {
            String::from("无法解码错误信息")
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        return Err(format!("Python脚本执行失败:\nstdout: {}\nstderr: {}", stdout, stderr));
    }

    // 获取生成的文件路径
    let pdf_path = format!("{}/{}.pdf", output_dir, file_prefix);

    let mut out_files = Vec::new();
    if Path::new(&pdf_path).exists() {
        out_files.push(pdf_path);
    }

    Ok(MergeResult {
        total: out_files.len(),
        output_dir,
        files: out_files,
    })
}
