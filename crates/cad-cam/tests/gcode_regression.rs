//! G-code regression: the exact bytes each postprocessor emits for one fixed toolpath.
//!
//! A postprocessor is the most dangerous component in the crate — a wrong modal word cuts the wrong
//! part — so the fixtures are literal strings, not reconstructions.

use cad_cam::{
    Machine, MachineEnvelope, PostFormat, Postprocessor, Setup, Stock, Toolpath, ToolpathId,
    ToolpathPoint, WorkCoordinate,
};

fn setup() -> Result<Setup, Box<dyn std::error::Error>> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?)
}

fn toolpath() -> Result<Toolpath, Box<dyn std::error::Error>> {
    let id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(1).ok_or("tool ids start at one")?;
    Ok(Toolpath::new(
        id,
        tool_id,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::plunge([0.0, 0.0, 18.0], 200.0),
            ToolpathPoint::feed([10.0, 0.0, 18.0], 600.0),
            ToolpathPoint::feed([10.0, 0.0, 25.0], 600.0),
        ],
        12_000.0,
        25.0,
    ))
}

fn second_toolpath() -> Result<Toolpath, Box<dyn std::error::Error>> {
    let id = ToolpathId::new(2).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(2).ok_or("tool ids start at one")?;
    Ok(Toolpath::new(
        id,
        tool_id,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::plunge([5.0, 5.0, 15.0], 200.0),
            ToolpathPoint::feed([15.0, 5.0, 15.0], 900.0),
            ToolpathPoint::feed([15.0, 5.0, 25.0], 900.0),
        ],
        9_000.0,
        25.0,
    ))
}

#[test]
fn grbl_program_matches_its_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let program = Postprocessor::new(PostFormat::Grbl, "atlas cam facing")
        .emit(&toolpath()?, &setup()?)?;
    assert_eq!(
        program,
        "(atlas cam facing)\n\
         G21\n\
         G90\n\
         G94\n\
         G17\n\
         G54\n\
         S12000 M3\n\
         G0 X0 Y0 Z25\n\
         G1 X0 Y0 Z18 F200\n\
         X10 Y0 Z18 F600\n\
         X10 Y0 Z25\n\
         M5\n\
         G0 Z25\n\
         M30\n"
    );
    Ok(())
}

#[test]
fn grbl_pauses_for_a_manual_tool_change() -> Result<(), Box<dyn std::error::Error>> {
    let first = toolpath()?;
    let second = second_toolpath()?;
    let program = Postprocessor::new(PostFormat::Grbl, "atlas cam job")
        .emit_program(&[&first, &second], &setup()?)?;
    assert!(
        program.contains("M5\nG0 Z25\n(change tool to T2 then cycle start)\nM0\nS9000 M3\n"),
        "{program}"
    );
    assert_eq!(program.matches("(atlas cam job)").count(), 1);
    assert_eq!(program.matches("M30").count(), 1);
    Ok(())
}

#[test]
fn linuxcnc_changes_tools_with_m6() -> Result<(), Box<dyn std::error::Error>> {
    let first = toolpath()?;
    let second = second_toolpath()?;
    let program = Postprocessor::new(PostFormat::LinuxCnc, "atlas cam job")
        .emit_program(&[&first, &second], &setup()?)?;
    assert!(
        program.contains("M5\nG0 Z25\nT2 M6\nS9000 M3\n"),
        "{program}"
    );
    assert_eq!(program.matches("G21").count(), 1);
    assert_eq!(program.matches("M2").count(), 1);
    Ok(())
}

#[test]
fn one_toolpath_is_byte_identical_through_both_entry_points()
-> Result<(), Box<dyn std::error::Error>> {
    let path = toolpath()?;
    let setup = setup()?;
    let post = Postprocessor::new(PostFormat::Mach3, "atlas cam facing");
    assert_eq!(post.emit(&path, &setup)?, post.emit_program(&[&path], &setup)?);
    Ok(())
}

#[test]
fn linuxcnc_program_matches_its_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let program = Postprocessor::new(PostFormat::LinuxCnc, "atlas cam facing")
        .emit(&toolpath()?, &setup()?)?;
    assert_eq!(
        program,
        "%\n\
         (atlas cam facing)\n\
         G21\n\
         G90\n\
         G94\n\
         G17\n\
         G40 G49 G80\n\
         G54\n\
         S12000 M3\n\
         G0 X0 Y0 Z25\n\
         G1 X0 Y0 Z18 F200\n\
         X10 Y0 Z18 F600\n\
         X10 Y0 Z25\n\
         M5\n\
         G0 Z25\n\
         M2\n\
         %\n"
    );
    Ok(())
}

#[test]
fn mach3_program_matches_its_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let program = Postprocessor::new(PostFormat::Mach3, "atlas cam facing")
        .emit(&toolpath()?, &setup()?)?;
    assert_eq!(
        program,
        "%\n\
         (atlas cam facing)\n\
         G21\n\
         G90\n\
         G94\n\
         G17\n\
         G54\n\
         S12000 M3\n\
         G0 X0 Y0 Z25\n\
         G1 X0 Y0 Z18 F200\n\
         X10 Y0 Z18 F600\n\
         X10 Y0 Z25\n\
         M5\n\
         G0 Z25\n\
         M30\n\
         %\n"
    );
    Ok(())
}

#[test]
fn the_same_toolpath_posts_identically_twice() -> Result<(), Box<dyn std::error::Error>> {
    let post = Postprocessor::new(PostFormat::Grbl, "atlas cam facing");
    let setup = setup()?;
    let first = post.emit(&toolpath()?, &setup)?;
    let second = post.emit(&toolpath()?, &setup)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn line_numbers_are_optional_and_sequential() -> Result<(), Box<dyn std::error::Error>> {
    let program = Postprocessor::new(PostFormat::Grbl, "atlas cam facing")
        .with_line_numbers(true)
        .emit(&toolpath()?, &setup()?)?;
    let lines: Vec<&str> = program.lines().collect();
    assert_eq!(lines[0], "N10 (atlas cam facing)");
    assert_eq!(lines[1], "N20 G21");
    assert_eq!(lines[2], "N30 G90");
    Ok(())
}

#[test]
fn a_program_name_cannot_break_out_of_its_comment() -> Result<(), Box<dyn std::error::Error>> {
    let program = Postprocessor::new(PostFormat::Grbl, "atlas (beta)")
        .emit(&toolpath()?, &setup()?)?;
    assert!(program.starts_with("(atlas _beta_)\n"), "{program}");
    Ok(())
}

#[test]
fn an_unsafe_toolpath_is_refused_before_posting() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(1).ok_or("tool ids start at one")?;
    let unsafe_path = Toolpath::new(
        id,
        tool_id,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::rapid([10.0, 10.0, 5.0]),
        ],
        12_000.0,
        25.0,
    );
    let error = Postprocessor::new(PostFormat::Grbl, "atlas")
        .emit(&unsafe_path, &setup)
        .err();
    assert!(error.is_some(), "a crash must not become a program");
    Ok(())
}

#[test]
fn format_names_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    for format in [PostFormat::Grbl, PostFormat::LinuxCnc, PostFormat::Mach3] {
        assert_eq!(PostFormat::parse(format.name())?, format);
        assert!(!format.extension().is_empty());
    }
    assert!(PostFormat::parse("fanuc").is_err());
    Ok(())
}
