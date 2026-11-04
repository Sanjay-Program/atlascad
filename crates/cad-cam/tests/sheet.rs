//! The setup sheet is generated from the same values as the program, so its numbers are asserted.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, Operation, Setup, SetupSheet, Stock, Tool,
    ToolGeometry, ToolId, ToolpathId, WorkCoordinate, generate,
};

type Failure = Box<dyn std::error::Error>;

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "tool ids start at one".into())
}

fn setup() -> Result<Setup, Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?)
}

fn end_mill() -> Result<Tool, Failure> {
    Ok(Tool::new(
        id(1)?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?)
}

#[test]
fn a_sheet_carries_the_setup_the_tools_and_the_operations() -> Result<(), Failure> {
    let setup = setup()?;
    let tool = end_mill()?;
    let path_id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;

    let facing = generate(
        &Operation::Facing {
            depth_mm: 2.0,
            stepover_mm: 4.0,
        },
        &setup,
        &tool,
        path_id,
    )?;
    let pocket = generate(
        &Operation::Pocket {
            region: [[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]],
            depth_mm: 4.0,
            stepdown_mm: 2.0,
            stepover_mm: 4.0,
        },
        &setup,
        &tool,
        path_id,
    )?;

    let mut sheet = SetupSheet::new("fixture job", &setup);
    sheet.add_operation("facing", &tool, &facing);
    sheet.add_operation("2D pocket", &tool, &pocket);

    let rendered = sheet.render();
    assert!(rendered.contains("# Setup sheet — fixture job"));
    assert!(rendered.contains("3-axis milling"));
    assert!(rendered.contains("40.0 x 30.0 x 20.0 mm"));
    assert!(rendered.contains("24000 mm³"));
    assert!(rendered.contains("25.0 mm"));
    assert!(rendered.contains("| T1 | 6 mm flat end mill | flat end mill | 6.0 |"));
    assert!(rendered.contains("| facing | T1 |"));
    assert!(rendered.contains("| 2D pocket | T1 |"));

    // The tool appears once in the tool table, even though two operations used it.
    assert_eq!(
        rendered
            .lines()
            .filter(|line| line.starts_with("| T1 |"))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn the_tool_list_is_its_own_table() -> Result<(), Failure> {
    let tool = end_mill()?;
    let mut sheet = SetupSheet::new("tool list", &setup()?);
    sheet.add_tool(&tool);
    sheet.add_tool(&tool);
    let list = sheet.tool_list();
    assert!(list.starts_with("| Tool | Name |"));
    assert!(list.contains("| T1 | 6 mm flat end mill |"));
    assert_eq!(list.matches("| T1 |").count(), 1);
    Ok(())
}

#[test]
fn an_empty_sheet_says_so_rather_than_inventing_operations() -> Result<(), Failure> {
    let sheet = SetupSheet::new("empty", &setup()?);
    let rendered = sheet.render();
    assert!(rendered.contains("_No operations in this job._"));
    assert!(rendered.contains("_No tools in this setup._"));
    Ok(())
}
