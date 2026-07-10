#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
PDF发票合并器
将多个PDF发票合并为一个PDF，每页显示2个发票
使用PyMuPDF (fitz)渲染为图片再合成，确保印章等元素完整保留
"""

import os
import sys
import json
import fitz  # PyMuPDF
from PIL import Image
import io


def pdf_page_to_image(pdf_path, page_num=0, dpi=300):
    """将PDF页面渲染为图片"""
    doc = fitz.open(pdf_path)
    if page_num >= len(doc):
        doc.close()
        return None

    page = doc[page_num]
    # 渲染为 pixmap
    mat = fitz.Matrix(dpi / 72, dpi / 72)  # 缩放矩阵
    pix = page.get_pixmap(matrix=mat)

    # 转换为 PIL Image
    img = Image.frombytes("RGB", [pix.width, pix.height], pix.samples)

    doc.close()
    return img


def merge_pdfs(input_files, output_dir, file_prefix):
    """将所有PDF合并为一个PDF文件，每页显示2个发票"""
    if not input_files:
        raise RuntimeError("没有需要合并的PDF")

    # 确保输出目录存在
    os.makedirs(output_dir, exist_ok=True)

    # A4尺寸（像素，300 DPI）
    DPI = 300
    A4_W_MM = 210.0
    A4_H_MM = 297.0
    px_to_mm = 25.4 / DPI
    A4_W_PX = int(A4_W_MM / px_to_mm)
    A4_H_PX = int(A4_H_MM / px_to_mm)
    MARGIN_PX = int(5 / px_to_mm)  # 5mm边距
    HALF_H_PX = A4_H_PX // 2

    output_files = []
    idx = 0
    page_count = 0

    try:
        while idx < len(input_files):
            # 创建A4白色背景
            a4_img = Image.new('RGB', (A4_W_PX, A4_H_PX), (255, 255, 255))

            # 处理第一个发票（上半部分）
            if idx < len(input_files):
                pdf_path = input_files[idx]
                if os.path.exists(pdf_path):
                    try:
                        invoice_img = pdf_page_to_image(pdf_path, dpi=DPI)
                        if invoice_img:
                            # 计算可用空间
                            available_w = A4_W_PX - MARGIN_PX * 2
                            available_h = HALF_H_PX - MARGIN_PX * 2

                            # 计算缩放比例（保持宽高比）
                            img_w, img_h = invoice_img.size
                            scale_w = available_w / img_w
                            scale_h = available_h / img_h
                            scale = min(scale_w, scale_h)

                            # 缩放图片
                            new_w = int(img_w * scale)
                            new_h = int(img_h * scale)
                            resized = invoice_img.resize((new_w, new_h), Image.LANCZOS)

                            # 居中放置（上半部分）
                            x = (A4_W_PX - new_w) // 2
                            y = (HALF_H_PX - new_h) // 2

                            a4_img.paste(resized, (x, y))
                            print(f"已添加(上): {os.path.basename(pdf_path)}")
                    except Exception as e:
                        print(f"添加失败 {pdf_path}: {str(e)}")
                idx += 1

            # 处理第二个发票（下半部分）
            if idx < len(input_files):
                pdf_path = input_files[idx]
                if os.path.exists(pdf_path):
                    try:
                        invoice_img = pdf_page_to_image(pdf_path, dpi=DPI)
                        if invoice_img:
                            # 计算可用空间
                            available_w = A4_W_PX - MARGIN_PX * 2
                            available_h = HALF_H_PX - MARGIN_PX * 2

                            # 计算缩放比例（保持宽高比）
                            img_w, img_h = invoice_img.size
                            scale_w = available_w / img_w
                            scale_h = available_h / img_h
                            scale = min(scale_w, scale_h)

                            # 缩放图片
                            new_w = int(img_w * scale)
                            new_h = int(img_h * scale)
                            resized = invoice_img.resize((new_w, new_h), Image.LANCZOS)

                            # 居中放置（下半部分）
                            x = (A4_W_PX - new_w) // 2
                            y = HALF_H_PX + (HALF_H_PX - new_h) // 2

                            a4_img.paste(resized, (x, y))
                            print(f"已添加(下): {os.path.basename(pdf_path)}")
                    except Exception as e:
                        print(f"添加失败 {pdf_path}: {str(e)}")
                idx += 1

            # 绘制中间分割线（黑色虚线，便于切割）
            from PIL import ImageDraw
            draw = ImageDraw.Draw(a4_img)
            line_width = 4  # 加粗线条
            # 先绘制白色背景，确保分割线可见
            draw.rectangle(
                [(MARGIN_PX, HALF_H_PX - line_width - 2), (A4_W_PX - MARGIN_PX, HALF_H_PX + line_width + 2)],
                fill=(255, 255, 255)
            )
            # 绘制虚线
            dash_length = 15
            gap_length = 8
            x = MARGIN_PX
            while x < A4_W_PX - MARGIN_PX:
                x_end = min(x + dash_length, A4_W_PX - MARGIN_PX)
                draw.line([(x, HALF_H_PX), (x_end, HALF_H_PX)], fill=(0, 0, 0), width=line_width)
                x += dash_length + gap_length

            # 保存页面为PNG
            page_count += 1
            page_path = os.path.join(output_dir, f".tmp_page_{page_count}.png")
            a4_img.save(page_path, "PNG")
            output_files.append(page_path)

        if not output_files:
            raise RuntimeError("没有成功添加任何PDF文件")

        # 将所有页面合并为PDF
        pdf_path = os.path.join(output_dir, f"{file_prefix}.pdf")
        
        # 使用 Pillow 将所有 PNG 合并为 PDF
        first_page = Image.open(output_files[0]).convert('RGB')
        other_pages = []
        for f in output_files[1:]:
            other_pages.append(Image.open(f).convert('RGB'))

        first_page.save(
            pdf_path,
            "PDF",
            save_all=True,
            append_images=other_pages,
            resolution=DPI
        )

        print(f"\n已生成PDF文件: {pdf_path}")
        print(f"合并了 {len(input_files)} 个文件，共 {page_count} 页")

        return [pdf_path]

    finally:
        # 清理临时文件
        for f in output_files:
            if os.path.exists(f):
                os.remove(f)


def main():
    if len(sys.argv) < 4:
        print("用法: python merge_pdfs.py <input_json> <output_dir> <file_prefix>")
        return

    input_json_path = sys.argv[1]
    output_dir = sys.argv[2]
    file_prefix = sys.argv[3]

    # 读取输入文件列表
    with open(input_json_path, 'r', encoding='utf-8') as f:
        input_files = json.load(f)

    # 合并PDF
    output_files = merge_pdfs(input_files, output_dir, file_prefix)

    print(f"\n生成完成:")
    for f in output_files:
        print(f"  {f}")


if __name__ == "__main__":
    main()
