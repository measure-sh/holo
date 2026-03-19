mod theme;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    text::Text,
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};

fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                break;
            }
        }
    }
    Ok(())
}

fn render(frame: &mut Frame) {
    let area = frame.area();

    let bg = Block::default().style(Style::new().bg(theme::BG));
    frame.render_widget(bg, area);

    let [_, center, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    let text = Text::from("Hello, msh!")
        .style(Style::new().fg(theme::ACCENT).bg(theme::BG));
    let paragraph = Paragraph::new(text).centered();
    frame.render_widget(paragraph, center);
}
