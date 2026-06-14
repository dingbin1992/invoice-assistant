use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::Serialize;

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
            let data = std::fs::read(&group[0]).map_err(|e| format!("读取源PDF失败 {}: {}", group[0], e))?;
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
    Ok(MergeResult { total: out_files.len(), output_dir, files: out_files })
}

fn merge_two_to_a4(top_pdf: &str, bottom_pdf: &str, out: &Path) -> Result<(), String> {
    let mut out_doc = Document::with_version("1.5");

    let a4_w: f64 = 595.0;
    let a4_h: f64 = 842.0;
    let half_h: f64 = a4_h / 2.0;

    let top_bytes = std::fs::read(top_pdf).map_err(|e| format!("读取PDF失败: {}", e))?;
    let bottom_bytes = std::fs::read(bottom_pdf).map_err(|e| format!("读取PDF失败: {}", e))?;

    let top_src = Document::load_mem(&top_bytes).map_err(|e| format!("解析PDF失败: {}", e))?;
    let bottom_src = Document::load_mem(&bottom_bytes).map_err(|e| format!("解析PDF失败: {}", e))?;

    let top_pages = top_src.get_pages();
    let bottom_pages = bottom_src.get_pages();

    let top_page_id = *top_pages.values().next().ok_or("顶部PDF无页面")?;
    let bottom_page_id = *bottom_pages.values().next().ok_or("底部PDF无页面")?;

    let top_bbox = page_bbox(&top_src, top_page_id).unwrap_or((595.0, 842.0));
    let bottom_bbox = page_bbox(&bottom_src, bottom_page_id).unwrap_or((595.0, 842.0));

    // 计算缩放比例，使发票适应A4半页
    let margin = 6.0;
    let top_scale = ((a4_w - margin * 2.0) / top_bbox.0).min((half_h - margin * 2.0) / top_bbox.1);
    let bottom_scale = ((a4_w - margin * 2.0) / bottom_bbox.0).min((half_h - margin * 2.0) / bottom_bbox.1);

    // 计算居中偏移
    let top_new_w = top_bbox.0 * top_scale;
    let top_new_h = top_bbox.1 * top_scale;
    let top_tx = (a4_w - top_new_w) / 2.0;
    let top_ty = half_h + (half_h - top_new_h) / 2.0;

    let bottom_new_w = bottom_bbox.0 * bottom_scale;
    let bottom_new_h = bottom_bbox.1 * bottom_scale;
    let bottom_tx = (a4_w - bottom_new_w) / 2.0;
    let bottom_ty = (half_h - bottom_new_h) / 2.0;

    // 将顶部PDF转换为Form XObject
    let (top_form_id, top_resources_id) = page_to_form_xobject(&top_src, top_page_id, &mut out_doc)?;
    let (bottom_form_id, bottom_resources_id) = page_to_form_xobject(&bottom_src, bottom_page_id, &mut out_doc)?;

    // 创建页面内容流：先绘制顶部发票，再绘制底部发票
    let content = format!(
        "q {0:.4} 0 0 {1:.4} {2:.4} {3:.4} cm /Fm1 Do Q \
         q {4:.4} 0 0 {5:.4} {6:.4} {7:.4} cm /Fm2 Do Q",
        top_scale, top_scale, top_tx, top_ty,
        bottom_scale, bottom_scale, bottom_tx, bottom_ty
    );
    
    let mut content_dict = Dictionary::new();
    content_dict.set(b"Length", Object::Integer(content.len() as i64));
    let content_stream = Stream::new(content_dict, content.as_bytes().to_vec());
    let content_id = out_doc.add_object(content_stream);

    // 创建资源字典 - 合并两个Form的资源
    let mut xobjects = Dictionary::new();
    xobjects.set(b"Fm1", Object::Reference(top_form_id));
    xobjects.set(b"Fm2", Object::Reference(bottom_form_id));
    
    let mut resources = Dictionary::new();
    resources.set(b"XObject", Object::Dictionary(xobjects));
    
    // 如果两个页面都有字体，需要合并
    if let Some(top_font_ref) = top_resources_id {
        if let Ok(Object::Dictionary(top_res)) = out_doc.get_object(top_font_ref) {
            if let Ok(font_obj) = top_res.get(b"Font") {
                resources.set(b"Font", font_obj.clone());
            }
        }
    }
    if let Some(bottom_font_ref) = bottom_resources_id {
        if let Ok(Object::Dictionary(bottom_res)) = out_doc.get_object(bottom_font_ref) {
            if let Ok(font_obj) = bottom_res.get(b"Font") {
                // 如果已经有Font，需要合并
                if resources.get(b"Font").is_err() {
                    resources.set(b"Font", font_obj.clone());
                }
            }
        }
    }
    
    let resources_id = out_doc.add_object(resources);

    // 创建页面
    let mut page_dict = Dictionary::new();
    page_dict.set(b"Type", Object::Name(b"Page".to_vec()));
    page_dict.set(b"MediaBox", Object::Array(vec![
        Object::from(0.0_f64), Object::from(0.0_f64),
        Object::from(a4_w), Object::from(a4_h),
    ]));
    page_dict.set(b"Contents", Object::Reference(content_id));
    page_dict.set(b"Resources", Object::Reference(resources_id));
    let page_id = out_doc.add_object(page_dict);

    // 创建页面树
    let mut pages_dict = Dictionary::new();
    pages_dict.set(b"Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set(b"Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_dict.set(b"Count", Object::Integer(1));
    let pages_id = out_doc.add_object(pages_dict);

    if let Ok(page_obj) = out_doc.get_object_mut(page_id) {
        if let Object::Dictionary(d) = page_obj {
            d.set(b"Parent", Object::Reference(pages_id));
        }
    }

    // 创建目录
    let mut catalog = Dictionary::new();
    catalog.set(b"Type", Object::Name(b"Catalog".to_vec()));
    catalog.set(b"Pages", Object::Reference(pages_id));
    let catalog_id = out_doc.add_object(catalog);

    out_doc.trailer.set(b"Root", Object::Reference(catalog_id));

    // 使用临时文件保存，然后重命名
    let out_dir = out.parent().unwrap_or(std::path::Path::new("."));
    let tmp_path = out_dir.join(format!(".tmp_merge_{}.pdf", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    out_doc.save(&tmp_path).map_err(|e| format!("保存临时PDF失败: {}", e))?;
    // 删除已存在的目标文件
    if out.exists() {
        std::fs::remove_file(out).ok();
    }
    std::fs::rename(&tmp_path, out).map_err(|e| format!("重命名PDF失败: {}", e))?;
    Ok(())
}

fn page_to_form_xobject(
    src: &Document,
    page_id: ObjectId,
    out_doc: &mut Document,
) -> Result<(ObjectId, Option<ObjectId>), String> {
    // 获取页面内容
    let content_data = src.get_page_content(page_id)
        .map_err(|e| format!("获取页面内容失败: {}", e))?;
    
    if content_data.is_empty() {
        return Err("PDF页面内容为空".into());
    }

    // 获取页面尺寸
    let bbox = page_bbox(src, page_id).unwrap_or((595.0, 842.0));

    // 获取页面资源
    let mut res_dict = Dictionary::new();
    let mut font_ref = None;
    
    if let Ok((Some(resources), _)) = src.get_page_resources(page_id) {
        // 复制字体资源
        if let Ok(font_obj) = resources.get(b"Font") {
            match font_obj {
                Object::Reference(r) => {
                    let new_id = out_doc.add_object(src.get_object(*r).map_err(|e| e.to_string())?.clone());
                    let mut new_font_dict = Dictionary::new();
                    // 复制字体字典中的每个字体
                    if let Ok(Object::Dictionary(font_dict)) = src.get_object(*r) {
                        for (key, value) in font_dict.iter() {
                            if let Object::Reference(fr) = value {
                                if let Ok(font_obj) = src.get_object(*fr) {
                                    let new_font_id = out_doc.add_object(font_obj.clone());
                                    new_font_dict.set(key.clone(), Object::Reference(new_font_id));
                                }
                            }
                        }
                    }
                    res_dict.set(b"Font".to_vec(), Object::Dictionary(new_font_dict));
                    font_ref = Some(new_id);
                }
                Object::Dictionary(d) => {
                    let mut new_font_dict = Dictionary::new();
                    for (key, value) in d.iter() {
                        if let Object::Reference(fr) = value {
                            if let Ok(font_obj) = src.get_object(*fr) {
                                let new_font_id = out_doc.add_object(font_obj.clone());
                                new_font_dict.set(key.clone(), Object::Reference(new_font_id));
                            }
                        }
                    }
                    res_dict.set(b"Font".to_vec(), Object::Dictionary(new_font_dict));
                }
                _ => {}
            }
        }
        
        // 复制其他资源（ExtGState等）
        if let Ok(gs_obj) = resources.get(b"ExtGState") {
            res_dict.set(b"ExtGState".to_vec(), gs_obj.clone());
        }
    }

    // 创建Form XObject
    let mut form_dict = Dictionary::new();
    form_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
    form_dict.set(b"Subtype", Object::Name(b"Form".to_vec()));
    form_dict.set(b"FormType", Object::Integer(1));
    form_dict.set(b"BBox", Object::Array(vec![
        Object::from(0.0_f64), Object::from(0.0_f64),
        Object::from(bbox.0), Object::from(bbox.1),
    ]));
    form_dict.set(b"Length", Object::Integer(content_data.len() as i64));
    
    if !res_dict.is_empty() {
        form_dict.set(b"Resources", Object::Dictionary(res_dict));
    }

    let form_stream = Stream::new(form_dict, content_data);
    let form_id = out_doc.add_object(form_stream);
    
    Ok((form_id, font_ref))
}

fn page_bbox(doc: &Document, page_id: ObjectId) -> Option<(f64, f64)> {
    let page_dict = doc.get_dictionary(page_id).ok()?;
    let mbox = page_dict.get(b"MediaBox").ok()?;
    let arr = mbox.as_array().ok()?;
    if arr.len() < 4 { return None; }
    let w = obj_to_f64(&arr[2])? - obj_to_f64(&arr[0])?;
    let h = obj_to_f64(&arr[3])? - obj_to_f64(&arr[1])?;
    Some((w.abs(), h.abs()))
}

fn obj_to_f64(o: &Object) -> Option<f64> {
    match o {
        Object::Real(r) => Some(*r as f64),
        Object::Integer(i) => Some(*i as f64),
        _ => None,
    }
}
