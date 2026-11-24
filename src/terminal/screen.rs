use super::{Flow, FlowError, microphone::Microphone};
use crate::startup::OllamaLogs;
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};

pub(super) struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    pub(super) title: String,
    pub(super) lines: Vec<String>,
}

impl Screen {
    pub(super) fn enter() -> io::Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        enable_raw_mode()?;
        let mut screen = Self {
            terminal,
            title: String::from("Koin"),
            lines: Vec::new(),
        };
        execute!(
            screen.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste
        )?;
        Ok(screen)
    }

    pub(super) fn menu(
        &mut self,
        title: &str,
        choices: &[String],
        selected: &mut usize,
        overview: &[String],
        logs: Option<&OllamaLogs>,
    ) -> Flow<usize> {
        let mut log_offset = 0_usize;
        let mut log_page = 1_usize;
        let mut offset = 0_usize;
        let mut page = 1_usize;
        let mut horizontal = 0_u16;
        let has_overview = !overview.is_empty();
        let mut action_focused = !has_overview;
        let mut state = ListState::default();
        loop {
            state.select(action_focused.then_some(*selected));
            self.terminal.draw(|frame| {
                let keys = match (has_overview, action_focused, logs.is_some()) {
                    (_, _, true) => {
                        "↑/↓ Menu   Enter Open   Esc Exit   PgUp/PgDn Logs   ←/→ Log text"
                    }
                    (true, false, _) => "→ Actions   ↑/↓ PgUp/PgDn Scroll   Shift+←/→ Text   Esc",
                    (true, true, _) => "↑/↓ Actions   Enter Open   ← Back to list   Esc",
                    (false, _, _) => "↑/↓ Move   Enter Select   Esc Back",
                };
                let rect = layout(frame, title, keys);
                let rect = if let Some(logs) = logs {
                    let [navigation, log_area] = Layout::horizontal([
                        Constraint::Percentage(38),
                        Constraint::Percentage(62),
                    ])
                    .areas(rect);
                    log_page = usize::from(log_area.height.saturating_sub(2)).max(1);
                    log_offset = render_logs(frame, log_area, logs, log_offset, horizontal);
                    navigation
                } else {
                    rect
                };
                let content = with_messages(frame, rect, &self.lines);
                let menu_area = if !has_overview {
                    content
                } else {
                    let [menu, rows] = Layout::vertical([
                        Constraint::Length(choices.len() as u16),
                        Constraint::Min(1),
                    ])
                    .areas(content);
                    page = usize::from(rows.height.saturating_sub(1)).max(1);
                    offset = offset.min(overview.len().saturating_sub(page));
                    let mut separator = Block::default().borders(Borders::TOP);
                    if !action_focused {
                        separator = separator.border_style(Style::default().fg(Color::Cyan));
                    }
                    let text_area = separator.inner(rows);
                    frame.render_widget(separator, rows);
                    let overview_lines: Vec<_> = overview
                        .iter()
                        .skip(offset)
                        .take(page)
                        .map(|row| Line::from(visible(row)))
                        .collect();
                    frame.render_widget(
                        Paragraph::new(overview_lines).scroll((0, horizontal)),
                        text_area,
                    );
                    menu
                };
                let list = List::new(choices.iter().map(|choice| ListItem::new(visible(choice))))
                    .highlight_symbol("› ")
                    .highlight_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    );
                frame.render_stateful_widget(list, menu_area, &mut state);
            })?;
            let event = if logs.is_some() {
                if !event::poll(Duration::from_millis(150))? {
                    continue;
                }
                checked_event(event::read()?)?
            } else {
                Some(next_input()?)
            };
            let Some(Event::Key(key)) = event else {
                continue;
            };
            match key.code {
                KeyCode::Up if !action_focused => {
                    offset = offset.saturating_sub(1);
                }
                KeyCode::Down if !action_focused => {
                    offset = offset.saturating_add(1);
                }
                KeyCode::Up => *selected = selected.checked_sub(1).unwrap_or(choices.len() - 1),
                KeyCode::Down => *selected = (*selected + 1) % choices.len(),
                KeyCode::PageUp if logs.is_some() => {
                    log_offset = log_offset.saturating_add(log_page)
                }
                KeyCode::PageDown if logs.is_some() => {
                    log_offset = log_offset.saturating_sub(log_page)
                }
                KeyCode::Left if logs.is_some() => horizontal = horizontal.saturating_sub(8),
                KeyCode::Right if logs.is_some() => horizontal = horizontal.saturating_add(8),
                KeyCode::Left if !action_focused && key.modifiers.contains(KeyModifiers::SHIFT) => {
                    horizontal = horizontal.saturating_sub(8);
                }
                KeyCode::Right
                    if !action_focused && key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    horizontal = horizontal.saturating_add(8);
                }
                KeyCode::Right if has_overview => action_focused = true,
                KeyCode::Left if has_overview => action_focused = false,
                KeyCode::Tab | KeyCode::BackTab if has_overview => action_focused = !action_focused,
                KeyCode::PageUp if has_overview => offset = offset.saturating_sub(page),
                KeyCode::PageDown if has_overview => offset = offset.saturating_add(page),
                KeyCode::Home if !action_focused => offset = 0,
                KeyCode::End if !action_focused => offset = overview.len(),
                KeyCode::Home => *selected = 0,
                KeyCode::End => *selected = choices.len() - 1,
                KeyCode::Enter => {
                    self.lines.clear();
                    return Ok(*selected);
                }
                KeyCode::Esc => {
                    self.lines.clear();
                    return Ok(choices.len());
                }
                _ => {}
            }
        }
    }

    pub(super) fn prompt(&mut self, label: &str, initial: &str) -> Flow<String> {
        let mut input: Vec<_> = initial.chars().filter(|c| !c.is_control()).collect();
        let mut cursor = input.len();
        let mut replace_on_type = !input.is_empty();
        loop {
            self.terminal.draw(|frame| {
                let rect = layout(frame, &self.title, "Enter Continue   Esc Cancel");
                let content = with_messages(frame, rect, &self.lines);
                let [label_area, input_area] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(content);
                frame.render_widget(
                    Paragraph::new(visible(label)).style(Style::default().fg(Color::Cyan)),
                    label_area,
                );

                let editable = input.get(cursor).copied().unwrap_or(' ');
                let prefix = Span::raw(input[..cursor].iter().collect::<String>());
                let offset = prefix
                    .width()
                    .saturating_sub(usize::from(input_area.width.saturating_sub(2)));
                let line = Line::from(vec![
                    prefix,
                    Span::styled(
                        editable.to_string(),
                        Style::default().add_modifier(Modifier::REVERSED),
                    ),
                    Span::raw(
                        input[(cursor + 1).min(input.len())..]
                            .iter()
                            .collect::<String>(),
                    ),
                ]);
                frame.render_widget(
                    Paragraph::new(line).scroll((0, offset.min(u16::MAX as usize) as u16)),
                    input_area,
                );
            })?;
            match next_input()? {
                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    ..
                }) => {
                    self.lines.clear();
                    return Ok(input.into_iter().collect::<String>().trim().to_owned());
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Esc, ..
                }) => {
                    self.lines.clear();
                    return Err(FlowError::Cancel);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Left,
                    ..
                }) => {
                    replace_on_type = false;
                    cursor = cursor.saturating_sub(1);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Right,
                    ..
                }) => {
                    replace_on_type = false;
                    cursor = (cursor + 1).min(input.len());
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Home,
                    ..
                }) => {
                    replace_on_type = false;
                    cursor = 0;
                }
                Event::Key(KeyEvent {
                    code: KeyCode::End, ..
                }) => {
                    replace_on_type = false;
                    cursor = input.len();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                }) if cursor > 0 => {
                    replace_on_type = false;
                    cursor -= 1;
                    input.remove(cursor);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Delete,
                    ..
                }) if cursor < input.len() => {
                    input.remove(cursor);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers,
                    ..
                }) if !c.is_control()
                    && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if std::mem::take(&mut replace_on_type) {
                        input.clear();
                        cursor = 0;
                    }
                    input.insert(cursor, c);
                    cursor += 1;
                }
                Event::Paste(text) => {
                    if std::mem::take(&mut replace_on_type) {
                        input.clear();
                        cursor = 0;
                    }
                    let pasted: Vec<_> = text.chars().filter(|c| !c.is_control()).collect();
                    let end = cursor + pasted.len();
                    input.splice(cursor..cursor, pasted);
                    cursor = end;
                }
                _ => {}
            }
        }
    }

    pub(super) fn activity(&mut self, title: &str, text: &str, keys: &str) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = layout(frame, title, keys);
            frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
        })?;
        Ok(())
    }

    pub(super) fn record(&mut self) -> Flow<(Vec<u8>, i64)> {
        self.activity("Audio recording", "Opening microphone…", "")?;
        let microphone = Microphone::start()?;
        let started = Instant::now();
        loop {
            let seconds = started.elapsed().as_secs();
            let elapsed = format!("Recording   {:02}:{:02}", seconds / 60, seconds % 60);
            self.activity(
                "Audio recording",
                &elapsed,
                "Enter Stop and save   Esc Discard",
            )?;
            if microphone.is_full()? {
                break;
            }
            if event::poll(Duration::from_millis(100))?
                && let Some(Event::Key(key)) = checked_event(event::read()?)?
            {
                match key.code {
                    KeyCode::Enter if key.kind == KeyEventKind::Press => break,
                    KeyCode::Esc => return Err(FlowError::Cancel),
                    _ => {}
                }
            }
        }
        Ok(microphone.finish()?)
    }

    pub(super) fn expense_table(&mut self, headers: &[String], rows: &[Vec<String>]) -> Flow<()> {
        let mut month_offset = 0_usize;
        let mut row_offset = 0_usize;
        let month_count = headers.len() - 2;
        let mut shown_months = 1_usize;
        let mut shown_rows = 1_usize;
        loop {
            self.terminal.draw(|frame| {
                let area = layout(
                    frame,
                    &self.title,
                    "←/→ Months   ↑/↓ Rows   PgUp/PgDn Page   Esc",
                );
                let [caption, table_area] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
                let category_width = (area.width / 4).clamp(8, 24);
                let average_width = 15;
                let month_width = 12;
                shown_months = usize::from(
                    area.width
                        .saturating_sub(category_width + average_width + 2)
                        / (month_width + 1),
                )
                .clamp(1, month_count);
                shown_rows = usize::from(table_area.height.saturating_sub(1)).max(1);
                month_offset = month_offset.min(month_count - shown_months);
                row_offset = row_offset.min(rows.len().saturating_sub(shown_rows));
                let columns = (0..2).chain(2 + month_offset..2 + month_offset + shown_months);
                let header = Row::new(columns.clone().map(|column| headers[column].as_str()))
                    .style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    );
                let visible_rows = rows
                    .iter()
                    .skip(row_offset)
                    .take(shown_rows)
                    .enumerate()
                    .map(|(index, row)| {
                        let mut table_row = Row::new(columns.clone().map(|column| {
                            let cell = visible(&row[column]);
                            if column == 0 {
                                Cell::from(cell)
                                    .style(Style::default().add_modifier(Modifier::BOLD))
                            } else {
                                let width = if column == 1 {
                                    average_width
                                } else {
                                    month_width
                                };
                                Cell::from(format!("{cell:>width$}", width = usize::from(width)))
                            }
                        }));
                        if (row_offset + index) % 2 == 1 {
                            table_row =
                                table_row.style(Style::default().add_modifier(Modifier::DIM));
                        }
                        table_row
                    });
                let widths = [
                    Constraint::Length(category_width),
                    Constraint::Length(average_width),
                ]
                .into_iter()
                .chain(std::iter::repeat_n(
                    Constraint::Length(month_width),
                    shown_months,
                ));
                let window = format!(
                    "Excluding the Prestito category   ·   Months {}–{} of {month_count}",
                    month_offset + 1,
                    month_offset + shown_months
                );
                frame.render_widget(Paragraph::new(window), caption);
                frame.render_widget(Table::new(visible_rows, widths).header(header), table_area);
            })?;
            let Event::Key(key) = next_input()? else {
                continue;
            };
            match key.code {
                KeyCode::Esc => return Ok(()),
                KeyCode::Left => month_offset = month_offset.saturating_sub(1),
                KeyCode::Right => month_offset = month_offset.saturating_add(1),
                KeyCode::Up => row_offset = row_offset.saturating_sub(1),
                KeyCode::Down => row_offset = row_offset.saturating_add(1),
                KeyCode::PageUp => row_offset = row_offset.saturating_sub(shown_rows),
                KeyCode::PageDown => row_offset = row_offset.saturating_add(shown_rows),
                KeyCode::Home => row_offset = 0,
                KeyCode::End => row_offset = rows.len(),
                _ => {}
            }
        }
    }

    pub(super) fn show_results(&mut self) -> Flow<()> {
        if self.lines.is_empty() {
            return Ok(());
        }
        let rows: Vec<_> = std::mem::take(&mut self.lines)
            .into_iter()
            .map(visible)
            .collect();
        let mut offset = 0_usize;
        let mut horizontal = 0_u16;
        loop {
            let mut height = 1;
            self.terminal.draw(|frame| {
                let area = layout(
                    frame,
                    &self.title,
                    "↑/↓ Scroll   PgUp/PgDn Page   Enter / Esc Back",
                );
                height = usize::from(area.height).max(1);
                offset = offset.min(rows.len().saturating_sub(height));
                let lines = rows
                    .iter()
                    .skip(offset)
                    .take(height)
                    .map(|row| Line::from(row.as_str()))
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(lines).scroll((0, horizontal)), area);
            })?;
            let Event::Key(key) = next_input()? else {
                continue;
            };
            match key.code {
                KeyCode::Esc | KeyCode::Enter => return Ok(()),
                KeyCode::Left => horizontal = horizontal.saturating_sub(8),
                KeyCode::Right => horizontal = horizontal.saturating_add(8),
                KeyCode::Up => offset = offset.saturating_sub(1),
                KeyCode::Down => offset = offset.saturating_add(1),
                KeyCode::PageUp => offset = offset.saturating_sub(height),
                KeyCode::PageDown => offset = offset.saturating_add(height),
                KeyCode::Home => offset = 0,
                KeyCode::End => offset = rows.len(),
                _ => {}
            }
        }
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        );
    }
}

