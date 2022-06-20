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
