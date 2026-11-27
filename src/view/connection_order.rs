use super::{connection_transport, destination_label, route_matches};
use meow_paw::{engine::controller::Connection, i18n::Message};
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ConnectionOrder {
    #[default]
    Original,
    Destination,
    TotalData,
}

impl ConnectionOrder {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Original => Self::Destination,
            Self::Destination => Self::TotalData,
            Self::TotalData => Self::Original,
        }
    }

    pub(super) fn message(self) -> Message {
        match self {
            Self::Original => Message::ConnectionSortOriginal,
            Self::Destination => Message::ConnectionSortDestination,
            Self::TotalData => Message::ConnectionSortTotalData,
        }
    }
}

pub(super) struct ConnectionListRow<'a> {
    pub(super) index: usize,
    pub(super) connection: &'a Connection,
    pub(super) destination: String,
    pub(super) route: String,
}

fn labels(c: &Connection) -> (String, String) {
    let target = if c.metadata.host.is_empty() {
        &c.metadata.destination_ip
    } else {
        &c.metadata.host
    };
    (
        destination_label(target, &c.metadata.destination_port),
        c.chains.join(" / "),
    )
}

fn matches_labels(c: &Connection, destination: &str, route: &str, query: &str) -> bool {
    route_matches(destination, query)
        || route_matches(route, query)
        || route_matches(&c.rule, query)
        || route_matches(
            connection_transport(c.metadata.network.as_deref(), ""),
            query,
        )
}

pub(super) fn matches_connection(c: &Connection, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let (destination, route) = labels(c);
    matches_labels(c, &destination, &route, query)
}

pub(super) fn matching_connections<'a>(
    connections: impl Iterator<Item = &'a Connection>,
    query: &str,
    order: ConnectionOrder,
) -> Vec<ConnectionListRow<'a>> {
    let mut rows: Vec<_> = connections
        .enumerate()
        .filter_map(|(index, connection)| {
            let (destination, route) = labels(connection);
            matches_labels(connection, &destination, &route, query).then_some(ConnectionListRow {
                index,
                connection,
                destination,
                route,
            })
        })
        .collect();
    match order {
        ConnectionOrder::Original => {}
        ConnectionOrder::Destination => rows.sort_by_cached_key(|row| {
            let has_destination = !row.connection.metadata.host.is_empty()
                || !row.connection.metadata.destination_ip.is_empty();
            (!has_destination, row.destination.to_lowercase(), row.index)
        }),
        ConnectionOrder::TotalData => rows.sort_unstable_by_key(|row| {
            let total = total_bytes(row.connection);
            (total.is_none(), Reverse(total), row.index)
        }),
    }
    rows
}

fn total_bytes(c: &Connection) -> Option<u128> {
    // An unreported direction cannot be treated as zero. Widen before adding.
    Some(u128::from(c.upload?) + u128::from(c.download?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meow_paw::engine::controller::Metadata;

    fn row(id: &str, host: &str, upload: Option<u64>, download: Option<u64>) -> Connection {
        Connection {
            id: id.into(),
            metadata: Metadata {
                host: host.into(),
                ..Default::default()
            },
            upload,
            download,
            ..Default::default()
        }
    }

    #[test]
    fn connection_order_preserves_indices_with_duplicate_ids_and_case_folded_ties() {
        let sample = [
            row("duplicate", "Zulu", Some(1), Some(2)),
            row("middle", "Alpha", Some(2), Some(1)),
            row("duplicate", "ALPHA", Some(1), Some(2)),
            row("unspecified", "", None, None),
        ];
        let indices = |order| {
            matching_connections(sample.iter(), "", order)
                .into_iter()
                .map(|row| row.index)
                .collect::<Vec<_>>()
        };
        assert_eq!(indices(ConnectionOrder::Original), [0, 1, 2, 3]);
        assert_eq!(indices(ConnectionOrder::Destination), [1, 2, 0, 3]);
        assert_eq!(indices(ConnectionOrder::TotalData), [0, 1, 2, 3]);

        let filtered = matching_connections(sample.iter(), "alpha", ConnectionOrder::Destination);
        assert_eq!(
            filtered.iter().map(|row| row.index).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(std::ptr::eq(filtered[1].connection, &sample[2]));
        assert_eq!(filtered[1].connection.id, "duplicate");
        assert_eq!(filtered[1].destination, "ALPHA");
    }

    #[test]
    fn connection_total_order_widens_numbers_and_puts_unknown_after_real_zero() {
        let sample = [
            row("partial", "a", Some(u64::MAX), None),
            row("overflow", "b", Some(u64::MAX), Some(u64::MAX)),
            row("zero", "c", Some(0), Some(0)),
            row("missing", "d", None, None),
            row("max", "e", Some(u64::MAX), Some(0)),
            row("above-max", "f", Some(1), Some(u64::MAX)),
            row("second-zero", "g", Some(0), Some(0)),
        ];
        let ordered = matching_connections(sample.iter(), "", ConnectionOrder::TotalData);
        assert_eq!(
            ordered.into_iter().map(|row| row.index).collect::<Vec<_>>(),
            [1, 5, 4, 2, 6, 0, 3],
        );
        assert_eq!(total_bytes(&sample[1]), Some(u128::from(u64::MAX) * 2));
        assert_eq!(total_bytes(&sample[2]), Some(0));
        assert_eq!(total_bytes(&sample[0]), None);
    }
}
