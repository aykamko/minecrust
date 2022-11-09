use std::f32::consts::PI;

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
    pub texel_size: [f32; 2],
    pub sun_planar_rotation: glam::Mat3,
    pub sun_planar_inverse_rotation: glam::Mat3,
    pub sun_xz_grid_cell_size: [f32; 2],
    pub sun_xz_grid_pos_remainder: [f32; 2],
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

        let texel_size: [f32; 2] = [
            (sunlight_ortho_proj_coords.right - sunlight_ortho_proj_coords.left)
                / shadow_map_pixel_size[0] as f32,
            (sunlight_ortho_proj_coords.top - sunlight_ortho_proj_coords.bottom)
                / shadow_map_pixel_size[1] as f32,
        ];

        let sun_y_angle_rad = (sun_position.y / sun_position.x).atan();
        let behind_sun_y_angle_rad = ((PI / 2.0) - sun_y_angle_rad).cos();

        let sun_z_angle_rad = (sun_position.z / sun_position.x).atan();
        let sun_xz_grid_cell_size = [
            (behind_sun_y_angle_rad.cos() * texel_size[1]) * 2.0,
            texel_size[0],
        ];

        let sun_planar_rotation = glam::Mat3::from_rotation_y(-sun_z_angle_rad);
        let sun_planar_inverse_rotation = glam::Mat3::from_rotation_y(sun_z_angle_rad);

        let sun_pos_planar_normalized = sun_planar_rotation * sun_position;

        let sun_xz_grid_pos_remainder = [
            //sun_pos_planar_normalized.x
            //    - (texel_size[0] * (sun_pos_planar_normalized.x / texel_size[0]).floor()),
            //sun_pos_planar_normalized.z
            //    - (texel_size[1] * (sun_pos_planar_normalized.z / texel_size[1]).floor()),
                0.0,0.0
        ];

        println!("Remainder is {:?}", sun_xz_grid_pos_remainder);

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
            texel_size,
            sun_planar_rotation,
            sun_planar_inverse_rotation,
            sun_xz_grid_cell_size,
            sun_xz_grid_pos_remainder,
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
        let sun_adjust = camera.initial_eye - camera.eye;

        let pos_adjusted_before_texel_normalization =
            self.sun_position + Vec3::new(sun_adjust.x, sun_adjust.y, sun_adjust.z);
        let pos_rotated = self.sun_planar_rotation * pos_adjusted_before_texel_normalization;
        let pos_rotated_texel_adjusted = Vec3::new(
            (self.sun_xz_grid_cell_size[0] * (pos_rotated.x / self.sun_xz_grid_cell_size[0]).floor()) + self.sun_xz_grid_pos_remainder[0],
            pos_rotated.y,
            (self.sun_xz_grid_cell_size[1] * (pos_rotated.z / self.sun_xz_grid_cell_size[1]).floor()) + self.sun_xz_grid_pos_remainder[1],
        );
        let texel_adjusted_sun_pos = self.sun_planar_inverse_rotation * pos_rotated_texel_adjusted;

        let target_adjusted_before_texel_normalization =
            self.sun_target + Vec3::new(sun_adjust.x, sun_adjust.y, sun_adjust.z);
        let target_rotated = self.sun_planar_rotation * target_adjusted_before_texel_normalization;
        let target_rotated_texel_adjusted = Vec3::new(
            (self.sun_xz_grid_cell_size[0] * (target_rotated.x / self.sun_xz_grid_cell_size[0]).floor()) + self.sun_xz_grid_pos_remainder[0],
            target_rotated.y,
            (self.sun_xz_grid_cell_size[1] * (target_rotated.z / self.sun_xz_grid_cell_size[1]).floor()) + self.sun_xz_grid_pos_remainder[1],
        );
        let texel_adjusted_sun_target = self.sun_planar_inverse_rotation * target_rotated_texel_adjusted;

        self.sun_position_camera_adjusted = texel_adjusted_sun_pos;
        self.sun_target_camera_adjusted = texel_adjusted_sun_target;
        println!("Sun position: {:?}, sun target: {:?}", self.sun_position_camera_adjusted, self.sun_target_camera_adjusted);
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
        Vertex::generate_quad_data_for_cube(
            &ortho_coords,
            Some(self.get_light_view_proj().inverse()),
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

        Vertex::generate_quad_data_for_cube(
            &CuboidCoords {
                left: self.sun_target_camera_adjusted.x - sunlight_cube_size,
                right: self.sun_target_camera_adjusted.x + sunlight_cube_size,
                bottom: self.sun_target_camera_adjusted.y - sunlight_cube_size,
                top: self.sun_target_camera_adjusted.y + sunlight_cube_size,
                near: self.sun_target_camera_adjusted.z - sunlight_cube_size,
                far: self.sun_target_camera_adjusted.z + sunlight_cube_size,
            },
            None,
            &mut sunlight_vertex_data,
        );

        sunlight_vertex_data
    }
}
