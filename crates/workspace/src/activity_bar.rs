use std::str::FromStr;

use gpui::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription,
    WeakEntity, Window, px,
};
use icons::IconName;
use settings::{
    ActivityBarCustomItem, ActivityBarItem, ActivityBarSide, Settings, SettingsStore,
};
use ui::prelude::*;
use ui::Tooltip;
use crate::{
    Dock, Workspace, WorkspaceSettings,
    dock::{DockPanelButton, PanelButtonStyle, render_panel_button},
};

pub(crate) const ACTIVITY_BAR_WIDTH: gpui::Pixels = px(48.);

pub(crate) struct ActivityBar {
    workspace: WeakEntity<Workspace>,
    overlay_layout: bool,
    _subscriptions: Vec<Subscription>,
    _dock_subscriptions: Vec<Subscription>,
}

impl ActivityBar {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe_global::<SettingsStore>(|_, cx| cx.notify()),
        ];

        let mut this = Self {
            workspace: workspace.downgrade(),
            overlay_layout: true,
            _subscriptions: subscriptions,
            _dock_subscriptions: Vec::new(),
        };
        this.observe_workspace_docks(workspace, cx);
        this
    }

    pub(crate) fn set_overlay_layout(&mut self, overlay_layout: bool, cx: &mut Context<Self>) {
        if self.overlay_layout != overlay_layout {
            self.overlay_layout = overlay_layout;
            cx.notify();
        }
    }

    pub(crate) fn side(cx: &App) -> ActivityBarSide {
        WorkspaceSettings::get_global(cx).activity_bar.side
    }

    pub(crate) fn has_items(cx: &App) -> bool {
        !WorkspaceSettings::get_global(cx)
            .activity_bar
            .items
            .is_empty()
    }

    pub(crate) fn set_workspace(&mut self, workspace: Entity<Workspace>, cx: &mut Context<Self>) {
        if self
            .workspace
            .upgrade()
            .is_some_and(|current| current == workspace)
        {
            return;
        }

        self.workspace = workspace.downgrade();
        self.observe_workspace_docks(workspace, cx);
        cx.notify();
    }

    fn observe_workspace_docks(&mut self, workspace: Entity<Workspace>, cx: &mut Context<Self>) {
        self._dock_subscriptions.clear();

        let docks = workspace
            .read(cx)
            .all_docks()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        for dock in docks {
            self._dock_subscriptions
                .push(cx.observe(&dock, |_, _, cx| cx.notify()));
        }
    }

    fn ordered_panel_buttons(&self, cx: &App) -> Vec<DockPanelButton> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };

        workspace
            .read(cx)
            .all_docks()
            .into_iter()
            .flat_map(|dock| Dock::panel_buttons(dock, cx))
            .collect()
    }

    fn resolved_slot_elements(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let items = WorkspaceSettings::get_global(cx).activity_bar.items.clone();
        let available_buttons = self.ordered_panel_buttons(cx);

        items
            .into_iter()
            .filter_map(|item| match item {
                ActivityBarItem::Builtin(builtin) => {
                    let panel_key = builtin.panel_key();
                    let button = available_buttons
                        .iter()
                        .find(|button| button.panel_key() == panel_key)
                        .cloned()?;
                    render_panel_button(
                        button,
                        PanelButtonStyle::ActivityBar {
                            side: Self::side(cx),
                        },
                        window,
                        cx,
                    )
                }
                ActivityBarItem::Custom(custom) => {
                    render_custom_activity_button(custom, Self::side(cx), window, cx)
                }
            })
            .collect()
    }
}

fn render_custom_activity_button(
    item: ActivityBarCustomItem,
    _side: ActivityBarSide,
    _window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let Ok(icon) = IconName::from_str(item.icon.trim()) else {
        log::warn!(
            "activity bar: unknown icon {:?} for {:?}",
            item.icon,
            item.action
        );
        return None;
    };

    let action_name = item.action.as_ref().to_string();
    let tooltip: SharedString = item
        .tooltip
        .clone()
        .unwrap_or_else(|| action_name.clone())
        .into();
    let control_id = SharedString::from(format!("activity_bar_custom:{}:{}", action_name, item.icon));

    let size = ACTIVITY_BAR_WIDTH;
    let action_for_tooltip = cx.build_action(&action_name, None).ok();

    let mut button =
        IconButton::new(control_id.clone(), icon)
            .size(ButtonSize::None)
            .width(size)
            .height(size.into())
            .icon_size(IconSize::Custom(rems_from_px(22.)))
            .on_click({
                let action_name = action_name.clone();
                move |_, window, cx| match cx.build_action(&action_name, None) {
                    Ok(action) => window.dispatch_action(action, cx),
                    Err(err) => log::warn!("activity bar: {action_name}: {err}"),
                }
            });

    button = if let Some(action) = action_for_tooltip.as_ref() {
        button.tooltip({
            let action = action.boxed_clone();
            let tooltip = tooltip.clone();
            move |_window, cx| Tooltip::for_action(tooltip.clone(), action.as_ref(), cx)
        })
    } else {
        button.tooltip({
            let tooltip = tooltip.clone();
            move |_window, cx| Tooltip::simple(tooltip.clone(), cx)
        })
    };

    Some(div().relative().size(size).child(button).into_any_element())
}

impl Render for ActivityBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = Self::side(cx);
        let top_padding = ui::utils::platform_title_bar_height(window);
        let buttons = self.resolved_slot_elements(window, cx);
        let colors = cx.theme().colors();

        let button_column = v_flex()
            .w(ACTIVITY_BAR_WIDTH)
            .h_full()
            .items_center()
            .pb_1()
            .bg(colors
                .title_bar_background
                .blend(colors.panel_background.opacity(0.25)))
            .when(side == ActivityBarSide::Left, |this| {
                this.border_r_1().border_color(colors.border)
            })
            .when(side == ActivityBarSide::Right, |this| {
                this.border_l_1().border_color(colors.border)
            })
            .children(buttons);

        if self.overlay_layout {
            div()
                .id("activity-bar")
                .absolute()
                .top(top_padding)
                .bottom_0()
                .w(ACTIVITY_BAR_WIDTH)
                .flex_shrink_0()
                .when(side == ActivityBarSide::Left, |el| el.left_0())
                .when(side == ActivityBarSide::Right, |el| el.right_0())
                .child(button_column)
        } else {
            div()
                .id("activity-bar")
                .relative()
                .w(ACTIVITY_BAR_WIDTH)
                .h_full()
                .flex_shrink_0()
                .bg(colors.title_bar_background)
                .child(button_column.absolute().top(top_padding).bottom_0().left_0())
        }
    }
}
