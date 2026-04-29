//! Ratatui layout: overview, network, storage, diagnostics, help.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Stylize};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs};
use ratatui::Frame;

use crate::app::{AppState, Page};
use crate::inventory::api::ApiStatus;
use crate::inventory::system::Health;
use crate::theme;

fn health_color(h: &Health) -> Color {
    match h {
        Health::Ok => theme::good(),
        Health::Degraded | Health::Unknown => theme::warn(),
        Health::Critical => theme::bad(),
    }
}

fn hlabel(h: &Health) -> &'static str {
    match h {
        Health::Ok => "OK",
        Health::Degraded => "Degraded",
        Health::Unknown => "Unknown",
        Health::Critical => "Critical",
    }
}

pub fn draw(f: &mut Frame<'_>, app: &AppState) {
    let area = f.area();
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
    if ch.len() < 4 {
        return;
    }
    let brand = ch[0];
    let tabbar = ch[1];
    let body = ch[2];
    let foot = ch[3];

    let title = Line::from(vec![
        Span::styled(
            " kcore hypervisor ",
            theme::title_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{}", app.snapshot.meta.version),
            Style::default().fg(theme::muted()),
        ),
    ]);
    f.render_widget(
        Paragraph::new(title).block(Block::new().bg(theme::bg())),
        brand,
    );

    let tab_ix = app.page as usize;
    let tabs = Tabs::new(
        ["Overview", "Network", "Storage", "Diagnostics", "Help"]
            .iter()
            .map(|s| Line::from(Span::styled(*s, Style::default().fg(theme::text()))))
            .collect::<Vec<Line>>(),
    )
    .select(tab_ix)
    .style(Style::default().fg(theme::muted()))
    .highlight_style(
        Style::default()
            .fg(theme::accent())
            .add_modifier(Modifier::BOLD),
    )
    .divider(Span::raw(" │ "));
    f.render_widget(
        tabs.block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::accent())),
        ),
        tabbar,
    );

    let (body, logo) = carve_logo_area(body);
    match app.page {
        Page::Overview => draw_overview(f, body, app),
        Page::Network => draw_network(f, app, body),
        Page::Storage => draw_storage(f, app, body),
        Page::Diagnostics => draw_diagnostics(f, body, app),
        Page::Help => draw_help(f, body, app.dev),
    }
    if let Some(logo) = logo {
        draw_logo(f, logo);
    }

    let footer = Line::from(vec![
        Span::styled("Tab/←/→  ", theme::muted()),
        Span::styled("r refresh  ", theme::muted()),
        Span::styled("? help  ", theme::muted()),
        Span::styled(
            if app.dev {
                "q quit (dev) "
            } else {
                "q disabled  "
            },
            theme::muted(),
        ),
    ]);
    f.render_widget(
        Paragraph::new(footer)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme::accent())),
            )
            .style(Style::default().fg(theme::muted())),
        foot,
    );
}

fn carve_logo_area(area: Rect) -> (Rect, Option<Rect>) {
    const LOGO_WIDTH: u16 = 52;
    const LOGO_HEIGHT: u16 = 8;

    if area.width < 64 || area.height < 20 {
        return (area, None);
    }

    let logo = Rect {
        x: area.x + area.width - LOGO_WIDTH,
        y: area.y + area.height - LOGO_HEIGHT,
        width: LOGO_WIDTH,
        height: LOGO_HEIGHT,
    };
    let content = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height - LOGO_HEIGHT,
    };
    (content, Some(logo))
}

