#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
pub unsafe fn dot_product_avx(w: &[f32], x: &[f32]) -> f32 {
    unsafe {
        let len = w.len();
        let mut sum = _mm256_setzero_ps();
        let mut i = 0;

        // Unroll 4 times (32 elements)
        while i + 32 <= len {
            let w0 = _mm256_loadu_ps(w.as_ptr().add(i));
            let x0 = _mm256_loadu_ps(x.as_ptr().add(i));
            let p0 = _mm256_mul_ps(w0, x0);
            sum = _mm256_add_ps(sum, p0);

            let w1 = _mm256_loadu_ps(w.as_ptr().add(i + 8));
            let x1 = _mm256_loadu_ps(x.as_ptr().add(i + 8));
            let p1 = _mm256_mul_ps(w1, x1);
            sum = _mm256_add_ps(sum, p1);

            let w2 = _mm256_loadu_ps(w.as_ptr().add(i + 16));
            let x2 = _mm256_loadu_ps(x.as_ptr().add(i + 16));
            let p2 = _mm256_mul_ps(w2, x2);
            sum = _mm256_add_ps(sum, p2);

            let w3 = _mm256_loadu_ps(w.as_ptr().add(i + 24));
            let x3 = _mm256_loadu_ps(x.as_ptr().add(i + 24));
            let p3 = _mm256_mul_ps(w3, x3);
            sum = _mm256_add_ps(sum, p3);

            i += 32;
        }

        // Handle remaining 8-blocks
        while i + 8 <= len {
            let w_vec = _mm256_loadu_ps(w.as_ptr().add(i));
            let x_vec = _mm256_loadu_ps(x.as_ptr().add(i));
            let prod = _mm256_mul_ps(w_vec, x_vec);
            sum = _mm256_add_ps(sum, prod);
            i += 8;
        }

        // Horizontal sum
        let mut temp = [0.0f32; 8];
        _mm256_storeu_ps(temp.as_mut_ptr(), sum);
        let mut total = temp.iter().sum::<f32>();

        // Handle remaining elements
        while i < len {
            total += w[i] * x[i];
            i += 1;
        }

        total
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx", enable = "fma")]
pub unsafe fn dot_product_fma(w: &[f32], x: &[f32]) -> f32 {
    unsafe {
        let len = w.len();
        let mut sum = _mm256_setzero_ps();
        let mut i = 0;

        // Unroll 4 times (32 elements)
        while i + 32 <= len {
            let w0 = _mm256_loadu_ps(w.as_ptr().add(i));
            let x0 = _mm256_loadu_ps(x.as_ptr().add(i));
            sum = _mm256_fmadd_ps(w0, x0, sum);

            let w1 = _mm256_loadu_ps(w.as_ptr().add(i + 8));
            let x1 = _mm256_loadu_ps(x.as_ptr().add(i + 8));
            sum = _mm256_fmadd_ps(w1, x1, sum);

            let w2 = _mm256_loadu_ps(w.as_ptr().add(i + 16));
            let x2 = _mm256_loadu_ps(x.as_ptr().add(i + 16));
            sum = _mm256_fmadd_ps(w2, x2, sum);

            let w3 = _mm256_loadu_ps(w.as_ptr().add(i + 24));
            let x3 = _mm256_loadu_ps(x.as_ptr().add(i + 24));
            sum = _mm256_fmadd_ps(w3, x3, sum);

            i += 32;
        }

        while i + 8 <= len {
            let w_vec = _mm256_loadu_ps(w.as_ptr().add(i));
            let x_vec = _mm256_loadu_ps(x.as_ptr().add(i));
            sum = _mm256_fmadd_ps(w_vec, x_vec, sum);
            i += 8;
        }

        let mut temp = [0.0f32; 8];
        _mm256_storeu_ps(temp.as_mut_ptr(), sum);
        let mut total = temp.iter().sum::<f32>();

        while i < len {
            total += w[i] * x[i];
            i += 1;
        }

        total
    }
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn dot_product_neon(w: &[f32], x: &[f32]) -> f32 {
    unsafe {
        let len = w.len();
        let mut sum_vec = vdupq_n_f32(0.0);
        let mut i = 0;

        // Unroll 4 times (16 elements)
        while i + 16 <= len {
            let w0 = vld1q_f32(w.as_ptr().add(i));
            let x0 = vld1q_f32(x.as_ptr().add(i));
            sum_vec = vfmaq_f32(sum_vec, w0, x0);

            let w1 = vld1q_f32(w.as_ptr().add(i + 4));
            let x1 = vld1q_f32(x.as_ptr().add(i + 4));
            sum_vec = vfmaq_f32(sum_vec, w1, x1);

            let w2 = vld1q_f32(w.as_ptr().add(i + 8));
            let x2 = vld1q_f32(x.as_ptr().add(i + 8));
            sum_vec = vfmaq_f32(sum_vec, w2, x2);

            let w3 = vld1q_f32(w.as_ptr().add(i + 12));
            let x3 = vld1q_f32(x.as_ptr().add(i + 12));
            sum_vec = vfmaq_f32(sum_vec, w3, x3);

            i += 16;
        }

        while i + 4 <= len {
            let w_vec = vld1q_f32(w.as_ptr().add(i));
            let x_vec = vld1q_f32(x.as_ptr().add(i));
            sum_vec = vfmaq_f32(sum_vec, w_vec, x_vec);
            i += 4;
        }

        let mut sum = vaddvq_f32(sum_vec);

        while i < len {
            sum += w[i] * x[i];
            i += 1;
        }
        sum
    }
}

pub fn dot_product(w: &[f32], x: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("fma") {
            return unsafe { dot_product_fma(w, x) };
        } else if is_x86_feature_detected!("avx") {
            return unsafe { dot_product_avx(w, x) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { dot_product_neon(w, x) };
    }
    #[allow(unreachable_code)]
    w.iter().zip(x.iter()).map(|(a, b)| a * b).sum()
}
