//! Owner-side VT state. Views receive complete screens, never replay partial escape streams.
use alacritty_terminal::{
    Term,
    event::VoidListener,
    grid::Dimensions,
    index::{Column, Line, Point},
    term::Config,
    vte::ansi::Processor,
};
struct Size(usize, usize);
impl Dimensions for Size {
    fn columns(&self) -> usize {
        self.0
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn total_lines(&self) -> usize {
        self.1
    }
}
pub struct Screen {
    term: Term<VoidListener>,
    parser: Processor,
}
impl Default for Screen {
    fn default() -> Self {
        Self {
            term: Term::new(Config::default(), &Size(80, 24), VoidListener),
            parser: Processor::new(),
        }
    }
}
impl Screen {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(Size(cols as usize, rows as usize));
    }
    pub fn dimensions(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }
    pub fn cursor(&self) -> (u16, u16) {
        let p = self.term.grid().cursor.point;
        (p.column.0 as u16, p.line.0.max(0) as u16)
    }
    pub fn lines(&self) -> Vec<String> {
        (0..self.term.screen_lines())
            .map(|row| {
                let mut line = String::new();
                for col in 0..self.term.columns() {
                    let cell = &self.term.grid()[Point::new(Line(row as i32), Column(col))];
                    if cell
                        .flags
                        .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    line.push(cell.c);
                    if let Some(extra) = cell.zerowidth() {
                        line.extend(extra.iter().take(8));
                    }
                }
                line.trim_end().to_owned()
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn erase_and_fragmented_utf8_are_screen_state() {
        let mut s = Screen::default();
        s.feed(b"old text\r\x1b[2K");
        s.feed(&[0xe2, 0x9c]);
        s.feed(&[0x93]);
        assert_eq!(s.lines()[0], "✓");
    }
    #[test]
    fn alternate_screen_restores_primary() {
        let mut s = Screen::default();
        s.feed(b"primary\x1b[?1049hsecondary");
        assert!(s.lines().join("").contains("secondary"));
        s.feed(b"\x1b[?1049l");
        assert_eq!(s.lines()[0], "primary");
    }
}
