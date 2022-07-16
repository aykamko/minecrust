/*
MIT License

Copyright (c) 2022 Christopher Collin Hall (aka DrPlantabyte)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
 */

//! # ZArray
//! Z-order indexed 2D and 3D arrays using Morton order (aka Z-order) with a convenient API for
//! common 2D and 3D access patterns. Use of zarray in place of a Vec of Vecs often improves
//! performance, especially for algorithms such as blurring and cellular automata.
//! ## About ZArray
//! The *zarray* crate  is a lightweight Rust library that provides structs for working with 2D and
//! 3D arrays, using internal Z-Order Morton indexing to improve data localization for better
//! cache-line performance.
//! ## Quickstart Guide
//! Simply import *zarray::z2d::ZArray2D* and/or *zarray::z3d::ZArray3D* and then use the
//! *ZArray_D::new(...)* function to initialize a new instance. The type will automatically be
//! inferred from the povided default value. Note that only types which implement the *Copy* trait
//! are allowed (ie not Vec or other heap-allocating types).
//!
//! For example, here's a simple blur operation using ZArray2D, which generally performs better
//! than using a Vec of Vecs by about 10-25%:
//! ```rust
//! use zarray::z2d::ZArray2D;
//! let h: isize = 200;
//! let w: isize = 300;
//! let radius: isize = 3;
//! let mut src = ZArray2D::new(w as usize, h as usize, 0u8);
//! // set values
//! src.bounded_fill(100, 100, 200, 150, 255u8);
//! // sum neighbors values with ZArray
//! let mut blurred = ZArray2D::new(w as usize, h as usize, 0u16);
//! for y in 0..h { for x in 0..w {
//!   let mut sum = 0;
//!   for dy in -radius..radius+1 { for dx in -radius..radius+1 {
//!     sum += *src.bounded_get(x+dx, y+dy).unwrap_or(&0u8) as u16;
//!   } }
//!   blurred.set(x as usize, y as usize, sum/((2*radius as u16+1).pow(2))).unwrap();
//! } }
//! ```
//!
//! ## How it works
//! the *ZArray_D* structs store data in 8x8 or 8x8x8 chuncks, using Z-order indexing to access the
//! data within each chunk (as described [here](https://en.wikipedia.org/wiki/Z-order_curve) ). In
//! so doing, the lowest 4 bits of each dimension are interdigitated to significantly improve data
//! locality and cache-line fetch efficiency (though not as much as a Hilbert curve would do)
//!
//! ## Why not just use Vec of Vecs (aka Vec<Vec<T>>)?
//! Most of the time, using a Vec<Vec<T>> would have great performance, so long as you remember to
//! structure your for-loops correctly. However, when the data is not accessed in a linear fashion,
//! such as when implementing a cellular automata or a blurring or ray tracing algorithm, then the
//! performance of a Vec<Vec<T>> can be significantly impaired by frequent RAM access and
//! cache-line misses. This is when data locality matters most for performance.
//!
//! ### Why not Z-Order the entire data array?
//! Two reasons: Firstly, Z-Order indexing only works on square/cube shaped data, so a pure Z-Order
//! index would waste huge amount of memory for 2D and 3D arrays that are long and thin. Second, on
//! most CPU architectures (Intel, AMD, and Arm), memory is accessed in 64-byte cache-lines, thus
//! the performance gains from Z-order indexing are less significant above 6 bits of linear
//! addressing space (ie 8x8 or 4x4x4)
//!
//! ## Note
//! Only types with the *Copy* trait can be stored in *ZArray_D* structs. Thus *zarray* works for
//! all numerical types and most simple data structs, but not for heap-allocating data types such
//! as Vec. This limitation arises from complexities in filling the data patches with initial
//! values. In addition, you would not see a performance improvement over a simple Vec<Vec<T>> if
//! the data resided on the heap.
//!
//! ## License
//! This library is provided under the MIT license. In other words: free to use as you wish.
//!
//! ## Contributing
//! If you'd like to contribute, go ahead and fork the GitHub repo and/or submit a pull request

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

/// This struct is an error type that is returned when attempting to get a value that is outside
/// the range of the data. It implements the Debug and Display traits so that it can be easily
/// printed as an error message.
pub struct LookUpError{
	/// coordinate that was out of bounds
	coord: Vec<usize>,
	/// bounds of the ZArray*D that was violated
	bounds: Vec<usize>,
}

impl Debug for LookUpError {
	// programmer-facing error message
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		return write!(f, "{{ file: {}, line: {}, coord: {}, bounds: {} }}", file!(), line!(), vec_to_string(&self.coord), vec_to_string(&self.bounds));
	}
}

impl Display for LookUpError {
	// user-facing error message
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		return write!(f, "Error: could not access coordinate {} because it is out of range for size {}", vec_to_string(&self.coord), vec_to_string(&self.bounds));
	}
}

impl Error for LookUpError{}

impl LookUpError {

}
/// Utility function for converting Vecs to Strings for the purpose of error reporting and debugging
fn vec_to_string(v: &Vec<usize>) -> String{
	let mut sb = String::from("(");
	let mut not_first = false;
	for n in v {
		if not_first {
			sb += &String::from(", ");
		} else {
			not_first = true;
		}
		sb += &String::from(n.to_string());
	}
	sb += &String::from(")");
	return sb;
}
/// This module is used for storing 2-dimensional data arrays, and internally uses Z-index arrays
/// to improve data localization and alignment to the CPU cache-line fetches. In other words, use
/// this to improve performance for 2D data that is randomly accessed rather than raster scanned
/// or if your data processing makes heavy use of neighbor look-up in both the X and Y directions.
/// # How It Works
/// When you initialize a zarray::z2d::ZArray2D struct, it creates an array of 8x8 data patches,
/// using Z-curve indexing within that patch. When you call a getter or setter method, it finds the
/// corresponding data patch and then looks up (or sets) the data from within the patch. Since the
/// cache-line size on most CPUs is 64 bytes (and up to only 128 bytes on more exotic chips), the
/// 8x8 patch is sufficient localization for the majority of applications.
/// # Example Usage
/// An example of a simple blurring operation
/// ```
/// use zarray::z2d::ZArray2D;
/// let w = 800;
/// let h = 600;
/// let mut input = ZArray2D::new(w, h, 0i32);
/// let mut blurred = ZArray2D::new(w, h, 0i32);
/// for y in 0..h {
///   for x in 0..w {
///     let random_number = (((x*1009+1031)*y*1013+1051) % 10) as i32;
///     input.set(x, y, random_number).unwrap();
///   }
/// }
/// let radius: i32 = 2;
/// for y in radius..h as i32-radius {
///   for x in radius..w as i32-radius {
///     let mut sum = 0;
///     for dy in -radius..radius+1 {
///       for dx in -radius..radius+1 {
///         sum += *input.bounded_get((x+dx) as isize, (y+dy) as isize).unwrap_or(&0);
///       }
///     }
///     blurred.set(x as usize, y as usize, sum/((2*radius+1).pow(2))).unwrap();
///   }
/// }
/// ```
pub mod z2d {
	// Z-order indexing in 2 dimensions

