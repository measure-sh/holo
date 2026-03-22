const LEVELS: [Option<char>; 7] = [
    None,
    Some('V'),
    Some('D'),
    Some('I'),
    Some('W'),
    Some('E'),
    Some('F'),
];

pub struct LogcatFilter {
    pub tag: String,
    pub search: String,
    pub level: Option<char>,
}

impl LogcatFilter {
    fn new() -> Self {
        Self {
            tag: String::new(),
            search: String::new(),
            level: None,
        }
    }
}

pub struct LogcatState {
    pub filter: LogcatFilter,
    pub scroll: usize,
    pub copied_at: Option<std::time::Instant>,
}

impl LogcatState {
    pub fn new() -> Self {
        Self {
            filter: LogcatFilter::new(),
            scroll: 0,
            copied_at: None,
        }
    }

    pub fn reset(&mut self) {
        self.filter = LogcatFilter::new();
        self.scroll = 0;
    }

    pub fn cycle_level(&mut self, forward: bool) {
        let current = LEVELS.iter().position(|l| *l == self.filter.level).unwrap_or(0);
        let next = if forward {
            (current + 1) % LEVELS.len()
        } else {
            (current + LEVELS.len() - 1) % LEVELS.len()
        };
        self.filter.level = LEVELS[next];
    }

    pub fn clamp_scroll(&mut self, total_lines: usize, visible_height: usize) {
        let max = total_lines.saturating_sub(visible_height);
        self.scroll = self.scroll.min(max);
    }

    pub fn adjust_scroll_for_new_lines(&mut self, count: usize) {
        if self.scroll > 0 {
            self.scroll += count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_empty_filter_and_zero_scroll() {
        let state = LogcatState::new();
        assert_eq!(state.filter.tag, "");
        assert_eq!(state.filter.search, "");
        assert_eq!(state.filter.level, None);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn reset_clears_filter_and_scroll() {
        let mut state = LogcatState::new();
        state.filter.tag = "MyTag".into();
        state.filter.search = "error".into();
        state.filter.level = Some('E');
        state.scroll = 5;
        state.reset();
        assert_eq!(state.filter.tag, "");
        assert_eq!(state.filter.search, "");
        assert_eq!(state.filter.level, None);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn cycle_level_forward() {
        let mut state = LogcatState::new();
        assert_eq!(state.filter.level, None);
        state.cycle_level(true);
        assert_eq!(state.filter.level, Some('V'));
        state.cycle_level(true);
        assert_eq!(state.filter.level, Some('D'));
    }

    #[test]
    fn cycle_level_backward() {
        let mut state = LogcatState::new();
        state.cycle_level(false);
        assert_eq!(state.filter.level, Some('F'));
    }

    #[test]
    fn cycle_level_wraps() {
        let mut state = LogcatState::new();
        for _ in 0..7 {
            state.cycle_level(true);
        }
        assert_eq!(state.filter.level, None);
    }

    #[test]
    fn clamp_scroll_limits_to_max() {
        let mut state = LogcatState::new();
        state.scroll = 100;
        state.clamp_scroll(50, 20);
        assert_eq!(state.scroll, 30);
    }

    #[test]
    fn adjust_scroll_increments_when_scrolled() {
        let mut state = LogcatState::new();
        state.scroll = 5;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 8);
    }

    #[test]
    fn adjust_scroll_noop_when_at_bottom() {
        let mut state = LogcatState::new();
        state.scroll = 0;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 0);
    }
}
