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
    pub shadow_map_pixel_size: [u32; 2],
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
        shadow_map_pixel_size: [u32; 2],
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
            shadow_map_pixel_size,
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
        // let sun_adjust = Vec3::new(
        //     f32::trunc(camera.eye.x) - camera.eye.x,
        //     camera.initial_eye.y - camera.eye.y,
        //     f32::trunc(camera.eye.z) - camera.eye.z,
        // );
        let sun_adjust = Vec3::new(0.0, camera.initial_eye.y - camera.eye.y, 0.0);

        self.sun_position_camera_adjusted = self.sun_position + sun_adjust;
        self.sun_target_camera_adjusted = self.sun_target + sun_adjust;
    }

    pub fn vertex_data_for_sunlight(&self) -> QuadListRenderData {
        let mut sunlight_vertex_data = QuadListRenderData {
            vertex_data: vec![],
            index_data: vec![],
        };

        // HACK: using inverse() and modifying the projection coords is jank, but it matches up
        // with what I see on screen...
        let mut ortho_coords = self.sunlight_ortho_proj_coords.clone();
        (ortho_coords.near, ortho_coords.far) = (-ortho_coords.far, ortho_coords.near);
        Vertex::generate_quad_data_for_cuboid(
            &ortho_coords,
            Some(self.get_light_view_proj().inverse()),
            &mut sunlight_vertex_data,
        );

        const SUNLIGHT_CUBE_SIZE: f32 = 1.0;
        Vertex::generate_quad_data_for_cuboid(
            &CuboidCoords {
                left: self.sun_position_camera_adjusted.x - SUNLIGHT_CUBE_SIZE,
                right: self.sun_position_camera_adjusted.x + SUNLIGHT_CUBE_SIZE,
                bottom: self.sun_position_camera_adjusted.y - SUNLIGHT_CUBE_SIZE,
                top: self.sun_position_camera_adjusted.y + SUNLIGHT_CUBE_SIZE,
                near: self.sun_position_camera_adjusted.z - SUNLIGHT_CUBE_SIZE,
                far: self.sun_position_camera_adjusted.z + SUNLIGHT_CUBE_SIZE,
            },
            None,
            &mut sunlight_vertex_data,
        );

        Vertex::generate_quad_data_for_cuboid(
            &CuboidCoords {
                left: self.sun_target_camera_adjusted.x - SUNLIGHT_CUBE_SIZE,
                right: self.sun_target_camera_adjusted.x + SUNLIGHT_CUBE_SIZE,
                bottom: self.sun_target_camera_adjusted.y - SUNLIGHT_CUBE_SIZE,
                top: self.sun_target_camera_adjusted.y + SUNLIGHT_CUBE_SIZE,
                near: self.sun_target_camera_adjusted.z - SUNLIGHT_CUBE_SIZE,
                far: self.sun_target_camera_adjusted.z + SUNLIGHT_CUBE_SIZE,
            },
            None,
            &mut sunlight_vertex_data,
        );

        sunlight_vertex_data
    }
}