	use std::marker::PhantomData;
	use super::LookUpError;

	/// Private struct for holding an 8x8 data patch
	#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
	#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
	struct Patch<T>{
		contents: [T;64]
	}

	impl<T> Patch<T> {
		/// data patch getter
		/// # Parameters
		/// * **x** - x coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **y** - y coord (only lowest 3 bits are used, rest of bits are ignored)
		/// # Returns
		/// Returns a reference to the value stored in the patch at location (x & 0x07), (y & 0x07)
		fn get(&self, x: usize, y:usize) -> &T {
			// 3-bit x 3-bit
			return &self.contents[zorder_4bit_to_8bit(x as u8 & 0x07, y as u8 & 0x07) as usize];
		}
		/// data patch setter
		/// # Parameters
		/// * **x** - x coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **y** - y coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **new_val** - value to set
		fn set(&mut self, x: usize, y:usize, new_val: T) {
			// 3-bit x 3-bit
			let i = zorder_4bit_to_8bit(x as u8 & 0x07, y as u8 & 0x07) as usize;
			//let old_val = &self.contents[i];
			self.contents[i] = new_val;
			//return old_val;
		}
	}

	/// function for converting coordinate to index of data patch in the array of patches
	fn patch_index(x: usize, y:usize, pwidth: usize) -> usize{
		return (x >> 3) + ((y >> 3) * (pwidth));
	}

	/// This is primary struct for z-indexed 2D arrays. Create new instances with
	/// ZArray2D::new(x_size, y_size, initial_value)
	#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
	#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
	pub struct ZArray2D<T> {
		// for heap allocated data
		width: usize,
		height: usize,
		pwidth: usize,
		patches: Vec<Patch<T>>,
		_phantomdata: PhantomData<T>,
	}

	impl<T> ZArray2D<T> where T: Copy {
		/// Create a Z-index 2D array of values, initially filled with the provided default value
		/// # Parameters
		/// * **width** - size of this 2D array in the X dimension
		/// * **height** - size of this 2D array in the Y dimension
		/// * **default_val** - initial fill value (if a struct type, then it must implement the
		/// Copy trait)
		/// # Returns
		/// Returns an initialized *ZArray2D* struct filled with *default_val*
		pub fn new(width: usize, height: usize, default_val: T) -> ZArray2D<T>{
			let pwidth = (width >> 3) + 1;
			let pheight = (height >> 3) + 1;
			let patch_count = pwidth * pheight;
			let mut p = Vec::with_capacity(patch_count);
			for _ in 0..patch_count{
				p.push(Patch{contents: [default_val; 64]});
			}
			return ZArray2D {width, height, pwidth, patches: p, _phantomdata: PhantomData};
		}

		/// Gets the (x, y) size of this 2D array
		/// # Returns
		/// Returns a tuple of (width, height) for this 2D array
		pub fn dimensions(&self) -> (usize, usize){
			return (self.width, self.height);
		}

		/// Gets the X-dimension size (aka width) of this 2D array
		/// # Returns
		/// Returns the size in the X dimension
		pub fn xsize(&self) -> usize {
			return self.width;
		}


		/// Alias for `xsize()`
		/// # Returns
		/// Returns the size in the X dimension
		pub fn width(&self) -> usize {
			return self.xsize();
		}

		/// Gets the Y-dimension size (aka height) of this 2D array
		/// # Returns
		/// Returns the size in the Y dimension
		pub fn ysize(&self) -> usize {
			return self.height;
		}

		/// Alias for `ysize()`
		/// # Returns
		/// Returns the size in the Y dimension
		pub fn height(&self) -> usize {
			return self.ysize();
		}

		/// Gets a value from the 2D array, or returns a *LookUpError* if the provided coordinate
		/// is out of bounds. If you are using a default value for out-of-bounds coordinates,
		/// then you should use the *bounded_get(x, y)* method instead. If you want access to
		/// wrap-around (eg (-2, 0) equivalent to (width-2,0)), then use the *wrapped_get(x, y)*
		/// method.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// # Returns
		/// Returns a Result type that holds either the returned data value (as a reference) from
		/// the 2D array, or a *LookUpError* signalling that the coordinate is out of bounds
		pub fn get(&self, x: usize, y: usize) -> Result<&T,LookUpError>{
			if x < self.width && y < self.height {
				Ok(self.patches[patch_index(x, y, self.pwidth)].get(x, y))
			} else {
				Err(LookUpError{coord: vec![x, y], bounds: vec![self.width, self.height]})
			}
		}

		/// Sets a value in the 2D array, or returns a *LookUpError* if the provided coordinate
		/// is out of bounds. If you want out-of-bound coordinates to result in a no-op, then use
		/// the *bounded_set(x, y, val)* method instead. If you want access to wrap-around (eg
		/// (-2, 0) equivalent to (width-2,0)), then use the *wrapped_set(x, y, val)* method.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **new_val** - value to store in the 2D array at (x, y)
		/// # Returns
		/// Returns a Result type that is either empty or a *LookUpError* signalling that the
		/// coordinate is out of bounds
		pub fn set(&mut self, x: usize, y: usize, new_val: T) -> Result<(),LookUpError>{
			if x < self.width && y < self.height {
				Ok(self.patches[patch_index(x, y, self.pwidth)].set(x, y, new_val))
			} else {
				Err(LookUpError{coord: vec![x, y], bounds: vec![self.width, self.height]})
			}
		}

		/// Gets a value from the 2D array without bounds checking
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// # Returns
		/// Returns a data value (as a reference) from the 2D array
		pub fn get_unchecked(&self, x: usize, y: usize) -> &T{
			return self.patches[patch_index(x, y, self.pwidth)].get(x, y);
		}

		/// Sets a value in the 2D array without bounds checking
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **new_val** - value to store in the 2D array at (x, y)
		pub fn set_unchecked(&mut self, x: usize, y: usize, new_val: T){
			self.patches[patch_index(x, y, self.pwidth)].set(x, y, new_val);
		}

		/// Gets a value from the 2D array, wrapping around the X and Y axese when the coordinates
		/// are negative or outside the size of this 2D array. Good for when you want tiling
		/// behavior.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// # Returns
		/// Returns a reference to the data stored at the provided coordinate (wrapping both x
		/// and y dimensions)
		pub fn wrapped_get(&self, x: isize, y: isize) -> &T{
			let x = (self.width as isize + (x % self.width as isize)) as usize % self.width;
			let y = (self.height as isize + (y % self.height as isize)) as usize % self.height;
			return &self.patches[patch_index(x, y, self.pwidth)].get(x, y);
		}

