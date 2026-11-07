use zui_image::{DecodeError, DecodeLimits, decode_png};
use zui_paint::ImageError;
use zui_types::Extent;
fn encode(
    color: png::ColorType,
    depth: png::BitDepth,
    pixels: &[u8],
    configure: impl FnOnce(&mut png::Encoder<'_, &mut Vec<u8>>),
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(color);
        encoder.set_depth(depth);
        configure(&mut encoder);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    bytes
}
#[test]
fn color_models_transparency_and_low_bit_palette_normalize_to_premultiplied_rgba() {
    for (color, pixels, expected) in [
        (
            png::ColorType::Rgba,
            vec![255, 100, 0, 128, 8, 9, 10, 0],
            vec![128, 50, 0, 128, 0, 0, 0, 0],
        ),
        (
            png::ColorType::Rgb,
            vec![1, 2, 3, 4, 5, 6],
            vec![1, 2, 3, 255, 4, 5, 6, 255],
        ),
        (
            png::ColorType::Grayscale,
            vec![8, 255],
            vec![8, 8, 8, 255, 255, 255, 255, 255],
        ),
        (
            png::ColorType::GrayscaleAlpha,
            vec![100, 128, 20, 255],
            vec![50, 50, 50, 128, 20, 20, 20, 255],
        ),
    ] {
        let image = decode_png(
            &encode(color, png::BitDepth::Eight, &pixels, |_| {}),
            DecodeLimits::default(),
        )
        .unwrap();
        assert_eq!(image.extent(), Extent::new(2, 1));
        assert_eq!(image.bytes(), expected);
    }
    let palette = encode(
        png::ColorType::Indexed,
        png::BitDepth::One,
        &[0b01000000],
        |e| {
            e.set_palette(vec![255, 0, 0, 0, 255, 0]);
            e.set_trns(vec![255, 128]);
        },
    );
    assert_eq!(
        decode_png(&palette, DecodeLimits::default())
            .unwrap()
            .bytes(),
        &[255, 0, 0, 255, 0, 128, 0, 128]
    );
    let gray = encode(
        png::ColorType::Grayscale,
        png::BitDepth::One,
        &[0b01000000],
        |_| {},
    );
    assert_eq!(
        decode_png(&gray, DecodeLimits::default()).unwrap().bytes(),
        &[0, 0, 0, 255, 255, 255, 255, 255]
    );
}
#[test]
fn sixteen_bit_samples_use_high_bytes_before_premultiplication() {
    let bytes = encode(
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Sixteen,
        &[0xff, 0xff, 0x80, 0x80, 0x12, 0xff, 0xff, 0xff],
        |_| {},
    );
    assert_eq!(
        decode_png(&bytes, DecodeLimits::default()).unwrap().bytes(),
        &[128, 128, 128, 128, 18, 18, 18, 255]
    );
}
#[test]
fn encoded_decoder_and_output_limits_fail_explicitly() {
    let bytes = encode(png::ColorType::Rgb, png::BitDepth::Eight, &[0; 6], |_| {});
    assert!(matches!(
        decode_png(
            &bytes,
            DecodeLimits {
                max_encoded_bytes: bytes.len() - 1,
                ..Default::default()
            }
        ),
        Err(DecodeError::EncodedTooLarge)
    ));
    assert!(matches!(
        decode_png(
            &bytes,
            DecodeLimits {
                max_decoder_bytes: 0,
                ..Default::default()
            }
        ),
        Err(DecodeError::DecoderLimit)
    ));
    for image in [
        zui_paint::ImageLimits {
            max_dimension: 1,
            max_bytes: 8,
        },
        zui_paint::ImageLimits {
            max_dimension: 2,
            max_bytes: 7,
        },
    ] {
        assert!(matches!(
            decode_png(
                &bytes,
                DecodeLimits {
                    image,
                    ..Default::default()
                }
            ),
            Err(DecodeError::Image(ImageError::TooLarge))
        ));
    }
    let image = decode_png(
        &bytes,
        DecodeLimits {
            image: zui_paint::ImageLimits {
                max_dimension: 2,
                max_bytes: 8,
            },
            max_encoded_bytes: bytes.len(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(image.bytes().len(), 8);
}
#[test]
fn malformed_truncated_corrupt_and_animated_png_are_not_silently_accepted() {
    let bytes = encode(png::ColorType::Rgb, png::BitDepth::Eight, &[0; 6], |_| {});
    for input in [&b"not png"[..], &bytes[..16], &bytes[..bytes.len() - 12]] {
        assert!(decode_png(input, DecodeLimits::default()).is_err());
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(decode_png(&corrupt, DecodeLimits::default()).is_err());
    let animated = encode(png::ColorType::Rgb, png::BitDepth::Eight, &[0; 6], |e| {
        e.set_animated(1, 0).unwrap();
    });
    assert!(matches!(
        decode_png(&animated, DecodeLimits::default()),
        Err(DecodeError::AnimatedPng)
    ));
    assert!(decode_png(&bytes, DecodeLimits::default()).is_ok());
}
