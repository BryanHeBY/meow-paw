//! A fixed DBusMenu tree. Hosts render the menu using the item's Menu path.
use super::{Locale, Signals};
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};
use zbus::zvariant::Value;

const SHOW: i32 = 1;
const QUIT: i32 = 2;
type Properties = HashMap<&'static str, Value<'static>>;
type Layout = (i32, Properties, Vec<Value<'static>>);

pub(super) struct Menu {
    signals: Arc<Signals>,
    locale: Locale,
}

impl Menu {
    pub(super) fn new(signals: Arc<Signals>, locale: Locale) -> Self {
        Self { signals, locale }
    }

    fn properties(&self, id: i32, names: &[String]) -> Properties {
        let label = match (id, self.locale) {
            (SHOW, Locale::En) => Some("Show"),
            (SHOW, Locale::ZhCn) => Some("显示"),
            (QUIT, Locale::En) => Some("Quit"),
            (QUIT, Locale::ZhCn) => Some("退出"),
            _ => None,
        };
        let mut props = HashMap::new();
        if let Some(label) = label
            && (names.is_empty() || names.iter().any(|name| name == "label"))
        {
            props.insert("label", Value::from(label));
        }
        if id == 0 && (names.is_empty() || names.iter().any(|name| name == "children-display")) {
            props.insert("children-display", Value::from("submenu"));
        }
        props
    }

    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Layout {
        let children = if id == 0 && depth != 0 {
            [SHOW, QUIT]
                .into_iter()
                .map(|child| {
                    Value::new((
                        child,
                        self.properties(child, names),
                        Vec::<Value<'static>>::new(),
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };
        (id, self.properties(id, names), children)
    }

    fn dispatch(&self, id: i32, event: &str) -> bool {
        if !matches!(id, 0 | SHOW | QUIT) {
            return false;
        }
        if event == "clicked" {
            match id {
                SHOW => self.signals.show.store(true, Ordering::Release),
                QUIT => self.signals.quit.store(true, Ordering::Release),
                _ => (),
            }
        }
        true
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }

    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, Layout)> {
        if !matches!(parent_id, 0 | SHOW | QUIT) {
            return Err(zbus::fdo::Error::InvalidArgs("Unknown menu item".into()));
        }
        Ok((1, self.layout(parent_id, recursion_depth, &property_names)))
    }
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let ids = if ids.is_empty() {
            vec![0, SHOW, QUIT]
        } else {
            ids
        };
        ids.into_iter()
            .filter(|id| matches!(*id, 0 | SHOW | QUIT))
            .map(|id| (id, self.properties(id, &property_names)))
            .collect()
    }
    fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<Value<'static>> {
        self.properties(id, &[name.to_owned()])
            .remove(name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Unknown menu property".into()))
    }
    fn event(
        &self,
        id: i32,
        event_id: &str,
        _data: Value<'_>,
        _timestamp: u32,
    ) -> zbus::fdo::Result<()> {
        if self.dispatch(id, event_id) {
            Ok(())
        } else {
            Err(zbus::fdo::Error::InvalidArgs("Unknown menu item".into()))
        }
    }
    fn event_group(&self, events: Vec<(i32, String, Value<'_>, u32)>) -> Vec<i32> {
        events
            .into_iter()
            .filter_map(|(id, event, _, _)| (!self.dispatch(id, &event)).then_some(id))
            .collect()
    }
    fn about_to_show(&self, id: i32) -> zbus::fdo::Result<bool> {
        if matches!(id, 0 | SHOW | QUIT) {
            Ok(false)
        } else {
            Err(zbus::fdo::Error::InvalidArgs("Unknown menu item".into()))
        }
    }
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (
            vec![],
            ids.into_iter()
                .filter(|id| !matches!(*id, 0 | SHOW | QUIT))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, OwnedValue, Type, serialized::Context, to_bytes};

    #[test]
    fn layout_serializes_to_protocol_shape_and_actions_are_scoped() {
        assert_eq!(Layout::SIGNATURE.to_string(), "(ia{sv}av)");
        let signals = Arc::new(Signals::default());
        let menu = Menu::new(signals.clone(), Locale::ZhCn);
        let layout = menu.layout(0, -1, &[]);
        let bytes = to_bytes(Context::new_dbus(LE, 0), &layout).unwrap();
        let (decoded, _): ((i32, HashMap<String, OwnedValue>, Vec<OwnedValue>), _) =
            bytes.deserialize().unwrap();
        assert_eq!(decoded.2.len(), 2);
        assert_eq!(decoded.2[0].value_signature().to_string(), "(ia{sv}av)");
        assert!(menu.dispatch(SHOW, "hovered"));
        assert!(!signals.take_show());
        assert!(menu.dispatch(SHOW, "clicked"));
        assert!(signals.take_show());
        assert!(menu.dispatch(QUIT, "clicked"));
        assert!(signals.take_quit());
        assert!(!menu.dispatch(99, "clicked"));
        assert!(menu.layout(0, 0, &[]).2.is_empty());
    }
}