		/// Sets a value in the 2D array at the provided coordinate, wrapping the X and Y axese
		/// if the coordinate is negative or out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **new_val** - value to store in the 2D array at (x, y), wrapping around both the x
		/// and y dimensions
		pub fn wrapped_set(&mut self, x: isize, y: isize, new_val: T) {
			let x = (self.width as isize + (x % self.width as isize)) as usize % self.width;
			let y = (self.height as isize + (y % self.height as isize)) as usize % self.height;
			self.patches[patch_index(x, y, self.pwidth)].set(x, y, new_val);
		}

		/// Gets a value from the 2D array as an Option that is None if the coordinate
		/// is out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// # Returns
		/// Returns an Option type that holds either the returned data value (as a reference) from
		/// the 2D array, or *None* signalling that the coordinate is out of bounds (which can be
		/// combined with .unwrap_or(default_value) to implement an out-of-bounds default)
		pub fn bounded_get(&self, x: isize, y: isize) -> Option<&T>{
			if x >= 0 && y >= 0 && x < self.width as isize && y < self.height as isize {
				return Some(&self.patches[patch_index(x as usize, y as usize, self.pwidth)]
					.get(x as usize, y as usize));
			} else {
				return None;
			}
		}

		/// Sets a value in the 2D array if and only if the provided coordinate is in bounds.
		/// Otherwise this method does nothing if the coordiante is out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **new_val** - value to store int eh 2D array at (x, y)
		pub fn bounded_set(&mut self, x: isize, y: isize, new_val: T) {
			if x >= 0 && y >= 0 && x < self.width as isize && y < self.height as isize {
				self.patches[patch_index(x as usize, y as usize, self.pwidth)]
					.set(x as usize, y as usize, new_val);
			} else {
				// no-op
			}
		}

		/// Fills a region of this 2D array with a given value, or returns a *LookUpError* if the
		/// provided coordinates go out of bounds. If you just want to ignore any
		/// out-of-bounds coordinates, then you should use the *bounded_fill(x1, y1, x2, y2)*
		/// method instead. If you want access to wrap-around (eg (-2, 0) equivalent to
		/// (width-2,0)), then use the *wrapped_fill(x, y)* method.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 2D array in the bounding box defined by
		/// (x1, y1) -> (x2, y2)
		/// # Returns
		/// Returns a Result type that is either empty or a *LookUpError* signalling that a
		/// coordinate is out of bounds
		pub fn fill(&mut self, x1: usize, y1: usize, x2: usize, y2: usize, new_val: T)
					-> Result<(), LookUpError> {
			for y in y1..y2{ for x in x1..x2{
				self.set(x, y, new_val)?;
			} }
			Ok(())
		}

		/// Fills a region of this 2D array with a given value, wrapping the axese when
		/// coordinates go out of bounds.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 2D array in the bounding box defined by
		/// (x1, y1) -> (x2, y2) with wrapped axese
		pub fn wrapped_fill(&mut self, x1: isize, y1: isize, x2: isize, y2: isize, new_val: T) {
			for y in y1..y2{ for x in x1..x2{
				self.wrapped_set(x, y, new_val);
			} }
		}

		/// Fills a region of this 2D array with a given value, ignoring any
		/// coordinates that go out of bounds.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 2D array in the bounding box defined by
		/// (x1, y1) -> (x2, y2)
		pub fn bounded_fill(&mut self, x1: isize, y1: isize, x2: isize, y2: isize, new_val: T) {
			for y in y1..y2{ for x in x1..x2{
				self.bounded_set(x, y, new_val);
			} }
		}

	}

	/// Used for Z-index look-up
	static ZLUT: [u8; 16] = [
		0b00000000,
		0b00000001,
		0b00000100,
		0b00000101,
		0b00010000,
		0b00010001,
		0b00010100,
		0b00010101,
		0b01000000,
		0b01000001,
		0b01000100,
		0b01000101,
		0b01010000,
		0b01010001,
		0b01010100,
		0b01010101
	];

	/// General purpose Z-index function to convert a two-dimensional coordinate into a localized
	/// one-dimensional coordinate
	/// # Parameters
	/// * **x** - x dimension coordinate *(ONLY THE LOWER 4 BITS WILL BE USED!)*
	/// * **y** - y dimension coordinate *(ONLY THE LOWER 4 BITS WILL BE USED!)*
	/// # Returns
	/// Z-curve index for use as an index in a linear array meant to hold 2D data. In other words,
	/// given the binary numbers X=0b0000xxxx and Y=0b0000yyyy, this method will return 0byxyxyxyx.
	pub fn zorder_4bit_to_8bit(x: u8, y: u8) -> u8 {
		let x_bits = ZLUT[(x & 0x0F) as usize];
		let y_bits = ZLUT[(y & 0x0F) as usize] << 1;
		return y_bits | x_bits;
	}

	/// General purpose Z-index function to convert a two-dimensional coordinate into a localized
	/// one-dimensional coordinate
	/// # Parameters
	/// * **x** - x dimension coordinate (8 bits)
	/// * **y** - y dimension coordinate (8 bits)
	/// # Returns
	/// Z-curve index for use as an index in a linear array meant to hold 2D data. In other words,
	/// given the binary numbers Y=0b0000xxxx and Y=0b0000yyyy, this method will return 0byxyxyxyx.
	pub fn zorder_8bit_to_16bit(x:u8, y:u8) -> u16 {
		return ((zorder_4bit_to_8bit(x >> 4, y >> 4) as u16) << 8) | zorder_4bit_to_8bit(x, y) as u16
	}

	/// General purpose Z-index function to convert a two-dimensional coordinate into a localized
	/// one-dimensional coordinate
	/// # Parameters
	/// * **x** - x dimension coordinate (16 bits)
	/// * **y** - y dimension coordinate (16 bits)
	/// # Returns
	/// Z-curve index for use as an index in a linear array meant to hold 2D data. In other words,
	/// given the binary numbers Y=0b0000xxxx and Y=0b0000yyyy, this method will return 0byxyxyxyx.
	pub fn zorder_16bit_to_32bit(x:u16, y:u16) -> u32 {
		return ((zorder_8bit_to_16bit((x & 0xFF) as u8, (y & 0xFF) as u8) as u32) << 16) | zorder_8bit_to_16bit((x >> 8) as u8, (y >> 8) as u8) as u32
	}

}

