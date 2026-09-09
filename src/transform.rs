use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::IMAGE_LEN_LIMIT;

/// ## Description:
/// Apply the linear transform.
///
/// `pixel_len` is how many element `T` in 1 pixel.
///
/// `return None` only if the transformed image len exceed `IMAGE_LEN_LIMIT`
///
/// ## Example:
/// ```rust
/// let (new_buf, new_width, new_height) = linear_transform(&buf, width, height, pixel_len, transformation_matrix)?;
/// ```
pub fn linear_transform<T>(
    buf: &[T],
    width: u32,
    height: u32,
    pixel_len: u32,
    mat: [[f64; 2]; 2],
) -> Option<(Vec<T>, u32, u32)>
where
    T: Default + Clone + Send + std::marker::Sync,
{
    /// run single threaded if true
    const SINGLE_THREADED: bool = false;

    // calculate new size, through the 4 corners
    // 0, (H-1) -> -(H-1)/2, (H-1)/2
    // 0, (W-1) -> -(W-1)/2, (W-1)/2
    // half width half height is to center the image
    let half_width = (width - 1) as f64 / 2.0;
    let half_height = (height - 1) as f64 / 2.0;

    // 1 2
    // 3 4
    let x1 = -half_width * mat[0][0] + half_height * mat[0][1];
    let x2 = half_width * mat[0][0] + half_height * mat[0][1];
    let x3 = -half_width * mat[0][0] - half_height * mat[0][1];
    let x4 = half_width * mat[0][0] - half_height * mat[0][1];

    let y1 = -half_width * mat[1][0] + half_height * mat[1][1];
    let y2 = half_width * mat[1][0] + half_height * mat[1][1];
    let y3 = -half_width * mat[1][0] - half_height * mat[1][1];
    let y4 = half_width * mat[1][0] - half_height * mat[1][1];

    let xmax = x1.max(x2).max(x3).max(x4);
    let xmin = x1.min(x2).min(x3).min(x4);
    let ymax = y1.max(y2).max(y3).max(y4);
    let ymin = y1.min(y2).min(y3).min(y4);

    let new_width = (xmax - xmin + 1.0) as u32;
    let new_height = (ymax - ymin + 1.0) as u32;
    let new_len = new_height as usize * new_width as usize * pixel_len as usize;

    if new_len * size_of::<T>() > IMAGE_LEN_LIMIT {
        return None;
    }

    let mut new_buf = vec![T::default(); new_len];

    let inverse_mat = {
        let det = mat[0][0] * mat[1][1] - mat[0][1] * mat[1][0];

        if det == 0.0 {
            return Some((new_buf, new_width, new_height));
        }

        let inv = [
            [mat[1][1] / det, -mat[0][1] / det],
            [-mat[1][0] / det, mat[0][0] / det],
        ];
        [
            [
                (inv[0][0] * 2f64.powi(32)) as i64,
                (inv[0][1] * 2f64.powi(32)) as i64,
            ],
            [
                (inv[1][0] * 2f64.powi(32)) as i64,
                (inv[1][1] * 2f64.powi(32)) as i64,
            ],
        ]
    };

    // We're using inverse mapping and incremental stepping here
    // Fixed-point numbers are 32x32 -- don't over complicate it, it's just an i64 multiply by 2^32

    // Starting from our top-left corner
    // This is simply inverse_mat * [xmin, ymax]

    let base_src_x = xmin as i64 * inverse_mat[0][0]
        + ymax as i64 * inverse_mat[0][1]
        + (half_width * 2f64.powi(32)) as i64;

    // NOTICE: y is inverted vertically
    // The Euclidean space assume y going **upwards**,
    // whereas our image has y going **downwards** so invert y is needed here
    // -- you'll notice that y consistently having the opposite sign to x in the code.
    let base_src_y = -xmin as i64 * inverse_mat[1][0] - ymax as i64 * inverse_mat[1][1]
        + (half_height * 2f64.powi(32)) as i64;

    let transform_line = |(i, dst_line): (usize, &mut [T])| {
        // for (i, dst_line) in new_buf.chunks_exact_mut(new_width * pixel_len).enumerate()

        let src_x = base_src_x - i as i64 * inverse_mat[0][1];
        let src_y = base_src_y + i as i64 * inverse_mat[1][1];

        // This is just a result from solving equations from the brute-force loop
        let (start_x, end_x) = if inverse_mat[0][0] > 0 {
            (
                -src_x / inverse_mat[0][0],
                (((width as i64) << 32) - src_x) / inverse_mat[0][0],
            )
        } else if inverse_mat[0][0] < 0 {
            (
                (((width as i64) << 32) - src_x) / inverse_mat[0][0],
                -src_x / inverse_mat[0][0],
            )
        } else {
            if src_x >= 0 && src_x < ((width as i64) << 32) {
                (0, new_width as i64)
            } else {
                (new_width as i64, 0)
            }
        };
        let (start_y, end_y) = if inverse_mat[1][0] > 0 {
            (
                (src_y - ((height as i64) << 32)) / inverse_mat[1][0],
                src_y / inverse_mat[1][0],
            )
        } else if inverse_mat[1][0] < 0 {
            (
                src_y / inverse_mat[1][0],
                (src_y - ((height as i64) << 32)) / inverse_mat[1][0],
            )
        } else {
            if src_y >= 0 && src_y < ((width as i64) << 32) {
                (0, new_width as i64)
            } else {
                (new_width as i64, 0)
            }
        };

        // `start` = first index where both x and y are valid
        // `end` = first index where either x or y invalid after start
        let mut start = start_x.max(start_y).clamp(0, new_width as i64);
        let mut end = end_x.min(end_y).clamp(0, new_width as i64);

        // `start` and `end` might be off by 1
        // calculations like `src_y / inverse_mat[1][0]`,
        // do a `floor` on `start` -- `start` supposed to be `ceil` instead
        // So we should run the brute-force loop
        // this would strongly ensure that `start` and `end` are valid
        while start < end {
            let x = start * inverse_mat[0][0] + src_x;
            let y = src_y - start * inverse_mat[1][0];
            if x >= 0 && x < ((width as i64) << 32) && y >= 0 && y < ((height as i64) << 32) {
                break;
            }
            start += 1;
        }
        while start < end {
            let x = (end - 1) * inverse_mat[0][0] + src_x;
            let y = src_y - (end - 1) * inverse_mat[1][0];
            if x >= 0 && x < ((width as i64) << 32) && y >= 0 && y < ((height as i64) << 32) {
                break;
            }
            end -= 1;
        }

        if start < end {
            let mut x = start * inverse_mat[0][0] + src_x;
            let mut y = src_y - start * inverse_mat[1][0];

            let start = start as usize * pixel_len as usize;
            let end = end as usize * pixel_len as usize;

            for dst in dst_line[start..end].chunks_exact_mut(pixel_len as usize) {
                let pixel = (y >> 32) as usize * width as usize * pixel_len as usize
                    + (x >> 32) as usize * pixel_len as usize;

                for (src, dst) in buf[pixel..pixel + pixel_len as usize].iter().zip(dst) {
                    *dst = src.clone();
                }

                x += inverse_mat[0][0];
                y -= inverse_mat[1][0];
            }
        }
    };

    if SINGLE_THREADED {
        new_buf
            .chunks_exact_mut(new_width as usize * pixel_len as usize)
            .enumerate()
            .for_each(transform_line);
    } else {
        new_buf
            .par_chunks_exact_mut(new_width as usize * pixel_len as usize)
            .enumerate()
            .for_each(transform_line);
    }

    return Some((new_buf, new_width, new_height));
}

/// This is `a * b`.
///
/// **Ordering** in matrix multiplication does **matter**.
pub fn matrix_mul(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}
