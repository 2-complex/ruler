use std::fmt;

use crate::ticket::Ticket;
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
    pub command : Vec<String>,
}

fn is_sorted(data: &Vec<String>) -> bool
{
    data.windows(2).all(|w| w[0] <= w[1])
}

/*  When a rule is first parsed, it goes into this struct, the targets,
    sources and command are simply parsed into vecs.  This is before the
    topological-sort step which puts the data into a list of Nodes and
    creates Nodes for sources that are not listed as targest of rules. */
impl Rule
{
    pub fn new(
        targets : Vec<String>,
        sources : Vec<String>,
        command : Vec<String>) -> Rule
    {
        Rule
        {
            targets: targets,
            sources: sources,
            command: command
        }
    }

    pub fn get_ticket(self: &Self) -> Ticket
    {
        if is_sorted(&self.targets) && is_sorted(&self.sources)
        {
            Ticket::from_strings(&self.targets, &self.sources, &self.command)
        }
        else
        {
            let mut t = self.targets.clone();
            let mut s = self.sources.clone();
            t.sort();
            s.sort();
            Ticket::from_strings(&t, &s, &self.command)
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
        for t in self.command.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n")
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError
{
    UnexpectedEmptyLine(String, usize),
    UnexpectedExtraColon(String, usize),
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
                write!(formatter, "Unexpected empty line {}:{}", filename, line_number),

            ParseError::UnexpectedExtraColon(filename, line_number) =>
                write!(formatter, "Unexpected extra ':' on line {}:{}", filename, line_number),

            ParseError::UnexpectedEndOfFileMidTargets(filename, line_number) =>
                write!(formatter, "Unexpected end of file mid-targets line {}:{}", filename, line_number),

            ParseError::UnexpectedEndOfFileMidSources(filename, line_number) =>
                write!(formatter, "Unexpected end of file mid-sources line {}:{}", filename, line_number),

            ParseError::UnexpectedEndOfFileMidCommand(filename, line_number) =>
                write!(formatter, "Unexpected end of file mid-command line {}:{}", filename, line_number),

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
    enum Mode
    {
        Pending,
        Targets,
        Sources,
        Command,
    }

    let mut rules = Vec::new();
    let mut target_lines = vec![];
    let mut source_lines = vec![];
    let mut command = vec![];
    let mut mode = Mode::Pending;
    let mut line_number = 1;

    let lines = content.split('\n').collect::<Vec<&str>>();

    for line in lines
    {
        match mode
        {
            Mode::Pending =>
            {
                match line
                {
                    "" => {},
                    ":" => return Err(ParseError::UnexpectedExtraColon(filename, line_number)),
                    _ =>
                    {
                        mode = Mode::Targets;
                        target_lines.push(line);
                    },
                }
            },
            Mode::Targets =>
            {
                match line
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(filename, line_number)),
                    ":" => mode = Mode::Sources,
                    _ => target_lines.push(line),
                }
            },
            Mode::Sources =>
            {
                match line
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(filename, line_number)),
                    ":" => mode = Mode::Command,
                    _ => source_lines.push(line),
                }
            },
            Mode::Command =>
            {
                match line
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(filename, line_number)),
                    ":" =>
                    {
                        mode = Mode::Pending;

                        let target_bundle = match PathBundle::parse_lines(target_lines)
                        {
                            Ok(bundle) => bundle,
                            Err(error) => return Err(ParseError::BundleError(filename, error)),
                        };

                        let source_bundle = match PathBundle::parse_lines(source_lines)
                        {
                            Ok(bundle) => bundle,
                            Err(error) => return Err(ParseError::BundleError(filename, error)),
                        };

                        let rule = Rule::new(
                            target_bundle.get_path_strings('/'),
                            source_bundle.get_path_strings('/'),
                            command);

                        rules.push(rule);

                        target_lines = vec![];
                        source_lines = vec![];
                        command = vec![];
                    }
                    _ => command.push(line.to_string()),
                }
            },
        }

        line_number += 1;
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
        ParseError,
    };

    #[test]
    fn rule_tickets_differ()
    {
        let z = Rule::new(vec!["".to_string()], vec!["".to_string()], vec!["".to_string()]);
        let a = Rule::new(vec!["a".to_string()], vec!["".to_string()], vec!["".to_string()]);
        let b = Rule::new(vec!["".to_string()], vec!["b".to_string()], vec!["".to_string()]);
        let c = Rule::new(vec!["".to_string()], vec!["".to_string()], vec!["c".to_string()]);

        assert_ne!(z.get_ticket(), a.get_ticket());
        assert_ne!(z.get_ticket(), b.get_ticket());
        assert_ne!(z.get_ticket(), c.get_ticket());

        assert_ne!(a.get_ticket(), b.get_ticket());
        assert_ne!(a.get_ticket(), c.get_ticket());

        assert_ne!(b.get_ticket(), c.get_ticket());
    }

    #[test]
    fn rule_target_orders_do_not_affect_ticket()
    {
        assert_eq!(
            Rule::new(
                vec!["".to_string()],
                vec!["apples".to_string(), "bananas".to_string()],
                vec!["".to_string()]).get_ticket(),
            Rule::new(
                vec!["".to_string()],
                vec!["bananas".to_string(), "apples".to_string()],
                vec!["".to_string()]).get_ticket()
        );

    }

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
                assert_eq!(v[0].command, vec!["c".to_string()]);
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
                assert_eq!(v[0].command, vec!["c".to_string()]);
                assert_eq!(v[1].targets, vec!["d".to_string()]);
                assert_eq!(v[1].sources, vec!["e".to_string()]);
                assert_eq!(v[1].command, vec!["f".to_string()]);
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
                    command: vec![
                        "c++ -c math.cpp -o build/math.o".to_string()
                    ]
                }
            ])
        );
    }

    #[test]
    fn parse_all_empty()
    {
        match parse_all(
            vec![])
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
                assert_eq!(v[0].command, vec!["c".to_string()]);
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
                assert_eq!(v[0].command, vec!["c".to_string()]);
                assert_eq!(v[1].targets, vec!["d".to_string()]);
                assert_eq!(v[1].sources, vec!["e".to_string()]);
                assert_eq!(v[1].command, vec!["f".to_string()]);
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

    /*  Call parse on rules that end with the final color, that is okay. */
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
        match parse(
            "fruit.rules".to_string(),
            "\
a
:
b

:
".to_string())
        {
            Ok(_) => panic!("Unexpected success"),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEmptyLine(filename, line_number) =>
                    {
                        assert_eq!(filename, "fruit.rules".to_string());
                        assert_eq!(line_number, 4);
                    }
                    error => panic!("Unexpected {}", error),
                }
            }
        };
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
            "a".to_string()), Err(ParseError::UnexpectedEndOfFileMidTargets("glass.rules".to_string(), 2)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_targets1()
    {
        assert_eq!(parse(
            "glass.rules".to_string(),
            "a\n".to_string()), Err(ParseError::UnexpectedEmptyLine("glass.rules".to_string(), 2)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets2()
    {
        match parse(
            "spider.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt".to_string())
        {
            Ok(_) => panic!("Unexpected success"),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidTargets(filename, line_number) =>
                    {
                        assert_eq!(filename, "spider.rules".to_string());
                        assert_eq!(line_number, 16);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_empty_line_mid_targets3()
    {
        assert_eq!(parse(
            "movie.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt\n".to_string()),
            Err(ParseError::UnexpectedEmptyLine("movie.rules".to_string(), 16)));
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets3()
    {
        assert_eq!(parse(
            "movie.rules".to_string(),
            "a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt".to_string()),
            Err(ParseError::UnexpectedEndOfFileMidTargets("movie.rules".to_string(), 16)));
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
".to_string()), Err(ParseError::UnexpectedEmptyLine("box.rules".to_string(), 10)));
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
:".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("box.rules".to_string(), 10)));
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
                        assert_eq!(line_number, 11);
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
".to_string()), Err(ParseError::UnexpectedEmptyLine("pi.rules".to_string(), 11)));
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
s".to_string()), Err(ParseError::UnexpectedEndOfFileMidSources("pi.rules".to_string(), 11)));
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
".to_string()), Err(ParseError::UnexpectedEmptyLine("green.rules".to_string(), 12)));
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
        Err(ParseError::UnexpectedEmptyLine("sunset.rules".to_string(), 12)));
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
:".to_string()), Err(ParseError::UnexpectedEndOfFileMidCommand("green.rules".to_string(), 12)));
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
        Err(ParseError::UnexpectedEndOfFileMidCommand("sunset.rules".to_string(), 12)));
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
                        assert_eq!(line_number, 13);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }
}
