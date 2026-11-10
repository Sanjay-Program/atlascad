//! Comprehensive integration tests for 2D Parametric Geometric and Dimensional Constraints.

use cad_constraints::{ConstraintKind, DofStatus, SketchSystem};

#[test]
fn test_horizontal_vertical_and_coincident() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    // Line 1: points p1, p2
    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(10.0, 3.0); // Not horizontal
    let l1 = sys.add_line(p1, p2)?;

    // Line 2: points p2, p3
    let p3 = sys.add_point(8.0, 20.0); // Not vertical
    let l2 = sys.add_line(p2, p3)?;

    // Fix p1 at origin
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    // Make l1 horizontal (y2 = y1 = 0)
    sys.add_constraint(ConstraintKind::Horizontal(l1))?;
    // Distance p1 to p2 = 25.0
    sys.add_constraint(ConstraintKind::Distance(p1, p2, 25.0))?;
    // Make l2 vertical (x3 = x2 = 25)
    sys.add_constraint(ConstraintKind::Vertical(l2))?;
    // Distance p2 to p3 = 15.0
    sys.add_constraint(ConstraintKind::Distance(p2, p3, 15.0))?;

    let report = sys.solve()?;
    assert!(report.converged);

    let pt1 = sys.point(p1).ok_or("p1 exists")?;
    let pt2 = sys.point(p2).ok_or("p2 exists")?;
    let pt3 = sys.point(p3).ok_or("p3 exists")?;

    assert!((pt1.x - 0.0).abs() < 1e-6);
    assert!((pt1.y - 0.0).abs() < 1e-6);

    assert!((pt2.x - 25.0).abs() < 1e-6);
    assert!((pt2.y - 0.0).abs() < 1e-6);

    assert!((pt3.x - 25.0).abs() < 1e-6);
    assert!((pt3.y - 15.0).abs() < 1e-6);

    // Fully constrained: 3 points = 6 parameters.
    // Constraints: FixPoint(2) + Horizontal(1) + Dist(1) + Vertical(1) + Dist(1) = 6 equations.
    let dof = sys.dof_analysis();
    assert_eq!(dof.total_dof, 0);
    assert_eq!(dof.status, DofStatus::Fully);

    Ok(())
}

#[test]
fn test_parallel_perpendicular_and_equal_length() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(10.0, 0.0);
    let l1 = sys.add_line(p1, p2)?;

    let p3 = sys.add_point(0.0, 10.0);
    let p4 = sys.add_point(15.0, 12.0);
    let l2 = sys.add_line(p3, p4)?;

    let p5 = sys.add_point(5.0, 5.0);
    let p6 = sys.add_point(5.0, 15.0);
    let l3 = sys.add_line(p5, p6)?;

    // Fix l1
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::FixPoint(p2, [20.0, 0.0]))?; // length 20, along X

    // l2 parallel to l1 and equal length to l1
    sys.add_constraint(ConstraintKind::Parallel(l1, l2))?;
    sys.add_constraint(ConstraintKind::EqualLength(l1, l2))?;
    sys.add_constraint(ConstraintKind::FixPoint(p3, [0.0, 10.0]))?;

    // l3 perpendicular to l1
    sys.add_constraint(ConstraintKind::Perpendicular(l1, l3))?;

    let report = sys.solve()?;
    assert!(report.converged);

    let pt3 = sys.point(p3).ok_or("p3")?;
    let pt4 = sys.point(p4).ok_or("p4")?;
    let pt5 = sys.point(p5).ok_or("p5")?;
    let pt6 = sys.point(p6).ok_or("p6")?;

    // l2 must be horizontal with length 20
    assert!((pt4.y - pt3.y).abs() < 1e-6);
    assert!(((pt4.x - pt3.x).abs() - 20.0).abs() < 1e-6);

    // l3 must be perpendicular to l1 (vertical)
    assert!((pt6.x - pt5.x).abs() < 1e-6);

    Ok(())
}

