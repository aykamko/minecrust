use std::f32::consts::PI;

use crate::{world::CHUNK_XZ_SIZE, DomControlsUserEvent};
use cgmath::{prelude::*, Matrix4, Point3, Vector3};
use collision::{Aabb3, Frustum, Plane};
use winit::event::{DeviceEvent, ElementState, VirtualKeyCode, WindowEvent};

pub struct Camera {
    pub initial_eye: cgmath::Point3<f32>,
    pub eye: cgmath::Point3<f32>,
    pub target: cgmath::Point3<f32>,
    pub up: cgmath::Vector3<f32>,
    pub world_up: cgmath::Vector3<f32>,

    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,

    pub frustum: collision::Frustum<f32>,
}

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

// From cgmath 18.0 source: https://docs.rs/cgmath/0.18.0/src/cgmath/matrix.rs.html#366-378
fn look_to_rh<S: cgmath::BaseFloat>(eye: Point3<S>, dir: Vector3<S>, up: Vector3<S>) -> Matrix4<S> {
    let f = dir.normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);

    #[cfg_attr(rustfmt, rustfmt_skip)]
    cgmath::Matrix4::new(
        s.x.clone(), u.x.clone(), -f.x.clone(), S::zero(),
        s.y.clone(), u.y.clone(), -f.y.clone(), S::zero(),
        s.z.clone(), u.z.clone(), -f.z.clone(), S::zero(),
        -eye.dot(s), -eye.dot(u), eye.dot(f), S::one(),
    )
}

pub fn look_at_rh<S: cgmath::BaseFloat>(
    eye: Point3<S>,
    center: Point3<S>,
    up: Vector3<S>,
) -> Matrix4<S> {
    look_to_rh(eye, center - eye, up)
}

pub fn look_at<S: cgmath::BaseFloat>(
    eye: Point3<S>,
    center: Point3<S>,
    up: Vector3<S>,
) -> Matrix4<S> {
    look_to_rh(eye, center.to_vec(), up)
}

impl Camera {
    pub fn new(
        eye: cgmath::Point3<f32>,
        target: cgmath::Point3<f32>,
        up: cgmath::Vector3<f32>,
        world_up: cgmath::Vector3<f32>,

        aspect: f32,
        fovy: f32,
        znear: f32,
        zfar: f32,
    ) -> Self {
        let dummy_plane = Plane::<f32>::new(cgmath::Vector3::new(0.0, 0.0, 0.0), 0.0);
        let dummy_frustum = Frustum::new(
            dummy_plane,
            dummy_plane,
            dummy_plane,
            dummy_plane,
            dummy_plane,
            dummy_plane,
        );
        let mut partial_self = Self {
            initial_eye: eye,
            eye,
            target,
            up,
            world_up,
            aspect,
            fovy,
            znear,
            zfar,
            frustum: dummy_frustum,
        };
        partial_self.update_frustum();

        partial_self
    }

    pub fn build_view_projection_matrix(&self) -> cgmath::Matrix4<f32> {
        // 1. The view matrix moves the world to be at the position and rotation of the camera. It's
        // essentially an inverse of whatever the transform matrix of the camera would be.

        let eye_vec = self.eye.to_vec();
        let target_shifted_by_origin = self.target - eye_vec;

        let view = look_at_rh(cgmath::Point3::origin(), target_shifted_by_origin, self.up);

        // 2. The proj matrix wraps the scene to give the effect of depth. Without this, objects up
        // close would be the same size as objects far away.
        let proj = cgmath::perspective(cgmath::Deg(self.fovy), self.aspect, self.znear, self.zfar);

        // 3. The coordinate system in Wgpu is based on DirectX, and Metal's coordinate systems.
        // That means that in normalized device coordinates the x axis and y axis are in the range
        // of -1.0 to +1.0, and the z axis is 0.0 to +1.0. The cgmath crate (as well as most game
        // math crates) is built for OpenGL's coordinate system. This matrix will scale and
        // translate our scene from OpenGL's coordinate system to WGPU's. We'll define it as
        // follows.
        return OPENGL_TO_WGPU_MATRIX * proj * view;
    }

