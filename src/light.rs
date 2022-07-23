use crate::camera::Camera;
use crate::vertex::{CuboidCoords, QuadListRenderData, Vertex};
use glam::{Mat4, Vec3};

pub struct LightUniform {
    pub position: Vec3,
    pub color: Vec3,
    pub sun_position: Vec3,
    pub sun_position_camera_adjusted: Vec3,
    pub sun_target: Vec3,
    pub sun_target_camera_adjusted: Vec3,
    pub sunlight_ortho_proj_coords: CuboidCoords,
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
        position: Vec3,
        color: Vec3,
        sun_position: Vec3,
        sunlight_ortho_proj_coords: CuboidCoords,
    ) -> Self {
        let sunlight_ortho_proj = glam::Mat4::orthographic_rh(
            sunlight_ortho_proj_coords.left,
            sunlight_ortho_proj_coords.right,
            sunlight_ortho_proj_coords.bottom,
            sunlight_ortho_proj_coords.top,
            sunlight_ortho_proj_coords.near,
            sunlight_ortho_proj_coords.far,
        );
        Self {
            position,
            color,
            sun_position,
            sun_position_camera_adjusted: sun_position,
            sun_target: [0.0, 0.0, 0.0].into(),
            sun_target_camera_adjusted: [0.0, 0.0, 0.0].into(),
            sunlight_ortho_proj_coords,
            sunlight_ortho_proj,
        }
    }

    fn get_light_view_proj(&self) -> Mat4 {
        glam::Mat4::look_at_rh(
            self.sun_position_camera_adjusted.into(),
            self.sun_target_camera_adjusted.into(),
            [0.0, 1.0, 0.0].into(),
        )
    }

    pub fn to_raw(&self) -> LightUniformRaw {
        let light_space_matrix =
            (self.sunlight_ortho_proj * self.get_light_view_proj()).to_cols_array_2d();

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

    pub fn vertex_data_for_sunlight(&self) -> QuadListRenderData {
        let mut sunlight_vertex_data = QuadListRenderData {
            vertex_data: vec![],
            index_data: vec![],
        };

        Vertex::generate_quad_data_for_cube(
            &self.sunlight_ortho_proj_coords,
            Some(self.get_light_view_proj()),
            &mut sunlight_vertex_data,
        );

        const sunlight_cube_size: f32 = 1.0;
        Vertex::generate_quad_data_for_cube(
            &CuboidCoords {
                left: self.sun_position_camera_adjusted.x - sunlight_cube_size,
                right: self.sun_position_camera_adjusted.x + sunlight_cube_size,
                bottom: self.sun_position_camera_adjusted.y - sunlight_cube_size,
                top: self.sun_position_camera_adjusted.y + sunlight_cube_size,
                near: self.sun_position_camera_adjusted.z - sunlight_cube_size,
                far: self.sun_position_camera_adjusted.z + sunlight_cube_size,
            },
            None,
            &mut sunlight_vertex_data,
        );

        sunlight_vertex_data
    }
}
