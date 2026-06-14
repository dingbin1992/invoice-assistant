use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// 由 Tauri 命令层在导入前设置，确保运行时能找到 pdftotext
static PDFTOTEXT_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_pdftotext_path(path: PathBuf) {
    let _ = PDFTOTEXT_PATH.set(path);
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedInvoice {
    pub file: String,
    pub file_name: String,
    pub invoice_type: String,
    pub issue_date: String,
    pub invoice_no: String,
    pub amount: String,
    pub buyer: String,
    pub project_name: String,
    pub category: String,
    pub is_invoice_pdf: bool,
    pub error: Option<String>,
    /// 调试用: 提取到的原始文本前200字符
    pub debug_text: String,
}

pub fn import_invoices(paths: Vec<String>) -> Vec<ParsedInvoice> {
    paths.into_iter().map(|p| parse_one(&p)).collect()
}

fn parse_one(path: &str) -> ParsedInvoice {
    let file_name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut inv = ParsedInvoice {
        file: path.to_string(),
        file_name,
        invoice_type: String::new(),
        issue_date: String::new(),
        invoice_no: String::new(),
        amount: String::new(),
        buyer: String::new(),
        project_name: String::new(),
        category: String::new(),
        is_invoice_pdf: false,
        error: None,
        debug_text: String::new(),
    };
    let (text, pdftotext_err) = extract_text(path);
    // 保存调试信息
    inv.debug_text = text.chars().filter(|c| !c.is_whitespace()).take(200).collect();
    if let Some(ref e) = pdftotext_err {
        inv.error = Some(e.clone());
        return inv;
    }
    if !looks_like_invoice(&text) {
        let snippet: String = text.chars().filter(|c| !c.is_whitespace()).take(40).collect();
        inv.error = Some(format!("非发票(特征不足),已跳过 [{}]", snippet));
        return inv;
    }
    inv.is_invoice_pdf = true;
    if text.contains("铁路电子客票") || text.contains("铁路") {
        inv.invoice_type = "普通发票".to_string();
        inv.project_name = "*铁路电子客票*".to_string();
        inv.issue_date = train_extract_date(&text);
        inv.invoice_no = extract_invoice_no(&text);
        inv.amount = train_extract_amount(&text);
        inv.buyer = train_extract_buyer(&text);
    } else {
        inv.invoice_type = detect_invoice_type(&text);
        inv.issue_date = extract_issue_date(&text);
        inv.invoice_no = extract_invoice_no(&text);
        inv.amount = extract_amount(&text);
        inv.buyer = extract_buyer(&text);
        inv.project_name = extract_project_name(&text);
    }
    // 如果 buyer 为空，尝试从文件名提取
    if inv.buyer.is_empty() {
        inv.buyer = extract_buyer_from_filename(&inv.file_name);
    }
    inv
}

/// 使用 pdftotext 提取文本 (通过 cmd /c 模拟命令行环境)
fn extract_text(path: &str) -> (String, Option<String>) {
    match run_pdftotext(path) {
        Ok(s) if !s.trim().is_empty() => (s.nfkd().collect::<String>(), None),
        Ok(_) => (String::new(), Some("pdftotext输出为空".to_string())),
        Err(e) => (String::new(), Some(e)),
    }
}

fn run_pdftotext(pdf_path: &str) -> Result<String, String> {
    let exe = locate_pdftotext().ok_or("pdftotext.exe 未找到".to_string())?;
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    // 直接启动 pdftotext，相对路径设 POPPLER_DATADIR 避免中文乱码
    let mut cmd = Command::new(&exe);
    cmd.arg("-layout")
       .arg("-enc")
       .arg("UTF-8")
       .arg(pdf_path)
       .arg("-")
       .current_dir(exe_dir)
       .env("POPPLER_DATADIR", "../../share/poppler");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = cmd.output()
        .map_err(|e| format!("pdftotext 启动失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pdftotext 执行失败: {}", stderr.trim()));
    }
    let bytes = output.stdout;
    if let Ok(s) = std::str::from_utf8(&bytes) { return Ok(s.to_string()); }
    let (s, _, had) = encoding_rs::GBK.decode(&bytes);
    if !had { return Ok(s.into_owned()); }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}