#[test]
fn test_circle_tangent_and_concentric() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    // Horizontal line y = 0
    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(50.0, 0.0);
    let l1 = sys.add_line(p1, p2)?;
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::FixPoint(p2, [50.0, 0.0]))?;

    // Circle 1 with radius 10 tangent to line l1
    let c1_center = sys.add_point(20.0, 15.0);
    let c1 = sys.add_circle(c1_center, 10.0)?;
    sys.add_constraint(ConstraintKind::FixRadius(c1, 10.0))?;
    sys.add_constraint(ConstraintKind::Tangent(l1, c1))?;

    // Circle 2 concentric with Circle 1, with radius 15
    let c2_center = sys.add_point(10.0, 10.0);
    let c2 = sys.add_circle(c2_center, 15.0)?;
    sys.add_constraint(ConstraintKind::Concentric(c1, c2))?;

    let report = sys.solve()?;
    assert!(report.converged);

    let cent1 = sys.point(c1_center).ok_or("c1 center")?;
    let cent2 = sys.point(c2_center).ok_or("c2 center")?;

    // Center y must equal radius 10.0 (distance to y=0 is 10.0)
    assert!((cent1.y.abs() - 10.0).abs() < 1e-5);
    // c2 concentric with c1
    assert!((cent1.x - cent2.x).abs() < 1e-6);
    assert!((cent1.y - cent2.y).abs() < 1e-6);

    Ok(())
}

#[test]
fn test_tangent_circles_and_equal_radius() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    let p1 = sys.add_point(0.0, 0.0);
    let c1 = sys.add_circle(p1, 10.0)?;
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::FixRadius(c1, 10.0))?;

    let p2 = sys.add_point(30.0, 0.0);
    let c2 = sys.add_circle(p2, 5.0)?;

    // Equal radius and tangent
    sys.add_constraint(ConstraintKind::EqualRadius(c1, c2))?;
    sys.add_constraint(ConstraintKind::TangentCircles(c1, c2))?;
    // Constrain p2 on horizontal axis
    let l_axis = sys.add_line(p1, p2)?;
    sys.add_constraint(ConstraintKind::Horizontal(l_axis))?;

    let report = sys.solve()?;
    assert!(report.converged);

    let circ2 = sys.circle(c2).ok_or("c2")?;
    let pt2 = sys.point(p2).ok_or("p2")?;

    assert!((circ2.radius - 10.0).abs() < 1e-5);
    assert!((pt2.x.abs() - 20.0).abs() < 1e-5); // Center-to-center distance = r1 + r2 = 20.0

    Ok(())
}

#[test]
fn test_angle_and_point_on_line() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(10.0, 0.0);
    let l1 = sys.add_line(p1, p2)?;
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::FixPoint(p2, [10.0, 0.0]))?;

    let p3 = sys.add_point(0.0, 0.0);
    let p4 = sys.add_point(10.0, 5.0);
    let l2 = sys.add_line(p3, p4)?;
    sys.add_constraint(ConstraintKind::Coincident(p1, p3))?;
    sys.add_constraint(ConstraintKind::FixLength(l2, 10.0))?;

    // 45 degrees angle between l1 and l2
    let angle_45 = std::f64::consts::FRAC_PI_4;
    sys.add_constraint(ConstraintKind::Angle(l1, l2, angle_45))?;

    // Point p5 on line l2 at distance 5 from origin
    let p5 = sys.add_point(2.0, 2.0);
    sys.add_constraint(ConstraintKind::PointOnLine(p5, l2))?;
    sys.add_constraint(ConstraintKind::Distance(p1, p5, 5.0))?;

    let report = sys.solve()?;
    assert!(report.converged);

    let pt4 = sys.point(p4).ok_or("p4")?;
    let pt5 = sys.point(p5).ok_or("p5")?;

    // pt4 should be at 10 * cos(45°), 10 * sin(45°)
    let expected_x = 10.0 * 2.0f64.sqrt() / 2.0;
    assert!((pt4.x - expected_x).abs() < 1e-4);
    assert!((pt4.y - expected_x).abs() < 1e-4);

    // pt5 should be at 5 * cos(45°), 5 * sin(45°)
    let exp_p5_x = 5.0 * 2.0f64.sqrt() / 2.0;
    assert!((pt5.x - exp_p5_x).abs() < 1e-4);
    assert!((pt5.y - exp_p5_x).abs() < 1e-4);

    Ok(())
}