fn draw_logo(f: &mut Frame<'_>, area: Rect) {
    const LOGO: &[&str] = &[
        "██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗",
        "██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝",
        "█████╔╝ ██║      ██║   ██║██████╔╝█████╗  ",
        "██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝  ",
        "██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗",
        "╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝",
    ];
    let lines = LOGO
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                *line,
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect::<Vec<_>>();

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_overview(f: &mut Frame<'_>, area: Rect, app: &AppState) {
    let m = &app.snapshot.meta;
    let hc = health_color(&m.health);
    let lines = vec![
        Line::from(vec![
            Span::styled("Node: ", theme::muted()),
            Span::styled(m.hostname.clone(), theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Version: ", theme::muted()),
            Span::styled(m.version.clone(), theme::text()),
            Span::raw("  "),
            Span::styled("Build: ", theme::muted()),
            Span::styled(m.build_id.clone(), theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Uptime: ", theme::muted()),
            Span::styled(m.uptime_str.clone(), theme::text()),
            Span::raw("  "),
            Span::styled("Time: ", theme::muted()),
            Span::styled(m.local_time.clone(), theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Cluster: ", theme::muted()),
            Span::styled(m.cluster_name.clone(), theme::text()),
            Span::raw("  "),
            Span::styled("Role: ", theme::muted()),
            Span::styled(m.node_role.clone(), theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Health: ", theme::muted()),
            Span::styled(
                hlabel(&m.health).to_string(),
                Style::default().fg(hc).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("API endpoint: ", theme::muted()),
            Span::styled(m.api_endpoint.clone(), theme::text()),
            Span::raw("  "),
            Span::styled("Status: ", theme::muted()),
            Span::styled(
                match &m.api_status {
                    ApiStatus::Unavailable => "unavailable",
                    ApiStatus::Reachable { .. } => "available",
                }
                .to_string(),
                theme::text(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Management: ", theme::muted()),
            Span::styled(m.management_url.clone(), theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Local login: ", theme::muted()),
            Span::styled(
                m.local_login.to_string(),
                Style::default()
                    .fg(theme::warn())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Remote: ", theme::muted()),
            Span::styled(
                m.remote_hint.clone(),
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Overview ")
                .title_style(theme::title_style())
                .border_style(Style::default().fg(theme::accent())),
        ),
        area,
    );
}

fn table_header_net() -> Row<'static> {
    Row::new(vec![
        Cell::from("Iface"),
        Cell::from("State"),
        Cell::from("MAC"),
        Cell::from("IPv4"),
        Cell::from("IPv6"),
        Cell::from("MTU"),
        Cell::from("Speed"),
        Cell::from("Driver"),
        Cell::from("def"),
        Cell::from("mgmt"),
    ])
    .style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme::accent()),
    )
    .height(1)
}

fn table_rows_net(app: &AppState) -> Vec<Row<'_>> {
    app.snapshot
        .nics
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let style = if i == app.network_sel {
                Style::default()
                    .fg(theme::text())
                    .bg(Color::Rgb(26, 36, 58))
            } else {
                Style::default().fg(theme::text())
            };
            Row::new(vec![
                Cell::from(n.name.clone()),
                Cell::from(n.oper_state.clone()),
                Cell::from(n.mac.clone()),
                Cell::from(n.ipv4.clone()),
                Cell::from(n.ipv6.clone()),
                Cell::from(n.mtu.clone()),
                Cell::from(n.speed.clone()),
                Cell::from(n.driver.clone()),
                Cell::from(if n.default_route { "•" } else { "—" }),
                Cell::from(if n.management { "•" } else { "—" }),
            ])
            .style(style)
        })
        .collect()
}

fn draw_network(f: &mut Frame<'_>, app: &AppState, area: Rect) {
    let rows = table_rows_net(app);
    let trows = if rows.is_empty() {
        vec![Row::new(
            (0..10).map(|_| Cell::from("—")).collect::<Vec<_>>(),
        )]
    } else {
        rows
    };
    let table = Table::new(
        trows,
        [
            Constraint::Min(6),
            Constraint::Min(5),
            Constraint::Min(14),
            Constraint::Min(10),
            Constraint::Min(14),
            Constraint::Min(4),
            Constraint::Min(10),
            Constraint::Min(6),
            Constraint::Min(3),
            Constraint::Min(3),
        ],
    )
    .header(table_header_net());
    f.render_widget(
        table
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Network inventory ")
                    .title_style(theme::title_style())
                    .border_style(Style::default().fg(theme::accent())),
            )
            .column_spacing(1),
        area,
    );
}

fn table_header_disk() -> Row<'static> {
    Row::new(vec![
        Cell::from("Name"),
        Cell::from("Path"),
        Cell::from("Model"),
        Cell::from("Serial"),
        Cell::from("Size"),
        Cell::from("Kind"),
        Cell::from("RO"),
        Cell::from("Mounts"),
        Cell::from("Health"),
        Cell::from("Role"),
    ])
    .style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme::accent()),
    )
}

fn table_rows_disk(app: &AppState) -> Vec<Row<'_>> {
    app.snapshot
        .disks
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let style = if i == app.storage_sel {
                Style::default()
                    .fg(theme::text())
                    .bg(Color::Rgb(26, 36, 58))
            } else {
                Style::default().fg(theme::text())
            };
            Row::new(vec![
                Cell::from(d.name.clone()),
                Cell::from(d.path.clone()),
                Cell::from(d.model.clone()),
                Cell::from(d.serial.clone()),
                Cell::from(d.size_text.clone()),
                Cell::from(d.kind.clone()),
                Cell::from(d.ro.clone()),
                Cell::from(d.mountpoints.clone()),
                Cell::from(d.health.clone()),
                Cell::from(d.usage_role.clone()),
            ])
            .style(style)
        })
        .collect()
}