fn locate_pdftotext() -> Option<PathBuf> {
    // 0. 优先使用 Tauri 命令层预设的路径
    if let Some(p) = PDFTOTEXT_PATH.get() {
        if p.exists() { return Some(p.clone()); }
    }
    // 1. 环境变量
    if let Ok(p) = std::env::var("INVOICE_PDFTOTEXT") {
        let pb = PathBuf::from(p);
        if pb.exists() { return Some(pb); }
    }
    // 2. 从 exe 所在目录向上递归搜索 poppler (覆盖 NSIS/portable 安装)
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for _ in 0..5 {
            for sub in &["poppler/Library/bin/pdftotext.exe", "resources/poppler/Library/bin/pdftotext.exe"] {
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
    // 3. 开发时 cwd 相对路径
    if let Ok(cwd) = std::env::current_dir() {
        for sub in &[
            "src-tauri/poppler/Library/bin/pdftotext.exe",
            "static/poppler/Library/bin/pdftotext.exe",
        ] {
            let c = cwd.join(sub);
            if c.exists() { return Some(c); }
        }
    }
    // 4. PATH 搜索
    if let Ok(p) = which("pdftotext", ".exe") { return Some(p); }
    None
}

fn which(cmd: &str, ext: &str) -> Result<PathBuf, ()> {
    let path = std::env::var_os("PATH").ok_or(())?;
    for p in std::env::split_paths(&path) {
        let cand = p.join(format!("{}{}", cmd, ext));
        if cand.exists() { return Ok(cand); }
    }
    Err(())
}

fn looks_like_invoice(text: &str) -> bool {
    let re_no = Regex::new(r"发票(号码|代码)").unwrap();
    if re_no.is_match(text) { return true; }
    // 兜底: 有20位以上数字(电子发票号/火车票号)即判为发票
    Regex::new(r"\b\d{20,}\b").unwrap().is_match(text)
}

static RE_INVOICE_TYPE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"电子发票[（(](普通发票|专用发票|铁路电子客票)[)）]").unwrap()
});
fn detect_invoice_type(text: &str) -> String {
    if text.contains("铁路电子客票") {
        return "普通发票".to_string();
    }
    if let Some(c) = RE_INVOICE_TYPE.captures(text) {
        return match &c[1] {
            "普通发票" => "普通发票".to_string(),
            "专用发票" => "专用发票".to_string(),
            _ => String::new(),
        };
    }
    if text.contains("专用发票") {
        "专用发票".to_string()
    } else if text.contains("普通发票") {
        "普通发票".to_string()
    } else {
        String::new()
    }
}

static RE_DATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d{4})\s*年\s*(\d{1,2})\s*月\s*(\d{1,2})\s*日").unwrap());
fn extract_issue_date(text: &str) -> String {
    if let Some(c) = RE_DATE.captures(text) {
        let y = &c[1];
        let m: u32 = c[2].parse().unwrap_or(0);
        let d: u32 = c[3].parse().unwrap_or(0);
        if m >= 1 && m <= 12 && d >= 1 && d <= 31 {
            return format!("{}年{:02}月{:02}日", y, m, d);
        }
    }
    // 回退: YYYYMMDD (CJK 解码失败时)
    let re_ascii = Regex::new(r"\b(\d{4})(\d{2})(\d{2})\b").unwrap();
    for c in re_ascii.captures_iter(text) {
        let y: u32 = c[1].parse().unwrap_or(0);
        let m: u32 = c[2].parse().unwrap_or(0);
        let d: u32 = c[3].parse().unwrap_or(0);
        if y >= 2020 && y <= 2099 && m >= 1 && m <= 12 && d >= 1 && d <= 31 {
            return format!("{}年{:02}月{:02}日", y, m, d);
        }
    }
    String::new()
}

static RE_INV_NO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"发票号码[：:\s]*([0-9A-Za-z]{8,30})").unwrap()
});
fn extract_invoice_no(text: &str) -> String {
    if let Some(c) = RE_INV_NO.captures(text) {
        return c[1].to_string();
    }
    // 回退: 20位数字
    let re = Regex::new(r"\b(\d{20,25})\b").unwrap();
    if let Some(c) = re.captures(text) {
        return c[1].to_string();
    }
    String::new()
}

