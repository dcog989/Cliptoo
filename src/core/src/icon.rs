use anyhow::Result;

/// Rasterize an SVG to RGBA pixels at the given size.
pub fn rasterize_svg(svg_data: &[u8], size: u32) -> Result<(Vec<u8>, u32, u32)> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg_data, &opt)?;

    let pixmap_size = tree.size();
    // A malformed SVG whose root declares no width/height parses to a 0×0
    // tree; scaling by max(0, 0) divides by zero and the pixmap collapses to
    // 0×0, so Pixmap::new fails and every retry re-parses the SVG. Render such
    // trees at the requested size so the output (possibly blank) is still
    // cacheable and the refetch loop stops.
    let (scale, w, h) = if pixmap_size.width() > 0.0 && pixmap_size.height() > 0.0 {
        let scale = size as f32 / pixmap_size.width().max(pixmap_size.height());
        (
            scale,
            (pixmap_size.width() * scale).ceil().max(1.0) as u32,
            (pixmap_size.height() * scale).ceil().max(1.0) as u32,
        )
    } else {
        (1.0, size.max(1), size.max(1))
    };
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap"))?;
    resvg::render(
        &tree,
        resvg::usvg::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.data().to_vec(), w, h))
}
