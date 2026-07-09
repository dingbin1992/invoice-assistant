use std::path::Path;
use std::process::Command;
use crate::invoice_parser::ParsedInvoice;

/// 生成费用台账（xlsx格式）
pub fn generate_ledger(
    invoices: &[ParsedInvoice],
    output_dir: &str,
) -> Result<Vec<String>, String> {
    // 生成JSON数据（需要转换为引用列表）
    let inv_refs: Vec<&ParsedInvoice> = invoices.iter().collect();
    let json_data = serialize_invoices(&inv_refs);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let json_path = format!("{}.json", timestamp);

    std::fs::write(&json_path, &json_data)
        .map_err(|e| format!("创建临时JSON失败: {}", e))?;

    // 获取Python脚本路径
    let script_path = get_ledger_script_path()?;

    // 调用Python脚本生成费用台账（后台运行，不弹出终端窗口）
    let mut cmd = Command::new("python");
    cmd.arg(&script_path)
       .arg(&json_path)
       .arg(output_dir);

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
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Python脚本执行失败: {}", stderr));
    }

    // 获取生成的文件路径
    let xlsx_path = format!("{}/费用台账.xlsx", output_dir);

    let mut output_files = Vec::new();
    if Path::new(&xlsx_path).exists() {
        output_files.push(xlsx_path);
    }

    Ok(output_files)
}

/// 生成报销封面（根据output_format生成xlsx/pdf/两种格式）
pub fn generate_cover(
    invoices: &[ParsedInvoice],
    output_dir: &str,
    output_format: &str,
) -> Result<Vec<String>, String> {
    // 按报销人+购买方分组
    let mut groups: std::collections::HashMap<String, Vec<&ParsedInvoice>> = std::collections::HashMap::new();
    for inv in invoices {
        if !inv.is_invoice_pdf {
            continue;
        }
        let key = format!("{}_{}", inv.owner, inv.buyer);
        groups.entry(key).or_default().push(inv);
    }

    let mut output_files = Vec::new();

    for (_key, group) in &groups {
        let owner = &group[0].owner;
        let buyer = &group[0].buyer;

        // 生成JSON数据
        let json_data = serialize_invoices(group);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let json_path = format!("{}.json", timestamp);
        
        std::fs::write(&json_path, &json_data)
            .map_err(|e| format!("创建临时JSON失败: {}", e))?;
        
        // 获取Python脚本路径
        let script_path = get_python_script_path()?;

        // 调用Python脚本生成报销封面（后台运行，不弹出终端窗口）
        let mut cmd = Command::new("python");
        cmd.arg(&script_path)
           .arg(&json_path)
           .arg(output_dir)
           .arg(owner)
           .arg(buyer)
           .arg(output_format);
        
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Python脚本执行失败: {}", stderr));
        }
        
        // 获取生成的文件路径
        let base_name = format!("费用报销审批单_{}_{}", owner, buyer);
        let xlsx_path = format!("{}/{}.xlsx", output_dir, base_name);
        let pdf_path = format!("{}/{}.pdf", output_dir, base_name);

        // 根据output_format决定添加哪些文件
        if output_format == "xlsx" || output_format == "both" {
            if Path::new(&xlsx_path).exists() {
                output_files.push(xlsx_path);
            }
        }
        if output_format == "pdf" || output_format == "both" {
            if Path::new(&pdf_path).exists() {
                output_files.push(pdf_path);
            }
        }
    }

    Ok(output_files)
}

/// 获取费用台账Python脚本路径
fn get_ledger_script_path() -> Result<String, String> {
    // 优先使用当前目录下的脚本
    let local_script = "generate_ledger.py";
    if Path::new(local_script).exists() {
        return Ok(local_script.to_string());
    }

    // 从exe所在目录查找
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

        // 1. 直接在exe目录
        let candidate = dir.join("generate_ledger.py");
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }

        // 2. 在output_py_tools目录
        let output_py_tools_candidate = dir.join("output_py_tools").join("generate_ledger.py");
        if output_py_tools_candidate.exists() {
            return Ok(output_py_tools_candidate.to_string_lossy().to_string());
        }

        // 3. 向上查找最多3层
        let mut dir = dir;
        for _ in 0..3 {
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
                let candidate = dir.join("generate_ledger.py");
                if candidate.exists() {
                    return Ok(candidate.to_string_lossy().to_string());
                }
                let output_py_tools_candidate = dir.join("output_py_tools").join("generate_ledger.py");
                if output_py_tools_candidate.exists() {
                    return Ok(output_py_tools_candidate.to_string_lossy().to_string());
                }
            } else {
                break;
            }
        }
    }

    Err("找不到generate_ledger.py脚本".to_string())
}

/// 获取Python脚本路径
fn get_python_script_path() -> Result<String, String> {
    // 优先使用当前目录下的脚本
    let local_script = "generate_cover.py";
    if Path::new(local_script).exists() {
        return Ok(local_script.to_string());
    }
    
    // 从exe所在目录查找
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        
        // 1. 直接在exe目录
        let candidate = dir.join("generate_cover.py");
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
        
        // 2. 在output_py_tools目录
        let output_py_tools_candidate = dir.join("output_py_tools").join("generate_cover.py");
        if output_py_tools_candidate.exists() {
            return Ok(output_py_tools_candidate.to_string_lossy().to_string());
        }
        
        // 3. 向上查找最多3层
        let mut dir = dir;
        for _ in 0..3 {
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
                let candidate = dir.join("generate_cover.py");
                if candidate.exists() {
                    return Ok(candidate.to_string_lossy().to_string());
                }
                let output_py_tools_candidate = dir.join("output_py_tools").join("generate_cover.py");
                if output_py_tools_candidate.exists() {
                    return Ok(output_py_tools_candidate.to_string_lossy().to_string());
                }
            } else {
                break;
            }
        }
    }
    
    Err("找不到generate_cover.py脚本".to_string())
}

/// 将发票数据序列化为JSON
fn serialize_invoices(invoices: &[&ParsedInvoice]) -> String {
    let mut items = Vec::new();
    for inv in invoices {
        let item = serde_json::json!({
            "file": inv.file,
            "file_name": inv.file_name,
            "invoice_type": inv.invoice_type,
            "issue_date": inv.issue_date,
            "invoice_no": inv.invoice_no,
            "amount": inv.amount,
            "buyer": inv.buyer,
            "project_name": inv.project_name,
            "category": inv.category,
            "owner": inv.owner,
            "is_invoice_pdf": inv.is_invoice_pdf
        });
        items.push(item);
    }
    serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string())
}