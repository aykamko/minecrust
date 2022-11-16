// Source: https://docs.rs/blend-srgb/latest/src/blend_srgb/convert.rs.html
#[inline]
pub fn rgb_to_srgb(v: f64) -> f64 {
	if v < 0.0031308 {
		v * 12.9232102
	} else {
		const GAMMA: f64 = 1.0 / 2.4;
		1.055 * v.powf(GAMMA) - 0.055
	}
}

// Source: https://docs.rs/blend-srgb/latest/src/blend_srgb/convert.rs.html
#[inline]
pub fn srgb_to_rgb(v: f64) -> f64 {
	if v < 0.0404599 {
		v / 12.9232102
	} else {
		((v + 0.055) / 1.055).powf(2.4)
	}
}