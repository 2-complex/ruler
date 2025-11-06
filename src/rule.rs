use std::fmt;

use crate::bundle::
{
    self,
    PathBundle
};

#[derive(Debug, PartialOrd, Ord, Eq, PartialEq, Clone)]
pub struct Rule
{
    pub targets : Vec<String>,
    pub sources : Vec<String>,
    pub command : String,
}

/*  When a rule is first parsed, it goes into this struct, the targets,
    sources and command are simply parsed into vecs.  This is before the
    topological-sort step which puts the data into a list of Nodes. */
impl Rule
{
    pub fn new(
        targets : Vec<String>,
        sources : Vec<String>,
        command : String) -> Rule
    {
        Rule
        {
            targets: targets,
            sources: sources,
            command: command
        }
    }
}

impl fmt::Display for Rule
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        for t in self.targets.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n").unwrap();
        for t in self.sources.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n").unwrap();
        write!(f, "{}", self.command).unwrap();
        write!(f, ":\n")
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError
{
    UnexpectedEmptyLine(String, usize),
    UnexpectedEndOfFileMidTargets(String, usize),
    UnexpectedEndOfFileMidSources(String, usize),
    UnexpectedEndOfFileMidCommand(String, usize),
    BundleError(String, bundle::ParseError),
}

impl fmt::Display for ParseError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ParseError::UnexpectedEmptyLine(filename, line_number) =>
                write!(formatter, "{}:{} Unexpected empty line", filename, line_number),

            ParseError::UnexpectedEndOfFileMidTargets(filename, line_number) =>
                write!(formatter, "{}:{} Unexpected end of file mid-targets line", filename, line_number),

            ParseError::UnexpectedEndOfFileMidSources(filename, line_number) =>
                write!(formatter, "{}:{} Unexpected end of file mid-sources line", filename, line_number),

            ParseError::UnexpectedEndOfFileMidCommand(filename, line_number) =>
                write!(formatter, "{}:{} Unexpected end of file mid-command line", filename, line_number),

            ParseError::BundleError(filename, bundle_error) =>
                write!(formatter, "Bundle parse error {}:{}", filename, bundle_error),
        }
    }
}

/*  Takes a vector of string-pairs representing (filename, content).  Parses
    each file's contents as rules and returns one big vector full of Rule objects.

    If the parsing of any one file presents an error, this function returns the
    ParseError object for the first error, and does not bother parsing the
    rest. */
pub fn parse_all(mut contents : Vec<(String, String)>)
-> Result<Vec<Rule>, ParseError>
{
    let mut result : Vec<Rule> = vec![];
    for (filename, content) in contents.drain(..)
    {
        result.extend(parse(filename, content)?);
    }

    Ok(result)
}

/*  Reads in a .rules file content as a String, and creates a vector of Rule
    objects. */
pub fn parse(filename : String, content : String)
-> Result<Vec<Rule>, ParseError>
{
    #[derive(Debug)]
    enum Mode
    {
        Pending,
        Targets,
        Sources,
        Command,
    }

    let mut rules = Vec::new();
    let mut mode = Mode::Pending;
    let mut line_number = 1;
    let mut section_start = 0usize;
    let mut triple = ('\0', '\0', '\0');

    let mut source_bundle = None;
    let mut target_bundle = None;

    for (i, c) in content.char_indices().chain(vec![(content.len(), '\0')].into_iter())
    {
        triple = (triple.1, triple.2, c);
        
        if c == '\n'
        {
            line_number += 1;
        }

        match mode
        {
            Mode::Pending =>
            {
                if triple.2 != '\n' && triple.2 != '\0'
                {
                    section_start = i;
                    mode = Mode::Targets;
                }
            },
            Mode::Targets =>
            {
                if triple.1 == '\n' && triple.2 == '\n'
                {
                    return Err(ParseError::UnexpectedEmptyLine(filename, line_number-1));
                }

                if triple == ('\n', ':', '\n') || triple == ('\n', ':', '\0')
                {
                    let section = &content[section_start..(i-2)];
                    section_start = i+1;
                    target_bundle = Some(match PathBundle::parse(section)
                    {
                        Ok(bundle) => bundle,
                        Err(error) => return Err(ParseError::BundleError(filename, error)),
                    });
                    mode = Mode::Sources;
                }
            },
            Mode::Sources =>
            {
                if triple.1 == '\n' && triple.2 == '\n'
                {
                    return Err(ParseError::UnexpectedEmptyLine(filename, line_number-1));
                }

                if triple == ('\n', ':', '\n') || triple == ('\n', ':', '\0')
                {
                    let section = &content[section_start..(i-2)];
                    section_start = i+1;
                    source_bundle = Some(match PathBundle::parse(section)
                    {
                        Ok(bundle) => bundle,
                        Err(error) => return Err(ParseError::BundleError(filename, error)),
                    });
                    mode = Mode::Command;
                }
            },
            Mode::Command =>
            {
                if triple.1 == '\n' && triple.2 == '\n'
                {
                    return Err(ParseError::UnexpectedEmptyLine(filename, line_number-1));
                }

                if triple == ('\n', ':', '\n') || triple == ('\n', ':', '\0')
                {
                    let section = &content[section_start..(i-2)];

                    rules.push(Rule::new(
                        target_bundle.take().unwrap().get_path_strings('/'),
                        source_bundle.take().unwrap().get_path_strings('/'),
                        section.to_string()));

                    mode = Mode::Pending
                }
            }
        }
    }

    match mode
    {
        Mode::Pending => return Ok(rules),
        Mode::Targets => return Err(ParseError::UnexpectedEndOfFileMidTargets(filename, line_number)),
        Mode::Sources => return Err(ParseError::UnexpectedEndOfFileMidSources(filename, line_number)),
        Mode::Command => return Err(ParseError::UnexpectedEndOfFileMidCommand(filename, line_number)),
    }
}

