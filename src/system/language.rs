use std::fmt;

#[derive(Debug, PartialEq)]
pub enum OutDestination
{
    StdOut,
    File(String),
    Command(Box<CommandScriptLine>),
}

#[derive(Debug, PartialEq)]
pub enum ErrDestination
{
    StdErr,
    File(String),
}

#[derive(Debug, PartialEq)]
pub struct CommandScriptLine
{
    pub exec: String,
    pub args: Vec<String>,
    pub out: OutDestination,
    pub err: ErrDestination,
}

impl CommandScriptLine
{
    fn new() -> Self
    {
        Self
        {
            exec: "".to_string(),
            args: vec![],
            out: OutDestination::StdOut,
            err: ErrDestination::StdErr,
        }
    }

    fn push(self:&mut Self, word: &str)
    {
        if word.len() == 0
        {
            return;
        }

        match &mut self.out
        {
            OutDestination::StdOut => {},
            OutDestination::File(path_string) =>
            {
                if path_string.len() == 0
                {
                    self.out = OutDestination::File(word.to_string());
                    return;
                }
            }
            OutDestination::Command(ref mut command_box) =>
            {
                (*command_box).push(word);
                return;
            },
        }

        match &mut self.err
        {
            ErrDestination::StdErr => {},
            ErrDestination::File(path_string) =>
            {
                if path_string.len() == 0
                {
                    self.err = ErrDestination::File(word.to_string());
                    return;
                }
            }
        }

        if self.exec.len() == 0
        {
            self.exec = word.to_string();
            return;
        }

        self.args.push(word.to_string());
    }

    fn pipe(self:&mut Self)
    {
        match &mut self.out
        {
            OutDestination::StdOut =>
            {
                self.out = OutDestination::Command(Box::new(CommandScriptLine::new()));
            },
            OutDestination::Command(ref mut command_box) =>
            {
                (*command_box).pipe();
            },
            _=>
            {
                // todo:error?
            }
        }
    }

    fn out_file(self:&mut Self)
    {
        match &mut self.out
        {
            OutDestination::StdOut =>
            {
                self.out = OutDestination::File("".to_string());
            },
            OutDestination::Command(ref mut command_box) =>
            {
                (*command_box).out_file();
            },
            _=>
            {
                // todo:error
            }
        }
    }

    fn err_file(self:&mut Self)
    {
        match &mut self.err
        {
            ErrDestination::StdErr =>
            {
                self.err = ErrDestination::File("".to_string());
            },
            ErrDestination::File(_) =>
            {
                // todo:error
            }
        }
    }

    fn non_trivial(self:&Self) -> bool
    {
        self.exec.len() != 0
    }
}

