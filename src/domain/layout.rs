/// Side-by-side placement of one block inside its overlap cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub column: usize,
    pub columns: usize,
}

/// Assigns columns to intervals `(start, end)` (in minutes). Overlapping intervals form a cluster
/// that shares the same column count, so each block takes `1 / columns` of the day width.
/// Returns one slot per input, in input order.
pub fn layout(intervals: &[(i64, i64)]) -> Vec<Slot> {
    let mut slots = vec![Slot { column: 0, columns: 1 }; intervals.len()];
    let mut order: Vec<usize> = (0..intervals.len()).collect();
    order.sort_by_key(|&i| (intervals[i].0, std::cmp::Reverse(intervals[i].1)));

    let mut column_ends: Vec<i64> = Vec::new();
    let mut cluster: Vec<usize> = Vec::new();
    let mut cluster_end = i64::MIN;

    for &i in &order {
        let (start, end) = intervals[i];
        if !cluster.is_empty() && start >= cluster_end {
            finish_cluster(&mut slots, &cluster, column_ends.len());
            cluster.clear();
            column_ends.clear();
            cluster_end = i64::MIN;
        }
        let column = match column_ends.iter().position(|&col_end| col_end <= start) {
            Some(c) => {
                column_ends[c] = end;
                c
            }
            None => {
                column_ends.push(end);
                column_ends.len() - 1
            }
        };
        slots[i].column = column;
        cluster.push(i);
        cluster_end = cluster_end.max(end);
    }
    if !cluster.is_empty() {
        finish_cluster(&mut slots, &cluster, column_ends.len());
    }
    slots
}

fn finish_cluster(slots: &mut [Slot], cluster: &[usize], columns: usize) {
    for &i in cluster {
        slots[i].columns = columns;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_overlapping_blocks_take_full_width() {
        let slots = layout(&[(0, 60), (120, 180)]);
        assert_eq!(slots[0], Slot { column: 0, columns: 1 });
        assert_eq!(slots[1], Slot { column: 0, columns: 1 });
    }

    #[test]
    fn overlapping_blocks_share_width() {
        let slots = layout(&[(540, 600), (570, 630), (600, 660)]);
        assert_eq!(slots[0], Slot { column: 0, columns: 2 });
        assert_eq!(slots[1], Slot { column: 1, columns: 2 });
        // Third starts when the first ends, so it reuses column 0.
        assert_eq!(slots[2], Slot { column: 0, columns: 2 });
    }

    #[test]
    fn cluster_width_is_the_maximum_concurrency() {
        let slots = layout(&[(0, 100), (10, 100), (20, 100), (200, 300)]);
        assert_eq!(slots[0].columns, 3);
        assert_eq!(slots[2].columns, 3);
        assert_eq!(slots[3], Slot { column: 0, columns: 1 });
    }

    #[test]
    fn empty_input() {
        assert!(layout(&[]).is_empty());
    }
}
