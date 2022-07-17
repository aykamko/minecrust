use std::ops::{Index, IndexMut};

#[derive(Debug)]
pub struct Vec3d<T> {
    vec: Vec<T>,
    dims: [usize; 3],
}

impl<T> Index<[usize; 3]> for Vec3d<T> {
    type Output = T;

    fn index(&self, index: [usize; 3]) -> &T {
        let [x, y, z] = index;
        &self.vec[x + (y * self.dims[0]) + (z * self.dims[0] * self.dims[1])]
        // &self.vec[(x * self.dims[1] * self.dims[2]) + (y * self.dims[2]) + z]
    }
}

impl<T> IndexMut<[usize; 3]> for Vec3d<T> {
    fn index_mut<'a>(&'a mut self, index: [usize; 3]) -> &'a mut T {
        let [x, y, z] = index;
        &mut self.vec[x + (y * self.dims[0]) + (z * self.dims[0] * self.dims[1])]
        // &mut self.vec[(x * self.dims[1] * self.dims[2]) + (y * self.dims[2]) + z]
    }
}

impl<T> Vec3d<T> {
    pub fn new(vec: Vec<T>, dims: [usize; 3]) -> Self {
        assert!(vec.len() == dims[0] * dims[1] * dims[2]);
        Self { vec, dims }
    }

    pub fn get_unchecked(&self, x: usize, y: usize, z: usize) -> &T {
        &self[[x, y, z]]
    }

    pub fn get_unchecked_mut(&mut self, x: usize, y: usize, z: usize) -> &mut T {
        &mut self[[x, y, z]]
    }

    pub fn dims(&self) -> [usize; 3] {
        self.dims
    }
}

#[derive(Debug)]
pub struct Vec2d<T> {
    pub vec: Vec<T>,
    pub dims: [usize; 2],
}

impl<T> Vec2d<T> {
    pub fn new(vec: Vec<T>, dims: [usize; 2]) -> Self {
        assert!(vec.len() == dims[0] * dims[1]);
        Self { vec, dims }
    }

    pub fn dims(&self) -> [usize; 2] {
        self.dims
    }
}

impl<T> Index<[usize; 2]> for Vec2d<T> {
    type Output = T;

    fn index(&self, index: [usize; 2]) -> &T {
        let [x, y] = index;
        &self.vec[x + (y * self.dims[0])]
    }
}

impl<T> IndexMut<[usize; 2]> for Vec2d<T> {
    fn index_mut<'a>(&'a mut self, index: [usize; 2]) -> &'a mut T {
        let [x, y] = index;
        &mut self.vec[x + (y * self.dims[0])]
    }
}