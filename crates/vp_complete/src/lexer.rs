#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashQuoteMode {
    None,
    Single,
    Double,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashLexResult {
    pub words: Vec<String>,
    pub current_index: usize,
    pub quote_mode: BashQuoteMode,
    pub has_closing_quote: bool,
    pub last_unescaped_wordbreak_end: Option<usize>,
}

#[inline]
pub fn parse_bash_line(line: &str, point: usize) -> BashLexResult {
    parse_bash_line_with_wordbreaks(line, point, " \t\n\"'><=;|&(:")
}

#[cold]
pub fn parse_bash_line_with_wordbreaks(
    line: &str,
    point: usize,
    wordbreaks: &str,
) -> BashLexResult {
    let clamped = point.min(line.len());
    let mut end = clamped;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let input = &line[..end];

    let mut words = Vec::new();
    let mut cur_word = String::new();
    let mut in_word = false;
    let mut has_trailing_space = false;
    let mut quote_state = BashQuoteMode::None;
    let mut open_quote_start: Option<usize> = None;
    let mut cur_wordbreak_end: Option<usize> = None;
    let mut cur_quoted_before_eq = false;
    let mut cur_saw_eq = false;
    let mut cur_is_assignment = false;
    let mut expect_redir_target = false;

    macro_rules! finish_word {
        () => {
            if in_word {
                let word = std::mem::take(&mut cur_word);
                if !expect_redir_target && !(words.is_empty() && cur_is_assignment) {
                    words.push(word);
                }
                in_word = false;
                cur_quoted_before_eq = false;
                cur_saw_eq = false;
                cur_is_assignment = false;
            }
            open_quote_start = None;
            cur_wordbreak_end = None;
        };
    }

    macro_rules! reset_cmd {
        () => {
            cur_word.clear();
            in_word = false;
            cur_quoted_before_eq = false;
            cur_saw_eq = false;
            cur_is_assignment = false;
            expect_redir_target = false;
            words.clear();
            open_quote_start = None;
            cur_wordbreak_end = None;
            has_trailing_space = false;
        };
    }

    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match quote_state {
            BashQuoteMode::Single => {
                in_word = true;
                has_trailing_space = false;
                if c == '\'' {
                    quote_state = BashQuoteMode::None;
                    open_quote_start = None;
                } else {
                    cur_word.push(c);
                }
            }
            BashQuoteMode::Double => {
                in_word = true;
                has_trailing_space = false;
                if c == '"' {
                    quote_state = BashQuoteMode::None;
                    open_quote_start = None;
                } else if c == '$' && chars.as_str().starts_with('(') {
                    let before = chars.as_str();
                    let mut scan = chars.clone();
                    if scan_balanced_parens(&mut scan, 32) {
                        cur_word.push('$');
                        cur_word.push_str(&before[..before.len() - scan.as_str().len()]);
                        chars = scan;
                    } else {
                        let _ = chars.next();
                        quote_state = BashQuoteMode::None;
                        reset_cmd!();
                    }
                } else if c == '`' {
                    let before = chars.as_str();
                    let mut scan = chars.clone();
                    if scan_backticks(&mut scan) {
                        cur_word.push('`');
                        cur_word.push_str(&before[..before.len() - scan.as_str().len()]);
                        chars = scan;
                    } else {
                        quote_state = BashQuoteMode::None;
                        reset_cmd!();
                    }
                } else if c == '\\' {
                    if let Some(next) = chars.as_str().chars().next() {
                        if next == '\n' {
                            let _ = chars.next();
                        } else if next == '\r' && chars.as_str().starts_with("\r\n") {
                            let _ = chars.next();
                            let _ = chars.next();
                        } else if matches!(next, '"' | '\\' | '$' | '`') {
                            cur_word.push(next);
                            let _ = chars.next();
                        } else {
                            cur_word.push(c);
                        }
                    } else {
                        cur_word.push(c);
                    }
                } else {
                    cur_word.push(c);
                }
            }
            BashQuoteMode::None => {
                if c == '\'' || c == '"' {
                    if !cur_saw_eq {
                        cur_quoted_before_eq = true;
                    }
                    open_quote_start = Some(cur_word.len());
                    in_word = true;
                    has_trailing_space = false;
                    quote_state =
                        if c == '\'' { BashQuoteMode::Single } else { BashQuoteMode::Double };
                } else if c == '\\' {
                    if let Some(next) = chars.next() {
                        if next == '\n' {
                            continue;
                        }
                        if next == '\r' && chars.as_str().starts_with('\n') {
                            let _ = chars.next();
                            continue;
                        }
                        if !cur_saw_eq {
                            cur_quoted_before_eq = true;
                        }
                        in_word = true;
                        has_trailing_space = false;
                        cur_word.push(next);
                    } else {
                        in_word = true;
                        has_trailing_space = false;
                        cur_word.push(c);
                    }
                } else if matches!(c, '$' | '<' | '>') && chars.as_str().starts_with('(') {
                    let before = chars.as_str();
                    let mut scan = chars.clone();
                    if scan_balanced_parens(&mut scan, 32) {
                        if !cur_saw_eq {
                            cur_quoted_before_eq = true;
                        }
                        in_word = true;
                        has_trailing_space = false;
                        cur_word.push(c);
                        cur_word.push_str(&before[..before.len() - scan.as_str().len()]);
                        chars = scan;
                    } else {
                        let _ = chars.next();
                        reset_cmd!();
                    }
                } else if c == '`' {
                    let before = chars.as_str();
                    let mut scan = chars.clone();
                    if scan_backticks(&mut scan) {
                        if !cur_saw_eq {
                            cur_quoted_before_eq = true;
                        }
                        in_word = true;
                        has_trailing_space = false;
                        cur_word.push('`');
                        cur_word.push_str(&before[..before.len() - scan.as_str().len()]);
                        chars = scan;
                    } else {
                        reset_cmd!();
                    }
                } else if c == '>' || c == '<' || (c == '&' && chars.as_str().starts_with('>')) {
                    if c != '&'
                        && in_word
                        && !cur_quoted_before_eq
                        && !cur_word.is_empty()
                        && cur_word.bytes().all(|b| b.is_ascii_digit())
                    {
                        cur_word.clear();
                        in_word = false;
                        cur_saw_eq = false;
                        cur_is_assignment = false;
                        open_quote_start = None;
                        cur_wordbreak_end = None;
                    } else {
                        finish_word!();
                    }

                    if c == '&' {
                        let _ = chars.next(); // '>'
                        if chars.as_str().starts_with('>') {
                            let _ = chars.next();
                        }
                    } else if c == '>' {
                        if chars.as_str().starts_with(['>', '&', '|']) {
                            let _ = chars.next();
                        }
                    } else if chars.as_str().starts_with('<') {
                        let _ = chars.next();
                        if chars.as_str().starts_with(['<', '-']) {
                            let _ = chars.next();
                        }
                    } else if chars.as_str().starts_with(['&', '>']) {
                        let _ = chars.next();
                    }

                    expect_redir_target = true;
                    has_trailing_space = false;
                } else if matches!(c, '\n' | '\r' | ';' | '|' | '&') {
                    reset_cmd!();
                    if (c == '|' || c == '&') && chars.as_str().starts_with(c) {
                        let _ = chars.next();
                    }
                } else if words.is_empty()
                    && cur_word.is_empty()
                    && (c == '('
                        || (c == '{'
                            && chars
                                .as_str()
                                .as_bytes()
                                .first()
                                .is_none_or(u8::is_ascii_whitespace)))
                {
                    reset_cmd!();
                } else if c.is_ascii_whitespace() {
                    if in_word {
                        finish_word!();
                        expect_redir_target = false;
                    }
                    has_trailing_space = true;
                } else {
                    if c == '=' && !cur_saw_eq {
                        cur_saw_eq = true;
                        if !cur_quoted_before_eq {
                            let lhs = cur_word.strip_suffix('+').unwrap_or(&cur_word);
                            cur_is_assignment = is_shell_ident(lhs);
                        }
                    }
                    in_word = true;
                    has_trailing_space = false;
                    cur_word.push(c);
                    if wordbreaks.contains(c) {
                        cur_wordbreak_end = Some(cur_word.len());
                    }
                }
            }
        }
    }

    let last_unescaped_wordbreak_end = if in_word {
        words.push(cur_word);
        if quote_state != BashQuoteMode::None {
            open_quote_start.filter(|&i| i > 0)
        } else {
            cur_wordbreak_end
        }
    } else {
        if has_trailing_space || words.is_empty() {
            words.push(String::new());
        }
        None
    };

    let after = &line[end..];
    let has_closing_quote = match quote_state {
        BashQuoteMode::Double => after.starts_with('"'),
        BashQuoteMode::Single => after.starts_with('\''),
        BashQuoteMode::None => false,
    };

    let current_index = words.len().saturating_sub(1);
    BashLexResult {
        words,
        current_index,
        quote_mode: quote_state,
        has_closing_quote,
        last_unescaped_wordbreak_end,
    }
}