/// This module is used for storing 3-dimensional data arrays, and internally uses Z-index arrays
/// to improve data localization and alignment to the CPU cache-line fetches. In other words, use
/// this to improve performance for 3D data that is randomly accessed rather than raster scanned
/// or if your data processing makes heavy use of neighbor look-up in the X, Y, and Z directions.
/// # How It Works
/// When you initialize a zarray::z3d::ZArray3D struct, it creates an array of 8x8x8 data patches
/// (512 total elements per patch), using Z-curve indexing within that patch. When you call a
/// getter or setter method, it finds the corresponding data patch and then looks up (or sets) the
/// data from within the patch.
/// # Example Usage
/// The following example could be used as part of an erosion simulation:
/// ```
/// use zarray::z3d::ZArray3D;
/// let width = 100;
/// let length = 200;
/// let depth = 25;
/// let air = 0f32;
/// let soil_hardness = 1f32;
/// let rock_hardness = 8f32;
/// let drip_power = 1.5f32;
/// let iterations = 12;
/// let mut map = ZArray3D::new(width, length, depth, air);
/// map.fill(0,0,5, width,length,depth, soil_hardness).unwrap();
/// map.fill(0,0,15, width,length,depth, rock_hardness).unwrap();
/// for boulder in [(34,88,6), (66,122,9), (11,154,5), (35,93,8), (72,75,12)]{
///   map.set(boulder.0, boulder.1, boulder.2, rock_hardness).unwrap();
/// }
/// for _ in 0..iterations{
///   for x in 0..width{for y in 0..length{
///     let mut drip = drip_power;
///     let mut z = 0;
///     while drip > 0f32 {
///       let h = *map.bounded_get(x as isize, y as isize, z).unwrap_or(&100f32);
///       if h > drip {
///         map.bounded_set(x as isize, y as isize, z, h - drip);
///         drip = 0.;
///       } else {
///         map.bounded_set(x as isize, y as isize, z, 0.);
///         drip -= h;
///       }
///       z += 1;
///     }
///   }}
/// }
/// ```
pub mod z3d {
	// Z-order indexing in 2 dimensions

	use std::marker::PhantomData;
	use super::LookUpError;


	/// Private struct for holding an 8x8x8 data patch
	#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
	#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
	struct Patch<T>{
		contents: [T;512]
	}

	impl<T> Patch<T> {
		/// data patch getter
		/// # Parameters
		/// * **x** - x coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **y** - y coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **z** - z coord (only lowest 3 bits are used, rest of bits are ignored)
		/// # Returns
		/// Returns a reference to the value stored in the patch at location (x, y, z) (lowest 3
		/// bits only)
		fn get(&self, x: usize, y:usize, z:usize) -> &T {
			// 3-bit x 3-bit x 3-bit
			return &self.contents[zorder_4bit_to_12bit(
				x as u8 & 0x07, y as u8 & 0x07, z as u8 & 0x07) as usize];
		}
		/// data patch setter
		/// # Parameters
		/// * **x** - x coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **y** - y coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **z** - z coord (only lowest 3 bits are used, rest of bits are ignored)
		/// * **new_val** - value to set at (x,y,z)
		fn set(&mut self, x: usize, y:usize, z:usize, new_val: T) {
			// 3-bit x 3-bit
			let i = zorder_4bit_to_12bit(
				x as u8 & 0x07, y as u8 & 0x07, z as u8 & 0x07) as usize;
			self.contents[i] = new_val;
		}

		fn get_mut(&mut self, x: usize, y:usize, z:usize) -> &mut T {
			// 3-bit x 3-bit x 3-bit
			return &mut self.contents[zorder_4bit_to_12bit(
				x as u8 & 0x07, y as u8 & 0x07, z as u8 & 0x07) as usize];
		}
	}

	/// function for converting coordinate to index of data patch in the array of patches
	fn patch_index(x: usize, y:usize, z:usize, pxsize: usize, pysize: usize) -> usize{
		return (x >> 3) + pxsize * ((y >> 3) + (pysize * (z >> 3)));
	}

	/// This is primary struct for z-indexed 3D arrays. Create new instances with
	/// ZArray3D::new(x_size, y_size, z_size, initial_value)
	#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
	#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
	pub struct ZArray3D<T> {
		// for heap allocated data
		xsize: usize,
		ysize: usize,
		zsize: usize,
		pxsize: usize,
		pysize: usize,
		patches: Vec<Patch<T>>,
		_phantomdata: PhantomData<T>,
	}

	impl<T> ZArray3D<T> where T: Copy {
		/// Create a Z-index 3D array of values, initially filled with the provided default value
		/// # Parameters
		/// * **xsize** - size of this 3D array in the X dimension
		/// * **ysize** - size of this 3D array in the Y dimension
		/// * **zsize** - size of this 3D array in the Z dimension
		/// * **default_val** - initial fill value (if a struct type, then it must implement the
		/// Copy trait)
		/// # Returns
		/// Returns an initialized *ZArray3D* struct filled with *default_val*
		pub fn new(xsize: usize, ysize: usize, zsize: usize, default_val: T) -> ZArray3D<T>{
			let px = (xsize >> 3) + 1;
			let py = (ysize >> 3) + 1;
			let pz = (zsize >> 3) + 1;
			let patch_count = px * py * pz;
			let mut p = Vec::with_capacity(patch_count);
			for _ in 0..patch_count{
				p.push(Patch{contents: [default_val; 512]});
			}
			return ZArray3D { xsize, ysize, zsize, pxsize: px, pysize: py,
				patches: p, _phantomdata: PhantomData};
		}

		/// Gets the (x, y, z) size of this 3D array
		/// # Returns
		/// Returns a tuple of (width, height, depth) for this 2D array
		pub fn dimensions(&self) -> (usize, usize, usize){
			return (self.xsize, self.ysize, self.zsize);
		}

		/// Gets the X-dimension size (aka width) of this 3D array
		/// # Returns
		/// Returns the size in the X dimension
		pub fn xsize(&self) -> usize {
			return self.xsize;
		}


		/// Alias for `xsize()`
		/// # Returns
		/// Returns the size in the X dimension
		pub fn width(&self) -> usize {
			return self.xsize();
		}

		/// Gets the Y-dimension size (aka height) of this 3D array
		/// # Returns
		/// Returns the size in the Y dimension
		pub fn ysize(&self) -> usize {
			return self.ysize;
		}

		/// Alias for `ysize()`
		/// # Returns
		/// Returns the size in the Y dimension
		pub fn height(&self) -> usize {
			return self.ysize();
		}

		/// Gets the Z-dimension size (aka depth) of this 3D array
		/// # Returns
		/// Returns the size in the Z dimension
		pub fn zsize(&self) -> usize {
			return self.zsize;
		}

		/// Alias for `zsize()`
		/// # Returns
		/// Returns the size in the Z dimension
		pub fn depth(&self) -> usize {
			return self.zsize();
		}


		/// Gets a value from the 3D array, or returns a *LookUpError* if the provided coordinate
		/// is out of bounds. If you are using a default value for out-of-bounds coordinates,
		/// then you should use the *bounded_get(x, y, z)* method instead. If you want access to
		/// wrap-around (eg (-2, 0, 1) equivalent to (width-2, 0, 1)), then use the
		/// *wrapped_get(x, y, z)* method.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// # Returns
		/// Returns a Result type that holds either the returned data value (as a reference) from
		/// the 3D array, or a *LookUpError* signalling that the coordinate is out of bounds
		pub fn get(&self, x: usize, y: usize, z: usize) -> Result<&T,LookUpError>{
			if x < self.xsize && y < self.ysize && z < self.zsize {
				Ok(self.patches[patch_index(x, y, z, self.pxsize, self.pysize)].get(x, y, z))
			} else {
				Err(LookUpError{coord: vec![x, y, z],
					bounds: vec![self.xsize, self.ysize, self.zsize]})
			}
		}

