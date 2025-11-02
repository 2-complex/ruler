#[derive(Debug, PartialEq)]
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
    Empty,
    UnclosedQuote(usize, usize),
    EmptyEscape(usize, usize)
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

fn is_newline(c: char) -> bool
{
    c == '\n'
}

fn is_escape(c: char) -> bool
{
    c == '\\'
}

fn normal_push(result: &mut Vec<CommandLineInvocation>, current_command: CommandLineInvocation) -> CommandLineInvocation
{
    if current_command.non_trivial()
    {
        result.push(current_command)
    }
    CommandLineInvocation::new()
}

#[derive(PartialEq, Debug)]
enum Mode
{
    Normal,
    Quote(usize, usize),
    Escape(Box<Mode>, usize, usize)
}

/*  Reads in a .rules file content as a String, and creates a vector of Rule
    objects. */
pub fn parse(content : String)
-> Result<Vec<CommandLineInvocation>, ParseError>
{
    let mut result = Vec::new();
    let mut current_command = CommandLineInvocation::new();
    let mut start = 0;
    let mut mode = Mode::Normal;
    let mut line_number = 1usize;
    let mut line_i = 0;

    for (i, c) in content.char_indices()
    {
        match mode
        {
            Mode::Normal =>
            {
                if is_quote(c)
                {
                    mode = Mode::Quote(line_number, i-line_i+1);
                    start = i + c.len_utf8();
                }
                else
                {
                    if is_end_line_character(c) || is_whitespace(c)
                    {
                        current_command.push(&content[start..i]);
                        start = i + c.len_utf8();
                    }

                    if is_end_line_character(c)
                    {
                        current_command = normal_push(&mut result, current_command);
                    }
                }
            },
            Mode::Quote(_line_number, _column_number) =>
            {
                if is_quote(c)
                {
                    current_command.push(&content[start..i]);
                    start = i + c.len_utf8();
                    mode = Mode::Normal;
                }

                if is_escape(c)
                {
                    mode = Mode::Escape(Box::new(mode), line_number, i-line_i+1);
                    start = i + c.len_utf8();
                }
            },
            Mode::Escape(previous_mode, _line_number, _column_number) =>
            {
                mode = *previous_mode;
            }
        }

        println!("{} {:?}", c, mode);

        if is_newline(c)
        {
            line_number += 1;
            line_i = i+1;
        }
    }

    match mode
    {
        Mode::Escape(_previous_mode, line_number, column_number) =>
            return Err(ParseError::EmptyEscape(line_number, column_number)),

        Mode::Quote(line_number, column_number) =>
            return Err(ParseError::UnclosedQuote(line_number, column_number)),

        Mode::Normal => {}
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
    fn empty()
    {
        assert_eq!(
            parse("".to_string()),
            Err(ParseError::Empty));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word()
    {
        assert_eq!(
            parse("run".to_string()),
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
    fn one_word_leading_space()
    {
        assert_eq!(
            parse(" run".to_string()),
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
    fn one_word_leading_tab()
    {
        assert_eq!(
            parse("\trun".to_string()),
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
    fn one_word_trailing_space()
    {
        assert_eq!(
            parse("run ".to_string()),
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
    fn one_word_trailing_tab()
    {
        assert_eq!(
            parse("run\t".to_string()),
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
    fn two_words_basic()
    {
        assert_eq!(
            parse("run program".to_string()),
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
    fn two_words_extra_semicolons()
    {
        assert_eq!(
            parse(";;;run program;;;".to_string()),
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
    fn two_words_eccentric_whitespace()
    {
        assert_eq!(
            parse("\t run\n\nprogram ".to_string()),
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
    fn two_commands_separated_by_semicolon()
    {
        assert_eq!(
            parse("run program;\nrun another".to_string()),
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
    fn two_commands_separated_by_semicolon_no_whitespace()
    {
        assert_eq!(
            parse("run program;run another".to_string()),
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
    fn two_commands_separated_by_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("   run\tprogram;\n \n run another  \n  ".to_string()),
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
    fn two_commands_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("   run\tprogram;\n \n run another  \n ; \n\n".to_string()),
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
    fn two_commands_many_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            parse("  ;;; run\tprogram;\n ;\n  ; run another  \n ; \n;\n".to_string()),
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
    fn one_word_in_quotes()
    {
        assert_eq!(
            parse("\"run\"".to_string()),
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
    fn two_words_in_quotes_whitespace_in_second_quotes()
    {
        assert_eq!(
            parse("\"run\" \" program \"".to_string()),
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
    fn two_words_in_quotes_semicolon_in_quotes()
    {
        assert_eq!(
            parse("\"run\" \"program;\"".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "run".to_string(),
                    args: vec!["program;".to_string()],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }

    /*  Parse just a single quite, expect an error */
    #[test]
    fn just_one_quote()
    {
        assert_eq!(
            parse("\"".to_string()),
            Err(ParseError::UnclosedQuote(1, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it, expect an error */
    #[test]
    fn three_quotes_with_lots_of_newlines()
    {
        assert_eq!(
            parse("\"\n\n\n\n\"\n\n\n\"".to_string()),
            Err(ParseError::UnclosedQuote(8, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it.
        Expect a weird command, but successful parse */
    #[test]
    fn escaped_quote_as_command()
    {
        assert_eq!(
            parse("\"\\\"\"".to_string()),
            Ok(vec![CommandLineInvocation
                {
                    command: "\"".to_string(),
                    args: vec![],
                    out: Destination::StdOut,
                    err: Destination::StdErr,
                }
            ]));
    }
}
