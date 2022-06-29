use std::ops::{Index, IndexMut};

#[derive(Debug)]
pub struct Vec3d<T> {
    vec: Vec<T>,
    dims: [usize; 3],
}

impl<T> Vec3d<T> {
    pub fn new(vec: Vec<T>, dims: [usize; 3]) -> Self {
        assert!(vec.len() == dims[0] * dims[1] * dims[2]);
        Self { vec, dims }
    }
}

impl<T> Index<[usize; 3]> for Vec3d<T> {
    type Output = T;

    fn index(&self, index: [usize; 3]) -> &T {
        let [x, y, z] = index;
        &self.vec[x + (y * self.dims[0]) + (z * self.dims[0] * self.dims[1])]
    }
}

impl<T> IndexMut<[usize; 3]> for Vec3d<T> {
    fn index_mut<'a>(&'a mut self, index: [usize; 3]) -> &'a mut T {
        let [x, y, z] = index;
        &mut self.vec[x + (y * self.dims[0]) + (z * self.dims[0] * self.dims[1])]
    }
}