use std::ops::{Index, IndexMut};

pub struct YXZ<const XSIZE: usize, const YSIZE: usize, const ZSIZE: usize> {}
pub struct XYZ<const XSIZE: usize, const YSIZE: usize, const ZSIZE: usize> {}
pub trait DimOrder: Sized {
    fn new() -> Self;
    fn array_index(&self, x: usize, y: usize, z: usize) -> usize;
    fn dims() -> &'static [usize; 3];
}

impl<const XSIZE: usize, const YSIZE: usize, const ZSIZE: usize> DimOrder
    for YXZ<XSIZE, YSIZE, ZSIZE>
{
    fn new() -> Self {
        Self {}
    }
    fn array_index(&self, x: usize, y: usize, z: usize) -> usize {
        y + (x * YSIZE) + (z * YSIZE * XSIZE)
    }
    fn dims() -> &'static [usize; 3] {
        &[YSIZE, XSIZE, ZSIZE]
    }
}
impl<const XSIZE: usize, const YSIZE: usize, const ZSIZE: usize> DimOrder
    for XYZ<XSIZE, YSIZE, ZSIZE>
{
    fn new() -> Self {
        Self {}
    }
    fn array_index(&self, x: usize, y: usize, z: usize) -> usize {
        x + (y * XSIZE) + (z * XSIZE * YSIZE)
    }
    fn dims() -> &'static [usize; 3] {
        &[XSIZE, YSIZE, ZSIZE]
    }
}

#[derive(Debug)]
pub struct Vec3d<T, DO: DimOrder> {
    vec: Vec<T>,
    dim_order: DO,
}

impl<T, DO: DimOrder> Vec3d<T, DO> {
    pub fn new(vec: Vec<T>) -> Self {
        let dims = DO::dims();
        assert!(vec.len() == dims[0] * dims[1] * dims[2]);

        Self {
            vec,
            dim_order: DO::new(),
        }
    }

    pub fn get_unchecked(&self, x: usize, y: usize, z: usize) -> &T {
        &self[[x, y, z]]
    }

    pub fn get_unchecked_mut(&mut self, x: usize, y: usize, z: usize) -> &mut T {
        &mut self[[x, y, z]]
    }

    pub unsafe fn get_raw_ptr_mut(&mut self, x: usize, y: usize, z: usize) -> *mut T {
        self.vec.as_mut_ptr().add(self.dim_order.array_index(x, y, z))
    }

    pub fn dims(&self) -> &[usize; 3] {
        DO::dims()
    }
}

impl<T, DO: DimOrder> Index<[usize; 3]> for Vec3d<T, DO> {
    type Output = T;

    fn index(&self, index: [usize; 3]) -> &T {
        let [x, y, z] = index;
        &self.vec[self.dim_order.array_index(x, y, z)]
    }
}

impl<T, DO: DimOrder> IndexMut<[usize; 3]> for Vec3d<T, DO> {
    fn index_mut<'a>(&'a mut self, index: [usize; 3]) -> &'a mut T {
        let [x, y, z] = index;
        &mut self.vec[self.dim_order.array_index(x, y, z)]
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
