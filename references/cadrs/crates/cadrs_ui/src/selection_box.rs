//! The rectangle of a box select (`reference/onshape/box_select.md`), drawn while it is dragged:
//! left to right a **window** (a solid `#5a8ccb` 1 px outline over a blue fill that lands on
//! `#d1dfee` over white: what lies wholly inside it), right to left a **crossing** (a dashed
//! 4/3 `#f0b400` outline over a yellow fill that lands on `#fff3cf`: whatever it touches). The
//! fills are chosen for Bevy's linear blending.
//!
//! A view calls [`sync`] each frame with the box's corners, from the press to the pointer, in
//! the coordinates of the node that hosts it (its viewport area), and with `None` once the drag
//! ends; this module keeps the node, its colours and its dashes.

use bevy::prelude::*;

pub struct SelectionBoxPlugin;

impl Plugin for SelectionBoxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, draw.before(bevy::ui::UiSystems::Prepare));
    }
}

/// A box select's rectangle: its corners in its parent's logical px.
#[derive(Component, Debug)]
pub struct SelectionBox {
    pub start: Vec2,
    pub end: Vec2,
    /// The size and mode its dashes were built for.
    built: Option<(Vec2, bool)>,
}

#[derive(Component)]
struct Dash;

/// Whether a box from `start` to `end` is a crossing (dragged right to left).
pub fn crossing(start: Vec2, end: Vec2) -> bool {
    end.x < start.x
}

/// The boxes [`sync`] keeps.
pub type Boxes<'w, 's> = Query<'w, 's, (Entity, &'static Name, &'static mut SelectionBox)>;

/// Shows the box named `name` from `start` to `end` (`span`, in `host`'s logical px) as a child
/// of `host`, or (`span` `None`, or no host) removes it.
pub fn sync(commands: &mut Commands, boxes: &mut Boxes, name: &str, host: Option<Entity>, span: Option<(Vec2, Vec2)>) {
    let mut found = None;
    for (e, n, b) in boxes.iter_mut() {
        if n.as_str() == name {
            found = Some((e, b));
        }
    }
    match (span, host, found) {
        (Some((start, end)), _, Some((_, mut b))) => {
            if b.start != start || b.end != end {
                b.start = start;
                b.end = end;
            }
        }
        (Some((start, end)), Some(host), None) => {
            let e = commands.spawn((Name::new(name.to_string()), SelectionBox { start, end, built: None }, Node { position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE)).id();
            commands.entity(host).add_child(e);
        }
        (_, _, Some((e, _))) => {
            commands.entity(e).try_despawn();
        }
        _ => {}
    }
}

/// The window's and the crossing's (fill, outline).
fn colors(cross: bool) -> (Color, Color) {
    if cross {
        (Color::srgba_u8(0xff, 0xe0, 0x45, 102), Color::srgb_u8(0xf0, 0xb4, 0x00))
    } else {
        (Color::srgba_u8(0x24, 0x93, 0xcc, 94), Color::srgb_u8(0x5a, 0x8c, 0xcb))
    }
}

/// The dashes of a crossing's outline, 4 px on and 3 off along each side: (x, y, w, h).
fn dashes(size: Vec2) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    let run = |len: f32, place: &mut dyn FnMut(f32, f32)| {
        let mut t = 0.0;
        while t < len {
            place(t, (len - t).min(4.0));
            t += 7.0;
        }
    };
    run(size.x, &mut |t, l| {
        out.push((t, 0.0, l, 1.0));
        out.push((t, size.y - 1.0, l, 1.0));
    });
    run(size.y, &mut |t, l| {
        out.push((0.0, t, 1.0, l));
        out.push((size.x - 1.0, t, 1.0, l));
    });
    out
}

#[allow(clippy::type_complexity)]
fn draw(mut q: Query<(Entity, &mut SelectionBox, &mut Node), Changed<SelectionBox>>, mut commands: Commands) {
    for (e, mut b, mut node) in &mut q {
        let cross = crossing(b.start, b.end);
        let (lo, hi) = (b.start.min(b.end).round(), b.start.max(b.end).round());
        let size = hi - lo;
        let want = Node {
            position_type: PositionType::Absolute,
            left: Val::Px(lo.x),
            top: Val::Px(lo.y),
            width: Val::Px(size.x),
            height: Val::Px(size.y),
            border: if cross { UiRect::ZERO } else { UiRect::all(Val::Px(1.0)) },
            ..default()
        };
        if *node != want {
            *node = want;
        }
        if b.built == Some((size, cross)) {
            continue;
        }
        b.built = Some((size, cross));
        let (fill, stroke) = colors(cross);
        let mut ec = commands.entity(e);
        ec.insert((BackgroundColor(fill), BorderColor::all(stroke)));
        ec.despawn_related::<Children>();
        if cross {
            ec.with_children(|p| {
                for (x, y, w, h) in dashes(size) {
                    p.spawn((
                        Dash,
                        Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(w), height: Val::Px(h), ..default() },
                        BackgroundColor(stroke),
                        Pickable::IGNORE,
                    ));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn a_box_follows_its_corners_and_goes() {
        let mut app = App::new();
        app.add_plugins(SelectionBoxPlugin);
        let host = app.world_mut().spawn(Node::default()).id();
        let show = move |app: &mut App, span: Option<(Vec2, Vec2)>| {
            app.world_mut()
                .run_system_once(move |mut commands: Commands, mut boxes: Boxes| sync(&mut commands, &mut boxes, "test-box", Some(host), span))
                .unwrap();
            app.update();
        };
        // Dragged left to right: a window, outlined, no dashes.
        show(&mut app, Some((Vec2::new(10.0, 20.0), Vec2::new(50.0, 40.0))));
        let w = app.world_mut();
        let (e, node, kids) = w.query::<(Entity, &Node, Option<&Children>)>().iter(w).find(|(e, ..)| w.get::<SelectionBox>(*e).is_some()).map(|(e, n, c)| (e, n.clone(), c.map_or(0, |c| c.len()))).unwrap();
        assert_eq!((node.left, node.top, node.width, node.height), (Val::Px(10.0), Val::Px(20.0), Val::Px(40.0), Val::Px(20.0)));
        assert_eq!(node.border, UiRect::all(Val::Px(1.0)));
        assert_eq!(kids, 0);
        assert_eq!(w.get::<ChildOf>(e).map(|c| c.parent()), Some(host));
        // Back past its start: a crossing, from the pointer, dashed.
        show(&mut app, Some((Vec2::new(10.0, 20.0), Vec2::new(0.0, 40.0))));
        let w = app.world_mut();
        let node = w.get::<Node>(e).unwrap().clone();
        assert_eq!((node.left, node.width, node.border), (Val::Px(0.0), Val::Px(10.0), UiRect::ZERO));
        assert!(w.get::<Children>(e).is_some_and(|c| c.len() > 4));
        // Ended: gone.
        show(&mut app, None);
        assert!(app.world().get_entity(e).is_err());
    }
}
