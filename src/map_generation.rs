use bmp::{Image, Pixel};

use crate::world::WORLD_XZ_SIZE;
use noise::{NoiseFn, Perlin, Seedable};

const BASE_FREQUENCY: f64 = 5.0;
const NUM_OCTAVES: usize = 4;

type ElevationMap = [[u16; WORLD_XZ_SIZE]; WORLD_XZ_SIZE];

// Source: https://www.redblobgames.com/maps/terrain-from-noise/
pub fn generate_elevation_map(min_elevation: u16, max_elevation: u16) -> ElevationMap {
    let mut elevation_map_f64 = [[0.0_f64; WORLD_XZ_SIZE]; WORLD_XZ_SIZE];

    let max_height = max_elevation - min_elevation;

    let noise = noise::OpenSimplex::new();
    for (x, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_XZ_SIZE) {
        let nx: f64 = ((x as f64) / (WORLD_XZ_SIZE as f64)) * BASE_FREQUENCY;
        let nz: f64 = ((z as f64) / (WORLD_XZ_SIZE as f64)) * BASE_FREQUENCY;

        let mut elevation = 0.0_f64;
        let mut sum_of_amplitudes = 0.0_f64;

        for i in 0..NUM_OCTAVES {
            let octave = i32::pow(2, i as u32) as f64;
            let amplitude = 1.0 / octave;

            // Normalize [-1.0, 1.0] to [0.0, 1.0]
            let noise_normalized = (noise.get([octave * nx, octave * nz]) + 1.0) / 2.0;
            elevation += amplitude * noise_normalized;
            sum_of_amplitudes += amplitude;
        }

        elevation /= sum_of_amplitudes;
        elevation = f64::powf(elevation, 1.4);
        elevation_map_f64[x][z] = elevation;
    }

    let mut elevation_map_out: ElevationMap = [[0_u16; WORLD_XZ_SIZE]; WORLD_XZ_SIZE];
    for (x, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_XZ_SIZE) {
        elevation_map_out[x][z] = (elevation_map_f64[x][z] * max_height as f64).floor() as u16 - min_elevation;
    }

    elevation_map_out
}

pub fn save_elevation_to_file(elevation_map: ElevationMap, filepath: &str) {
    let mut img = Image::new(WORLD_XZ_SIZE as u32, WORLD_XZ_SIZE as u32);

    for (x, z) in img.coordinates() {
        let brightness = elevation_map[x as usize][z as usize];
        img.set_pixel(x, z, px!(brightness, brightness, brightness))
    }
    
    let _ = img.save(filepath);
}