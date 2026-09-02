// Wireframe overlay: two positions per edge, colored by edge flags.
#include "common3d.wgsl"

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) pos: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    let flag = edge_flag(vi / 2u);
    out.color = overlay_color(view.overlay[0], flag);
    if (flag & 4u) != 0u {
        out.color.a = 0.0;
    }
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if in.color.a <= 0.0 {
        discard;
    }
    return in.color;
}
