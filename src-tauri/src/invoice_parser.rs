use std::path::{Path, PathBuf};
use std::process::Command;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

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
    };
    let text = match extract_text(path) {
        Ok(t) => t,
        Err(e) => {
            inv.error = Some(format!("读取失败: {}", e));
            return inv;
        }
    };
    if !looks_like_invoice(&text) {
        inv.error = Some("非发票PDF,已跳过".into());
        return inv;
    }
    inv.is_invoice_pdf = true;
    inv.invoice_type = detect_invoice_type(&text);
    inv.issue_date = extract_issue_date(&text);
    inv.invoice_no = extract_invoice_no(&text);
    inv.amount = extract_amount(&text);
    inv.buyer = extract_buyer(&text);
    inv.project_name = extract_project_name(&text);
    inv
}

/// 优先用 poppler pdftotext, 失败时回退到 lopdf 提取
fn extract_text(path: &str) -> Result<String, String> {
    if let Some(s) = run_pdftotext(path) {
        if !s.trim().is_empty() {
            return Ok(s);
        }
    }
    // 回退 lopdf (UniGB-UCS2-H 时 CMap 会失败, 但至少能拿到部分字段)
    let doc = lopdf::Document::load(Path::new(path)).map_err(|e| e.to_string())?;
    let mut out = String::new();
    let pages = doc.get_pages();
    for (i, _) in pages.iter().enumerate() {
        if let Ok(t) = doc.extract_text(&[(i + 1) as u32]) {
            if !t.trim().is_empty() {
                out.push_str(&t);
                out.push('\n');
            }
        }
    }
    if !out.trim().is_empty() { return Ok(out); }
    Err("PDF 文本提取为空".into())
}

fn run_pdftotext(pdf_path: &str) -> Option<String> {
    let exe = locate_pdftotext()?;
    let output = Command::new(&exe)
        .arg("-layout")
        .arg(pdf_path)
        .arg("-")
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let bytes = output.stdout;
    // pdftotext 26.02 默认 UTF-8 (chcp 65001 时), 但 Windows 旧版可能是 GBK
    if let Ok(s) = std::str::from_utf8(&bytes) { return Some(s.to_string()); }
    let (s, _, had) = encoding_rs::GBK.decode(&bytes);
    if !had { return Some(s.into_owned()); }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn locate_pdftotext() -> Option<PathBuf> {
    // 1) 环境变量 INVOICE_PDFTOTEXT
    if let Ok(p) = std::env::var("INVOICE_PDFTOTEXT") {
        let pb = PathBuf::from(p);
        if pb.exists() { return Some(pb); }
    }
    // 2) 应用自带: exe 同级或相对目录
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?.to_path_buf();
        // 打包后: <install>/poppler/pdftotext.exe (resources 映射)
        for rel in [
            "poppler/pdftotext.exe",
            "resources/poppler/pdftotext.exe",
            "../poppler/pdftotext.exe",
        ] {
            let cand = dir.join(rel);
            if cand.exists() { return Some(cand); }
        }
        // 开发期: 从工程根目录的 static/poppler/...
        if let Ok(cwd) = std::env::current_dir() {
            for sub in [
                "static/poppler/Library/bin/pdftotext.exe",
                "static/poppler/poppler-26.02.0/Library/bin/pdftotext.exe",
            ] {
                let cand = cwd.join(sub);
                if cand.exists() { return Some(cand); }
            }
            for p in [&cwd.join(".."), &cwd.join("../..")] {
                for sub in [
                    "static/poppler/Library/bin/pdftotext.exe",
                    "static/poppler/poppler-26.02.0/Library/bin/pdftotext.exe",
                ] {
                    let cand = p.join(sub);
                    if cand.exists() { return Some(cand); }
                }
            }
        }
    }
    // 3) PATH 中查找
    for ext in &["", ".exe"] {
        if let Ok(p) = which("pdftotext", ext) { return Some(p); }
    }
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
    let has_ele = text.contains("电子发票") || text.contains("增值税");
    let has_type = text.contains("普通发票") || text.contains("专用发票");
    has_ele && has_type
}

static RE_INVOICE_TYPE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"电子发票[（(](普通发票|专用发票)[)）]").unwrap()
});
fn detect_invoice_type(text: &str) -> String {
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
    Regex::new(r"购买方信息[\s\S]*?名\s*称[：:]\s*([^\n\r]+)").unwrap()
});
fn extract_buyer(text: &str) -> String {
    if let Some(c) = RE_BUYER_NAME.captures(text) {
        return c[1].trim().to_string();
    }
    // 回退: 购买方信息 ... 名称: XXX
    let re = Regex::new(r"购\s*[\s\S]{0,8}?名\s*称[：:]\s*([^\n\r]+)").unwrap();
    if let Some(c) = re.captures(text) {
        return c[1].trim().to_string();
    }
    String::new()
}

static RE_PROJECT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"项目名称[\s\S]{0,40}?(\S+)").unwrap()
});
fn extract_project_name(text: &str) -> String {
    if let Some(c) = RE_PROJECT.captures(text) {
        let v = c[1].trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    String::new()
}
