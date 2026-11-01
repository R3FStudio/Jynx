use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType},
};
use dialoguer::{FuzzySelect, Input, Password, theme::ColorfulTheme};
use std::io::{self, IsTerminal, Write};
use unicode_width::UnicodeWidthChar;

pub const COMMANDS: &[(&str, &str)] = &[
    ("/model", "Choose a local model or cloud endpoint"),
    ("/settings", "Model loading, context and tools"),
    ("/permissions", "Choose what Jynx may do"),
    ("/context", "Change the context window"),
    ("/status", "Inspect the current session"),
    ("/compact", "Compact this conversation"),
    ("/clear", "Start a fresh conversation"),
    ("/tools", "Inspect tool arguments and results"),
    ("/logs", "Inspect managed runtime output"),
    ("/help", "Keyboard shortcuts and commands"),
    ("/exit", "Leave Jynx"),
];

fn theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: console::style("\u{2726}".to_owned()).magenta(),
        active_item_style: console::Style::new().magenta().bold(),
        active_item_prefix: console::style("\u{203a}".to_owned()).cyan(),
        values_style: console::Style::new().cyan(),
        ..ColorfulTheme::default()
    }
}

pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

pub fn choose(title: &str, items: &[String], selected: usize) -> Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }
    if interactive() {
        Ok(FuzzySelect::with_theme(&theme())
            .with_prompt(title)
            .items(items)
            .default(selected.min(items.len() - 1))
            .max_length(
                terminal::size()
                    .map(|(_, h)| h.saturating_sub(5).clamp(1, 12) as usize)
                    .unwrap_or(10),
            )
            .interact_opt()?)
    } else {
        println!("{title}");
        for (index, item) in items.iter().enumerate() {
            println!("  {}  {item}", index + 1);
        }
        let answer = input("Selection (blank cancels)", "")?;
        Ok(answer
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0 && *n <= items.len())
            .map(|n| n - 1))
    }
}

pub fn input(label: &str, initial: &str) -> Result<String> {
    if interactive() {
        Ok(Input::<String>::with_theme(&theme())
            .with_prompt(label)
            .with_initial_text(initial)
            .allow_empty(true)
            .interact_text()?)
    } else {
        print!("{label}: ");
        io::stdout().flush()?;
        let mut text = String::new();
        io::stdin().read_line(&mut text)?;
        Ok(text.trim().to_owned())
    }
}

pub fn secret(label: &str) -> Result<String> {
    if !interactive() {
        return input(label, "");
    }
    Ok(Password::with_theme(&theme())
        .with_prompt(label)
        .allow_empty_password(true)
        .interact()?)
}

pub fn palette() -> Result<Option<String>> {
    let items = COMMANDS
        .iter()
        .map(|(command, description)| format!("{command:<15} {description}"))
        .collect::<Vec<_>>();
    Ok(
        choose("Jynx / Actions / type to filter / Esc to return", &items, 0)?
            .map(|i| COMMANDS[i].0.to_owned()),
    )
}

pub fn banner(model: &str, workspace: &str, context: usize, permission: &str) {
    let colored = interactive() && std::env::var_os("NO_COLOR").is_none();
    let wide = terminal::size().map(|(w, _)| w >= 60).unwrap_or(false);
    println!();
    if wide {
        let mark = [
            r"       _ __   __ _   _ __  __",
            r"      | |\ \ / /| \ | |\ \/ /",
            r"   _  | | \ V / |  \| | >  < ",
            r"  | |_| |  | |  | |\  |/ /\ \",
            r"   \___/   |_|  |_| \_/_/  \_\",
        ];
        for (index, line) in mark.iter().enumerate() {
            if colored {
                println!(
                    "\x1b[38;2;{};{};{}m{line}\x1b[0m",
                    183 - index * 14,
                    157 + index * 16,
                    255 - index * 12
                );
            } else {
                println!("{line}");
            }
        }
    } else if colored {
        println!("\x1b[38;2;183;157;255m  \u{2726} Jynx\x1b[0m");
    } else {
        println!("  Jynx");
    }
    println!(
        "\n  Your Coding Agent.  /  v{}\n",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "  Model      {model}\n  Workspace  {workspace}\n  Context    {context} tokens / {permission}\n"
    );
    println!("  F2 actions  |  F3 models  |  F4 permissions  |  / commands");
    println!("  Up/Down history  |  Alt+Enter new line  |  Ctrl+D exit\n");
}

pub fn tool_heading(icon: &str, label: &str, failed: bool) {
    if interactive() && std::env::var_os("NO_COLOR").is_none() {
        let color = if failed { "235;120;120" } else { "126;215;199" };
        println!("\x1b[38;2;{color}m  {icon} {label}\x1b[0m");
    } else {
        println!("  {icon} {label}");
    }
}

struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), event::EnableBracketedPaste)?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), event::DisableBracketedPaste, cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

fn position(text: &[char], width: usize) -> (usize, usize) {
    let (mut row, mut col) = (0, 2);
    for ch in text {
        if *ch == '\n' {
            row += 1;
            col = 2;
            continue;
        }
        let size = ch.width().unwrap_or(0);
        if col + size >= width {
            row += 1;
            col = 0;
        }
        col += size;
    }
    (row, col)
}