    pub fn update_frustum(&mut self) {
        let half_v_side = self.zfar * (self.fovy * 0.5).tan();
        let half_h_side = half_v_side * self.aspect;

        let forward_norm = (self.target - self.eye).normalize();
        let forward_zfar = forward_norm * self.zfar;

        let right_norm = forward_norm.cross(self.world_up).normalize();
        let up_norm = right_norm.cross(forward_norm).normalize();

        self.frustum.left = Plane::from_point_normal(
            self.eye,
            (forward_zfar - right_norm * half_h_side)
                .cross(up_norm)
                .normalize(),
        );
        self.frustum.right = Plane::from_point_normal(
            self.eye,
            up_norm
                .cross(forward_zfar + right_norm * half_h_side)
                .normalize(),
        );
        self.frustum.bottom = Plane::from_point_normal(
            self.eye,
            (forward_zfar + up_norm * half_v_side)
                .cross(right_norm)
                .normalize(),
        );
        self.frustum.top = Plane::from_point_normal(
            self.eye,
            right_norm
                .cross(forward_zfar - up_norm * half_v_side)
                .normalize(),
        );
        self.frustum.near =
            Plane::from_point_normal(self.eye + self.znear * forward_norm, forward_norm);
        self.frustum.far = Plane::from_point_normal(self.eye + forward_zfar, -forward_norm);
    }

    pub fn filter_visible_chunks(&self, mut chunk_geoms: &Vec<Aabb3<f32>>) {}
}

// We need this for Rust to store our data correctly for the shaders
#[repr(C)]
// This is so we can store this in a buffer
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    // We can't use cgmath with bytemuck directly so we'll have
    // to convert the Matrix4 into a 4x4 f32 array
    view_proj: [[f32; 4]; 4],
    eye_pos: [f32; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: cgmath::Matrix4::identity().into(),
            eye_pos: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().into();
        self.eye_pos = [camera.eye.x, camera.eye.y, camera.eye.z, 1.0];
    }
}

pub struct CameraController {
    _speed: f32,
    mouse_sensitivity: f64,
    is_forward_pressed: bool,
    is_backward_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
    is_space_pressed: bool,
    is_shift_pressed: bool,
    is_sprint_pressed: bool,
    last_mouse_delta: (f64, f64),
    last_joystick_vector: (f64, f64),
    num_updates: u64,
}

pub struct CameraUpdateResult {
    pub did_move: bool,
    pub did_move_blocks: bool,
    pub did_move_chunks: bool,
    pub new_block_location: cgmath::Point3<usize>,
    pub old_chunk_location: [usize; 2],
    pub new_chunk_location: [usize; 2],
}

impl CameraController {
    pub fn new(speed: f32, mouse_sensitivity: f64) -> Self {
        Self {
            _speed: speed,
            mouse_sensitivity,
            is_forward_pressed: false,
            is_backward_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
            is_space_pressed: false,
            is_shift_pressed: false,
            is_sprint_pressed: false,
            last_mouse_delta: (0.0, 0.0),
            last_joystick_vector: (0.0, 0.0),
            num_updates: 0,
        }
    }

