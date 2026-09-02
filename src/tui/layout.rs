use ratatui::layout::Rect;

pub const PREFERRED_WIDTH: u16 = 80;
pub const PREFERRED_HEIGHT: u16 = 24;
pub const MINIMUM_WIDTH: u16 = 60;
pub const MINIMUM_HEIGHT: u16 = 20;
const SHELL_MAX_WIDTH: u16 = 120;
const SHELL_MAX_HEIGHT: u16 = 38;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutMode {
    Preferred,
    Narrow,
    ResizeMessage,
}

impl LayoutMode {
    #[must_use]
    pub const fn for_area(area: Rect) -> Self {
        if area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT {
            Self::ResizeMessage
        } else if area.width < PREFERRED_WIDTH || area.height < PREFERRED_HEIGHT {
            Self::Narrow
        } else {
            Self::Preferred
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellLayout {
    pub title: Rect,
    pub mark: Rect,
    pub branch: Rect,
    pub status: Rect,
}

impl ShellLayout {
    #[must_use]
    pub const fn new(area: Rect, mode: LayoutMode) -> Option<Self> {
        if matches!(mode, LayoutMode::ResizeMessage) {
            return None;
        }
        let shell = centered(area, SHELL_MAX_WIDTH, SHELL_MAX_HEIGHT);
        let inner = Rect::new(
            shell.x.saturating_add(2),
            shell.y.saturating_add(1),
            shell.width.saturating_sub(4),
            shell.height.saturating_sub(2),
        );
        let title = Rect::new(inner.x, inner.y, inner.width, 3);
        let content_y = inner.y.saturating_add(3);
        let content_height = inner.height.saturating_sub(5);
        let status = Rect::new(
            inner.x,
            inner.y.saturating_add(inner.height.saturating_sub(2)),
            inner.width,
            2,
        );
        let (mark, branch) = match mode {
            LayoutMode::Preferred => {
                let gap = 2;
                let content_width = inner.width.saturating_sub(gap);
                let mark_width = content_width.saturating_mul(3) / 5;
                (
                    Rect::new(inner.x, content_y, mark_width, content_height),
                    Rect::new(
                        inner.x.saturating_add(mark_width).saturating_add(gap),
                        content_y,
                        content_width.saturating_sub(mark_width),
                        content_height,
                    ),
                )
            }
            LayoutMode::Narrow => {
                let mark_height = if content_height < 3 {
                    content_height
                } else {
                    3
                };
                let gap = if content_height > mark_height { 1 } else { 0 };
                (
                    Rect::new(inner.x, content_y, inner.width, mark_height),
                    Rect::new(
                        inner.x,
                        content_y.saturating_add(mark_height).saturating_add(gap),
                        inner.width,
                        content_height
                            .saturating_sub(mark_height)
                            .saturating_sub(gap),
                    ),
                )
            }
            LayoutMode::ResizeMessage => return None,
        };
        Some(Self {
            title,
            mark,
            branch,
            status,
        })
    }
}

#[must_use]
pub const fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = if width < area.width {
        width
    } else {
        area.width
    };
    let height = if height < area.height {
        height
    } else {
        area.height
    };
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_boundaries_match_the_product_contract() {
        assert_eq!(
            LayoutMode::for_area(Rect::new(0, 0, 80, 24)),
            LayoutMode::Preferred
        );
        assert_eq!(
            LayoutMode::for_area(Rect::new(0, 0, 79, 24)),
            LayoutMode::Narrow
        );
        assert_eq!(
            LayoutMode::for_area(Rect::new(0, 0, 60, 20)),
            LayoutMode::Narrow
        );
        assert_eq!(
            LayoutMode::for_area(Rect::new(0, 0, 59, 20)),
            LayoutMode::ResizeMessage
        );
        assert_eq!(
            LayoutMode::for_area(Rect::new(0, 0, 60, 19)),
            LayoutMode::ResizeMessage
        );
    }

    #[test]
    fn every_interactive_region_stays_inside_the_terminal() {
        for area in [
            Rect::new(0, 0, 80, 24),
            Rect::new(0, 0, 60, 20),
            Rect::new(0, 0, 160, 60),
        ] {
            let shell = ShellLayout::new(area, LayoutMode::for_area(area)).unwrap();
            for region in [shell.title, shell.mark, shell.branch, shell.status] {
                assert!(region.right() <= area.right());
                assert!(region.bottom() <= area.bottom());
            }
        }
        let narrow = ShellLayout::new(
            Rect::new(0, 0, MINIMUM_WIDTH, MINIMUM_HEIGHT),
            LayoutMode::Narrow,
        )
        .unwrap();
        assert!(narrow.branch.height >= 9, "all seven branch choices fit");
    }

    #[test]
    fn large_terminals_keep_the_shell_compact_and_centered() {
        let area = Rect::new(0, 0, 160, 60);
        let shell = ShellLayout::new(area, LayoutMode::Preferred).unwrap();
        let left = shell.title.x.min(shell.mark.x).min(shell.branch.x);
        let right = shell
            .status
            .right()
            .max(shell.mark.right())
            .max(shell.branch.right());
        let top = shell.title.y;
        let bottom = shell
            .status
            .bottom()
            .max(shell.mark.bottom())
            .max(shell.branch.bottom());

        assert!(right.saturating_sub(left) <= SHELL_MAX_WIDTH);
        assert!(bottom.saturating_sub(top) <= SHELL_MAX_HEIGHT);
        assert_eq!(
            left.saturating_sub(area.x),
            area.right().saturating_sub(right)
        );
        assert_eq!(
            top.saturating_sub(area.y),
            area.bottom().saturating_sub(bottom)
        );
    }
}