fn paint(text: &[char], index: usize, old_row: &mut usize) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = width.max(10) as usize;
    let (cursor_row, col) = position(&text[..index], width);
    let budget = height.saturating_sub(2).max(1) as usize;
    let top = cursor_row.saturating_sub(budget - 1);
    let mut lines = vec![String::from("\u{203a} ")];
    let mut column = 2;
    for ch in text {
        if *ch == '\n' {
            lines.push("  ".into());
            column = 2;
            continue;
        }
        let size = ch.width().unwrap_or(0);
        if column + size >= width {
            lines.push(String::new());
            column = 0;
        }
        lines.last_mut().unwrap().push(*ch);
        column += size;
    }
    let bottom = (top + budget).min(lines.len());
    let mut out = io::stdout();
    execute!(out, cursor::MoveToColumn(0))?;
    if *old_row > 0 {
        execute!(out, cursor::MoveUp(*old_row as u16))?;
    }
    execute!(out, Clear(ClearType::FromCursorDown))?;
    for (i, line) in lines[top..bottom].iter().enumerate() {
        if i > 0 {
            write!(out, "\r\n")?;
        }
        write!(out, "{line}")?;
    }
    let end_row = bottom - top - 1;
    let row = cursor_row - top;
    if end_row > row {
        execute!(out, cursor::MoveUp((end_row - row) as u16))?;
    }
    execute!(out, cursor::MoveToColumn(col as u16))?;
    *old_row = row;
    out.flush()?;
    Ok(())
}

pub fn prompt(history: &[String]) -> Result<Option<String>> {
    if !interactive() {
        let mut line = String::new();
        return Ok((io::stdin().read_line(&mut line)? > 0).then(|| line.trim_end().to_owned()));
    }
    let mut guard = TerminalGuard::enter()?;
    let mut text = Vec::<char>::new();
    let (mut index, mut row, mut history_index) = (0, 0, history.len());
    let mut draft = Vec::new();
    paint(&text, index, &mut row)?;
    loop {
        let action = match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter
                    if key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
                {
                    text.insert(index, '\n');
                    index += 1;
                    None
                }
                KeyCode::Enter => Some(text.iter().collect::<String>()),
                KeyCode::F(2) => Some("/".into()),
                KeyCode::F(3) => Some("/model".into()),
                KeyCode::F(4) => Some("/permissions".into()),
                KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL) && text.is_empty() =>
                {
                    Some("/exit".into())
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text.clear();
                    index = 0;
                    None
                }
                KeyCode::Char('/') if text.is_empty() => Some("/".into()),
                KeyCode::Tab if text.first() == Some(&'/') => Some("/".into()),
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text.insert(index, ch);
                    index += 1;
                    None
                }
                KeyCode::Backspace if index > 0 => {
                    index -= 1;
                    text.remove(index);
                    None
                }
                KeyCode::Delete if index < text.len() => {
                    text.remove(index);
                    None
                }
                KeyCode::Left => {
                    index = index.saturating_sub(1);
                    None
                }
                KeyCode::Right => {
                    index = (index + 1).min(text.len());
                    None
                }
                KeyCode::Home => {
                    index = 0;
                    None
                }
                KeyCode::End => {
                    index = text.len();
                    None
                }
                KeyCode::Up if history_index > 0 => {
                    if history_index == history.len() {
                        draft = text.clone();
                    }
                    history_index -= 1;
                    text = history[history_index].chars().collect();
                    index = text.len();
                    None
                }
                KeyCode::Down if history_index < history.len() => {
                    history_index += 1;
                    text = if history_index == history.len() {
                        draft.clone()
                    } else {
                        history[history_index].chars().collect()
                    };
                    index = text.len();
                    None
                }
                KeyCode::Esc => {
                    text.clear();
                    index = 0;
                    None
                }
                _ => None,
            },
            Event::Paste(value) => {
                let clean = value.replace("\r\n", "\n").replace('\r', "\n");
                let chars = clean
                    .chars()
                    .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
                    .collect::<Vec<_>>();
                let count = chars.len();
                text.splice(index..index, chars);
                index += count;
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            paint(&text, text.len(), &mut row)?;
            print!("\r\n");
            io::stdout().flush()?;
            drop(guard);
            if action == "/" || action.is_empty() {
                if let Some(command) = palette()? {
                    return Ok(Some(command));
                }
                guard = TerminalGuard::enter()?;
                text.clear();
                index = 0;
                row = 0;
            } else {
                return Ok(Some(action));
            }
        }
        paint(&text, index, &mut row)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_tracks_unicode_and_newlines() {
        assert_eq!(
            position(&"a\u{754c}".chars().collect::<Vec<_>>(), 80),
            (0, 5)
        );
        assert_eq!(position(&"abc\nxy".chars().collect::<Vec<_>>(), 80), (1, 4));
        assert_eq!(
            position(&"abcdefgh".chars().collect::<Vec<_>>(), 10),
            (1, 1)
        );
    }
}
