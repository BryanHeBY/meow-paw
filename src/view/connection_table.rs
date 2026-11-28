//! The Connections table keeps counters and actions visible in both layouts.
use super::*;
use gpui_kit::base::TestTextExt;
use meow_paw::engine::controller::Connection;

const GAP: f32 = 12.;
const TRANSFER_WIDTH: f32 = 118.;
const COMPACT_TRANSFER_WIDTH: f32 = 152.;
const TWO_LINE_BELOW: f32 = 960.;

pub(super) struct Layout {
    wide: bool,
}

impl Layout {
    pub(super) fn for_viewport(viewport: Pixels) -> Self {
        Self {
            wide: viewport >= px(TWO_LINE_BELOW),
        }
    }

    pub(super) fn header(&self, locale: Locale, closed: bool) -> Option<Div> {
        self.wide.then(|| {
            let cells = [
                Message::DestinationColumn,
                Message::RouteColumn,
                Message::RuleColumn,
                Message::ConnectionTransfersColumn,
            ]
            .map(|message| section_label(locale.message(message)));
            row_base().bg(rgb(CHROME)).child(
                desktop_tracks(cells, div().into_any_element(), closed)
                    .id("connections-column-headings")
                    .test_support(),
            )
        })
    }

    pub(super) fn row(
        &self,
        cells: [Div; 4],
        actions: AnyElement,
        closed: bool,
        locale: Locale,
    ) -> Div {
        if self.wide {
            return row_base().child(desktop_tracks(cells, actions, closed));
        }

        let [destination, route, rule, transfers] = cells;
        // Keep the actionable first line separate at narrow sizes. All
        // original columns remain visible on the compact second line.
        row_base()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .child(weighted(destination, 1.))
                    .child(actions_slot(actions, closed)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(GAP))
                    .child(weighted(
                        compact_field(locale.message(Message::RouteColumn), route),
                        2.,
                    ))
                    .child(weighted(
                        compact_field(locale.message(Message::RuleColumn), rule),
                        1.,
                    ))
                    .child(
                        compact_field(
                            locale.message(Message::ConnectionTransfersColumn),
                            transfers,
                        )
                        .w(px(COMPACT_TRANSFER_WIDTH))
                        .flex_shrink_0(),
                    ),
            )
    }

    pub(super) fn transfers(&self, index: usize, connection: &Connection) -> Div {
        let direction = |key, arrow, value: Option<u64>, ink| {
            let shown = value.map(bytes).unwrap_or_else(|| "—".to_owned());
            let text = SharedString::from(format!("{arrow} {shown}"));
            div()
                .id((key, index))
                .test_support()
                .text_xs()
                .min_w_0()
                .truncate()
                .text_color(rgb(if value.is_some() { ink } else { FAINT }))
                .child(text.clone().test_text(text))
        };
        div()
            .flex()
            .when(self.wide, |row| row.flex_col().gap(px(1.)))
            .when(!self.wide, |row| row.flex_wrap().gap(px(5.)))
            .child(direction(
                "connection-row-download",
                "↓",
                connection.download,
                STREAM_DOWN,
            ))
            .child(direction(
                "connection-row-upload",
                "↑",
                connection.upload,
                STREAM_UP,
            ))
    }
}

fn row_base() -> Div {
    div()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(rgb(LINE))
        .text_sm()
}

fn weighted(cell: Div, grow: f32) -> Div {
    cell.flex_basis(px(0.)).flex_grow(grow).min_w_0()
}

fn actions_slot(actions: AnyElement, closed: bool) -> Div {
    div()
        .w(px(if closed { 80. } else { 132. }))
        .flex_shrink_0()
        .child(actions)
}

fn desktop_tracks(
    [destination, route, rule, transfers]: [Div; 4],
    actions: AnyElement,
    closed: bool,
) -> Div {
    div()
        .flex()
        .gap(px(GAP))
        .items_center()
        .child(weighted(destination, 2.))
        .child(weighted(route.text_color(rgb(FAINT)), 2.))
        .child(weighted(rule.text_color(rgb(FAINT)), 1.))
        .child(transfers.w(px(TRANSFER_WIDTH)).flex_shrink_0())
        .child(actions_slot(actions, closed))
}

fn compact_field(title: &'static str, content: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_color(rgb(FAINT))
        .child(div().text_xs().truncate().child(title))
        .child(content)
}

pub(super) fn value(value: &str) -> Div {
    div()
        .min_w_0()
        // The unabridged value remains available in Details and search.
        .truncate()
        .child(if value.is_empty() { "—" } else { value }.to_owned())
}
