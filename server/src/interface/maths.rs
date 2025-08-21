// Resample a u64 series to exactly `width` points for visual stretching in sparklines
pub fn resample_to_width_u64(values: &[u64], width: usize) -> Vec<u64> {
    if width == 0 {
        return Vec::new();
    }
    if values.is_empty() {
        return vec![0; width];
    }
    if values.len() == 1 {
        return vec![values[0]; width];
    }
    let src_len = values.len();
    let dst_len = width;
    let mut out = Vec::with_capacity(dst_len);
    let denom = (dst_len - 1) as f64;
    let src_max = (src_len - 1) as f64;
    for i in 0..dst_len {
        let pos = if denom == 0.0 {
            0.0
        } else {
            (i as f64) * (src_max / denom)
        };
        let idx = pos.round().clamp(0.0, src_max) as usize;
        out.push(values[idx]);
    }
    out
}
