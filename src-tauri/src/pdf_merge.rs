use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use image::GenericImageView;
use lopdf::{Document, Object};
use printpdf::{PdfDocument, Image, Mm, ImageTransform};
use serde::Serialize;

/// 由 Tauri 命令层在导入前设置，确保运行时能找到 pdftocairo
static PDFTOCAIRO_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_pdftocairo_path(path: PathBuf) {
    let _ = PDFTOCAIRO_PATH.set(path);
}

pub fn debug_pdf(path: &str) -> Result<String, String> {
    let doc = Document::load(path).map_err(|e| format!("加载PDF失败: {}", e))?;
    let pages = doc.get_pages();
    let mut info = format!("PDF: {}, 页面数: {}\n", path, pages.len());

    for (page_num, (&page_num_key, &page_id)) in pages.iter().enumerate() {
        info += &format!("\n=== 页面 {} (页码: {}, ID: {:?}) ===\n", page_num + 1, page_num_key, page_id);

        match doc.get_page_contents(page_id) {
            content_ids if !content_ids.is_empty() => {
                info += &format!("内容流ID: {:?}\n", content_ids);
                for cid in &content_ids {
                    match doc.get_object(*cid) {
                        Ok(Object::Stream(st)) => {
                            info += &format!("  流{}长度: {}\n", cid.0, st.content.len());
                        }
                        Ok(_) => info += &format!("  对象{}不是流\n", cid.0),
                        Err(e) => info += &format!("  获取对象{}失败: {}\n", cid.0, e),
                    }
                }
            }
            _ => info += "无内容流\n",
        }

        match doc.get_page_content(page_id) {
            Ok(content) => {
                info += &format!("解码后内容长度: {}\n", content.len());
            }
            Err(e) => info += &format!("解码内容失败: {}\n", e),
        }
    }
    Ok(info)
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub total: usize,
    pub output_dir: String,
    pub files: Vec<String>,
}

pub fn merge_pdfs(
    input_files: Vec<String>,
    output_dir: String,
    file_prefix: String,
) -> Result<MergeResult, String> {
    if input_files.is_empty() {
        return Err("没有需要合并的PDF".into());
    }

    // 确保输出目录存在
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    let mut out_files = Vec::new();
    let mut idx = 0usize;
    let len = input_files.len();
    while idx < len {
        let end = (idx + 2).min(len);
        let group = &input_files[idx..end];
        let out_path = std::path::Path::new(&output_dir)
            .join(format!("{}_{:03}.pdf", file_prefix, out_files.len() + 1));
        if group.len() == 1 {
            // 单文件：读取并保存到临时文件，然后重命名
            let tmp_path = std::path::Path::new(&output_dir)
                .join(format!(".tmp_{}.pdf", out_files.len()));
            let data = std::fs::read(&group[0])
                .map_err(|e| format!("读取源PDF失败 {}: {}", group[0], e))?;
            std::fs::write(&tmp_path, &data).map_err(|e| format!("写入临时文件失败: {}", e))?;
            // 删除已存在的目标文件
            if out_path.exists() {
                std::fs::remove_file(&out_path).ok();
            }
            std::fs::rename(&tmp_path, &out_path).map_err(|e| format!("重命名文件失败: {}", e))?;
        } else {
            merge_two_to_a4(&group[0], &group[1], &out_path)?;
        }
        out_files.push(out_path.to_string_lossy().to_string());
        idx = end;
    }
    Ok(MergeResult {
        total: out_files.len(),
        output_dir,
        files: out_files,
    })
}