static RE_AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[（(]\s*小写\s*[)）]\s*¥?\s*([0-9]+(?:,[0-9]{3})*\.[0-9]{2})")
        .unwrap()
});
fn extract_amount(text: &str) -> String {
    if let Some(c) = RE_AMOUNT.captures(text) {
        return c[1].replace(',', "");
    }
    let re2 = Regex::new(r"¥\s*([0-9]+(?:,[0-9]{3})*\.[0-9]{2})").unwrap();
    if let Some(c) = re2.captures(text) {
        return c[1].replace(',', "");
    }
    String::new()
}

static RE_BUYER_NAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"购\s*名称[：:]\s*([^\n\r]+?)\s{2,}").unwrap()
});
static RE_BUYER_NAME2: Lazy<Regex> = Lazy::new(|| {
    // 兼容 dzfp_ 类 PDF: "购"与"名称"分两行, 实际字段为"买 名称："
    // 终止条件: 2+空格 或 售/销名称 (兼容 pdftotext 和 lopdf)
    Regex::new(r"买\s*名称[：:]\s*(.+?)\s{2,}").unwrap()
});
static RE_BUYER_NAME2_LOOSE: Lazy<Regex> = Lazy::new(|| {
    // lopdf 回退: 公司名后没有双空格, 以"售"或"销"为边界
    Regex::new(r"买\s*名称[：:]\s*(.+?)\s*[售销]\s*名称").unwrap()
});
static RE_BUYER_NAME3: Lazy<Regex> = Lazy::new(|| {
    // 购买方名称 (火车票及部分普通发票格式)
    Regex::new(r"购买方名称[：:]\s*([^\n\r]+)").unwrap()
});
static RE_BUYER_NAKED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"名称[：:]\s*(.+?)\s+名称[：:]").unwrap()
});
fn extract_buyer(text: &str) -> String {
    // 1. 购 名称
    if let Some(c) = RE_BUYER_NAME.captures(text) {
        return c[1].trim().to_string();
    }
    // 2. 购买方名称
    if let Some(c) = RE_BUYER_NAME3.captures(text) {
        let raw = c[1].trim();
        if let Some(pos) = raw.find("统一社会信用代码") {
            return raw[..pos].trim().to_string();
        }
        return raw.to_string();
    }
    // 3. 买 名称 (pdftotext: 双空格终止)
    if let Some(c) = RE_BUYER_NAME2.captures(text) {
        let val = c[1].trim().to_string();
        if !val.starts_with("售") && !val.contains("售 名称") {
            return val;
        }
    }
    // 4. 买 名称 (lopdf 回退: 以售/销名称为边界)
    if let Some(c) = RE_BUYER_NAME2_LOOSE.captures(text) {
        let val = c[1].trim().to_string();
        if !val.is_empty() && !val.starts_with("售") {
            return val;
        }
    }
    // 5. 裸露名称 (无购/买前缀, 第一个名称=买方, 第二个名称=卖方)
    if let Some(c) = RE_BUYER_NAKED.captures(text) {
        return c[1].trim().to_string();
    }
    String::new()
}

/// 从文件名中提取购买方名称 (回退方案)
/// 文件名格式: 前缀_发票号_公司名[_日期].pdf
fn extract_buyer_from_filename(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".pdf").unwrap_or(file_name);
    // 去掉末尾的日期后缀 (如 _20260324193919)
    let stem = if let Some(pos) = stem.rfind("_20") {
        &stem[..pos]
    } else {
        stem
    };
    // 取最后一个 _ 后面的部分
    if let Some(pos) = stem.rfind('_') {
        let name = &stem[pos + 1..];
        // 确保包含中文字符 (CJK 基本区: U+4E00..U+9FFF)
        if name.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)) {
            return name.to_string();
        }
    }
    String::new()
}

