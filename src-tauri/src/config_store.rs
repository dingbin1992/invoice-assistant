use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub fn get_config_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法获取程序路径: {}", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法获取程序目录".to_string())?
        .join("config");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    Ok(dir)
}

pub fn mapping_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_config_dir(app)?.join("mapping.json"))
}

pub fn category_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_config_dir(app)?.join("category.json"))
}

pub fn ensure_initial_config(app: &AppHandle) -> Result<(), String> {
    let mp = mapping_path(app)?;
    let cp = category_path(app)?;
    if !mp.exists() {
        if let Ok(res_dir) = app.path().resource_dir() {
            let bundled_mp = res_dir.join("config/mapping.json");
            let bundled_cp = res_dir.join("config/category.json");
            if bundled_mp.exists() {
                std::fs::copy(&bundled_mp, &mp).map_err(|e| e.to_string())?;
            }
            if !cp.exists() && bundled_cp.exists() {
                std::fs::copy(&bundled_cp, &cp).map_err(|e| e.to_string())?;
            }
            if mp.exists() { return Ok(()); }
        }
        let seed = serde_json::json!([
            {"项目名称": "*住宿*", "通用项目名称": "住宿", "大类别": "其他费用", "报销类别": "住宿费"},
            {"项目名称": "*餐饮*", "通用项目名称": "餐饮", "大类别": "其他费用", "报销类别": "餐饮费"},
            {"项目名称": "*汽油*|*燃油*", "通用项目名称": "加油", "大类别": "交通", "报销类别": "加油费"},
            {"项目名称": "*通行*|*ETC*", "通用项目名称": "过路", "大类别": "交通", "报销类别": "过路费"}
        ]);
        std::fs::write(&mp, serde_json::to_string_pretty(&seed).unwrap())
            .map_err(|e| e.to_string())?;
    }
    if !cp.exists() {
        let seed = serde_json::json!([
            "加油费", "过路费", "餐饮费", "住宿费",
            "邮寄费", "办公用品", "通讯费", "其他费用"
        ]);
        std::fs::write(&cp, serde_json::to_string_pretty(&seed).unwrap())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
