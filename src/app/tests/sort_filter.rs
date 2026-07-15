//! Sort/filter tests.

use super::super::*;
use super::support::*;

#[test]
fn filter_narrows_and_clears() {
    let mut app = demo_app();
    let total = app.all_issues.len();
    // Cycle to the first status filter.
    app.cycle_filter();
    assert!(app.filter_status.is_some());
    let filtered = app.filter_status.clone().unwrap();
    assert!(app.issues.iter().all(|i| i.status == filtered));
    assert!(app.issues.len() <= total);
    // Cycle all the way back to "all".
    for _ in 0..20 {
        if app.filter_status.is_none() {
            break;
        }
        app.cycle_filter();
    }
    assert!(app.filter_status.is_none());
    assert_eq!(app.issues.len(), total);
}

#[test]
fn sort_reorders_and_preserves_selection() {
    let mut app = demo_app();
    // Select a known issue, then re-sort; selection should follow the key.
    let key = app.issues[2].key.clone();
    app.selected = 2;
    app.sort_key = SortKey::Key;
    app.sort_asc = true;
    app.recompute_view();
    assert_eq!(app.selected_issue().unwrap().key, key);
    // Ascending by key: keys are non-decreasing.
    let nums: Vec<u64> = app
        .issues
        .iter()
        .map(|i| i.key.rsplit('-').next().unwrap().parse().unwrap())
        .collect();
    assert!(nums.windows(2).all(|w| w[0] <= w[1]));
}
