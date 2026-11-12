//! A stable insertion sort for the short lists the host keeps (a desktop's windows, the monitors, the timers).
//! Each distinct `slice::sort_by` call costs several KB of release binary; this costs a few hundred bytes.
use std::cmp::Ordering;

/// Sorts `list` by `cmp`, keeping equal elements in order, like `slice::sort_by`.
// shortcut: O(n^2) compares, fine for lists of a few dozen; use `sort_by` if one can reach thousands.
pub fn by<T>(list: &mut [T], mut cmp: impl FnMut(&T, &T) -> Ordering) {
    for at in 1..list.len() {
        let mut to = at;
        while to > 0 && cmp(&list[to - 1], &list[to]) == Ordering::Greater {
            list.swap(to - 1, to);
            to -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_stably_and_handles_short_lists() {
        let mut list = [(3, 'a'), (1, 'b'), (3, 'c'), (2, 'd'), (1, 'e')];
        by(&mut list, |a, b| a.0.cmp(&b.0));
        assert_eq!(list, [(1, 'b'), (1, 'e'), (2, 'd'), (3, 'a'), (3, 'c')]);
        let mut empty: [(i32, char); 0] = [];
        by(&mut empty, |a, b| a.0.cmp(&b.0));
        let mut one = [7];
        by(&mut one, |a, b| a.cmp(b));
        assert_eq!(one, [7]);
    }
}
