//! Escritor mínimo de PDF: uma página por imagem JPEG (DCTDecode).
use std::io::Write;
use std::path::Path;

pub struct Page {
    pub width_px: u32,
    pub height_px: u32,
    pub jpeg: Vec<u8>,
}

/// Página no tamanho da imagem a 96 dpi (1 px = 0,75 pt).
pub fn write_pdf(pages: &[Page], out: &Path) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let n = pages.len();
    let total_objs = 2 + 3 * n;
    let begin = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, num: usize| {
        offsets.push(buf.len());
        write!(buf, "{num} 0 obj\n").unwrap();
    };

    begin(&mut buf, &mut offsets, 1);
    buf.extend_from_slice(b"<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    begin(&mut buf, &mut offsets, 2);
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + 3 * i)).collect();
    write!(buf, "<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n", kids.join(" "), n).unwrap();

    for (i, p) in pages.iter().enumerate() {
        let page_id = 3 + 3 * i;
        let img_id = page_id + 1;
        let content_id = page_id + 2;
        let w = p.width_px as f64 * 0.75;
        let h = p.height_px as f64 * 0.75;

        begin(&mut buf, &mut offsets, page_id);
        write!(
            buf,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] /Resources << /XObject << /Im0 {img_id} 0 R >> >> /Contents {content_id} 0 R >>\nendobj\n"
        )
        .unwrap();

        begin(&mut buf, &mut offsets, img_id);
        write!(
            buf,
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            p.width_px,
            p.height_px,
            p.jpeg.len()
        )
        .unwrap();
        buf.extend_from_slice(&p.jpeg);
        buf.extend_from_slice(b"\nendstream\nendobj\n");

        let content = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
        begin(&mut buf, &mut offsets, content_id);
        write!(buf, "<< /Length {} >>\nstream\n{}\nendstream\nendobj\n", content.len(), content).unwrap();
    }

    let xref = buf.len();
    write!(buf, "xref\n0 {}\n0000000000 65535 f \n", total_objs + 1).unwrap();
    for off in &offsets {
        write!(buf, "{off:010} 00000 n \n").unwrap();
    }
    write!(buf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", total_objs + 1, xref).unwrap();
    std::fs::write(out, buf)
}
