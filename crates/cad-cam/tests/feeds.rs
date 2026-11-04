//! Feeds and speeds: the formula, the machine limits, and the error cases.

use cad_cam::{FeedsError, FeedsRequest, Material, recommend};

type Failure = Box<dyn std::error::Error>;

fn request(material: Material, diameter_mm: f64, flutes: u32) -> FeedsRequest {
    FeedsRequest {
        material,
        diameter_mm,
        flutes,
        max_spindle_rpm: 24_000.0,
        max_feed_mm_min: 5_000.0,
    }
}

#[test]
fn aluminium_6_mm_three_flute_matches_the_formula() -> Result<(), Failure> {
    let feeds = recommend(&request(Material::Aluminium, 6.0, 3))?;
    // 1000 * 250 / (pi * 6) = 13262.9 rpm, floored.
    assert_eq!(feeds.spindle_rpm, 13_262.0);
    // 13262 * 3 * 0.020 = 795.7 mm/min, floored.
    assert_eq!(feeds.feed_mm_min, 795.0);
    assert_eq!(feeds.plunge_feed_mm_min, 318.0);
    assert!(!feeds.spindle_limited);
    assert!(!feeds.feed_limited);
    assert!((feeds.surface_speed_m_min - 250.0).abs() < 0.05);
    assert!((feeds.chip_load_mm_per_tooth - 0.020).abs() < 1.0e-4);
    Ok(())
}

#[test]
fn a_slow_spindle_is_reported_and_the_feed_follows_it() -> Result<(), Failure> {
    let mut request = request(Material::Aluminium, 6.0, 3);
    request.max_spindle_rpm = 8_000.0;
    let feeds = recommend(&request)?;
    assert_eq!(feeds.spindle_rpm, 8_000.0);
    assert_eq!(feeds.feed_mm_min, 480.0);
    assert!(feeds.spindle_limited);
    assert!(!feeds.feed_limited);
    assert!((feeds.surface_speed_m_min - 150.796).abs() < 0.01);
    Ok(())
}

#[test]
fn a_slow_feed_is_reported_with_the_chip_load_it_achieves() -> Result<(), Failure> {
    let mut request = request(Material::Aluminium, 6.0, 3);
    request.max_feed_mm_min = 300.0;
    let feeds = recommend(&request)?;
    assert_eq!(feeds.feed_mm_min, 300.0);
    assert_eq!(feeds.plunge_feed_mm_min, 120.0);
    assert!(feeds.feed_limited);
    assert!(!feeds.spindle_limited);
    // 300 / (13262 * 3) = 0.00754 mm per tooth, not the 0.020 the material asked for.
    assert!((feeds.chip_load_mm_per_tooth - 0.007_54).abs() < 1.0e-5);
    assert!((feeds.target_chip_load_mm_per_tooth - 0.020).abs() < 1.0e-9);
    Ok(())
}

#[test]
fn chip_load_scales_with_the_tool() -> Result<(), Failure> {
    let small = recommend(&request(Material::Aluminium, 3.0, 2))?;
    let large = recommend(&request(Material::Aluminium, 12.0, 2))?;
    assert!((small.target_chip_load_mm_per_tooth - 0.010).abs() < 1.0e-9);
    assert!((large.target_chip_load_mm_per_tooth - 0.040).abs() < 1.0e-9);
    // A 12 mm tool turns slower and feeds faster than a 3 mm tool in the same material.
    assert!(large.spindle_rpm < small.spindle_rpm);
    assert!(large.feed_mm_min > small.feed_mm_min);
    Ok(())
}

#[test]
fn impossible_inputs_are_refused() -> Result<(), Failure> {
    for diameter in [0.0, -6.0, f64::NAN, 60.0] {
        let error = recommend(&request(Material::Brass, diameter, 2))
            .err()
            .ok_or("a bad diameter was accepted")?;
        assert!(matches!(error, FeedsError::InvalidDiameter(_)), "{error}");
    }
    let error = recommend(&request(Material::Brass, 6.0, 0))
        .err()
        .ok_or("zero flutes were accepted")?;
    assert!(matches!(error, FeedsError::InvalidFlutes(0)), "{error}");

    let mut request = request(Material::Brass, 6.0, 2);
    request.max_spindle_rpm = 0.0;
    assert!(recommend(&request).is_err());
    request.max_spindle_rpm = 24_000.0;
    request.max_feed_mm_min = f64::INFINITY;
    assert!(recommend(&request).is_err());
    Ok(())
}

#[test]
fn materials_parse_their_common_names_and_carry_their_source() -> Result<(), Failure> {
    for (text, expected) in [
        ("aluminum", Material::Aluminium),
        ("6061", Material::Aluminium),
        ("Mild Steel", Material::MildSteel),
        ("steel", Material::MildSteel),
        ("POM", Material::Acetal),
        ("stainless", Material::StainlessSteel),
        ("wood", Material::Hardwood),
    ] {
        assert_eq!(Material::parse(text), Some(expected), "{text}");
    }
    assert_eq!(Material::parse("unobtainium"), None);
    for material in Material::ALL {
        let data = material.cutting_data();
        assert!(data.surface_speed_m_min > 0.0);
        assert!(data.chip_load_mm_per_tooth > 0.0);
        assert!(
            data.source.contains("starting values"),
            "{}",
            material.name()
        );
        assert_eq!(Material::parse(material.name()), Some(material));
    }
    Ok(())
}
