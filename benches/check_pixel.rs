#![feature(portable_simd)]
use std::{io::Cursor, simd::u8x32};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use image::io::Reader;
use rgb::{FromSlice, RGBA};

const IMAGE: &[u8] = include_bytes!("/mnt/SSHD/AI/Datasets/Izu-Adl/img/10_izu-adl/35.png");

fn check_transparent_pixel(image: &[RGBA<u8>]) -> bool {
    image.iter().any(|pixel| pixel.a != 255)
}

fn check_transparent_pixel_simd(image: &[RGBA<u8>]) -> bool {
    // Isolate only the alpha channel.
    let pixel_alpha = Vec::from_iter(image.iter().map(|pixel| pixel.a));

    pixel_alpha.chunks(32).all(|pixel| {
        // This is just ugly, but better than having to deal with incomplete chunks and SIMD exploding
        let mut extra_data = [255; 32];
        let pxl = if pixel.len() != 32 {
            extra_data.copy_from_slice(pixel);
            &extra_data
        } else {
            pixel
        };

        // let cmp = unsafe {
        //     let alpha_reg = _mm256_loadu_si256(pxl.as_ptr() as *const __m256i);
        //     let alpha_mask = _mm256_set1_epi8(-1);

        //     // Whatever happens, this thing generates 4 bytes that I need to check if they are 0b11111111 (-1)
        //     let alpha_cmp = _mm256_cmpeq_epi8(alpha_reg, alpha_mask);

        //     // Yup, this is the one. Not sure why I would only want to compare the leftmost bit, but seems faster.
        //     _mm256_movemask_epi8(alpha_cmp)
        // };

        // cmp.eq(&-1)

        // I just cannot comprehend how Rust made this so simple.
        let alpha_reg = u8x32::from_slice(pxl);
        let alpha_mask = u8x32::splat(255);

        alpha_reg == alpha_mask
    })
}

fn criterion_benchmark(c: &mut Criterion) {
    let image_rd = Reader::new(Cursor::new(IMAGE))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    let image_bytes = image_rd.into_rgba8();

    c.bench_function("Check Pixel SIMD", |b| {
        b.iter(|| check_transparent_pixel_simd(black_box(image_bytes.as_rgba())))
    });

    c.bench_function("Check Pixel Simple iter", |b| {
        b.iter(|| check_transparent_pixel(black_box(image_bytes.as_rgba())))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