impl fmt::Display for CommandScriptLine
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        write!(formatter, "{}", self.exec)?;
        write!(formatter, " {}", self.args.join(" "))?;

        match &self.err
        {
            ErrDestination::StdErr => {},
            ErrDestination::File(path_string) =>
            {
                write!(formatter, " 2> {}", path_string)?; // TODO: escape the string
            },
        }

        match &self.out
        {
            OutDestination::StdOut => {},
            OutDestination::File(path_string) =>
            {
                write!(formatter, " > {}", path_string)?;
            },
            OutDestination::Command(command_box) =>
            {
                write!(formatter, " | {}", command_box)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError
{
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

fn is_pipe(c: char) -> bool
{
    c == '|'
}

fn is_out_file_indicator(s: &str) -> bool
{
    s == ">"
}

fn is_err_file_indicator(s: &str) -> bool
{
    s == "2>"
}

#[derive(Debug, PartialEq)]
pub struct CommandScript
{
    pub lines: Vec<CommandScriptLine>
}

impl CommandScript
{
    fn new() -> Self
    {
        Self
        {
            lines: vec![]
        }
    }

    pub fn from_string_vec_after_join(lines: Vec<String>) -> Result<Self, ParseError>
    {
        Self::parse(lines.join("\n"))
    }

    pub fn from_str(lines: &str) -> Result<Self, ParseError>
    {
        Self::parse(lines.to_string())
    }

    fn push(self: &mut Self, line: CommandScriptLine) -> CommandScriptLine
    {
        if line.non_trivial()
        {
            self.lines.push(line)
        }
        CommandScriptLine::new()
    }

    pub fn parse(content : String) -> Result<Self, ParseError>
    {
        let mut result = Self::new();
        let mut current_command = CommandScriptLine::new();
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
                    if is_pipe(c)
                    {
                        current_command.pipe();
                        start = i + c.len_utf8();
                    }
                    else if is_quote(c)
                    {
                        mode = Mode::Quote(line_number, i-line_i+1);
                        start = i + c.len_utf8();
                    }
                    else
                    {
                        if is_end_line_character(c) || is_whitespace(c)
                        {
                            if is_out_file_indicator(&content[start..i])
                            {
                                current_command.out_file();
                            }
                            else if is_err_file_indicator(&content[start..i])
                            {
                                current_command.err_file();
                            }
                            else
                            {
                                current_command.push(&content[start..i]);
                            }
                            start = i + c.len_utf8();
                        }

                        if is_end_line_character(c)
                        {
                            current_command = result.push(current_command);
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

            _ => {}
        }

        current_command.push(&content[start..]);
        result.push(current_command);
        Ok(result)
    }
}

impl fmt::Display for CommandScript
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {

        write!(formatter, "{}", self.lines.iter().map(|item|{format!("{}", item)}).collect::<Vec<String>>().join(";\n"))?;
        Ok(())
    }
}

#[derive(PartialEq, Debug)]
enum Mode
{
    Normal,
    Quote(usize, usize),
    Escape(Box<Mode>, usize, usize),
}

#[cfg(test)]
mod tests
{
    use crate::system::language::
    {
        OutDestination,
        ErrDestination,
        CommandScriptLine,
        CommandScript,
        ParseError,
    };

    /*  Call parse on an empty string, check that it errors I guess. */
    #[test]
    fn empty()
    {
        assert_eq!(
            CommandScript::parse("".to_string()),
            Ok(CommandScript::new()));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word()
    {
        assert_eq!(
            CommandScript::parse("run".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_leading_space()
    {
        assert_eq!(
            CommandScript::parse(" run".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_leading_tab()
    {
        assert_eq!(
            CommandScript::parse("\trun".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_trailing_space()
    {
        assert_eq!(
            CommandScript::parse("run ".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_trailing_tab()
    {
        assert_eq!(
            CommandScript::parse("run\t".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn two_words_basic()
    {
        assert_eq!(
            CommandScript::parse("run program".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn two_words_extra_semicolons()
    {
        assert_eq!(
            CommandScript::parse(";;;run program;;;".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_words_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("\t run\n\nprogram ".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon()
    {
        assert_eq!(
            CommandScript::parse("run program;\nrun another".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon_no_whitespace()
    {
        assert_eq!(
            CommandScript::parse("run program;run another".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("   run\tprogram;\n \n run another  \n  ".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("   run\tprogram;\n \n run another  \n ; \n\n".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_many_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("  ;;; run\tprogram;\n ;\n  ; run another  \n ; \n;\n".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, in quotes, expect a command with
        standard routinng */
    #[test]
    fn one_word_in_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\"".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn two_words_in_quotes_whitespace_in_second_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\" \" program \"".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![" program ".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn two_words_in_quotes_semicolon_in_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\" \"program;\"".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program;".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Parse just a single quite, expect an error */
    #[test]
    fn just_one_quote()
    {
        assert_eq!(
            CommandScript::parse("\"".to_string()),
            Err(ParseError::UnclosedQuote(1, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it, expect an error */
    #[test]
    fn three_quotes_with_lots_of_newlines()
    {
        assert_eq!(
            CommandScript::parse("\"\n\n\n\n\"\n\n\n\"".to_string()),
            Err(ParseError::UnclosedQuote(8, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it.
        Expect a weird command, but successful parse */
    #[test]
    fn escaped_quote_as_command()
    {
        assert_eq!(
            CommandScript::parse("\"\\\"\"".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "\"".to_string(),
                    args: vec![],
                    out: OutDestination::StdOut,
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  One one-word command piped into another one-word command */
    #[test]
    fn pipe_basic()
    {
        assert_eq!(
            CommandScript::parse("build | log".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "build".to_string(),
                    args: vec![],
                    out: OutDestination::Command(
                        Box::new(CommandScriptLine
                        {
                            exec: "log".to_string(),
                            args: vec![],
                            out: OutDestination::StdOut,
                            err: ErrDestination::StdErr,
                        })
                    ),
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  One one-word command piped into another one-word command */
    #[test]
    fn pipe_two_levels()
    {
        assert_eq!(
            CommandScript::parse("build | postprocess | log".to_string()),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "build".to_string(),
                    args: vec![],
                    out: OutDestination::Command(
                        Box::new(CommandScriptLine
                        {
                            exec: "postprocess".to_string(),
                            args: vec![],
                            out: OutDestination::Command(
                                Box::new(CommandScriptLine
                                {
                                    exec: "log".to_string(),
                                    args: vec![],
                                    out: OutDestination::StdOut,
                                    err: ErrDestination::StdErr,
                                })
                            ),
                            err: ErrDestination::StdErr,
                        })
                    ),
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Two-word invocation piped with output directed to a file */
    #[test]
    fn out_file_basic()
    {
        assert_eq!(
            CommandScript::parse("python build.py > build/out".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out: OutDestination::File("build/out".to_string()),
                    err: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Two-word invocation piped with error directed to a file */
    #[test]
    fn err_file_basic()
    {
        assert_eq!(
            CommandScript::parse("python build.py 2> build/out.err".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out: OutDestination::StdOut,
                    err: ErrDestination::File("build/out.err".to_string()),
                }
            ]}));
    }

    /*  Two-word command with output directed to one file and error to another file */
    #[test]
    fn err_and_out_each_go_to_a_file()
    {
        assert_eq!(
            CommandScript::parse("python build.py 
                > build/out
                2> build/err".to_string()),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out: OutDestination::File("build/out".to_string()),
                    err: ErrDestination::File("build/err".to_string()),
                }
            ]}));
    }
}
