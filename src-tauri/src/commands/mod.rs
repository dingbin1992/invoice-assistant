use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::config_store::{category_path, ensure_initial_config, get_config_dir, mapping_path};
use crate::invoice_parser::ParsedInvoice;
use crate::invoice_parser as parser;
use crate::pdf_merge as merger;

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
pub fn list_pdfs(path: String) -> Result<Vec<String>, String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("工作目录不存在: {}", path));
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&p).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ep = entry.path();
        if ep.is_file() {
            if let Some(ext) = ep.extension() {
                if ext.eq_ignore_ascii_case("pdf") {
                    out.push(ep.to_string_lossy().to_string());
                }
            }
        }
    }
    out.sort();
    Ok(out)
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
pub fn read_category(app: AppHandle) -> Result<serde_json::Value, String> {
    let _ = ensure_initial_config(&app);
    let p = category_path(&app)?;
    let s = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_invoices(_app: AppHandle, paths: Vec<String>) -> Vec<ParsedInvoice> {
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
    parser::import_invoices(paths)
}

#[tauri::command]
pub fn merge_pdfs(
    input_files: Vec<String>,
    output_dir: String,
    file_prefix: String,
) -> Result<merger::MergeResult, String> {
    merger::merge_pdfs(input_files, output_dir, file_prefix)
}

#[tauri::command]
pub fn debug_pdf(path: String) -> Result<String, String> {
    merger::debug_pdf(&path)
}