/// 用 pdftocairo 将 PDF 转换为 PNG，然后用 printpdf 合并到 A4
fn merge_two_to_a4(top_pdf: &str, bottom_pdf: &str, out: &Path) -> Result<(), String> {
    let a4_w_mm: f32 = 210.0;
    let a4_h_mm: f32 = 297.0;
    let half_h_mm: f32 = a4_h_mm / 2.0;
    let margin_mm: f32 = 3.0;

    // 将 PDF 转换为 PNG
    let top_png = pdf_to_png(top_pdf)?;
    let bottom_png = pdf_to_png(bottom_pdf)?;

    // 读取图片获取尺寸
    let top_img = image::open(&top_png).map_err(|e| format!("读取顶部图片失败: {}", e))?;
    let bottom_img = image::open(&bottom_png).map_err(|e| format!("读取底部图片失败: {}", e))?;

    let (top_w, top_h) = top_img.dimensions();
    let (bottom_w, bottom_h) = bottom_img.dimensions();

    // 计算图片在 A4 半页中的缩放比例（mm）
    // 假设 300 DPI: 1 像素 = 25.4/300 mm
    let px_to_mm: f32 = 25.4 / 300.0;
    let top_w_mm = top_w as f32 * px_to_mm;
    let top_h_mm = top_h as f32 * px_to_mm;
    let bottom_w_mm = bottom_w as f32 * px_to_mm;
    let bottom_h_mm = bottom_h as f32 * px_to_mm;

    // 缩放比例：适应半页高度和 A4 宽度
    let top_scale = ((a4_w_mm - margin_mm * 2.0) / top_w_mm)
        .min((half_h_mm - margin_mm * 2.0) / top_h_mm);
    let bottom_scale = ((a4_w_mm - margin_mm * 2.0) / bottom_w_mm)
        .min((half_h_mm - margin_mm * 2.0) / bottom_h_mm);

    // 居中偏移（mm）
    let top_draw_w = top_w_mm * top_scale;
    let top_draw_h = top_h_mm * top_scale;
    let top_x = (a4_w_mm - top_draw_w) / 2.0;
    let top_y = half_h_mm + (half_h_mm - top_draw_h) / 2.0;

    let bottom_draw_w = bottom_w_mm * bottom_scale;
    let bottom_draw_h = bottom_h_mm * bottom_scale;
    let bottom_x = (a4_w_mm - bottom_draw_w) / 2.0;
    let bottom_y = (half_h_mm - bottom_draw_h) / 2.0;

    // 创建 PDF
    let doc = PdfDocument::empty("发票汇总");
    let (page1, layer1) = doc.add_page(Mm(a4_w_mm), Mm(a4_h_mm), "发票");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // 嵌入顶部图片
    let top_image = Image::from_dynamic_image(&top_img);
    top_image.add_to_layer(
        current_layer.clone(),
        ImageTransform {
            translate_x: Some(Mm(top_x)),
            translate_y: Some(Mm(top_y)),
            rotate: None,
            scale_x: Some(top_scale),
            scale_y: Some(top_scale),
            dpi: Some(300.0),
        },
    );

    // 嵌入底部图片
    let bottom_image = Image::from_dynamic_image(&bottom_img);
    bottom_image.add_to_layer(
        current_layer,
        ImageTransform {
            translate_x: Some(Mm(bottom_x)),
            translate_y: Some(Mm(bottom_y)),
            rotate: None,
            scale_x: Some(bottom_scale),
            scale_y: Some(bottom_scale),
            dpi: Some(300.0),
        },
    );

    // 保存 PDF
    let out_dir = out.parent().unwrap_or(Path::new("."));
    let tmp_path = out_dir.join(format!(
        ".tmp_merge_{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    let file = std::fs::File::create(&tmp_path)
        .map_err(|e| format!("创建临时PDF失败: {}", e))?;
    let mut writer = std::io::BufWriter::new(file);
    doc.save(&mut writer)
        .map_err(|e| format!("保存PDF失败: {}", e))?;

    // 清理临时 PNG
    std::fs::remove_file(&top_png).ok();
    std::fs::remove_file(&bottom_png).ok();

    // 重命名到目标路径
    if out.exists() {
        std::fs::remove_file(out).ok();
    }
    std::fs::rename(&tmp_path, out).map_err(|e| format!("重命名PDF失败: {}", e))?;
    Ok(())
}

/// 用 pdftocairo 将 PDF 第一页转换为 PNG，返回临时 PNG 路径
fn pdf_to_png(pdf_path: &str) -> Result<String, String> {
    let exe = locate_pdftocairo().ok_or("pdftocairo.exe 未找到")?;
    let exe_dir = exe.parent().unwrap_or(Path::new("."));

    // 临时输出路径（不带扩展名，pdftocairo 会自动加 .png）
    let tmp_base = format!("{}.tmp_pdf2png_{}",
        std::env::temp_dir().to_string_lossy(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let mut cmd = Command::new(&exe);
    cmd.arg("-png")
        .arg("-r").arg("300")
        .arg("-singlefile")
        .arg(pdf_path)
        .arg(&tmp_base)
        .current_dir(exe_dir)
        .env("POPPLER_DATADIR", "../../share/poppler");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output().map_err(|e| format!("pdftocairo 启动失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pdftocairo 执行失败: {}", stderr.trim()));
    }

    let png_path = format!("{}.png", tmp_base);
    if !Path::new(&png_path).exists() {
        return Err("pdftocairo 未生成 PNG 文件".into());
    }
    Ok(png_path)
}

fn locate_pdftocairo() -> Option<PathBuf> {
    // 0. 优先使用预设路径
    if let Some(p) = PDFTOCAIRO_PATH.get() {
        if p.exists() { return Some(p.clone()); }
    }
    // 1. 从 exe 所在目录向上递归搜索 poppler
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
    // 2. 开发时 cwd 相对路径
    if let Ok(cwd) = std::env::current_dir() {
        for sub in &[
            "src-tauri/poppler/Library/bin/pdftocairo.exe",
            "static/poppler/Library/bin/pdftocairo.exe",
        ] {
            let c = cwd.join(sub);
            if c.exists() { return Some(c); }
        }
    }
    // 3. PATH 搜索
    if let Ok(p) = which("pdftocairo", ".exe") { return Some(p); }
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
