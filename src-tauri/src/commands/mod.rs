use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::config_store::{category_path, ensure_initial_config, get_config_dir, mapping_path};
use crate::invoice_parser::{ParsedInvoice, PdfEntry};
use crate::invoice_parser as parser;
use crate::py_merge as py_merger;
use crate::cover_generator as cover_gen;

#[tauri::command]
pub fn get_initial_paths(app: AppHandle) -> Result<serde_json::Value, String> {
    let _ = ensure_initial_config(&app);
    let config_dir = get_config_dir(&app)?;
    let home = dirs_home().unwrap_or_else(|| ".".to_string());
    let downloads = dirs_known(&home, "Downloads")
        .or_else(|| Some(home.clone()))
        .unwrap();
    let desktop = dirs_known(&home, "Desktop")
        .or_else(|| Some(home.clone()))
        .unwrap();
    Ok(serde_json::json!({
        "workDir": downloads,
        "outputDir": desktop,
        "configDir": config_dir.to_string_lossy().to_string()
    }))
}

#[tauri::command]
pub async fn pick_directory(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    let title_str = title.unwrap_or_else(|| "选择目录".to_string());
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title(&title_str)
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });
    let path = rx.recv().map_err(|e| e.to_string())?;
    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
pub fn open_directory(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("目录不存在: {}", path));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_pdfs(path: String) -> Result<Vec<PdfEntry>, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("工作目录不存在: {}", path));
    }
    let mut out = Vec::new();
    // 只扫描子文件夹，文件夹名即报销人
    for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ep = entry.path();
        if ep.is_dir() {
            let owner = ep.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            collect_pdfs_recursive(&ep, &owner, &mut out);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect_pdfs_recursive(dir: &Path, owner: &str, out: &mut Vec<PdfEntry>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let ep = entry.path();
        if ep.is_file() {
            if let Some(ext) = ep.extension() {
                if ext.eq_ignore_ascii_case("pdf") {
                    out.push(PdfEntry {
                        path: ep.to_string_lossy().to_string(),
                        owner: owner.to_string(),
                    });
                }
            }
        } else if ep.is_dir() {
            collect_pdfs_recursive(&ep, owner, out);
        }
    }
}

fn dirs_home() -> Option<String> {
    if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").ok()
    } else {
        std::env::var("HOME").ok()
    }
}

fn dirs_known(home: &str, sub: &str) -> Option<String> {
    let p = PathBuf::from(home).join(sub);
    if p.exists() { Some(p.to_string_lossy().to_string()) } else { None }
}

#[tauri::command]
pub fn read_mapping(app: AppHandle) -> Result<serde_json::Value, String> {
    let _ = ensure_initial_config(&app);
    let p = mapping_path(&app)?;
    let s = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_mapping(app: AppHandle, data: serde_json::Value) -> Result<(), String> {
    let p = mapping_path(&app)?;
    let s = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_mapping(app: AppHandle, src: String) -> Result<String, String> {
    let cp = category_path(&app)?;
    let cat_text = std::fs::read_to_string(&cp).map_err(|e| e.to_string())?;
    let categories: Vec<String> = serde_json::from_str(&cat_text).map_err(|_| "category.json 损坏".to_string())?;
    let raw = std::fs::read_to_string(&src).map_err(|e| format!("读取失败: {}", e))?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "导入失败".to_string())?;
    let arr = v.as_array().ok_or_else(|| "导入失败".to_string())?;
    for item in arr {
        let obj = item.as_object().ok_or_else(|| "导入失败".to_string())?;
        if !obj.contains_key("项目名称")
            || !obj.contains_key("通用项目名称")
            || !obj.contains_key("大类别")
            || !obj.contains_key("报销类别")
        {
            return Err("导入失败".into());
        }
        let cat = obj.get("报销类别").and_then(|v| v.as_str()).unwrap_or("");
        if !categories.iter().any(|c| c == cat) {
            return Err("导入失败".into());
        }
    }
    let dest = mapping_path(&app)?;
    std::fs::write(&dest, serde_json::to_string_pretty(&v).unwrap())
        .map_err(|e| e.to_string())?;
    Ok("ok".into())
}

#[tauri::command]
pub fn export_mapping(app: AppHandle, dest: String) -> Result<String, String> {
    let p = mapping_path(&app)?;
    let s = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    std::fs::write(&dest, s).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[tauri::command]
pub fn get_mapping_path(app: AppHandle) -> Result<String, String> {
    let _ = ensure_initial_config(&app);
    let p = mapping_path(&app)?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn read_category(app: AppHandle) -> Result<serde_json::Value, String> {
    let _ = ensure_initial_config(&app);
    let p = category_path(&app)?;
    let s = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_invoices(app: AppHandle, entries: Vec<PdfEntry>) -> Vec<ParsedInvoice> {
    // 从安装目录向上递归搜索 pdftotext
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        'outer: for _ in 0..5 {
            for sub in &["poppler/Library/bin/pdftotext.exe", "resources/poppler/Library/bin/pdftotext.exe"] {
                let cand = dir.join(sub);
                if cand.exists() {
                    parser::set_pdftotext_path(cand);
                    break 'outer;
                }
            }
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    // 加载 mapping.json 用于自动匹配报销类别
    let mappings = load_mapping_for_match(&app);
    parser::import_invoices(entries, mappings)
}

fn load_mapping_for_match(app: &AppHandle) -> Vec<parser::MappingRule> {
    let _ = ensure_initial_config(app);
    let p = match mapping_path(app) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let s = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let v: serde_json::Value = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let arr = match v.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter().filter_map(|item| {
        let obj = item.as_object()?;
        let pattern = obj.get("项目名称")?.as_str()?.to_string();
        let category = obj.get("报销类别")?.as_str()?.to_string();
        Some(parser::MappingRule { pattern, category })
    }).collect()
}

#[tauri::command]
pub fn merge_pdfs(
    input_files: Vec<String>,
    output_dir: String,
    file_prefix: String,
) -> Result<py_merger::MergeResult, String> {
    py_merger::merge_pdfs(input_files, output_dir, file_prefix)
}

#[tauri::command]
pub fn generate_cover_pdf(
    invoices: Vec<parser::ParsedInvoice>,
    output_dir: String,
    output_format: String,
) -> Result<py_merger::MergeResult, String> {
    // 创建报销封面子目录
    let cover_dir = format!("{}/_报销封面", output_dir);
    std::fs::create_dir_all(&cover_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 调用Python脚本生成报销封面
    let out_files = cover_gen::generate_cover(&invoices, &cover_dir, &output_format)?;
    let total_files = out_files.len();

    Ok(py_merger::MergeResult {
        total: total_files,
        output_dir: cover_dir,
        files: out_files,
    })
}

#[tauri::command]
pub fn generate_ledger_pdf(
    invoices: Vec<parser::ParsedInvoice>,
    output_dir: String,
) -> Result<py_merger::MergeResult, String> {
    // 确保输出目录存在
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 调用Python脚本生成费用台账
    let out_files = cover_gen::generate_ledger(&invoices, &output_dir)?;
    let total_files = out_files.len();

    Ok(py_merger::MergeResult {
        total: total_files,
        output_dir,
        files: out_files,
    })
}