		/// Sets a value in the 3D array, or returns a *LookUpError* if the provided coordinate
		/// is out of bounds. If you want out-of-bound coordinates to result in a no-op, then use
		/// the *bounded_set(x, y, z, val)* method instead. If you want access to wrap-around (eg
		/// (-2, 0, 1) equivalent to (width-2, 0, 1)), then use the
		/// *wrapped_set(x, y, z, val)* method.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// * **new_val** - value to store in the 3D array at (x, y, z)
		/// # Returns
		/// Returns a Result type that is either empty or a *LookUpError* signalling that the
		/// coordinate is out of bounds
		pub fn set(&mut self, x: usize, y: usize, z: usize, new_val: T) -> Result<(),LookUpError>{
			if x < self.xsize && y < self.ysize && z < self.zsize {
				Ok(self.patches[patch_index(x, y, z, self.pxsize, self.pysize)]
					.set(x, y, z, new_val))
			} else {
				Err(LookUpError{coord: vec![x, y, z],
					bounds: vec![self.xsize, self.ysize, self.zsize]})
			}
		}

		/// Gets a value from the 3D array without bounds checking
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// # Returns
		/// Returns the data value (as a reference) from the 3D array
		pub fn get_unchecked(&self, x: usize, y: usize, z: usize) -> &T {
			return self.patches[patch_index(x, y, z, self.pxsize, self.pysize)].get(x, y, z);
		}

		pub fn get_unchecked_mut(&mut self, x: usize, y: usize, z: usize) -> &mut T {
			return self.patches[patch_index(x, y, z, self.pxsize, self.pysize)].get_mut(x, y, z);
		}

		/// Sets a value in the 3D array without bounds checking
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// * **new_val** - value to store in the 3D array at (x, y, z)
		pub fn set_unchecked(&mut self, x: usize, y: usize, z: usize, new_val: T) {
			self.patches[patch_index(x, y, z, self.pxsize, self.pysize)]
					.set(x, y, z, new_val);
		}

		/// Gets a value from the 3D array, wrapping around the X and Y axese when the coordinates
		/// are negative or outside the size of this 2D array. Good for when you want tiling
		/// behavior.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// # Returns
		/// Returns a reference to the data stored at the provided coordinate (wrapping both x
		/// and y dimensions)
		pub fn wrapped_get(&self, x: isize, y: isize, z: isize) -> &T{
			let x = (self.xsize as isize + (x % self.xsize as isize)) as usize % self.xsize;
			let y = (self.ysize as isize + (y % self.ysize as isize)) as usize % self.ysize;
			let z = (self.zsize as isize + (z % self.zsize as isize)) as usize % self.zsize;
			return &self.patches[patch_index(x, y, z, self.pxsize, self.pysize)].get(x, y, z);
		}

		/// Sets a value in the 3D array at the provided coordinate, wrapping the X, Y, and Z axese
		/// if the coordinate is negative or out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// * **new_val** - value to store in the 3D array at (x, y, z), wrapping around
		/// the x, y, and z dimensions
		pub fn wrapped_set(&mut self, x: isize, y: isize, z: isize, new_val: T) {
			let x = (self.xsize as isize + (x % self.xsize as isize)) as usize % self.xsize;
			let y = (self.ysize as isize + (y % self.ysize as isize)) as usize % self.ysize;
			let z = (self.zsize as isize + (z % self.zsize as isize)) as usize % self.zsize;
			self.patches[patch_index(x, y, z, self.pxsize, self.pysize)].set(x, y, z, new_val);
		}

		/// Gets a value from the 3D array as an Option that is None if the coordinate
		/// is out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// # Returns
		/// Returns an Option type that holds either the returned data value (as a reference) from
		/// the 3D array, or *None* signalling that the coordinate is out of bounds (which can be
		/// combined with .unwrap_or(default_value) to implement an out-of-bounds default)
		pub fn bounded_get(&self, x: isize, y: isize, z: isize) -> Option<&T>{
			if x >= 0 && y >= 0 && z >= 0
				&& x < self.xsize as isize && y < self.ysize as isize && z < self.zsize as isize {
				return Some(&self.patches[
					patch_index(x as usize, y as usize, z as usize, self.pxsize, self.pysize)]
					.get(x as usize, y as usize, z as usize));
			} else {
				return None;
			}
		}

		/// Sets a value in the 3D array if and only if the provided coordinate is in bounds.
		/// Otherwise this method does nothing if the coordinate is out of bounds.
		/// # Parameters
		/// * **x** - x dimension coordinate
		/// * **y** - y dimension coordinate
		/// * **z** - z dimension coordinate
		/// * **new_val** - value to store int eh zD array at (x, y, z)
		pub fn bounded_set(&mut self, x: isize, y: isize, z: isize, new_val: T) {
			if x >= 0 && y >= 0 && z >= 0
				&& x < self.xsize as isize && y < self.ysize as isize && z < self.zsize as isize {
				self.patches[
					patch_index(x as usize, y as usize, z as usize, self.pxsize, self.pysize)]
					.set(x as usize, y as usize, z as usize, new_val);
			} else {
				// no-op
			}
		}

		/// Fills a region of this 3D array with a given value, or returns a *LookUpError* if the
		/// provided coordinates go out of bounds. If you just want to ignore any
		/// out-of-bounds coordinates, then you should use the
		/// *bounded_fill(x1, y1, z1, x2, y2, z2)*
		/// method instead. If you want access to wrap-around (eg (-2, 0, 1) equivalent to
		/// (width-2, 0, 1)), then use the *wrapped_fill(x, y, z)* method.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **z1** - the first z dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **z2** - the second z dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 2D array in the bounding box defined by
		/// (x1, y1, z1) -> (x2, y2, z2)
		/// # Returns
		/// Returns a Result type that is either empty or a *LookUpError* signalling that a
		/// coordinate is out of bounds
		pub fn fill(&mut self, x1: usize, y1: usize, z1: usize, x2: usize, y2: usize, z2: usize,
					new_val: T)
					-> Result<(), LookUpError> {
			for y in y1..y2{ for x in x1..x2{ for z in z1..z2{
				self.set(x, y, z, new_val)?;
			} } }
			Ok(())
		}

		/// Fills a region of this 3D array with a given value, wrapping the axese when
		/// coordinates go out of bounds.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **z1** - the first z dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **z2** - the second z dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 3D array in the bounding box defined by
		/// (x1, y1, z1) -> (x2, y2, z2)
		pub fn wrapped_fill(&mut self, x1: isize, y1: isize, z1: isize,
							x2: isize, y2: isize, z2: isize, new_val: T) {
			for y in y1..y2{ for x in x1..x2{ for z in z1..z2{
				self.wrapped_set(x, y, z, new_val);
			} } }
		}

