pub fn parse(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut last_space = true;
    let mut arg = String::new();
    for c in command.chars() {
        if c == ' ' && !quoted {
            if !last_space {
                args.push(std::mem::take(&mut arg));
            }
            last_space = true;
            continue;
        }
        last_space = false;
        if c == '"' {
            if !escaped {
                quoted = !quoted;
            } else {
                arg.push(c);
                escaped = false;
            }
            continue;
        }
        if c == '\\' && escaped {
            arg.push('\\');
            arg.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
            continue;
        }
        if escaped && c != '"' {
            arg.push('\\');
        }
        arg.push(c);
        escaped = false;
    }
    if !last_space {
        args.push(arg.trim().to_owned());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_arguments_match_original_direct_spawn_semantics() {
        assert_eq!(
            parse(r#"tool "one two" | cat > output"#),
            vec!["tool", "one two", "|", "cat", ">", "output"]
        );
        assert_eq!(
            parse(r#"tool "" "C:\maps\file.png" "a\"b""#),
            vec!["tool", "", r"C:\maps\file.png", "a\"b"]
        );
        assert_eq!(parse("  tool   value  "), vec!["tool", "value"]);
    }
}