#[cold]
fn scan_balanced_parens(scan: &mut std::str::Chars<'_>, rem_depth: u8) -> bool {
    if rem_depth == 0 {
        return false;
    }
    let Some('(') = scan.next() else {
        return false;
    };
    let mut depth = 1usize;
    let mut quote = BashQuoteMode::None;
    while depth > 0 {
        let Some(ch) = scan.next() else {
            return false;
        };
        match quote {
            BashQuoteMode::Single => {
                if ch == '\'' {
                    quote = BashQuoteMode::None;
                }
            }
            BashQuoteMode::Double => match ch {
                '"' => quote = BashQuoteMode::None,
                '$' if scan.as_str().starts_with('(') => {
                    if !scan_balanced_parens(scan, rem_depth - 1) {
                        return false;
                    }
                }
                '`' => {
                    if !scan_backticks(scan) {
                        return false;
                    }
                }
                '\\' => {
                    let _ = scan.next();
                }
                _ => {}
            },
            BashQuoteMode::None => match ch {
                '\'' => quote = BashQuoteMode::Single,
                '"' => quote = BashQuoteMode::Double,
                '$' | '<' | '>' if scan.as_str().starts_with('(') => {
                    if !scan_balanced_parens(scan, rem_depth - 1) {
                        return false;
                    }
                }
                '`' => {
                    if !scan_backticks(scan) {
                        return false;
                    }
                }
                '\\' => {
                    let _ = scan.next();
                }
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            },
        }
    }
    true
}

#[cold]
fn scan_backticks(scan: &mut std::str::Chars<'_>) -> bool {
    while let Some(ch) = scan.next() {
        if ch == '`' {
            return true;
        }
        if ch == '\\' {
            let _ = scan.next();
        }
    }
    false
}

fn is_shell_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