		/// Fills a region of this 3D array with a given value, ignoring any
		/// coordinates that go out of bounds.
		/// # Parameters
		/// * **x1** - the first x dimension coordinate (inclusive)
		/// * **y1** - the first y dimension coordinate (inclusive)
		/// * **z1** - the first z dimension coordinate (inclusive)
		/// * **x2** - the second x dimension coordinate (exclusive)
		/// * **y2** - the second y dimension coordinate (exclusive)
		/// * **z2** - the second z dimension coordinate (exclusive)
		/// * **new_val** - value to store in the 3D array in the bounding box defined by
		/// (x1, y1, z1) -> (x2, y2, z2)
		pub fn bounded_fill(&mut self, x1: isize, y1: isize, z1: isize,
							x2: isize, y2: isize, z2: isize, new_val: T) {
			for y in y1..y2{ for x in x1..x2{ for z in z1..z2{
				self.bounded_set(x, y, z, new_val);
			} } }
		}
	}

	/// Used for converting 3D coords to linear Z-index
	static ZLUT: [u16; 16] = [
		0b0000000000000000,
		0b0000000000000001,
		0b0000000000001000,
		0b0000000000001001,
		0b0000000001000000,
		0b0000000001000001,
		0b0000000001001000,
		0b0000000001001001,
		0b0000001000000000,
		0b0000001000000001,
		0b0000001000001000,
		0b0000001000001001,
		0b0000001001000000,
		0b0000001001000001,
		0b0000001001001000,
		0b0000001001001001
	];

	/// General purpose Z-index function to convert a three-dimensional coordinate into a localized
	/// one-dimensional coordinate
	/// # Parameters
	/// * **x** - x dimension coordinate *(ONLY THE LOWER 4 BITS WILL BE USED!)*
	/// * **y** - y dimension coordinate *(ONLY THE LOWER 4 BITS WILL BE USED!)*
	/// * **z** - z dimension coordinate *(ONLY THE LOWER 4 BITS WILL BE USED!)*
	/// # Returns
	/// Z-curve index for use as an index in a linear array meant to hold 2D data. In other words,
	/// given the binary numbers X=0b0000xxxx, Y=0b0000yyyy, and Z=0b0000zzzz, then this method
	/// will return 0b0000zyxzyxzyxzyx.
	pub fn zorder_4bit_to_12bit(x: u8, y: u8, z: u8) -> u16 {
		let x_bits = ZLUT[(x & 0x0F) as usize];
		let y_bits = ZLUT[(y & 0x0F) as usize] << 1;
		let z_bits = ZLUT[(z & 0x0F) as usize] << 2;
		return z_bits | y_bits | x_bits;
	}
	/// General purpose Z-index function to convert a three-dimensional coordinate into a localized
	/// one-dimensional coordinate
	/// # Parameters
	/// * **x** - x dimension coordinate (8 bit)
	/// * **y** - y dimension coordinate (8 bit)
	/// * **z** - z dimension coordinate (8 bit)
	/// # Returns
	/// Z-curve index for use as an index in a linear array meant to hold 2D data. In other words,
	/// given the binary numbers X=0b0000xxxx, Y=0b0000yyyy, and Z=0b0000zzzz, then this method
	/// will return 0b0000zyxzyxzyxzyx.
	pub fn zorder_8bit_to_24bit(x:u8, y:u8, z: u8) -> u32 {
		return ((zorder_4bit_to_12bit(x >> 4, y >> 4, z >> 4) as u32) << 12)
			| zorder_4bit_to_12bit(x, y, z) as u32
	}

}


#[cfg(test)]
mod tests {
	use super::z2d::ZArray2D;
	use super::z3d::ZArray3D;
	use rand::{rngs::StdRng, Rng, SeedableRng};


	fn seed_arrays_u8(w: usize, h: usize) -> (Vec<Vec<u8>>, ZArray2D<u8>){
		let ref_map: Vec<Vec<u8>> = vec![vec![0u8;w];h];
		let map = ZArray2D::new(w, h, 0u8);
		return (ref_map, map);
	}

	#[test]
	fn test_zarray2dmap_get_set(){
		let h: usize = 601;
		let w: usize = 809;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// assert get sizes
		assert_eq!(map.dimensions().0, w);
		assert_eq!(map.dimensions().1, h);
		assert_eq!(map.xsize(), w);
		assert_eq!(map.width(), w);
		assert_eq!(map.ysize(), h);
		assert_eq!(map.height(), h);
		// set values
		for y in 0..h {
			for x in 0..w {
				let v: u8 = prng.gen();
				ref_map[y][x] = v;
				map.set(x, y, v).unwrap();
			}
		}
		// get values
		for y in 0..h {
			for x in 0..w {
				assert_eq!(ref_map[y][x], *map.get(x, y).unwrap());
			}
		}
	}

	#[test]
	fn test_zarray2dmap_wrapped_get_set(){
		let h: usize = 20;
		let w: usize = 20;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in -10..10 as isize{
			for x in -10..10 as isize {
				let v: u8 = prng.gen();
				ref_map[((20+y%20)%20) as usize][((20+x%20)%20) as usize] = v;
				map.wrapped_set(x, y, v);
			}
		}
		let m: isize = 101;
		let v: u8 = prng.gen();
		ref_map[((20+m%20)%20) as usize][((20+(3*m)%20)%20) as usize] = v;
		map.wrapped_set(3*m, m, v);
		// get values
		for y in 0..h {
			for x in 0..w {
				assert_eq!(ref_map[y][x], *map.get(x, y).unwrap());
			}
		}
	}


	#[test]
	fn test_zarray2dmap_bounded_get_set(){
		let h: usize = 20;
		let w: usize = 20;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in -10..10 as isize{
			for x in -10..10 as isize {
				let v: u8 = prng.gen();
				if x >= 0 && x < w as isize && y >= 0 && y < h as isize {
					ref_map[y as usize][x as usize] = v;
				}
				map.bounded_set(x, y, v);
			}
		}
		let oob: u8 = 127;
		let m: isize = 101;
		let v: u8 = prng.gen();
		map.bounded_set(3*m, m, v); // should be a no-op
		// get values
		for y in 0..h {
			for x in 0..w {
				assert_eq!(ref_map[y][x], *map.bounded_get(x as isize, y as isize).unwrap_or(&oob));
			}
		}
		assert_eq!(oob, *map.bounded_get(-1, 0).unwrap_or(&oob));
		assert_eq!(oob, *map.bounded_get(0,  -1).unwrap_or(&oob));
		assert_eq!(oob, *map.bounded_get(w as isize,  h as isize).unwrap_or(&oob));
	}

	#[test]
	fn test_zarray2dmap_power_of_8(){
		let h: usize = 64;
		let w: usize = 64;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in 0..h {
			for x in 0..w {
				let v: u8 = prng.gen();
				ref_map[y][x] = v;
				map.set(x, y, v).unwrap();
			}
		}
		// get values
		for y in 0..h {
			for x in 0..w {
				assert_eq!(ref_map[y][x], *map.get(x, y).unwrap());
			}
		}
	}