fn next_input() -> Flow<Event> {
    loop {
        if let Some(event) = checked_event(event::read()?)? {
            return Ok(event);
        }
    }
}

fn checked_event(event: Event) -> Flow<Option<Event>> {
    if let Event::Key(key) = &event {
        if key.kind == KeyEventKind::Release {
            return Ok(None);
        }
        if matches!(key.code, KeyCode::Char('c' | 'C'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Err(FlowError::Exit);
        }
    }
    Ok(Some(event))
}

fn render_logs(
    frame: &mut Frame<'_>,
    area: Rect,
    logs: &OllamaLogs,
    from_tail: usize,
    horizontal: u16,
) -> usize {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Ollama logs ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = logs.snapshot();
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new("Start Ollama from the Audio menu to see its logs.")
                .wrap(Wrap { trim: true }),
            inner,
        );
        return 0;
    }
    let height = usize::from(inner.height);
    let end = lines.len() - from_tail.min(lines.len().saturating_sub(height));
    let start = end.saturating_sub(height);
    let text: Vec<_> = lines[start..end]
        .iter()
        .map(|line| Line::from(visible(line)))
        .collect();
    frame.render_widget(Paragraph::new(text).scroll((0, horizontal)), inner);
    lines.len() - end
}

fn layout(frame: &mut Frame<'_>, title: &str, footer: &str) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "));
    let area = frame.area();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [content, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        footer_area,
    );
    content
}

fn with_messages(frame: &mut Frame<'_>, area: Rect, messages: &[String]) -> Rect {
    if messages.is_empty() {
        return area;
    }
    let height = (area.height / 3).max(1).min(area.height);
    let [content, message_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(height)]).areas(area);
    let text = messages
        .iter()
        .map(|message| Line::from(visible(message)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Yellow)),
        message_area,
    );
    content
}

fn visible(text: impl AsRef<str>) -> String {
    text.as_ref().chars().filter(|c| !c.is_control()).collect()
}
