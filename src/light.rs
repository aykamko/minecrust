pub struct LightUniform {
    pub position: glam::Vec3,
    pub color: glam::Vec3,
    pub sun_position: glam::Vec3,
    pub sunlight_ortho_proj: glam::Mat4,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniformRaw {
    position: [f32; 3],
    _padding: u32,
    color: [f32; 3],
    _padding2: u32,
    light_space_matrix: [[f32; 4]; 4],
}

impl LightUniform {
    pub fn to_raw(&self) -> LightUniformRaw {
        let light_view = glam::Mat4::look_at_rh(
            self.sun_position.into(),
            [0.0, 0.0, 0.0].into(), /* where light is pointing */
            [0.0, 1.0, 0.0].into(),
        );

        let light_space_matrix = (self.sunlight_ortho_proj * light_view).to_cols_array_2d();

        LightUniformRaw {
            position: self.position.into(),
            _padding: 0,
            color: self.color.into(),
            _padding2: 0,
            light_space_matrix,
        }
    }
}