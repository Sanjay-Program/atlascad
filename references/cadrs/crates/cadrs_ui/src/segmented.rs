//! A segmented control: a few small buttons joined in one outlined row, one of them on, like
//! gpui-component's `ButtonGroup` with a selected button (a derived board's "3D | Layout").
//!
//! The row gets the builder's name and each segment `<name>-<index>`. Choosing a segment
//! triggers [`SegmentedSelect`] on the row (also when it is already on); the row shows the
//! choice at once.

use std::borrow::Cow;

use bevy::ecs::spawn::SpawnWith;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::button::Button;
use crate::style::Selected;
use crate::theme::Theme;

pub struct SegmentedPlugin;

impl Plugin for SegmentedPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_segment);
    }
}

/// A segment was chosen. Targets the row.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct SegmentedSelect {
    pub entity: Entity,
    pub index: usize,
}

/// The row's chosen segment.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedState {
    pub selected: usize,
}

/// One segment of a row.
#[derive(Component, Debug, Clone, Copy)]
struct Segment {
    row: Entity,
    index: usize,
}

/// Builder for a segmented control.
pub struct Segmented {
    name: Cow<'static, str>,
    options: Vec<(String, Option<&'static str>, String)>,
    selected: usize,
}

impl Segmented {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into(), options: Vec::new(), selected: 0 }
    }

    /// A segment: its label, an icon (or none) and a tooltip.
    pub fn option(mut self, label: impl Into<String>, icon: Option<&'static str>, tooltip: impl Into<String>) -> Self {
        self.options.push((label.into(), icon, tooltip.into()));
        self
    }

    pub fn selected(mut self, i: usize) -> Self {
        self.selected = i;
        self
    }

    pub fn build(self, theme: &Theme) -> impl Bundle {
        let t = theme.clone();
        let name = self.name.into_owned();
        let selected = self.selected;
        let options = self.options;
        (
            Name::new(name.clone()),
            SegmentedState { selected },
            Node { column_gap: Val::Px(1.0), padding: UiRect::all(Val::Px(1.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(t.radius)), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() },
            BorderColor::all(t.border),
            Children::spawn(SpawnWith(move |p: &mut ChildSpawner| {
                let row = p.target_entity();
                for (i, (label, icon, tip)) in options.into_iter().enumerate() {
                    let mut b = Button::new(format!("{name}-{i}")).label(label).small().ghost().selected(i == selected).tooltip(tip);
                    if let Some(icon) = icon {
                        b = b.icon(icon).icon_size(12.0);
                    }
                    p.spawn((b.build(&t), Segment { row, index: i })).entry::<Node>().and_modify(|mut n| {
                        n.height = Val::Px(18.0);
                        n.border = UiRect::ZERO;
                        n.padding = UiRect::horizontal(Val::Px(6.0));
                    });
                }
            })),
        )
    }
}

fn on_segment(a: On<Activate>, q: Query<&Segment>, mut rows: Query<(&mut SegmentedState, &Children)>, segments: Query<&Segment>, mut commands: Commands) {
    let Ok(seg) = q.get(a.entity) else { return };
    if let Ok((mut state, children)) = rows.get_mut(seg.row) {
        if state.selected != seg.index {
            state.selected = seg.index;
        }
        for c in children.iter() {
            if let Ok(s) = segments.get(c) {
                if s.index == seg.index {
                    commands.entity(c).insert(Selected);
                } else {
                    commands.entity(c).remove::<Selected>();
                }
            }
        }
    }
    commands.trigger(SegmentedSelect { entity: seg.row, index: seg.index });
}
