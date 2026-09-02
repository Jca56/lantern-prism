// Vertex dots: one screen-space quad per vertex, instanced over positions.
#include "common3d.wgsl"

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32, @location(0) pos: vec3<f32>) -> VsOut {
    var out: VsOut;
    var clip = view.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    let flag = vert_flag(ii);
    var size = view.point.x;
    if (flag & 4u) != 0u {
        size = 0.0;
    }
    if (flag & 3u) != 0u {
        size = size * 1.3;
    }
    // Corners of a quad from the vertex index: (0,0) (1,0) (1,1) (0,0) (1,1) (0,1).
    var corner = vec2<f32>(0.0, 0.0);
    switch vi {
        case 1u: { corner = vec2<f32>(1.0, 0.0); }
        case 2u, 4u: { corner = vec2<f32>(1.0, 1.0); }
        case 5u: { corner = vec2<f32>(0.0, 1.0); }
        default: { corner = vec2<f32>(0.0, 0.0); }
    }
    let offset = (corner - 0.5) * size * 2.0 / view.viewport.zw;
    clip = vec4<f32>(clip.xy + offset * clip.w, clip.zw);
    out.clip = clip;
    out.uv = corner;
    out.color = overlay_color(view.overlay[1], flag);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Round dot.
    let d = length(in.uv - 0.5) * 2.0;
    if d > 1.0 {
        discard;
    }
    return in.color;
}