	#[test]
	fn test_zarray2dmap_small(){
		let h: usize = 3;
		let w: usize = 5;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in 0..h {
			for x in 0..w {
				let v: u8 = prng.gen();
				ref_map[y][x] = v;
				map.set(x, y, v).unwrap();
			}
		}
		// get values
		for y in 0..h {
			for x in 0..w {
				assert_eq!(ref_map[y][x], *map.get(x, y).unwrap());
			}
		}
	}

	#[test]
	fn test_zarray2dmap_performance_neighborsum(){
		use std::time::Instant;
		let h: usize = 300;
		let w: usize = 300;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in 0..h {
			for x in 0..w {
				let v: u8 = prng.gen();
				ref_map[y][x] = v;
				map.set(x, y, v).unwrap();
			}
		}
		// sum neighbors values with benchmark reference (vecs)
		let mut ref_map_sums: Vec<Vec<u16>> = vec![vec![0u16;w];h];
		let radius: usize = 2;
		let rad_plus = radius * 2 + 1;
		let t0 = Instant::now();
		for y in radius..h-radius {
			for x in radius..w-radius {
				let mut sum = 0;
				for ry in 0..rad_plus as i32 {
					let dy = ry - radius as i32;
					for rx in 0..rad_plus as i32 {
						let dx = rx - radius as i32;
						sum += ref_map[(y as i32+dy) as usize][(x as i32+dx) as usize] as u16;
					}
				}
				ref_map_sums[y][x] = sum;
			}
		}
		let t1 = Instant::now();
		let ref_time =  (t1-t0).as_secs_f64()*1e6;
		println!("Vec<Vec<u16>> {}x{} sum of neighbors in radius {} performance: {} micros", w, h,
				 radius, ref_time as i32);

		// sum neighbors values with ZArray
		let mut map_sums = ZArray2D::new(w, h, 0u16);
		let t0 = Instant::now();
		for y in radius..h-radius {
			for x in radius..w-radius {
				let mut sum = 0;
				for ry in 0..rad_plus as i32 {
					let dy = ry - radius as i32;
					for rx in 0..rad_plus as i32 {
						let dx = rx - radius as i32;
						sum += *map.get((x as i32+dx) as usize, (y as i32+dy) as usize).unwrap() as u16;
					}
				}
				map_sums.set(x, y, sum).unwrap();
			}
		}
		let t1 = Instant::now();
		let my_time = (t1-t0).as_secs_f64()*1e6;
		println!("ZArray2D {}x{} sum of neighbors in radius {} performance: {} micros", w, h,
			radius, my_time as i32);
		println!("Performance improved by {}%", (100. * (ref_time / my_time - 1.)) as i32);
	}

	#[test]
	fn test_zarray2dmap_performance_pathfinding(){
		use std::time::Instant;
		use pathfinding::prelude::{absdiff, astar};
		let h: usize = 300;
		let w: usize = 300;
		let (mut ref_map, mut map) = seed_arrays_u8(w, h);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for y in 0..h {
			for x in 0..w {
				let v: u8 = prng.gen();
				ref_map[y][x] = v;
				map.set(x, y, v).unwrap();
			}
		}
		// A* pathfinding with benchmark reference (vecs)
		let oob: u8 = 127;
		let goal: (i32, i32) = (w as i32 - 1, h as i32 - 1);
		let start: (i32, i32) = (1, 1);
		let t0 = Instant::now();
		let result = astar(
			&start,
			|&(x, y)| vec![
							(x+1,y), (x-1,y), (x,y+1), (x,y-1)
					].into_iter().map(|p:(i32, i32)| (p,
						if p.0 >= 0 && p.1 >= 0 && p.0 < w as i32 && p.1 < h as i32 {
							ref_map[p.1 as usize][p.0 as usize] as i32} else {oob as i32})),
			|&(x, y)| absdiff(x, goal.0) + absdiff(y, goal.1),
			|&p| p == goal
		);
		let (ref_path, ref_cost) = result.unwrap();
		let t1 = Instant::now();
		let ref_time =  (t1-t0).as_secs_f64()*1e6;
		println!("Vec<Vec<u16>> {}x{} A* path from ({},{}) to ({},{}) (path length = {}, cost = \
		{}) performance: {} micros",
				 w, h, start.0, start.1, goal.0, goal.1, ref_path.len(), ref_cost, ref_time);

		// A* pathfinding with ZArray
		let t0 = Instant::now();
		let result = astar(
			&start,
			|&(x, y)| vec![
				(x+1,y), (x-1,y), (x,y+1), (x,y-1)
			].into_iter().map(|p:(i32, i32)| (p, *map.bounded_get(p.0 as isize, p.1 as isize )
				.unwrap_or(&oob) as i32)),
			|&(x, y)| absdiff(x, goal.0) + absdiff(y, goal.1),
			|&p| p == goal
		);
		let (my_path, my_cost) = result.unwrap();
		let t1 = Instant::now();
		let my_time =  (t1-t0).as_secs_f64()*1e6;
		println!("ZArray2D {}x{} A* path from ({},{}) to ({},{}) (path length = {}, cost = \
		{}) performance: {} micros",
				 w, h, start.0, start.1, goal.0, goal.1, my_path.len(), my_cost, my_time);
		assert_eq!(ref_path.len(), my_path.len());
		assert_eq!(ref_cost, my_cost);
		println!("Performance improved by {}%", (100. * (ref_time / my_time - 1.)) as i32);

	}

	fn seed_3darrays_u8(w: usize, h: usize, d: usize) -> (Vec<Vec<Vec<u8>>>, ZArray3D<u8>){
		let ref_map: Vec<Vec<Vec<u8>>> = vec![vec![vec![0u8;w];h];d];
		let map = ZArray3D::new(w, h, d, 0u8);
		return (ref_map, map);
	}

