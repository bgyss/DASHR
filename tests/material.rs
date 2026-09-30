use dashr::material::*;
#[test]
fn normal_bake_uses_wrapped_byte_deltas_and_snorm() {
    let p = Pixels {
        width: 3,
        height: 3,
        rgba: (0..9)
            .flat_map(|i| [if i == 4 { 255 } else { 0 }, 0, 0, 255])
            .collect(),
    };
    let n = normal_map(&p, 8.).unwrap();
    assert_eq!(&n[16..20], &[0, 0, 127, 0]);
    assert!((n[12] as i8) < 0);
    assert!((n[20] as i8) > 0);
    assert!(
        normal_map(
            &Pixels {
                width: 0,
                height: 0,
                rgba: vec![]
            },
            1.
        )
        .is_err()
    );
}
#[test]
fn reference_decode_discards_low_byte_and_repeat_does_not_resample() {
    let img = image::DynamicImage::ImageLuma16(
        image::ImageBuffer::from_raw(2, 1, vec![0x12ffu16, 0xab01]).unwrap(),
    );
    let mut p = decode_reference(img);
    assert_eq!(p.rgba, [0x12, 0x12, 0x12, 255, 0xab, 0xab, 0xab, 255]);
    repeat(&mut p, 2).unwrap();
    assert_eq!(p.rgba, [0x12, 0x12, 0x12, 255, 0x12, 0x12, 0x12, 255]);
    assert!(repeat(&mut p, 0).is_err());
}
