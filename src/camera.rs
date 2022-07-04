use crate::world::CHUNK_XZ_SIZE;
#[cfg(not(target_arch = "wasm32"))]
use cgmath::Rotation;
use winit::event::{DeviceEvent, ElementState, VirtualKeyCode, WindowEvent};

pub struct Camera {
    pub eye: cgmath::Point3<f32>,
    pub target: cgmath::Point3<f32>,
    pub up: cgmath::Vector3<f32>,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

#[rustfmt::skip]
const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

impl Camera {
    pub fn build_view_projection_matrix(&self) -> cgmath::Matrix4<f32> {
        // 1. The view matrix moves the world to be at the position and rotation of the camera. It's
        // essentially an inverse of whatever the transform matrix of the camera would be.
        let view = cgmath::Matrix4::look_at_rh(self.eye, self.target, self.up);
        // 2. The proj matrix wraps the scene to give the effect of depth. Without this, objects up
        // close would be the same size as objects far away.
        let proj = cgmath::perspective(cgmath::Deg(self.fovy), self.aspect, self.znear, self.zfar);

        // 3. The coordinate system in Wgpu is based on DirectX, and Metal's coordinate systems. That
        // means that in normalized device coordinates (opens new window)the x axis and y axis are
        // in the range of -1.0 to +1.0, and the z axis is 0.0 to +1.0. The cgmath crate (as well as
        // most game math crates) is built for OpenGL's coordinate system. This matrix will scale
        // and translate our scene from OpenGL's coordinate system to WGPU's. We'll define it as
        // follows.
        return OPENGL_TO_WGPU_MATRIX * proj * view;
    }
}

// We need this for Rust to store our data correctly for the shaders
#[repr(C)]
// This is so we can store this in a buffer
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    // We can't use cgmath with bytemuck directly so we'll have
    // to convert the Matrix4 into a 4x4 f32 array
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        use cgmath::SquareMatrix;
        Self {
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().into();
    }
}

pub struct CameraController {
    speed: f32,
    mouse_sensitivity: f64,
    is_forward_pressed: bool,
    is_backward_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
    last_mouse_delta: (f64, f64),
}

pub struct CameraUpdateResult {
    pub did_move_blocks: bool,
    pub did_move_chunks: bool,
    pub new_block_location: cgmath::Point3<usize>,
    pub new_chunk_location: [usize; 2],
}

impl CameraController {
    pub fn new(speed: f32, mouse_sensitivity: f64) -> Self {
        Self {
            speed,
            mouse_sensitivity,
            is_forward_pressed: false,
            is_backward_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
            last_mouse_delta: (0.0, 0.0),
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

    pub fn reset_mouse_delta(&mut self) {
        self.last_mouse_delta = (0.0, 0.0);
    }

    pub fn update_camera(&self, camera: &mut Camera) -> CameraUpdateResult {
        let pre_update_block_location = cgmath::Point3::<usize>::new(
            camera.eye.x as usize,
            camera.eye.y as usize,
            camera.eye.z as usize,
        );
        let pre_update_chunk_location = pre_update_block_location / CHUNK_XZ_SIZE;

        use cgmath::InnerSpace;
        // Vector pointing out of the camera's eye towards the target
        let forward = camera.target - camera.eye;
        let forward_norm = forward.normalize();
        let forward_mag = forward.magnitude();

        // Prevents glitching when camera gets too close to the
        // center of the scene.
        //if self.is_forward_pressed && forward_mag > self.speed {
        if self.is_forward_pressed {
            camera.eye += forward_norm * self.speed;
            camera.target += forward_norm * self.speed;
        }
        if self.is_backward_pressed {
            camera.eye -= forward_norm * self.speed;
            camera.target -= forward_norm * self.speed;
        }

        // Strafing vector
        let right_norm = forward_norm.cross(camera.up);

        // "Vertical" strafing vector
        let up_norm = right_norm.cross(forward).normalize();

        if self.is_right_pressed {
            camera.eye += right_norm * self.speed;
            camera.target += right_norm * self.speed;
        }
        if self.is_left_pressed {
            camera.eye -= right_norm * self.speed;
            camera.target -= right_norm * self.speed;
        }

        let (x_delta, y_delta) = self.last_mouse_delta;
        if y_delta != 0.0 {
            let theta = cgmath::Rad((-y_delta * self.mouse_sensitivity) as f32);
            let rot: cgmath::Basis3<f32> = cgmath::Rotation3::from_axis_angle(right_norm, theta);
            let new_forward = rot.rotate_vector(forward_norm) * forward_mag;
            let forward_diff = new_forward - forward;
            let new_target = camera.target + forward_diff;
            camera.target = new_target;
        }
        if x_delta != 0.0 {
            let theta = cgmath::Rad((-x_delta * self.mouse_sensitivity) as f32);
            let rot: cgmath::Basis3<f32> = cgmath::Rotation3::from_axis_angle(up_norm, theta);
            let new_forward = rot.rotate_vector(forward_norm) * forward_mag;
            let forward_diff = new_forward - forward;
            let new_target = camera.target + forward_diff;
            camera.target = new_target;
        }

        let post_update_block_location = cgmath::Point3::new(
            camera.eye.x as usize,
            camera.eye.y as usize,
            camera.eye.z as usize,
        );
        let post_update_chunk_location = pre_update_block_location / CHUNK_XZ_SIZE;

        CameraUpdateResult {
            did_move_blocks: pre_update_block_location != post_update_block_location,
            did_move_chunks: pre_update_chunk_location != post_update_chunk_location,
            new_block_location: post_update_block_location,
            new_chunk_location: [post_update_chunk_location.x, post_update_chunk_location.z],
        }

        // println!(
        //     "Camera eye: {:?}\nCamera target: {:?}\nForward mag: {:?}\n",
        //     camera.eye, camera.target, forward_mag
        // );
    }
}
