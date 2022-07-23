use crate::camera::Camera;

pub struct LightUniform {
    pub position: glam::Vec3,
    pub color: glam::Vec3,
    pub sun_position: glam::Vec3,
    pub sun_position_camera_adjusted: glam::Vec3,
    pub sun_target: glam::Vec3,
    pub sun_target_camera_adjusted: glam::Vec3,
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
    pub fn new(
        position: glam::Vec3,
        color: glam::Vec3,
        sun_position: glam::Vec3,
        sunlight_ortho_proj: glam::Mat4,
    ) -> Self {
        Self {
            position,
            color,
            sun_position,
            sun_position_camera_adjusted: sun_position,
            sun_target: [0.0, 0.0, 0.0].into(),
            sun_target_camera_adjusted: [0.0, 0.0, 0.0].into(),
            sunlight_ortho_proj,
        }
    }

    pub fn to_raw(&self) -> LightUniformRaw {
        let light_view = glam::Mat4::look_at_rh(
            self.sun_position_camera_adjusted.into(),
            self.sun_target_camera_adjusted.into(),
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

    pub fn update_light_space_proj(&mut self, camera: &Camera) {
        let sun_y_adjust = camera.initial_eye.y - camera.eye.y;

        self.sun_position_camera_adjusted = self.sun_position;
        self.sun_target_camera_adjusted = self.sun_target;
        self.sun_position_camera_adjusted.y += sun_y_adjust;
        self.sun_target_camera_adjusted.y += sun_y_adjust;
    }
}
