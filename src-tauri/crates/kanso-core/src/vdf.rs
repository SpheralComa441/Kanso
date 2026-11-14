/// Lightweight parser for Valve's KeyValues text format (`.vdf` and `.acf` files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdfValue {
    Str(String),
    Section(VdfSection),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VdfSection {
    pub entries: Vec<(String, VdfValue)>,
}

impl VdfSection {
    /// Case-insensitive lookup for a string value.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|(k, v)| {
            if k.eq_ignore_ascii_case(key) {
                if let VdfValue::Str(s) = v {
                    return Some(s.as_str());
                }
            }
            None
        })
    }

    /// Case-insensitive lookup for a nested section.
    pub fn get_section(&self, key: &str) -> Option<&VdfSection> {
        self.entries.iter().find_map(|(k, v)| {
            if k.eq_ignore_ascii_case(key) {
                if let VdfValue::Section(sec) = v {
                    return Some(sec);
                }
            }
            None
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Str(String),
    OpenBrace,
    CloseBrace,
}

/// Parses a Valve KeyValues (`.vdf` / `.acf`) document into a root [`VdfSection`].
pub fn parse_vdf(input: &str) -> Result<VdfSection, String> {
    let stripped = input.strip_prefix('\u{feff}').unwrap_or(input);
    let tokens = tokenize(stripped)?;
    let mut pos = 0;
    parse_section_entries(&tokens, &mut pos, false)
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        // Skip `//` single-line comments
        if ch == '/' {
            chars.next();
            if chars.peek() == Some(&'/') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            } else {
                // Bare slash starting an unquoted token
                let mut buf = String::from("/");
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                        break;
                    }
                    buf.push(c);
                    chars.next();
                }
                tokens.push(Token::Str(buf));
                continue;
            }
        }

        match ch {
            '{' => {
                chars.next();
                tokens.push(Token::OpenBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::CloseBrace);
            }
            '"' => {
                chars.next(); // consume opening quote
                let mut s = String::new();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(esc) = chars.next() {
                            match esc {
                                '\\' => s.push('\\'),
                                '"' => s.push('"'),
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                'r' => s.push('\r'),
                                other => {
                                    s.push('\\');
                                    s.push(other);
                                }
                            }
                        } else {
                            s.push('\\');
                        }
                    } else if c == '"' {
                        closed = true;
                        break;
                    } else {
                        s.push(c);
                    }
                }
                if !closed {
                    return Err("Unterminated quoted string in VDF/ACF file".to_string());
                }
                tokens.push(Token::Str(s));
            }
            _ => {
                // Unquoted token fallback
                let mut buf = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                        break;
                    }
                    buf.push(c);
                    chars.next();
                }
                tokens.push(Token::Str(buf));
            }
        }
    }

    Ok(tokens)
}

fn parse_section_entries(
    tokens: &[Token],
    pos: &mut usize,
    expect_closing_brace: bool,
) -> Result<VdfSection, String> {
    let mut entries = Vec::new();

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::CloseBrace => {
                if expect_closing_brace {
                    *pos += 1;
                    return Ok(VdfSection { entries });
                } else {
                    return Err("Unexpected closing brace '}' in VDF/ACF file".to_string());
                }
            }
            Token::OpenBrace => {
                return Err("Unexpected opening brace '{' without key in VDF/ACF file".to_string());
            }
            Token::Str(key) => {
                let key = key.clone();
                *pos += 1;

                if *pos >= tokens.len() {
                    return Err(format!("Unexpected EOF after key '{key}' in VDF/ACF file"));
                }

                match &tokens[*pos] {
                    Token::Str(val) => {
                        let val = val.clone();
                        *pos += 1;
                        entries.push((key, VdfValue::Str(val)));
                    }
                    Token::OpenBrace => {
                        *pos += 1;
                        let child = parse_section_entries(tokens, pos, true)?;
                        entries.push((key, VdfValue::Section(child)));
                    }
                    Token::CloseBrace => {
                        return Err(format!(
                            "Unexpected '}}' immediately after key '{key}' in VDF/ACF file"
                        ));
                    }
                }
            }
        }
    }

    if expect_closing_brace {
        return Err("Unclosed section '{' in VDF/ACF file".to_string());
    }

    Ok(VdfSection { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_escaped_paths_and_nested_sections() {
        let sample = r#"
        // Valve KeyValues test
        "libraryfolders"
        {
            "0"
            {
                "path"		"C:\\Program Files (x86)\\Steam"
                "label"		""
            }
            "1"
            {
                "path"		"D:\\Games\\SteamLibrary"
            }
        }
        "#;

        let doc = parse_vdf(sample).expect("VDF should parse");
        let libs = doc.get_section("libraryfolders").expect("libraryfolders section");
        let zero = libs.get_section("0").expect("entry 0");
        assert_eq!(zero.get_str("path"), Some(r"C:\Program Files (x86)\Steam"));
        let one = libs.get_section("1").expect("entry 1");
        assert_eq!(one.get_str("path"), Some(r"D:\Games\SteamLibrary"));
    }
}
