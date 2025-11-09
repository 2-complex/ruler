use std::fmt;
use std::iter::once;

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
    pub out_destination: OutDestination,
    pub err_destination: ErrDestination,
}

impl CommandScriptLine
{
    fn new() -> Self
    {
        Self
        {
            exec: "".to_string(),
            args: vec![],
            out_destination: OutDestination::StdOut,
            err_destination: ErrDestination::StdErr,
        }
    }

    fn push(self:&mut Self, word: String)
    {
        if word.len() == 0
        {
            return;
        }

        match &mut self.out_destination
        {
            OutDestination::StdOut => {},
            OutDestination::File(path_string) =>
            {
                if path_string.len() == 0
                {
                    self.out_destination = OutDestination::File(word);
                    return;
                }
            }
            OutDestination::Command(ref mut command_box) =>
            {
                (*command_box).push(word);
                return;
            },
        }

        match &mut self.err_destination
        {
            ErrDestination::StdErr => {},
            ErrDestination::File(path_string) =>
            {
                if path_string.len() == 0
                {
                    self.err_destination = ErrDestination::File(word);
                    return;
                }
            }
        }

        if self.exec.len() == 0
        {
            self.exec = word;
            return;
        }

        self.args.push(word);
    }

    fn pipe(self:&mut Self)
    {
        match &mut self.out_destination
        {
            OutDestination::StdOut =>
            {
                self.out_destination = OutDestination::Command(Box::new(CommandScriptLine::new()));
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
        match &mut self.out_destination
        {
            OutDestination::StdOut =>
            {
                self.out_destination = OutDestination::File("".to_string());
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
        match &mut self.err_destination
        {
            ErrDestination::StdErr =>
            {
                self.err_destination = ErrDestination::File("".to_string());
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

fn escape_string(s: &str) -> String
{
    fn should_use_quites(s: &str) -> bool
    {
        for c in s.chars()
        {
            if is_whitespace(c) || c == '\"'
            {
                return true;
            }
        }
        return false;
    }

    if ! should_use_quites(s)
    {
        return s.to_string()
    }

    once("\"".to_string()).chain(
        s.chars().map(|c|
        {
            if c=='\"' {"\\\"".to_string()} else {c.to_string()}
        }))
        .chain(once("\"".to_string()))
        .collect::<Vec<String>>().join("")
}

impl fmt::Display for CommandScriptLine
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        write!(formatter, "{}", escape_string(self.exec.as_str()))?;
        write!(formatter, " {}", self.args.iter().map(|s|{escape_string(s.as_str())}).collect::<Vec<String>>().join(" "))?;

        match &self.err_destination
        {
            ErrDestination::StdErr => {},
            ErrDestination::File(path_string) =>
            {
                write!(formatter, " 2> {}", escape_string(path_string))?;
            },
        }

        match &self.out_destination
        {
            OutDestination::StdOut => {},
            OutDestination::File(path_string) =>
            {
                write!(formatter, " > {}", escape_string(path_string))?;
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

impl fmt::Display for ParseError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ParseError::UnclosedQuote(line_number, column_number) =>
                write!(formatter, "Unclosed quote line {} column {}", line_number, column_number),

            ParseError::EmptyEscape(line_number, column_number) =>
                write!(formatter, "Empty escape line {} column {}", line_number, column_number),
        }
    }
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

    fn push(self: &mut Self, line: CommandScriptLine) -> CommandScriptLine
    {
        if line.non_trivial()
        {
            self.lines.push(line)
        }
        CommandScriptLine::new()
    }

    pub fn parse(content : &str) -> Result<Self, ParseError>
    {
        let mut result = Self::new();
        let mut current_command = CommandScriptLine::new();
        let mut start = 0;
        let mut mode = Mode::Normal;
        let mut word = String::new();
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
                            let section = &content[start..i];
                            if is_out_file_indicator(section)
                            {
                                current_command.out_file();
                            }
                            else if is_err_file_indicator(section)
                            {
                                current_command.err_file();
                            }
                            else
                            {
                                word.push_str(section);
                                current_command.push(word);
                                word = String::new();
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
                        word.push_str(&content[start..i]);
                        current_command.push(word);
                        word = String::new();
                        start = i + c.len_utf8();
                        mode = Mode::Normal;
                    }

                    if is_escape(c)
                    {
                        mode = Mode::Escape(Box::new(mode), line_number, i-line_i+1);
                    }
                },
                Mode::Escape(previous_mode, _line_number, _column_number) =>
                {
                    mode = *previous_mode;
                    word.push_str(&content[start..(i-1)]);
                    start = i;
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

        word.push_str(&content[start..]);
        current_command.push(word);
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
        escape_string
    };

    #[test]
    fn escape_string_empty()
    {
        assert_eq!(escape_string("one"), "one".to_string());
    }

    #[test]
    fn escape_string_basic()
    {
        assert_eq!(escape_string("one"), "one".to_string());
    }

    #[test]
    fn escape_string_space()
    {
        assert_eq!(escape_string("one two"), "\"one two\"".to_string());
    }

    #[test]
    fn escape_string_newline()
    {
        assert_eq!(escape_string("one\ntwo"), "\"one\ntwo\"".to_string());
    }

    #[test]
    fn escape_string_mix_quote_and_space()
    {
        assert_eq!(escape_string("one\" two"), "\"one\\\" two\"".to_string())
    }

    #[test]
    fn escape_string_mix_quote_and_newline()
    {
        assert_eq!(escape_string("one\"\ntwo"), "\"one\\\"\ntwo\"".to_string())
    }

    /*  Call parse on an empty string, check that it errors I guess. */
    #[test]
    fn empty()
    {
        assert_eq!(
            CommandScript::parse(""),
            Ok(CommandScript::new()));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word()
    {
        assert_eq!(
            CommandScript::parse("run"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_leading_space()
    {
        assert_eq!(
            CommandScript::parse(" run"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_leading_tab()
    {
        assert_eq!(
            CommandScript::parse("\trun"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_trailing_space()
    {
        assert_eq!(
            CommandScript::parse("run "),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, expect a command with
        standard routinng */
    #[test]
    fn one_word_trailing_tab()
    {
        assert_eq!(
            CommandScript::parse("run\t"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn two_words_basic()
    {
        assert_eq!(
            CommandScript::parse("run program"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on a two word invocation, expect a command with
        standard routing */
    #[test]
    fn two_words_extra_semicolons()
    {
        assert_eq!(
            CommandScript::parse(";;;run program;;;"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_words_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("\t run\n\nprogram "),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon()
    {
        assert_eq!(
            CommandScript::parse("run program;\nrun another"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon_no_whitespace()
    {
        assert_eq!(
            CommandScript::parse("run program;run another"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_separated_by_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("   run\tprogram;\n \n run another  \n  "),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("   run\tprogram;\n \n run another  \n ; \n\n"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, throw in some arbitrary whitespace,
        expect a command with standard routing */
    #[test]
    fn two_commands_many_extra_semicolon_eccentric_whitespace()
    {
        assert_eq!(
            CommandScript::parse("  ;;; run\tprogram;\n ;\n  ; run another  \n ; \n;\n"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                },
                CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["another".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a one word invocation, in quotes, expect a command with
        standard routinng */
    #[test]
    fn one_word_in_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\""),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on two words in quotes with a an escaped quote */
    #[test]
    fn one_two_words_one_escaped_quote()
    {
        assert_eq!(
            CommandScript::parse("\"one\\\" two\""),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "one\" two".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn two_words_in_quotes_whitespace_in_second_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\" \" program \""),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec![" program ".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Call parse on just a two word invocation, both words in quotes, the second
        has spaces in it, expect a command with standard routing */
    #[test]
    fn two_words_in_quotes_semicolon_in_quotes()
    {
        assert_eq!(
            CommandScript::parse("\"run\" \"program;\""),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "run".to_string(),
                    args: vec!["program;".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Parse just a single quite, expect an error */
    #[test]
    fn just_one_quote()
    {
        assert_eq!(
            CommandScript::parse("\""),
            Err(ParseError::UnclosedQuote(1, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it, expect an error */
    #[test]
    fn three_quotes_with_lots_of_newlines()
    {
        assert_eq!(
            CommandScript::parse("\"\n\n\n\n\"\n\n\n\""),
            Err(ParseError::UnclosedQuote(8, 1)));
    }

    /*  Parse a single quite with a lot of newlines around it.
        Expect a weird command, but successful parse */
    #[test]
    fn escaped_quote_as_command()
    {
        assert_eq!(
            CommandScript::parse("\"\\\"\""),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "\"".to_string(),
                    args: vec![],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  One one-word command piped into another one-word command */
    #[test]
    fn pipe_basic()
    {
        assert_eq!(
            CommandScript::parse("build | log"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "build".to_string(),
                    args: vec![],
                    out_destination: OutDestination::Command(
                        Box::new(CommandScriptLine
                        {
                            exec: "log".to_string(),
                            args: vec![],
                            out_destination: OutDestination::StdOut,
                            err_destination: ErrDestination::StdErr,
                        })
                    ),
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  One one-word command piped into another one-word command */
    #[test]
    fn pipe_two_levels()
    {
        assert_eq!(
            CommandScript::parse("build | postprocess | log"),
            Ok(CommandScript{lines:vec![CommandScriptLine
                {
                    exec: "build".to_string(),
                    args: vec![],
                    out_destination: OutDestination::Command(
                        Box::new(CommandScriptLine
                        {
                            exec: "postprocess".to_string(),
                            args: vec![],
                            out_destination: OutDestination::Command(
                                Box::new(CommandScriptLine
                                {
                                    exec: "log".to_string(),
                                    args: vec![],
                                    out_destination: OutDestination::StdOut,
                                    err_destination: ErrDestination::StdErr,
                                })
                            ),
                            err_destination: ErrDestination::StdErr,
                        })
                    ),
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Two-word invocation piped with output directed to a file */
    #[test]
    fn out_file_basic()
    {
        assert_eq!(
            CommandScript::parse("python build.py > build/out"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out_destination: OutDestination::File("build/out".to_string()),
                    err_destination: ErrDestination::StdErr,
                }
            ]}));
    }

    /*  Two-word invocation piped with error directed to a file */
    #[test]
    fn err_file_basic()
    {
        assert_eq!(
            CommandScript::parse("python build.py 2> build/out.err"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out_destination: OutDestination::StdOut,
                    err_destination: ErrDestination::File("build/out.err".to_string()),
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
                2> build/err"),
            Ok(CommandScript{lines:vec![
                CommandScriptLine
                {
                    exec: "python".to_string(),
                    args: vec!["build.py".to_string()],
                    out_destination: OutDestination::File("build/out".to_string()),
                    err_destination: ErrDestination::File("build/err".to_string()),
                }
            ]}));
    }
}
