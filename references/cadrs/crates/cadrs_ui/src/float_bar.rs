//! A bar floating at the top middle of an area, over what it shows: an edit mode's bar over the
//! viewport (gpui-component's `Popover`-style card, as a row), and a one-line prompt under it.
//!
//! The bar is centred by a full-width row around it (no fixed offsets, so it stays centred
//! whatever its contents), which lets the pointer through beside the bar. The row is named
//! `<name>-row`, the bar `<name>`.

use std::borrow::Cow;

use bevy::ecs::spawn::SpawnWith;
use bevy::prelude::*;

use crate::theme::Theme;

/// Builder for a floating bar.
pub struct FloatBar {
    name: Cow<'static, str>,
    top: f32,
    prompt: bool,
}

impl FloatBar {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into(), top: 8.0, prompt: false }
    }

    /// Its distance from the area's top (px).
    pub fn top(mut self, px: f32) -> Self {
        self.top = px;
        self
    }

    /// A prompt rather than a bar: the info background, no shadow, tighter.
    pub fn prompt(mut self) -> Self {
        self.prompt = true;
        self
    }

    /// The row and its bar; `content` fills the bar.
    pub fn build(self, t: &Theme, content: impl FnOnce(&mut ChildSpawner) + Send + Sync + 'static) -> impl Bundle {
        let name = self.name.into_owned();
        let (background, padding, radius) = if self.prompt {
            (t.info_background, UiRect::axes(Val::Px(10.0), Val::Px(4.0)), 4.0)
        } else {
            (t.background, UiRect::new(Val::Px(10.0), Val::Px(4.0), Val::Px(4.0), Val::Px(4.0)), 6.0)
        };
        let shadow = !self.prompt;
        (
            Name::new(format!("{name}-row")),
            Node { position_type: PositionType::Absolute, top: Val::Px(self.top), left: Val::Px(0.0), right: Val::Px(0.0), justify_content: JustifyContent::Center, ..default() },
            Pickable::IGNORE,
            Children::spawn(SpawnWith(move |p: &mut ChildSpawner| {
                let mut bar = p.spawn((
                    Name::new(name),
                    // In a narrow area it wraps onto a second row rather than run under what is at
                    // the area's corners (the view cube).
                    Node {
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(4.0),
                        align_items: AlignItems::Center,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        max_width: Val::Percent(72.0),
                        padding,
                        border_radius: BorderRadius::all(Val::Px(radius)),
                        ..default()
                    },
                    BackgroundColor(background),
                ));
                if shadow {
                    bar.insert(BoxShadow::default());
                }
                bar.with_children(content);
            })),
        )
    }
}
