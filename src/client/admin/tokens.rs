//! Small bounded command tokenizer. Double quotes preserve spaces; backslash
//! escapes a quote or backslash. No shell expansion or execution.
use bloxgloom_host_api::actions::MAX_COMMAND_ARGUMENTS;
pub(super) fn parse(input: &str) -> Result<Vec<String>, &'static str> {
    Ok(spanned(input)?
        .into_iter()
        .map(|(value, _)| value)
        .collect())
}
pub(super) fn spanned(input: &str) -> Result<Vec<(String, usize)>, &'static str> {
    if input.len() > 1024 {
        return Err("Command is too long");
    }
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let trimmed = input.trim_start().trim_start_matches('/');
    let offset = input.len() - trimmed.len();
    let mut start = 0;
    let mut chars = trimmed.char_indices();
    while let Some((index, c)) = chars.next() {
        if !started && !c.is_whitespace() {
            start = offset + index;
        }
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' => {
                let (_, next) = chars.next().ok_or("Unfinished command escape")?;
                if !matches!(next, '"' | '\\') {
                    return Err("Only quote and backslash escapes are supported");
                }
                current.push(next);
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    values.push((std::mem::take(&mut current), start));
                    started = false;
                }
                if values.len() > MAX_COMMAND_ARGUMENTS + 1 {
                    return Err("Too many command arguments");
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("Unclosed command quote");
    }
    if started {
        values.push((current, start));
    }
    if values.len() > MAX_COMMAND_ARGUMENTS + 1 {
        return Err("Too many command arguments");
    }
    Ok(values)
}
