/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use cool_asserts::assert_matches;
use semester::{classes, static_classes, Classes, StaticClasses};

#[test]
fn test_static_classes() {
    let classeses: Vec<_> = [true, false]
        .iter()
        .flat_map(|&c1| [true, false].iter().map(move |&c2| (c1, c2)))
        .map(|(c1, c2)| {
            static_classes! {
                "class1",
                "maybe1": c1,
                "maybe2": c2,
                "both": c1 && c2,
                "never": false,
                "always": true,
            }
        })
        .collect();

    assert_matches!(
        classeses.iter().map(|classes| classes.as_str()),
        [
            "class1 maybe1 maybe2 both always",
            "class1 maybe1 always",
            "class1 maybe2 always",
            "class1 always",
        ]
    )
}

#[test]
fn test_dynamic_classes() {
    let classeses: Vec<_> = [true, false]
        .iter()
        .flat_map(|&c1| [true, false].iter().map(move |&c2| (c1, c2)))
        .map(|(c1, c2)| {
            classes! {
                "class1",
                "class2",
                "maybe1": c1,
                "maybe2": c2,
                "both": c1 && c2,
                "never": false,
                "always": true,
            }
        })
        .collect();

    assert_matches!(
        classeses.iter(),
        [
            classes if classes.render() == "class1 class2 maybe1 maybe2 both always" && classes.len() == 6,
            classes if classes.render() == "class1 class2 maybe1 always" && classes.len() == 4,
            classes if classes.render() == "class1 class2 maybe2 always" && classes.len() == 4,
            classes if classes.render() == "class1 class2 always" && classes.len() == 3,
        ]
    )
}

#[test]
fn test_all_conditional() {
    let b1 = true;
    let b2 = true;

    let classes = classes!("class1": b1, "class2": b2, "both": b1 && b2);
    assert_eq!(classes.render(), "class1 class2 both");
    assert_eq!(classes.len(), 3)
}

#[test]
fn test_dynamic_no_alloc() {
    let classes = classes!(
        "class1",
        "class2",
        "maybe": 10 == 12,
    );

    assert_eq!(classes.try_as_str(), Some("class1 class2"))
}

#[test]
fn test_fixed_static() {
    let classes = static_classes!("class1", "class2", "class3");

    assert_eq!(classes.as_str(), "class1 class2 class3");
    assert_eq!(classes.class_set(), &["class1", "class2", "class3"]);
    assert_eq!(classes.to_string(), "class1 class2 class3");
}

#[test]
fn test_fixed_dynamic() {
    let classes = classes!("class1", "class2", "class3");

    assert_eq!(classes.as_str(), "class1 class2 class3");
    assert_eq!(classes.class_set(), &["class1", "class2", "class3"]);
    assert_eq!(classes.to_string(), "class1 class2 class3")
}
