#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Destination
{
    StdOut,
    StdErr,
    Exceutable(String),
    File(String),
}

#[derive(Debug, PartialEq)]
pub struct CommandLineInvocation
{
    command: String,
    args: Vec<String>,
    out: Destination,
    err: Destination,
}

impl CommandLineInvocation
{
    fn new() -> Self
    {
        Self
        {
            command: "".to_string(),
            args: vec![],
            out: Destination::StdOut,
            err: Destination::StdErr,
        }
    }

    fn push(self:&mut Self, word: &str)
    {
        if word.len() == 0
        {
            return;
        }

        if self.command.len() == 0
        {
            self.command = word.to_string();
            return;
        }

        self.args.push(word.to_string());
    }

    fn non_trivial(self:&Self) -> bool
    {
        self.command.len() != 0
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError
{
    Empty
}

fn is_whitespace(c: char) -> bool
{
    "\t\n\r ".contains(c)
}

fn is_end_line_character(c: char) -> bool
{
    c == ';'
}

fn is_quote(c: char) -> bool
{
    c == '"'
}

fn normal_push(result: &mut Vec<CommandLineInvocation>, current_command: CommandLineInvocation) -> CommandLineInvocation
{
    if current_command.non_trivial()
    {
        result.push(current_command)
    }
    CommandLineInvocation::new()
}

enum Mode
{
    Normal,
    Quote
}

/*  Reads in a .rules file content as a String, and creates a vector of Rule
    objects. */
pub fn parse(_filename : String, content : String)
-> Result<Vec<CommandLineInvocation>, ParseError>
{
    let mut result = Vec::new();
    let mut current_command = CommandLineInvocation::new();
    let mut start = 0;
    let mut mode = Mode::Normal;

    for (i, c) in content.char_indices()
    {
        match mode
        {
            Mode::Normal =>
            {
                if is_quote(c)
                {
                    mode = Mode::Quote;
                    start = i + c.len_utf8();
                    continue;
                }

                if is_end_line_character(c) || is_whitespace(c)
                {
                    current_command.push(&content[start..i]);
                    start = i + c.len_utf8();
                }

                if is_end_line_character(c)
                {
                    current_command = normal_push(&mut result, current_command);
                }
            },
            Mode::Quote => 
            {
                if is_quote(c)
                {
                    current_command.push(&content[start..i]);
                    start = i + c.len_utf8();
                    mode = Mode::Normal;
                }
            }
        }
    }

    current_command.push(&content[start..]);
    normal_push(&mut result, current_command);

    if result.len() == 0
    {
        Err(ParseError::Empty)
    }
    else
    {
        Ok(result)
    }
}

#[cfg(test)]
mod tests
{
    use crate::language::
    {
        Destination,
        CommandLineInvocation,
        ParseError,
        parse
    };

    /*  Call parse on an empty string, check that it errors I guess. */
    #[test]
    fn parse_empty()
    {
        assert_eq!(
            parse("empty.script".to_string(), "".to_string()),
            Err(ParseError::Empty));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word_leading_space()
    {
        assert_eq!(
            parse("empty.script".to_string(), " run".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word_leading_tab()
    {
        assert_eq!(
            parse("empty.script".to_string(), "\trun".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word_trailing_space()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run ".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word_trailing_tab()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run\t".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn parse_two_words_basic()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run program".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn parse_two_words_extra_semicolons()
    {
        assert_eq!(
            parse("empty.script".to_string(), ";;;run program;;;".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_words_eccentric_whitespace()
    {
        assert_eq!(
            parse("empty.script".to_string(), "\t run\n\nprogram ".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_commands_separated_by_semicolon()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run program;\nrun another".to_string()),
            Ok(vec![
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                },
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_commands_separated_by_semicolon_no_whitespace()
    {
        assert_eq!(
            parse("empty.script".to_string(), "run program;run another".to_string()),
            Ok(vec![
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                },
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_commands_separated_by_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("empty.script".to_string(), "   run\tprogram;\n \n run another  \n  ".to_string()),
            Ok(vec![
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                },
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_commands_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("empty.script".to_string(), "   run\tprogram;\n \n run another  \n ; \n\n".to_string()),
            Ok(vec![
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                },
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn parse_two_commands_many_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("empty.script".to_string(), "  ;;; run\tprogram;\n ;\n  ; run another  \n ; \n;\n".to_string()),
            Ok(vec![
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                },
                CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a one word invocation, in quotes, expect a command with
        standard routinng */
    #[test]
    fn parse_one_word_in_quotes()
    {
        assert_eq!(
            parse("empty.script".to_string(), "\"run\"".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn parse_two_words_in_quotes_whitespace_in_second_quotes()
    {
        assert_eq!(
            parse("empty.script".to_string(), "\"run\" \" program \"".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec![" program ".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn parse_two_words_in_quotes_semicolon_in_quotes()
    {
        assert_eq!(
            parse("empty.script".to_string(), "\"run\" \"program;\"".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program;".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }
}