#[test]
fn test_dof_and_over_constrained_detection() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(10.0, 0.0);
    let l1 = sys.add_line(p1, p2)?;

    // 2 points = 4 free parameters
    let dof0 = sys.dof_analysis();
    assert_eq!(dof0.total_dof, 4);
    assert_eq!(dof0.status, DofStatus::Unconstrained);

    // Fix point 1: removes 2 DOF -> 2 DOF remaining
    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    let dof1 = sys.dof_analysis();
    assert_eq!(dof1.total_dof, 2);
    assert_eq!(dof1.status, DofStatus::Under);

    // Horizontal line: removes 1 DOF -> 1 DOF remaining
    sys.add_constraint(ConstraintKind::Horizontal(l1))?;
    let dof2 = sys.dof_analysis();
    assert_eq!(dof2.total_dof, 1);
    assert_eq!(dof2.status, DofStatus::Under);

    // Fix length: removes 1 DOF -> 0 DOF remaining (Fully constrained)
    sys.add_constraint(ConstraintKind::FixLength(l1, 15.0))?;
    let dof3 = sys.dof_analysis();
    assert_eq!(dof3.total_dof, 0);
    assert_eq!(dof3.status, DofStatus::Fully);
    assert!(dof3.redundant_constraints.is_empty());

    // Add duplicate / redundant Horizontal constraint
    let red_id = sys.add_constraint(ConstraintKind::Horizontal(l1))?;
    let dof4 = sys.dof_analysis();
    assert_eq!(dof4.status, DofStatus::Over);
    assert!(dof4.redundant_constraints.contains(&red_id));

    Ok(())
}

#[test]
fn test_conflicting_constraints_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = SketchSystem::new();

    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(10.0, 0.0);

    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::FixPoint(p2, [10.0, 0.0]))?; // Distance is 10.0

    // Contradictory distance requirement: distance = 25.0
    let c_bad = sys.add_constraint(ConstraintKind::Distance(p1, p2, 25.0))?;

    let res = sys.solve();
    assert!(res.is_err());
    match res {
        Err(cad_constraints::ConstraintError::Conflict(ids)) => {
            assert!(ids.contains(&c_bad));
        }
        _ => return Err("expected Conflict error with contradictory constraint ID".into()),
    }

    Ok(())
}

#[test]
fn test_solve_performance_under_5ms() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();

    // Set up a 4-bar linkage / quadrilateral system
    let mut sys = SketchSystem::new();
    let p1 = sys.add_point(0.0, 0.0);
    let p2 = sys.add_point(30.0, 2.0);
    let p3 = sys.add_point(28.0, 22.0);
    let p4 = sys.add_point(2.0, 18.0);

    let l1 = sys.add_line(p1, p2)?;
    let l2 = sys.add_line(p2, p3)?;
    let l3 = sys.add_line(p3, p4)?;
    let l4 = sys.add_line(p4, p1)?;

    sys.add_constraint(ConstraintKind::FixPoint(p1, [0.0, 0.0]))?;
    sys.add_constraint(ConstraintKind::Horizontal(l1))?;
    sys.add_constraint(ConstraintKind::FixLength(l1, 30.0))?;
    sys.add_constraint(ConstraintKind::FixLength(l2, 20.0))?;
    sys.add_constraint(ConstraintKind::FixLength(l3, 30.0))?;
    sys.add_constraint(ConstraintKind::FixLength(l4, 20.0))?;
    sys.add_constraint(ConstraintKind::Parallel(l1, l3))?;
    sys.add_constraint(ConstraintKind::Parallel(l2, l4))?;
    sys.add_constraint(ConstraintKind::Perpendicular(l1, l2))?;

    let report = sys.solve()?;
    let elapsed = start.elapsed();

    assert!(report.converged);
    assert!(
        elapsed.as_millis() < 5,
        "solve took {} ms, exceeding 5ms target",
        elapsed.as_millis()
    );

    Ok(())
}
