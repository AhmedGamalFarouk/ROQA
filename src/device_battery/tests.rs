use super::*;

fn peripheral(name: &str, percent: Option<u8>) -> Report {
    Report {
        name: name.into(),
        connected: true,
        paired: true,
        percent,
        ..Report::default()
    }
}
fn child(container: &str, percent: Option<u8>) -> Report {
    Report {
        container: container.into(),
        percent,
        ..Report::default()
    }
}
#[test]
fn multiple_devices_are_named_sorted_and_never_get_an_invented_reading() {
    let containers = BTreeMap::from([
        ("mouse".into(), peripheral("Mouse", Some(0))),
        ("headset".into(), peripheral("Headset", Some(100))),
        ("keys".into(), peripheral("Keyboard", None)),
    ]);
    assert_eq!(
        rows(&containers, &BTreeMap::new()),
        [
            "Headset  \u{b7}  100%",
            "Keyboard  \u{b7}  Unavailable",
            "Mouse  \u{b7}  0%"
        ]
    );
}
#[test]
fn interfaces_merge_by_container_not_name_and_conflicts_are_unavailable() {
    let containers = BTreeMap::from([
        ("one".into(), peripheral("Mouse", None)),
        ("two".into(), peripheral("Mouse", None)),
    ]);
    let mut children = BTreeMap::from([
        ("a".into(), child("one", Some(60))),
        ("b".into(), child("one", Some(60))),
    ]);
    assert_eq!(
        rows(&containers, &children),
        ["Mouse  \u{b7}  60%", "Mouse  \u{b7}  Unavailable"]
    );
    children.insert("b".into(), child("one", Some(80)));
    assert_eq!(
        rows(&containers, &children),
        ["Mouse  \u{b7}  Unavailable", "Mouse  \u{b7}  Unavailable"]
    );
}
#[test]
fn connected_receiver_battery_is_included_without_pairing() {
    let containers = BTreeMap::from([(
        "usb".into(),
        Report {
            paired: false,
            ..peripheral("Receiver device", None)
        },
    )]);
    let children = BTreeMap::from([("hid".into(), child("usb", Some(75)))]);
    assert_eq!(
        rows(&containers, &children),
        ["Receiver device  \u{b7}  75%"]
    );
}
#[test]
fn local_disconnected_and_unidentified_wired_devices_are_excluded() {
    let containers = BTreeMap::from([
        (
            "local".into(),
            Report {
                local: true,
                ..peripheral("Computer", Some(90))
            },
        ),
        (
            "off".into(),
            Report {
                connected: false,
                ..peripheral("Headset", Some(90))
            },
        ),
        (
            "wired".into(),
            Report {
                paired: false,
                ..peripheral("USB device", None)
            },
        ),
    ]);
    assert!(rows(&containers, &BTreeMap::new()).is_empty());
}
#[test]
fn disconnect_reconnect_requires_a_fresh_battery_report() {
    let mut catalog = Catalog::default();
    catalog.store(true, "one", peripheral("Mouse", Some(60)), true);
    catalog.store(false, "child", child("one", Some(60)), true);
    catalog.store(
        true,
        "one",
        Report {
            connected: false,
            ..peripheral("Mouse", Some(60))
        },
        false,
    );
    assert!(rows(&catalog.containers, &catalog.devices).is_empty());
    catalog.store(true, "one", peripheral("Mouse", Some(60)), false);
    catalog.store(false, "child", child("one", Some(60)), false);
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Mouse  \u{b7}  Unavailable"]
    );
    catalog.store(false, "child", child("one", Some(55)), true);
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Mouse  \u{b7}  55%"]
    );
    catalog.remove(true, "one");
    assert!(rows(&catalog.containers, &catalog.devices).is_empty());
    catalog.store(true, "one", peripheral("Mouse", None), true);
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Mouse  \u{b7}  Unavailable"]
    );
}
#[test]
fn battery_removal_and_child_removal_clear_the_old_reading() {
    let mut catalog = Catalog::default();
    catalog.store(true, "one", peripheral("Keyboard", Some(40)), true);
    catalog.store(true, "one", peripheral("Keyboard", None), true);
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Keyboard  \u{b7}  Unavailable"]
    );
    catalog.store(false, "child", child("one", Some(35)), true);
    catalog.remove(false, "child");
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Keyboard  \u{b7}  Unavailable"]
    );
    assert_eq!(identity("{ABC-DEF}"), identity("abc-def"));
}

#[test]
fn windows_unknown_battery_sentinels_are_never_percentages() {
    assert_eq!(percentage(0), Some(0));
    assert_eq!(percentage(100), Some(100));
    assert_eq!(percentage(101), None);
    assert_eq!(percentage(255), None);
}

#[test]
fn receiver_with_unknown_or_removed_reading_stays_unavailable() {
    let mut catalog = Catalog::default();
    catalog.store(
        true,
        "receiver",
        Report {
            paired: false,
            ..peripheral("Receiver", None)
        },
        true,
    );
    catalog.store(
        false,
        "hid",
        Report {
            battery_reported: true,
            ..child("receiver", percentage(101))
        },
        true,
    );
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Receiver  \u{b7}  Unavailable"]
    );
    catalog.store(false, "hid", child("receiver", None), true);
    assert_eq!(
        rows(&catalog.containers, &catalog.devices),
        ["Receiver  \u{b7}  Unavailable"]
    );
    catalog.remove(false, "hid");
    assert!(rows(&catalog.containers, &catalog.devices).is_empty());
}
