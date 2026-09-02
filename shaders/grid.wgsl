// Infinite XZ grid and the viewport background, from one fullscreen
// triangle. Rays are reconstructed per pixel; the plane hit writes real depth
// so geometry occludes the grid correctly.
#include "common3d.wgsl"

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.5, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

struct FsOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

fn line_coverage(coord: vec2<f32>, spacing: f32) -> f32 {
    let c = coord / spacing;
    let d = abs(fract(c - 0.5) - 0.5) / fwidth(c);
    return 1.0 - min(min(d.x, d.y), 1.0);
}

@fragment
fn fs_main(in: VsOut) -> FsOut {
    var out: FsOut;
    out.color = view.bg;
    out.depth = 0.0;

    // Two points along the pixel's ray in view space (reverse-Z: 1 = near).
    let n4 = view.inv_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let f4 = view.inv_proj * vec4<f32>(in.ndc, 0.5, 1.0);
    let near_v = n4.xyz / n4.w;
    let far_v = f4.xyz / f4.w;
    let rot = mat3x3<f32>(view.view_rot[0].xyz, view.view_rot[1].xyz, view.view_rot[2].xyz);
    let inv_rot = transpose(rot);
    let dir = normalize(inv_rot * (far_v - near_v));
    let origin_rel = inv_rot * near_v;               // relative to the camera
    let origin = view.cam_pos.xyz + origin_rel;
    if abs(dir.y) < 1e-6 {
        return out;
    }
    let t = -origin.y / dir.y;
    if t <= 0.0 {
        return out;
    }
    let hit_rel = origin_rel + dir * t;               // camera-relative hit
    let hit = origin + dir * t;

    let minor = line_coverage(hit.xz, view.grid.x);
    let major = line_coverage(hit.xz, view.grid.y);
    let fw = fwidth(hit.xz) * 1.5;
    let ax = 1.0 - min(abs(hit.z) / max(fw.y, 1e-6), 1.0); // x axis runs along z = 0
    let az = 1.0 - min(abs(hit.x) / max(fw.x, 1e-6), 1.0); // z axis runs along x = 0

    var color = view.grid_colors[0];
    var alpha = minor * view.grid_colors[0].a;
    if major > 0.0 {
        color = view.grid_colors[1];
        alpha = max(alpha, major * view.grid_colors[1].a);
    }
    if ax > 0.0 {
        color = view.grid_colors[2];
        alpha = max(alpha, ax);
    }
    if az > 0.0 {
        color = view.grid_colors[3];
        alpha = max(alpha, az);
    }
    // Fade with distance and grazing angle.
    let fade = clamp(1.0 - t / view.grid.z, 0.0, 1.0);
    let grazing = clamp(abs(dir.y) * 4.0, 0.0, 1.0);
    alpha = alpha * fade * grazing;

    let clip = view.view_proj * vec4<f32>(hit_rel, 1.0);
    out.depth = clip.z / clip.w;
    out.color = vec4<f32>(mix(view.bg.rgb, color.rgb, alpha), 1.0);
    return out;
}