fn draw_storage(f: &mut Frame<'_>, app: &AppState, area: Rect) {
    let rows = table_rows_disk(app);
    let trows = if rows.is_empty() {
        vec![Row::new(
            (0..10).map(|_| Cell::from("—")).collect::<Vec<_>>(),
        )]
    } else {
        rows
    };
    let table = Table::new(
        trows,
        [
            Constraint::Min(6),
            Constraint::Min(8),
            Constraint::Min(10),
            Constraint::Min(8),
            Constraint::Min(9),
            Constraint::Min(5),
            Constraint::Min(2),
            Constraint::Min(12),
            Constraint::Min(5),
            Constraint::Min(8),
        ],
    )
    .header(table_header_disk());
    f.render_widget(
        table
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Storage inventory ")
                    .title_style(theme::title_style())
                    .border_style(Style::default().fg(theme::accent())),
            )
            .column_spacing(1),
        area,
    );
}

fn draw_diagnostics(f: &mut Frame<'_>, area: Rect, app: &AppState) {
    let mut line = String::new();
    for s in &app.snapshot.diag {
        line.push_str(&format!(" {}: {}  │", s.name, s.status));
    }
    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" kcore services (local) ")
                .title_style(theme::title_style())
                .border_style(Style::default().fg(theme::accent())),
        ),
        area,
    );
}

fn draw_help(f: &mut Frame<'_>, area: Rect, _dev: bool) {
    let t = "kcore hypervisor appliance — local display is read-only.\
        \nUse the management URL and kcorectl from a trusted machine.\
        \nReboot, shutdown, and root shell are intentionally unavailable here.\
        \n\nSecurity: protect boot (UEFI, GRUB), mask extra getty, prefer SSH.\
        \nSee: docs/appliance-console.md in the kcore source tree.\n";
    f.render_widget(
        Paragraph::new(t).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .title_style(theme::title_style())
                .border_style(Style::default().fg(theme::accent())),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_area_is_reserved_at_bottom_right() {
        let area = Rect::new(0, 0, 120, 32);

        let (content, logo) = carve_logo_area(area);

        assert_eq!(content, Rect::new(0, 0, 120, 24));
        assert_eq!(logo, Some(Rect::new(68, 24, 52, 8)));
    }

    #[test]
    fn logo_area_is_skipped_on_small_consoles() {
        let area = Rect::new(0, 0, 63, 20);

        let (content, logo) = carve_logo_area(area);

        assert_eq!(content, area);
        assert_eq!(logo, None);
    }

    #[test]
    fn logo_area_is_available_on_standard_tty_size() {
        let area = Rect::new(0, 0, 80, 22);

        let (content, logo) = carve_logo_area(area);

        assert_eq!(content, Rect::new(0, 0, 80, 14));
        assert_eq!(logo, Some(Rect::new(28, 14, 52, 8)));
    }
}
