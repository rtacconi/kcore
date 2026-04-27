//! Color palette: dark background, cyan/blue accent, health colors.

use ratatui::style::{Color, Modifier, Style};

pub fn bg() -> Color {
    Color::Rgb(12, 18, 40)
}

pub fn accent() -> Color {
    Color::Rgb(99, 180, 255)
}

pub fn text() -> Color {
    Color::Rgb(230, 233, 240)
}

pub fn muted() -> Color {
    Color::Rgb(140, 150, 170)
}

pub fn good() -> Color {
    Color::Rgb(80, 200, 120)
}

pub fn warn() -> Color {
    Color::Rgb(255, 200, 100)
}

pub fn bad() -> Color {
    Color::Rgb(255, 100, 100)
}

pub fn title_style() -> Style {
    Style::default().fg(accent()).add_modifier(Modifier::BOLD)
}

pub fn health_style(s: &str) -> Style {
    let c = if s == "OK" {
        good()
    } else if s == "Degraded" {
        warn()
    } else if s == "Critical" {
        bad()
    } else {
        warn()
    };
    Style::default().fg(c).add_modifier(Modifier::BOLD)
}
