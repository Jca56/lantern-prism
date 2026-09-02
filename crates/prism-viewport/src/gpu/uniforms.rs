//! Uniform layouts. Must match `shaders/common3d.wgsl`.

use prism_core::impl_pod;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ViewUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub inv_proj: [[f32; 4]; 4],
    pub view_rot: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub viewport: [f32; 4],
    pub bg: [f32; 4],
    pub grid: [f32; 4],
    pub grid_colors: [[f32; 4]; 4],
    pub overlay: [[f32; 4]; 4],
    pub point: [f32; 4],
}
impl_pod!(ViewUniforms);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ObjectUniforms {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
    pub color: [f32; 4],
    pub flags: [u32; 4],
}
impl_pod!(ObjectUniforms);

/// Dynamic-offset strides (multiples of 256).
pub const VIEW_STRIDE: u64 = 512;
pub const OBJECT_STRIDE: u64 = 256;

pub const FLAG_SELECTED: u8 = 1;
pub const FLAG_ACTIVE: u8 = 2;
pub const FLAG_HIDDEN: u8 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_fit_their_strides() {
        assert!(size_of::<ViewUniforms>() as u64 <= VIEW_STRIDE);
        assert!(size_of::<ObjectUniforms>() as u64 <= OBJECT_STRIDE);
        assert_eq!(size_of::<ViewUniforms>() % 16, 0);
        assert_eq!(size_of::<ObjectUniforms>() % 16, 0);
    }
}
