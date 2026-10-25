//! Involute spur gear add-in. Appends a Sketch (polygon outline) and an Extrude to the
//! document. Runs unchanged in the browser and under `cadrust addin run`.

use cadrust_addin_sdk::{addin, num, serde_json::{json, Value}};

const MANIFEST: &str = r#"{
  "id": "gear-generator",
  "name": "Gear generator",
  "version": "0.1.0",
  "description": "Parametric involute spur gear (module, teeth, pressure angle, thickness, bore)",
  "commands": [
    { "id": "gear", "label": "Spur gear", "params": [
      { "k": "module", "label": "Module (mm)", "default": 2 },
      { "k": "teeth", "label": "Teeth", "default": 20 },
      { "k": "pressure", "label": "Pressure angle (deg)", "default": 20 },
      { "k": "thickness", "label": "Thickness (mm)", "default": 8 },
      { "k": "bore", "label": "Bore diameter (mm)", "default": 6 },
      { "k": "cx", "label": "Center X", "default": 0 },
      { "k": "cy", "label": "Center Y", "default": 0 }
    ] }
  ]
}"#;

addin!(MANIFEST, handle);

fn handle(req: Value) -> Value {
    let command = req["command"].as_str().unwrap_or("");
    if command != "gear" {
        return json!({ "error": format!("unknown command {command}") });
    }
    let p = &req["params"];
    let module = num(p, "module", 2.0);
    let teeth = num(p, "teeth", 20.0).round().max(4.0) as usize;
    let pressure = num(p, "pressure", 20.0).to_radians();
    let thickness = num(p, "thickness", 8.0);
    let bore = num(p, "bore", 6.0);
    let cx = num(p, "cx", 0.0);
    let cy = num(p, "cy", 0.0);
    if module <= 0.0 || thickness <= 0.0 {
        return json!({ "error": "module and thickness must be positive" });
    }
    let outline = gear_outline(module, teeth, pressure);
    let pts: Vec<Value> = outline.iter().map(|(x, y)| json!([round3(x + cx), round3(y + cy)])).collect();

    let mut doc = req["document"].clone();
    let timeline = match doc.get_mut("timeline").and_then(Value::as_array_mut) {
        Some(t) => t,
        None => return json!({ "error": "document has no timeline" }),
    };
    let n = timeline.iter().filter(|f| f["id"].as_str().map_or(false, |s| s.starts_with("gear"))).count() + 1;
    let sk = format!("gear{n}_sketch");
    let body = format!("gear{n}");
    let mut entities = vec![json!({ "type": "Polygon", "points": pts })];
    if bore > 0.0 {
        entities.push(json!({ "type": "Circle", "cx": cx, "cy": cy, "r": bore / 2.0 }));
    }
    timeline.push(json!({ "type": "Sketch", "id": sk, "plane": "XY", "offset": 0, "entities": entities, "constraints": [] }));
    timeline.push(json!({ "type": "Extrude", "id": body, "sketch": sk, "distance": thickness, "op": "NewBody" }));
    json!({ "document": doc, "message": format!("gear: {teeth} teeth, module {module}, {} outline points", outline.len()) })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Closed outline of an involute spur gear, centred at the origin, counter-clockwise.
fn gear_outline(m: f64, z: usize, alpha: f64) -> Vec<(f64, f64)> {
    let zf = z as f64;
    let rp = m * zf / 2.0; // pitch radius
    let rb = rp * alpha.cos(); // base radius
    let ra = rp + m; // addendum radius
    let rf = (rp - 1.25 * m).max(rb * 0.9); // dedendum radius
    let pitch = std::f64::consts::TAU / zf;
    // Involute: point at radius r has polar angle inv(theta) = tan(theta) - theta with cos(theta) = rb / r.
    let inv = |r: f64| -> f64 {
        let th = (rb / r).clamp(-1.0, 1.0).acos();
        th.tan() - th
    };
    // Half tooth thickness angle at the pitch circle is pitch/4; shift so the tooth is symmetric about angle 0.
    let half = pitch / 4.0 + inv(rp);
    let steps = 6;
    let mut flank = Vec::with_capacity(steps + 2);
    let r0 = rb.max(rf);
    for i in 0..=steps {
        let r = r0 + (ra - r0) * i as f64 / steps as f64;
        flank.push((r, inv(r)));
    }
    let root_pts = 3;
    let mut out = Vec::with_capacity(z * (2 * flank.len() + root_pts));
    for k in 0..z {
        let c = k as f64 * pitch;
        // Rising flank, falling flank, then a root arc at rf up to the next tooth.
        for (r, a) in &flank {
            out.push(polar(*r, c - half + a));
        }
        for (r, a) in flank.iter().rev() {
            out.push(polar(*r, c + half - a));
        }
        let a0 = c + half;
        let a1 = c + pitch - half;
        for i in 1..=root_pts {
            out.push(polar(rf, a0 + (a1 - a0) * i as f64 / (root_pts + 1) as f64));
        }
    }
    out
}

fn polar(r: f64, a: f64) -> (f64, f64) {
    (r * a.cos(), r * a.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_is_closed_and_ccw() {
        let pts = gear_outline(2.0, 20, 20f64.to_radians());
        assert_eq!(pts.len(), 20 * 17);
        // Shoelace area positive => counter-clockwise, and close to the pitch circle area.
        let mut area = 0.0;
        for i in 0..pts.len() {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % pts.len()];
            area += x0 * y1 - x1 * y0;
        }
        area *= 0.5;
        let pitch_area = std::f64::consts::PI * 20.0f64.powi(2);
        assert!(area > 0.0 && (area - pitch_area).abs() / pitch_area < 0.15, "area {area} vs {pitch_area}");
    }
}
