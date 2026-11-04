//! Pure bounded navigation over borrowed diagnostic lines. No report copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Rows(i32),
    Pages(i32),
    Start,
    End,
}
pub fn last_start(total: usize, rows: usize) -> usize {
    total.saturating_sub(rows.max(1))
}
pub fn navigate(start: usize, total: usize, rows: usize, command: Command) -> usize {
    let maximum = last_start(total, rows);
    let start = start.min(maximum);
    let (count, step) = match command {
        Command::Start => return 0,
        Command::End => return maximum,
        Command::Rows(count) => (count, 1),
        // Keep one line of context across pages; tiny viewports still advance.
        Command::Pages(count) => (count, rows.saturating_sub(1).max(1)),
    };
    let distance = (count.unsigned_abs() as usize).saturating_mul(step);
    if count < 0 {
        start.saturating_sub(distance)
    } else {
        start.saturating_add(distance).min(maximum)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_overlap_and_end_shows_a_full_last_page() {
        assert_eq!(navigate(0, 23, 5, Command::Pages(1)), 4);
        assert_eq!(navigate(4, 23, 5, Command::Pages(-1)), 0);
        assert_eq!(navigate(0, 23, 5, Command::End), 18);
        assert_eq!(navigate(18, 23, 5, Command::Rows(1)), 18);
        assert_eq!(navigate(18, 23, 5, Command::Start), 0);
    }
    #[test]
    fn empty_short_and_single_line_viewports_are_bounded() {
        for total in [0, 1, 4] {
            assert_eq!(navigate(100, total, 5, Command::End), 0);
        }
        assert_eq!(navigate(0, 10, 0, Command::Pages(1)), 1);
        assert_eq!(navigate(0, 10, 1, Command::Pages(1)), 1);
        assert_eq!(navigate(9, 10, 1, Command::End), 9);
    }
    #[test]
    fn resize_and_extreme_input_never_wrap_or_leave_a_blank_tail() {
        assert_eq!(navigate(18, 23, 12, Command::Rows(0)), 11);
        assert_eq!(
            navigate(0, usize::MAX, usize::MAX, Command::Pages(i32::MAX)),
            0
        );
        assert_eq!(navigate(0, usize::MAX, 5, Command::Rows(i32::MIN)), 0);
        assert_eq!(navigate(10, 23, 5, Command::Pages(i32::MAX)), 18);
        assert_eq!(navigate(10, 23, 5, Command::Pages(i32::MIN)), 0);
    }
}
