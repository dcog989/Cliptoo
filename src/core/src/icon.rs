use anyhow::Result;

/// Rasterize an SVG to RGBA pixels at the given size.
pub fn rasterize_svg(svg_data: &[u8], size: u32) -> Result<(Vec<u8>, u32, u32)> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg_data, &opt)?;

    let pixmap_size = tree.size();
    let scale = size as f32 / pixmap_size.width().max(pixmap_size.height());
    let w = (pixmap_size.width() * scale).ceil() as u32;
    let h = (pixmap_size.height() * scale).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap"))?;
    resvg::render(
        &tree,
        resvg::usvg::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.data().to_vec(), w, h))
}
