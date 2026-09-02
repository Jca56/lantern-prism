// Solid shading: three fixed studio lights in view space, two-sided, with
// selection tints for objects (object mode) and faces (edit mode).
#include "common3d.wgsl"

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) face: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) n_view: vec3<f32>,
    @location(1) @interpolate(flat) face: u32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let p = obj.model * vec4<f32>(in.pos, 1.0);
    out.clip = view.view_proj * p;
    let n_world = (obj.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.n_view = (view.view_rot * vec4<f32>(n_world, 0.0)).xyz;
    out.face = in.face;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var n = normalize(in.n_view);
    if n.z < 0.0 {
        n = -n; // back faces shade like front faces
    }
    var base = obj.color.rgb;
    let oflags = obj.flags.x;
    if obj.flags.z != 0u {
        let ff = face_flag(in.face);
        if (ff & 4u) != 0u {
            discard;
        }
        if (ff & 1u) != 0u {
            base = mix(base, view.overlay[2].rgb, 0.45);
        }
        if (ff & 2u) != 0u {
            base = mix(base, view.overlay[3].rgb, 0.35);
        }
    } else {
        if (oflags & 1u) != 0u {
            base = mix(base, view.overlay[2].rgb, 0.25);
        }
        if (oflags & 2u) != 0u {
            base = mix(base, view.overlay[3].rgb, 0.15);
        }
    }
    let key = normalize(vec3<f32>(-0.35, 0.6, 0.72));
    let fill = normalize(vec3<f32>(0.7, 0.1, 0.6));
    let rim = normalize(vec3<f32>(0.1, 0.9, -0.3));
    let diff = 0.30 + 0.55 * max(dot(n, key), 0.0) + 0.22 * max(dot(n, fill), 0.0) + 0.12 * max(dot(n, rim), 0.0);
    let spec = pow(max(dot(reflect(-key, n), vec3<f32>(0.0, 0.0, 1.0)), 0.0), 24.0) * 0.16;
    return vec4<f32>(base * diff + vec3<f32>(spec), 1.0);
}