static RE_PROJECT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(\*[^*\n]+\*[^\s\n]+)").unwrap()
});
fn extract_project_name(text: &str) -> String {
    if let Some(c) = RE_PROJECT.captures(text) {
        return c[1].trim().to_string();
    }
    String::new()
}

// ---------- 火车票专用 ----------

fn train_extract_date(text: &str) -> String {
    // 火车票有两处日期: 开票日期(抬头) 和 乘车日期。优先匹配"开票日期"后面的日期
    let re_kp = Regex::new(r"开票日期[：:]\s*(\d{4})\s*年\s*(\d{1,2})\s*月\s*(\d{1,2})\s*日").unwrap();
    if let Some(c) = re_kp.captures(text) {
        let y = &c[1];
        let m: u32 = c[2].parse().unwrap_or(0);
        let d: u32 = c[3].parse().unwrap_or(0);
        if m >= 1 && m <= 12 && d >= 1 && d <= 31 {
            return format!("{}年{:02}月{:02}日", y, m, d);
        }
    }
    // 回退通用日期提取
    extract_issue_date(text)
}

fn train_extract_amount(text: &str) -> String {
    // 火车票金额在"票价"字段
    let re = Regex::new(r"票价[：:]\s*¥?\s*([0-9]+(?:\.[0-9]{2})?)").unwrap();
    if let Some(c) = re.captures(text) {
        return c[1].to_string();
    }
    // 回退: 找含¥的数字
    extract_amount(text)
}

fn train_extract_buyer(text: &str) -> String {
    // 火车票: 购买方名称
    let re = Regex::new(r"购买方名称[：:]\s*([^\n\r]+)").unwrap();
    if let Some(c) = re.captures(text) {
        let raw = c[1].trim();
        // 截断"统一社会信用代码"及之后的内容(含中间多个空格)
        let re_credit = Regex::new(r"\s{2,}统一社会信用代码.*").unwrap();
        return re_credit.replace(raw, "").trim().to_string();
    }
    // 回退通用
    extract_buyer(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_pdfs() {
        let pdf_dir = std::path::Path::new("需求文档");
        if !pdf_dir.exists() {
            eprintln!("需求文档 dir not found, skipping test");
            return;
        }
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(pdf_dir).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            if p.extension().map(|e| e == "pdf").unwrap_or(false) {
                paths.push(p.to_string_lossy().to_string());
            }
        }
        paths.sort();
        println!("\n=== 解析 {} 个 PDF ===", paths.len());
        let results = import_invoices(paths);
        for (i, inv) in results.iter().enumerate() {
            println!("\n--- PDF {}: {} ---", i + 1, inv.file_name);
            println!("  is_invoice_pdf: {}", inv.is_invoice_pdf);
            println!("  invoice_type:   '{}'", inv.invoice_type);
            println!("  issue_date:     '{}'", inv.issue_date);
            println!("  invoice_no:     '{}'", inv.invoice_no);
            println!("  amount:         '{}'", inv.amount);
            println!("  buyer:          '{}'", inv.buyer);
            println!("  project_name:   '{}'", inv.project_name);
            if let Some(ref e) = inv.error {
                println!("  error:          '{}'", e);
            }
        }
        // 检查是否有空字段但未报错的
        let mut issues = Vec::new();
        for inv in &results {
            if inv.is_invoice_pdf {
                let empty_fields: Vec<&str> = vec![
                    if inv.invoice_type.is_empty() { Some("invoice_type") } else { None },
                    if inv.issue_date.is_empty() { Some("issue_date") } else { None },
                    if inv.invoice_no.is_empty() { Some("invoice_no") } else { None },
                    if inv.amount.is_empty() { Some("amount") } else { None },
                    if inv.buyer.is_empty() { Some("buyer") } else { None },
                    if inv.project_name.is_empty() { Some("project_name") } else { None },
                ].into_iter().flatten().collect();
                if !empty_fields.is_empty() {
                    issues.push(format!("{}: 空字段 {:?}", inv.file_name, empty_fields));
                }
            }
        }
        if !issues.is_empty() {
            println!("\n⚠ 问题:");
            for issue in &issues {
                println!("  - {}", issue);
            }
        }
    }
}
