// Shared by every 3D viewport shader. Camera-relative: object matrices
// already include translate(-camera), so `view_proj` carries no translation
// and the GPU never sees a large number (D004). Reverse-Z (D004).

struct ViewUniforms {
    view_proj: mat4x4<f32>,   // rotation-only view × projection
    inv_proj: mat4x4<f32>,    // clip → view space (for the grid rays)
    view_rot: mat4x4<f32>,    // world → view rotation (3×3 in a 4×4)
    cam_pos: vec4<f32>,       // world camera position, f32 (grid only)
    viewport: vec4<f32>,      // x, y, width, height in pixels
    bg: vec4<f32>,            // viewport background, linear
    grid: vec4<f32>,          // minor spacing, major spacing, fade distance, ortho flag
    grid_colors: array<vec4<f32>, 4>, // minor, major, x axis, z axis
    overlay: array<vec4<f32>, 4>,     // wire, vertex dot, selected, active
    point: vec4<f32>,         // x = vertex dot size in pixels
};

struct ObjectUniforms {
    model: mat4x4<f32>,       // translate(-camera) × world
    normal: mat4x4<f32>,      // inverse-transpose of world (3×3 in a 4×4)
    color: vec4<f32>,         // base color, linear
    flags: vec4<u32>,         // x: 1 selected | 2 active; y: object id; z: edit mode; w: pick mode (0 object, 1 element)
};

@group(0) @binding(0) var<uniform> view: ViewUniforms;
@group(1) @binding(0) var<uniform> obj: ObjectUniforms;
// Per-element flags, one byte each, packed four per word: 1 selected, 2 active, 4 hidden.
@group(2) @binding(0) var<storage, read> face_flags: array<u32>;
@group(2) @binding(1) var<storage, read> vert_flags: array<u32>;
@group(2) @binding(2) var<storage, read> edge_flags: array<u32>;

fn face_flag(i: u32) -> u32 {
    return (face_flags[i / 4u] >> ((i % 4u) * 8u)) & 0xffu;
}
fn vert_flag(i: u32) -> u32 {
    return (vert_flags[i / 4u] >> ((i % 4u) * 8u)) & 0xffu;
}
fn edge_flag(i: u32) -> u32 {
    return (edge_flags[i / 4u] >> ((i % 4u) * 8u)) & 0xffu;
}

// Color of an overlay element from its flags.
fn overlay_color(base: vec4<f32>, flag: u32) -> vec4<f32> {
    if (flag & 2u) != 0u {
        return view.overlay[3];
    }
    if (flag & 1u) != 0u {
        return view.overlay[2];
    }
    return base;
}
