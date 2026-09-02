// ID pass: writes element or object ids into an R32Uint target. Zero means
// nothing. Element ids carry their kind in the top two bits (1 face, 2 edge,
// 3 vertex); object ids use the low bits only. Faces, then edges, then
// vertices are drawn with a depth bias so the finer elements win.
#include "common3d.wgsl"

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) id: u32,
};

@vertex
fn vs_faces(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) face: u32) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    if obj.flags.w == 0u {
        out.id = obj.flags.y;
    } else {
        out.id = (face + 1u) | (1u << 30u);
        if (face_flag(face) & 4u) != 0u {
            out.id = 0u;
        }
    }
    return out;
}

@vertex
fn vs_lines(@builtin(vertex_index) vi: u32, @location(0) pos: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    let e = vi / 2u;
    out.id = (e + 1u) | (2u << 30u);
    if (edge_flag(e) & 4u) != 0u {
        out.id = 0u;
    }
    return out;
}

@vertex
fn vs_points(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32, @location(0) pos: vec3<f32>) -> VsOut {
    var out: VsOut;
    var clip = view.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    var size = view.point.x * 1.5;
    if (vert_flag(ii) & 4u) != 0u {
        size = 0.0;
    }
    var corner = vec2<f32>(0.0, 0.0);
    switch vi {
        case 1u: { corner = vec2<f32>(1.0, 0.0); }
        case 2u, 4u: { corner = vec2<f32>(1.0, 1.0); }
        case 5u: { corner = vec2<f32>(0.0, 1.0); }
        default: { corner = vec2<f32>(0.0, 0.0); }
    }
    let offset = (corner - 0.5) * size * 2.0 / view.viewport.zw;
    out.clip = vec4<f32>(clip.xy + offset * clip.w, clip.zw);
    out.id = (ii + 1u) | (3u << 30u);
    return out;
}

@fragment
fn fs_pick(in: VsOut) -> @location(0) u32 {
    if in.id == 0u {
        discard;
    }
    return in.id;
}