    fn speed(&self) -> f32 {
        if self.is_sprint_pressed {
            //self._speed / 8.0
            self._speed * 4.0
        } else {
            self._speed
        }
    }

    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                input:
                    winit::event::KeyboardInput {
                        state,
                        virtual_keycode: Some(keycode),
                        ..
                    },
                ..
            } => {
                let is_pressed = *state == ElementState::Pressed;
                match keycode {
                    VirtualKeyCode::W | VirtualKeyCode::Up => {
                        self.is_forward_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::A | VirtualKeyCode::Left => {
                        self.is_left_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::S | VirtualKeyCode::Down => {
                        self.is_backward_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::D | VirtualKeyCode::Right => {
                        self.is_right_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::Space => {
                        self.is_space_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::LShift | VirtualKeyCode::RShift => {
                        self.is_shift_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::LControl | VirtualKeyCode::RControl => {
                        self.is_sprint_pressed = is_pressed;
                        true
                    }
                    VirtualKeyCode::Minus => {
                        self._speed *= 0.5;
                        true
                    }
                    VirtualKeyCode::Equals => {
                        self._speed *= 2.0;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn process_device_event(&mut self, event: &DeviceEvent) -> bool {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                self.last_mouse_delta = *delta;
                true
            }
            _ => false,
        }
    }

    pub fn process_web_dom_button_event(&mut self, event: &DomControlsUserEvent) -> bool {
        match event {
            DomControlsUserEvent::UpPressed => {
                self.is_forward_pressed = true;
                true
            }
            DomControlsUserEvent::UpReleased => {
                self.is_forward_pressed = false;
                true
            }
            DomControlsUserEvent::DownPressed => {
                self.is_backward_pressed = true;
                true
            }
            DomControlsUserEvent::DownReleased => {
                self.is_backward_pressed = false;
                true
            }
            DomControlsUserEvent::LeftPressed => {
                self.is_left_pressed = true;
                true
            }
            DomControlsUserEvent::LeftReleased => {
                self.is_left_pressed = false;
                true
            }
            DomControlsUserEvent::RightPressed => {
                self.is_right_pressed = true;
                true
            }
            DomControlsUserEvent::RightReleased => {
                self.is_right_pressed = false;
                true
            }
            DomControlsUserEvent::PitchYawJoystickMoved { vector } => {
                const PITCH_YAW_JOYSTICK_SCALE_FACTOR: f64 = 1.8;
                self.last_joystick_vector = (
                    vector.0 * PITCH_YAW_JOYSTICK_SCALE_FACTOR,
                    vector.1 * PITCH_YAW_JOYSTICK_SCALE_FACTOR,
                );
                true
            }
            DomControlsUserEvent::PitchYawJoystickReleased => {
                self.last_joystick_vector = (0.0, 0.0);
                true
            }
            DomControlsUserEvent::TranslationJoystickDirectionChanged { direction } => {
                self.clear_translational_inputs();
                match direction {
                    0 => {
                        self.is_forward_pressed = true;
                    }
                    1 => {
                        self.is_right_pressed = true;
                    }
                    2 => {
                        self.is_backward_pressed = true;
                    }
                    3 => {
                        self.is_left_pressed = true;
                    }
                    _ => (),
                }
                true
            }
            DomControlsUserEvent::TranslationJoystickReleased => {
                self.clear_translational_inputs();
                true
            }
            _ => {
                log::info!("got some other user event: {:?}", event);
                false
            }
        }
    }

    fn clear_translational_inputs(&mut self) {
        self.is_forward_pressed = false;
        self.is_right_pressed = false;
        self.is_backward_pressed = false;
        self.is_left_pressed = false;
    }

    pub fn reset_mouse_delta(&mut self) {
        self.last_mouse_delta = (0.0, 0.0);
    }

    pub fn update_camera(
        &mut self,
        camera: &mut Camera,
        world_state: &crate::world::WorldState,
    ) -> CameraUpdateResult {
        let pre_update_block_location = cgmath::Point3::<usize>::new(
            camera.eye.x as usize,
            camera.eye.y as usize,
            camera.eye.z as usize,
        );
        let pre_update_chunk_location = [
            pre_update_block_location.x / CHUNK_XZ_SIZE,
            pre_update_block_location.z / CHUNK_XZ_SIZE,
        ];
        let mut did_move = false;
        let mut did_translate = false;

        // Vector pointing out of the camera's eye towards the target
        let forward = camera.target - camera.eye;
        let forward_norm = forward.normalize();
        let forward_mag = forward.magnitude();

        let mut next_eye = camera.eye;
        let mut next_target = camera.target;

        // Prevents glitching when camera gets too close to the
        // center of the scene.
        //if self.is_forward_pressed && forward_mag > self.speed {
        if self.is_forward_pressed {
            did_move = true;
            did_translate = true;
            next_eye += forward_norm * self.speed();
            next_target += forward_norm * self.speed();
        }
        if self.is_backward_pressed {
            did_move = true;
            did_translate = true;
            next_eye -= forward_norm * self.speed();
            next_target -= forward_norm * self.speed();
        }

        // Strafing vector
        let right_norm = forward_norm.cross(camera.up);

        if self.is_right_pressed {
            did_move = true;
            did_translate = true;
            next_eye += right_norm * self.speed();
            next_target += right_norm * self.speed();
        }
        if self.is_left_pressed {
            did_move = true;
            did_translate = true;
            next_eye -= right_norm * self.speed();
            next_target -= right_norm * self.speed();
        }

        if self.is_space_pressed {
            did_move = true;
            did_translate = true;
            next_eye += camera.world_up * self.speed();
            next_target += camera.world_up * self.speed();
        }
        if self.is_shift_pressed {
            did_move = true;
            did_translate = true;
            next_eye -= camera.world_up * self.speed();
            next_target -= camera.world_up * self.speed();
        }

        // "Vertical" strafing vector
        let up_norm = right_norm.cross(forward).normalize();

        let (x_delta, y_delta) =
            if self.last_joystick_vector.0 != 0.0 || self.last_joystick_vector.1 != 0.0 {
                self.last_joystick_vector
            } else {
                self.last_mouse_delta
            };
        if y_delta != 0.0 {
            let theta = cgmath::Rad((-y_delta * self.mouse_sensitivity) as f32);
            let rot: cgmath::Basis3<f32> = cgmath::Rotation3::from_axis_angle(right_norm, theta);
            let new_forward = rot.rotate_vector(forward_norm) * forward_mag;
            let forward_diff = new_forward - forward;
            did_translate = true;
            next_target += forward_diff;
        }
        if x_delta != 0.0 {
            let theta = cgmath::Rad((-x_delta * self.mouse_sensitivity) as f32);
            let rot: cgmath::Basis3<f32> = cgmath::Rotation3::from_axis_angle(up_norm, theta);
            let new_forward = rot.rotate_vector(forward_norm) * forward_mag;
            let forward_diff = new_forward - forward;
            did_translate = true;
            next_target += forward_diff;
        }

        if did_move {
            let maybe_collision_normal =
                world_state.collision_normal_from_ray_2(&camera, &next_eye);
            if let Some(collision_normal) = maybe_collision_normal {
                if collision_normal.x != 0.0 {
                    next_eye.x = camera.eye.x;
                    next_target.x = camera.target.x;
                }
                if collision_normal.y != 0.0 {
                    next_eye.y = camera.eye.y;
                    next_target.y = camera.target.y;
                }
                if collision_normal.z != 0.0 {
                    next_eye.z = camera.eye.z;
                    next_target.z = camera.target.z;
                }
            }
            if world_state.block_collidable_at_point(&next_eye) {
                // Scoot camera backwards if there's a collision after sliding
                let translate_vector = next_eye - camera.eye;
                next_eye = camera.eye - translate_vector;
            }
        }

        if did_move {
            camera.eye = next_eye;
        }
        if did_translate {
            camera.target = next_target;
        }

        // Update view frustum
        camera.update_frustum();

        let post_update_block_location = cgmath::Point3::new(
            camera.eye.x as usize,
            camera.eye.y as usize,
            camera.eye.z as usize,
        );
        let post_update_chunk_location = [
            post_update_block_location.x / CHUNK_XZ_SIZE,
            post_update_block_location.z / CHUNK_XZ_SIZE,
        ];

        if self.num_updates % 200 == 0 {
            println!("Camera position at {:?}", camera.eye);
        }
        self.num_updates += 1;

        CameraUpdateResult {
            did_move,
            did_move_blocks: pre_update_block_location != post_update_block_location,
            did_move_chunks: pre_update_chunk_location != post_update_chunk_location,
            new_block_location: post_update_block_location,
            old_chunk_location: pre_update_chunk_location,
            new_chunk_location: post_update_chunk_location,
        }

        // println!(
        //     "Camera eye: {:?}\nCamera target: {:?}\nForward mag: {:?}\n",
        //     camera.eye, camera.target, forward_mag
        // );
    }
}
