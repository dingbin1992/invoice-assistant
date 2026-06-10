use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

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
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    let mut out_files = Vec::new();
    let mut idx = 0usize;
    let len = input_files.len();
    while idx < len {
        let end = (idx + 2).min(len);
        let group = &input_files[idx..end];
        let out_path = std::path::Path::new(&output_dir)
            .join(format!("{}_{:03}.pdf", file_prefix, out_files.len() + 1));
        if group.len() == 1 {
            std::fs::copy(&group[0], &out_path).map_err(|e| e.to_string())?;
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
    out_doc.compress();

    let a4_w_pt: f64 = 595.0;
    let a4_h_pt: f64 = 842.0;
    let half_h_pt: f64 = a4_h_pt / 2.0;

    let top_bytes = std::fs::read(top_pdf).map_err(|e| e.to_string())?;
    let bottom_bytes = std::fs::read(bottom_pdf).map_err(|e| e.to_string())?;

    let (top_form_id, top_box) = register_as_form(&mut out_doc, &top_bytes)?;
    let (bottom_form_id, bottom_box) = register_as_form(&mut out_doc, &bottom_bytes)?;

    let (top_cm, _) = compute_matrix(top_box, a4_w_pt, half_h_pt, true);
    let (bottom_cm, _) = compute_matrix(bottom_box, a4_w_pt, half_h_pt, false);

    let content = format!("q {} cm /Fm1 Do Q q {} cm /Fm2 Do Q", top_cm, bottom_cm);
    let content_stream = lopdf::Stream::new(Dictionary::new(), content.as_bytes().to_vec());
    let content_id = out_doc.add_object(content_stream);

    let mut xobjects = Dictionary::new();
    xobjects.set(b"Fm1", Object::Reference(top_form_id));
    xobjects.set(b"Fm2", Object::Reference(bottom_form_id));
    let mut resources = Dictionary::new();
    resources.set(b"XObject", Object::Dictionary(xobjects));
    let resources_id = out_doc.add_object(resources);

    let mut page_dict = Dictionary::new();
    page_dict.set(b"Type", Object::Name(b"Page".to_vec()));
    page_dict.set(b"MediaBox", Object::Array(vec![
        Object::from(0.0_f64), Object::from(0.0_f64),
        Object::from(a4_w_pt), Object::from(a4_h_pt),
    ]));
    page_dict.set(b"Contents", Object::Reference(content_id));
    page_dict.set(b"Resources", Object::Reference(resources_id));
    let page_id = out_doc.add_object(page_dict);

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

    let mut catalog = Dictionary::new();
    catalog.set(b"Type", Object::Name(b"Catalog".to_vec()));
    catalog.set(b"Pages", Object::Reference(pages_id));
    let catalog_id = out_doc.add_object(catalog);

    out_doc.trailer.set(b"Root", Object::Reference(catalog_id));

    out_doc.save(out).map_err(|e| e.to_string())?;
    Ok(())
}

fn register_as_form(out_doc: &mut Document, pdf_bytes: &[u8]) -> Result<(ObjectId, (f64, f64)), String> {
    let src = Document::load_mem(pdf_bytes).map_err(|e| format!("PDF 解析失败: {}", e))?;
    let pages = src.get_pages();
    if pages.is_empty() {
        return Err("源PDF无页面".into());
    }
    let (src_page_id, _) = pages.into_iter().next().unwrap();

    let bbox = page_bbox(&src, src_page_id).unwrap_or((595.0, 842.0));

    let src_page_dict = src.get_dictionary((src_page_id, 0u16)).map_err(|e| e.to_string())?;
    let content_data = collect_page_contents(&src, src_page_dict)?;

    let mut res_dict = Dictionary::new();
    if let Ok(resources_obj) = src_page_dict.get(b"Resources") {
        let res = match resources_obj {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => src.get_dictionary(*r).ok().cloned(),
            _ => None,
        };
        if let Some(res) = res {
            copy_resource_dict(&src, &res, out_doc, &mut res_dict)?;
        }
    }

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

    let form_stream = lopdf::Stream::new(form_dict, content_data);
    let form_id = out_doc.add_object(form_stream);
    Ok((form_id, bbox))
}

fn copy_resource_dict(
    src: &Document,
    src_res: &Dictionary,
    out_doc: &mut Document,
    out_res: &mut Dictionary,
) -> Result<(), String> {
    for (key, value) in src_res.iter() {
        match value {
            Object::Reference(r) => {
                if let Ok(obj) = src.get_object(*r) {
                    let new_id = out_doc.add_object(obj.clone());
                    out_res.set(key.clone(), Object::Reference(new_id));
                }
            }
            Object::Dictionary(d) => {
                let mut new_sub = Dictionary::new();
                for (k, v) in d.iter() {
                    if let Object::Reference(r) = v {
                        if let Ok(obj) = src.get_object(*r) {
                            let nid = out_doc.add_object(obj.clone());
                            new_sub.set(k.clone(), Object::Reference(nid));
                        }
                    } else {
                        new_sub.set(k.clone(), v.clone());
                    }
                }
                out_res.set(key.clone(), Object::Dictionary(new_sub));
            }
            _ => {
                out_res.set(key.clone(), value.clone());
            }
        }
    }
    Ok(())
}

fn collect_page_contents(src: &Document, page_dict: &Dictionary) -> Result<Vec<u8>, String> {
    let contents = page_dict.get(b"Contents").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    match contents {
        Object::Reference(r) => {
            if let Ok(Object::Stream(st)) = src.get_object(*r) {
                out.extend_from_slice(&st.content);
            }
        }
        Object::Array(arr) => {
            for item in arr {
                if let Object::Reference(r) = item {
                    if let Ok(Object::Stream(st)) = src.get_object(*r) {
                        out.extend_from_slice(&st.content);
                        out.push(b'\n');
                    }
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

fn page_bbox(doc: &Document, page_id: u32) -> Option<(f64, f64)> {
    let page_dict = doc.get_dictionary((page_id, 0u16)).ok()?;
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

fn compute_matrix(src_box: (f64, f64), page_w: f64, half_h: f64, is_top: bool) -> (String, f64) {
    let (sw, sh) = src_box;
    let margin_pt = 6.0;
    let avail_w = page_w - margin_pt * 2.0;
    let avail_h = half_h - margin_pt * 2.0;
    let scale = (avail_w / sw).min(avail_h / sh);
    let new_w = sw * scale;
    let new_h = sh * scale;
    let tx = (page_w - new_w) / 2.0;
    let ty = if is_top {
        half_h + (half_h - new_h) / 2.0
    } else {
        (half_h - new_h) / 2.0
    };
    let cm = format!("{:.4} 0 0 {:.4} {:.4} {:.4}", scale, scale, tx, ty);
    (cm, scale)
}