#[cfg(test)]
mod tests
{
    use crate::rule::
    {
        Rule,
        parse,
        parse_all,
        ParseError
    };

    /*  Call parse on an empty string, check that the rule list is empty. */
    #[test]
    fn parse_empty()
    {
        assert_eq!(parse("empty.rules".to_string(), "".to_string()).unwrap(), vec![]);
    }

    /*  Call parse on a properly formatted rule, check that the targets,
        sources and command are what was in the text. */
    #[test]
    fn parse_one_rule()
    {
        let result = parse(
            "one.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n".to_string());

        match result
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].targets, vec!["a".to_string()]);
                assert_eq!(v[0].sources, vec!["b".to_string()]);
                assert_eq!(v[0].command, "c".to_string());
            },
            Err(why) => panic!("Expected success, got: {}", why),
        };
    }

    /*  Call parse on twp properly formatted rules, check that the targets,
        sources and command are what was in the text. */
    #[test]
    fn parse_two()
    {
        match parse(
            "paper.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].targets, vec!["a".to_string()]);
                assert_eq!(v[0].sources, vec!["b".to_string()]);
                assert_eq!(v[0].command, "c".to_string());
                assert_eq!(v[1].targets, vec!["d".to_string()]);
                assert_eq!(v[1].sources, vec!["e".to_string()]);
                assert_eq!(v[1].command, "f".to_string());
            },
            Err(why) => panic!("Expected success, got: {}", why),
        };
    }

    #[test]
    fn parse_bundles()
    {
        let content = "\
build
\tmath.o
:
cpp
\tmath.cpp
\tmath.h
:
c++ -c math.cpp -o build/math.o
:
".to_string();
        assert_eq!(
            parse("parsnip.rules".to_string(), content),
            Ok(vec![
                Rule
                {
                    targets: vec!["build/math.o".to_string()],
                    sources: vec![
                        "cpp/math.cpp".to_string(),
                        "cpp/math.h".to_string(),
                    ],
                    command: "c++ -c math.cpp -o build/math.o".to_string()
                }
            ])
        );
    }

    #[test]
    fn parse_all_empty()
    {
        match parse_all(vec![])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 0);
            },
            Err(why) => panic!("Expected success, got: {}", why),
        };
    }

    #[test]
    fn parse_all_one()
    {
        match parse_all(vec![("rulesfile1".to_string(), "a\n:\nb\n:\nc\n:\n".to_string())])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].targets, vec!["a".to_string()]);
                assert_eq!(v[0].sources, vec!["b".to_string()]);
                assert_eq!(v[0].command, "c".to_string());
            },
            Err(why) => panic!("Expected success, got: {}", why),
        };
    }

    #[test]
    fn parse_all_two()
    {
        match parse_all(
            vec![
                ("rulesfile1".to_string(), "a\n:\nb\n:\nc\n:\n".to_string()),
                ("rulesfile2".to_string(), "d\n:\ne\n:\nf\n:\n".to_string())
                ])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].targets, vec!["a".to_string()]);
                assert_eq!(v[0].sources, vec!["b".to_string()]);
                assert_eq!(v[0].command, "c".to_string());
                assert_eq!(v[1].targets, vec!["d".to_string()]);
                assert_eq!(v[1].sources, vec!["e".to_string()]);
                assert_eq!(v[1].command, "f".to_string());
            },
            Err(why) => panic!("Expected success, got: {}", why),
        };
    }

    /*  Call parse on rules with some extra empty lines in there, that is okay */
    #[test]
    fn parse_allow_empty_lines_at_the_beginning_of_the_file()
    {
        parse("banana.rules".to_string(),
"
a
:
b
:
c
:
".to_string()).unwrap();
    }

    /*  Call parse on rules that end with the final colon, that is okay. */
    #[test]
    fn parse_allow_no_newline_at_end_of_file()
    {
        parse("banana.rules".to_string(),
"\
a
:
b
:
c
:".to_string()).unwrap();
    }

    /*  Call parse on rules with extra empty lines in the sources, that errors */
    #[test]
    fn parse_extra_newline_mid_sources_error()
    {
        assert_eq!(parse(
            "fruit.rules".to_string(),
            "\
a
:
b

:
".to_string()),
        Err(ParseError::UnexpectedEmptyLine("fruit.rules".to_string(), 4)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn allow_empty_lines_between_rules()
    {
        parse(
            "well.rules".to_string(),
"\
a
:
b
:
c
:


d
:
e
:
f
:
".to_string()).unwrap();
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets1()
    {
        assert_eq!(parse(
            "glass.rules".to_string(),
            "a".to_string()), Err(ParseError::UnexpectedEndOfFileMidTargets("glass.rules".to_string(), 1)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_targets1()
    {
        assert_eq!(parse(
            "glass.rules".to_string(),
            "a\n".to_string()), Err(ParseError::UnexpectedEndOfFileMidTargets("glass.rules".to_string(), 2)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets2()
    {
        assert_eq!(parse(
            "spider.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt".to_string()),
            Err(ParseError::UnexpectedEndOfFileMidTargets("spider.rules".to_string(), 15)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_targets3()
    {
        assert_eq!(parse(
            "movie.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt\n".to_string()),
            Err(ParseError::UnexpectedEndOfFileMidTargets("movie.rules".to_string(), 16)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets3()
    {
        assert_eq!(parse(
            "movie.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt".to_string()),
            Err(ParseError::UnexpectedEndOfFileMidTargets("movie.rules".to_string(), 15)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_newline_mid_sources1()
    {
        assert_eq!(parse(
            "box.rules".to_string(),
"\
a
:
b
:
c
:

d
:
".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("box.rules".to_string(), 10)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources1()
    {
        assert_eq!(parse(
            "box.rules".to_string(),
"\
a
:
b
:
c
:

d
:".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("box.rules".to_string(), 9)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources2()
    {
        match parse(
            "house".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ns".to_string())
        {
            Ok(_) => panic!("Unexpected success"),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidSources(filename, line_number) =>
                    {
                        assert_eq!(filename, "house".to_string());
                        assert_eq!(line_number, 10);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_sources3()
    {
        assert_eq!(parse(
            "pi.rules".to_string(),
            "\
a
:
b
:
c
:

d
:
s
".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("pi.rules".to_string(), 11)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources3()
    {
        assert_eq!(parse(
            "pi.rules".to_string(),
            "\
a
:
b
:
c
:

d
:
s".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("pi.rules".to_string(), 10)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_command1()
    {
        assert_eq!(parse(
            "green.rules".to_string(),
"\
a
:
b
:
c
:

d
:
e
:
".to_string()), Err(ParseError::UnexpectedEndOfFileMidCommand("green.rules".to_string(), 12)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_command2()
    {
        assert_eq!(parse(
            "sunset.rules".to_string(),
"\
a
:
b
:
c
:

d
:
e
:
".to_string()),
        Err(ParseError::UnexpectedEndOfFileMidCommand("sunset.rules".to_string(), 12)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command1()
    {
        assert_eq!(parse(
            "green.rules".to_string(),
"\
a
:
b
:
c
:

d
:
e
:".to_string()), Err(ParseError::UnexpectedEndOfFileMidCommand("green.rules".to_string(), 11)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command2()
    {
        assert_eq!(parse(
            "sunset.rules".to_string(),
"\
a
:
b
:
c
:

d
:
e
:".to_string()),
        Err(ParseError::UnexpectedEndOfFileMidCommand("sunset.rules".to_string(), 11)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command3()
    {
        match parse(
            "tape.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf".to_string())
        {
            Ok(_) => panic!("Unexpected success"),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidCommand(filename, line_number) =>
                    {
                        assert_eq!(filename, "tape.rules".to_string());
                        assert_eq!(line_number, 12);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }
}