	#[test]
	fn test_zarray3dmap_get_set(){
		let h: usize = 11;
		let w: usize = 39;
		let d: usize = 23;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// assert get sizes
		assert_eq!(map.dimensions().0, w);
		assert_eq!(map.dimensions().1, h);
		assert_eq!(map.dimensions().2, d);
		assert_eq!(map.xsize(), w);
		assert_eq!(map.width(), w);
		assert_eq!(map.ysize(), h);
		assert_eq!(map.height(), h);
		assert_eq!(map.zsize(), d);
		assert_eq!(map.depth(), d);
		// set values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					let v: u8 = prng.gen();
					ref_map[z][y][x] = v;
					map.set(x, y, z, v).unwrap();
				}
			}
		}
		// get values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					assert_eq!(ref_map[z][y][x], *map.get(x, y, z).unwrap());
				}
			}
		}
	}

	#[test]
	fn test_zarray3dmap_wrapped_get_set(){
		let h: usize = 20;
		let w: usize = 20;
		let d: usize = 20;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for z in -10..10 as isize {
			for y in -10..10 as isize {
				for x in -10..10 as isize {
					let v: u8 = prng.gen();
					ref_map[((20 + z % 20) % 20) as usize][((20 + y % 20) % 20) as usize][((20 + x % 20) % 20) as usize]
						= v;
					map.wrapped_set(x, y, z, v);
				}
			}
		}
		let m: isize = 101;
		let v: u8 = prng.gen();
		ref_map[((20+(m/2)%20)%20) as usize][((20+m%20)%20) as usize]
			[((20+(3*m)%20)%20) as usize] = v;
		map.wrapped_set(3*m, m, m/2, v);
		// get values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					assert_eq!(ref_map[z][y][x], *map.get(x, y, z).unwrap());
				}
			}
		}
	}


	#[test]
	fn test_zarray3dmap_bounded_get_set(){
		let h: usize = 20;
		let w: usize = 20;
		let d: usize = 20;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for z in -10..10 as isize {
			for y in -10..10 as isize {
				for x in -10..10 as isize {
					let v: u8 = prng.gen();
					if x >= 0 && x < w as isize && y >= 0 && y < h as isize
						&& z >= 0 && z < d as isize{
						ref_map[z as usize][y as usize][x as usize] = v;
					}
					map.bounded_set(x, y, z, v);
				}
			}
		}
		let oob: u8 = 127;
		let m: isize = 101;
		let v: u8 = prng.gen();
		map.bounded_set(3*m, m, m/2, v); // should be a no-op
		// get values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					assert_eq!(ref_map[z][y][x],
							   *map.bounded_get(x as isize, y as isize, z as isize)
								   .unwrap_or(&oob));
				}
			}
		}
		assert_eq!(oob, *map.bounded_get(-1, 0, 0).unwrap_or(&oob));
		assert_eq!(oob, *map.bounded_get(0,  -1, 0).unwrap_or(&oob));
		assert_eq!(oob, *map.bounded_get(0,  0, -1).unwrap_or(&oob));
		assert_eq!(oob, *map.bounded_get(w as isize,  h as isize, d as isize).unwrap_or(&oob));
	}

	#[test]
	fn test_zarray3dmap_power_of_8(){
		let h: usize = 8;
		let w: usize = 8;
		let d: usize = 8;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					let v: u8 = prng.gen();
					ref_map[z][y][x] = v;
					map.set(x, y, z, v).unwrap();
				}
			}
		}
		// get values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					assert_eq!(ref_map[z][y][x], *map.get(x, y, z).unwrap());
				}
			}
		}
	}

	#[test]
	fn test_zarray3dmap_small(){
		let h: usize = 3;
		let w: usize = 2;
		let d: usize = 2;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					let v: u8 = prng.gen();
					ref_map[z][y][x] = v;
					map.set(x, y, z, v).unwrap();
				}
			}
		}
		// get values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					assert_eq!(ref_map[z][y][x], *map.get(x, y, z).unwrap());
				}
			}
		}
	}

	#[test]
	fn test_zarray3dmap_performance_neighborsum(){
		use std::time::Instant;
		let h: usize = 20;
		let w: usize = 10;
		let d: usize = 30;
		let (mut ref_map, mut map) = seed_3darrays_u8(w, h, d);
		let mut prng = StdRng::seed_from_u64(20220331u64);
		// set values
		for z in 0..d {
			for y in 0..h {
				for x in 0..w {
					let v: u8 = prng.gen();
					ref_map[z][y][x] = v;
					map.set(x, y, z, v).unwrap();
				}
			}
		}
		// sum neighbors values with benchmark reference (vecs)
		let mut ref_map_sums: Vec<Vec<Vec<u32>>> = vec![vec![vec![0u32;w];h];d];
		let radius: usize = 2;
		let rad_plus = radius * 2 + 1;
		let t0 = Instant::now();
		for z in radius..d - radius {
			for y in radius..h - radius {
				for x in radius..w - radius {
					let mut sum = 0;
					for rz in 0..rad_plus as i32 {
						let dz = rz - radius as i32;
						for ry in 0..rad_plus as i32 {
							let dy = ry - radius as i32;
							for rx in 0..rad_plus as i32 {
								let dx = rx - radius as i32;
								sum += ref_map[(z as i32 + dz) as usize][(y as i32 + dy) as usize]
									[(x as i32 + dx) as usize] as u32;
							}
						}
					}
					ref_map_sums[z][y][x] = sum;
				}
			}
		}
		let t1 = Instant::now();
		let ref_time =  (t1-t0).as_secs_f64()*1e6;
		println!("Vec<Vec<u16>> {}x{}x{} sum of neighbors in radius {} performance: {} micros",
			w, h, d, radius, ref_time as i32);

		// sum neighbors values with ZArray
		let mut map_sums = ZArray3D::new(w, h, d, 0u32);
		let t0 = Instant::now();
		for z in radius..d - radius {
			for y in radius..h - radius {
				for x in radius..w - radius {
					let mut sum = 0;
					for rz in 0..rad_plus as i32 {
						let dz = rz - radius as i32;
						for ry in 0..rad_plus as i32 {
							let dy = ry - radius as i32;
							for rx in 0..rad_plus as i32 {
								let dx = rx - radius as i32;
								sum += *map.get((x as i32+dx) as usize, (y as i32+dy) as usize,
								(z as i32+dz) as usize).unwrap() as u32;
							}
						}
					}
					map_sums.set(x, y, z, sum).unwrap();
				}
			}
		}
		let t1 = Instant::now();
		let my_time = (t1-t0).as_secs_f64()*1e6;
		println!("ZArray2D {}x{}x{} sum of neighbors in radius {} performance: {} micros", w, h, d,
				 radius, my_time as i32);
		println!("Performance improved by {}%", (100. * (ref_time / my_time - 1.)) as i32);
	}

	#[test]
	fn test_erosion_sim(){
		let width = 100;
		let length = 200;
		let depth = 25;
		let air = 0f32;
		let soil_hardness = 1f32;
		let rock_hardness = 8f32;
		let drip_power = 1.5f32;
		let iterations = 12;
		let mut map = ZArray3D::new(width, length, depth, air);
		map.fill(0,0,5, width,length,depth, soil_hardness).unwrap();
		map.fill(0,0,15, width,length,depth, rock_hardness).unwrap();
		for boulder in [(34,88,6), (66,122,9), (11,154,5), (35,93,8), (72,75,12)]{
			map.set(boulder.0, boulder.1, boulder.2, rock_hardness).unwrap();
		}
		for _ in 0..iterations{
			for x in 0..width{for y in 0..length{
				let mut drip = drip_power;
				let mut z = 0;
				while drip > 0f32 {
					let h = *map.bounded_get(x as isize, y as isize, z).unwrap_or(&100f32);
					if h > drip {
						map.bounded_set(x as isize, y as isize, z, h - drip);
						drip = 0.;
					} else {
						map.bounded_set(x as isize, y as isize, z, 0.);
						drip -= h;
					}
					z += 1;
				}
			}}
		}
	}
}
