//! A render pass that only clears.

use prism_math::Color;

/// Clear `color` (and `depth` to `0.0`, the reverse-Z far plane) in one
/// empty pass.
pub fn clear_pass(encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView, depth: Option<&wgpu::TextureView>, clear: Color) {
    let l = clear.to_linear();
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("prism clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r: l.r, g: l.g, b: l.b, a: l.a }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: depth.map(|d| wgpu::RenderPassDepthStencilAttachment {
            view: d,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
}
