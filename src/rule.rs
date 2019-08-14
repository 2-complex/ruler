use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt;

pub struct Rule
{
    pub targets : Vec<String>,
    pub sources : Vec<String>,
    pub command : Vec<String>,
}

pub struct Record
{
    pub targets: Vec<String>,
    pub source_indices: Vec<(usize, usize)>,
    pub command : Vec<String>,
    sources: Vec<String>,
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

impl Record
{
    pub fn all(&self) -> String
    {
        return "asdfasdfasdf".to_string();
    }
}

struct EndpointPair
{
    start : usize,
    end : usize,
}

fn get_line_endpoints(content : &str) -> Vec<EndpointPair>
{
    let mut endpoints = Vec::new();
    let mut last_i = 0usize;
    for (i, c) in content.chars().enumerate()
    {
        match c
        {
            '\n' =>
            {
                endpoints.push(EndpointPair{
                    start:last_i,
                    end:i,
                });
                last_i = i+1;
            },
            _ => {},
        }
    }

    endpoints
}

fn split_along_endpoints(
    mut content : String,
    mut endpoints : Vec<EndpointPair>) -> Vec<String>
{
    let mut result = Vec::new();
    let mut total = 0usize;

    for p in endpoints.drain(..)
    {
        let mut chunk = content.split_off(p.start - total);
        content = chunk.split_off(p.end - p.start);
        chunk.shrink_to_fit();
        total = p.end;
        result.push(chunk);
    }

    result
}

enum Mode
{
    Targets,
    Sources,
    Command,
    NewLine
}

use self::Mode::Targets;
use self::Mode::Sources;
use self::Mode::Command;
use self::Mode::NewLine;

pub fn parse(mut content: String) -> Result<Vec<Rule>, String>
{
    let mut rules = Vec::new();
    let mut targets = vec![];
    let mut sources = vec![];
    let mut command = vec![];
    let mut mode = Targets;
    let mut line_number = 1;

    let endpoints = get_line_endpoints(&content);
    for line in split_along_endpoints(content, endpoints).drain(..)
    {
        match mode
        {
            Targets =>
            {
                match line.as_ref()
                {
                    "" => return Err(format!("Unexpected empty line ({})", line_number)),
                    ":" => mode = Sources,
                    _ => targets.push(line),
                }
            },
            Sources =>
            {
                match line.as_ref()
                {
                    "" => return Err(format!("Unexpected empty line ({})", line_number)),
                    ":" => mode = Command,
                    _ => sources.push(line),
                }
            },
            Command =>
            {
                match line.as_ref()
                {
                    "" => return Err(format!("Unexpected empty line {}", line_number)),
                    ":" =>
                    {
                        mode = NewLine;
                        rules.push(
                            Rule
                            {
                                targets : targets,
                                sources : sources,
                                command : command,
                            }
                        );
                        targets = vec![];
                        sources = vec![];
                        command = vec![];
                    }
                    _ => command.push(line),
                }
            },
            NewLine =>
            {
                match line.as_ref()
                {
                    "" => mode = Targets,
                    ":" => return Err(format!("Unexpected extra ':' on line {}", line_number)),
                    _ => return Err(format!("Unexpected content on line {}", line_number)),
                }
            },
        }

        line_number += 1;
    }

    match mode
    {
        NewLine => return Ok(rules),
        Targets => return Err(format!("Unexpected end of file mid-targets")),
        Sources => return Err(format!("Unexpected end of file mid-sources")),
        Command => return Err(format!("Unexpected end of file mid-command")),
    }
}

struct Frame
{
    record: Record,
    visited: bool,
}

/*  Consume Rules, and in their place, make Records.
    In each Record, leave 'source_indices' empty.

    Returns:
        record_buffer:
            A vector of optional records corresponding to original rules
        to_buffer_index:
            A map that tells us the index in record_buffer of the
            record that has the given string as a target */
fn rules_to_record_buffer(mut rules : Vec<Rule>) -> Result<(
        Vec<Option<Record>>,
        HashMap<String, (usize, usize)>
    ), String>
{
    let mut record_buffer : Vec<Option<Record>> = Vec::new();
    let mut to_buffer_index : HashMap<String, (usize, usize)> = HashMap::new();

    let mut current_buffer_index = 0usize;
    for rule in rules.drain(..)
    {
        for (sub_index, target) in rule.targets.iter().enumerate()
        {
            let t_string = target.to_string();
            match to_buffer_index.get(&t_string)
            {
                Some(_) => return Err(format!("Target found in more than one rule: {}", t_string)),
                None => to_buffer_index.insert(t_string, (current_buffer_index, sub_index)),
            };
        }

        record_buffer.push(Some(
            Record
            {
                targets: rule.targets,
                sources: rule.sources,
                command: rule.command,
                source_indices: vec![],
            }
        ));

        current_buffer_index+=1;
    }

    Ok((record_buffer, to_buffer_index))
}


pub fn topological_sort(
    rules : Vec<Rule>,
    goal_target : &str) -> Result<VecDeque<Record>, String>
{
    match rules_to_record_buffer(rules)
    {
        Err(why) => return Err(why),
        Ok((mut record_buffer, mut to_buffer_index)) =>
        {
            let mut current_buffer_index = record_buffer.len();

            let mut stack : Vec<Frame> = Vec::new();
            match to_buffer_index.get(goal_target)
            {
                Some((index, _)) =>
                    stack.push(
                        Frame
                        {
                            record: record_buffer[*index].take().unwrap(),
                            visited: false,
                        }
                    ),
                None => return Err(format!("Target missing from rules: {}", goal_target)),
            }

            let mut result : VecDeque<Record> = VecDeque::new();
            let mut index_bijection : Vec<usize> = Vec::new();

            while let Some(frame) = stack.pop()
            {
                if frame.visited
                {
                    index_bijection.push(result.len());
                    result.push_back(frame.record);
                }
                else
                {
                    let mut buffer = Vec::new();

                    for source in frame.record.sources.iter()
                    {
                        match to_buffer_index.get(source)
                        {
                            Some((buffer_index, _sub_index)) =>
                            {
                                if let Some(record) = record_buffer[*buffer_index].take()
                                {
                                    buffer.push(
                                        Frame
                                        {
                                            record: record,
                                            visited: false,
                                        }
                                    );
                                }
                            },
                            None =>
                            {
                                result.push_back(
                                    Record
                                    {
                                        targets: vec![source.clone()],
                                        sources: vec![],
                                        command: vec![],
                                        source_indices: vec![],
                                    }
                                );
                                index_bijection.push(result.len());
                                record_buffer.push(None);
                                to_buffer_index.insert(source.to_string(), (current_buffer_index, 0));
                                current_buffer_index+=1;
                            },
                        }
                    }

                    stack.push(
                        Frame
                        {
                            record: frame.record,
                            visited: true
                        }
                    );

                    stack.append(&mut buffer);
                }
            }

            /*  Finally, remap the sources of all the records to indices in the new result vector itself.*/
            for record in result.iter_mut()
            {
                for source in record.sources.drain(..)
                {
                    let (buffer_index, sub_index) = to_buffer_index.get(&source).unwrap();
                    record.source_indices.push((index_bijection[*buffer_index], *sub_index));
                }
            }

            Ok(result)
        }
    }

}

mod tests
{
    use crate::rule::rules_to_record_buffer;
    use crate::rule::topological_sort;
    use crate::rule::{EndpointPair, split_along_endpoints, parse, get_line_endpoints};
    use crate::rule::Rule;

    #[test]
    fn rules_are_rules()
    {
        let rulefile = "abc".to_string();
        let r = Rule
        {
            targets : vec![rulefile[0..1].to_string()],
            sources : vec![rulefile[1..2].to_string()],
            command : vec![rulefile[2..3].to_string()],
        };

        assert_eq!(r.targets[0], "a");
        assert_eq!(r.sources[0], "b");
        assert_eq!(r.command[0], "c");
    }

    #[test]
    fn rules_to_record_buffer_empty_to_empty()
    {
        match rules_to_record_buffer(vec![])
        {
            Ok((record_buffer, to_record_buffer_index)) =>
            {
                assert_eq!(record_buffer.len(), 0);
                assert_eq!(to_record_buffer_index.len(), 0);
            },
            Err(_) => panic!("Error on empty vector"),
        }
    }

    #[test]
    fn rules_to_record_buffer_one_to_one()
    {
        match rules_to_record_buffer(
                vec![
                    Rule
                    {
                        targets: vec!["plant".to_string(), "fruit".to_string()],
                        sources: vec!["soil".to_string(), "seed".to_string()],
                        command: vec!["water every day".to_string()],
                    },
                ]
            )
        {
            Ok((record_buffer, to_record_buffer_index)) =>
            {
                assert_eq!(record_buffer.len(), 1);
                assert_eq!(to_record_buffer_index.len(), 2);

                assert_eq!(*to_record_buffer_index.get("plant").unwrap(), (0usize, 0usize));
                assert_eq!(*to_record_buffer_index.get("fruit").unwrap(), (0usize, 1usize));

                let (record_index, target_index) = to_record_buffer_index.get("plant").unwrap();
                assert_eq!(*record_index, 0usize);

                match &record_buffer[*record_index]
                {
                    Some(record) => assert_eq!(record.targets[*target_index], "plant"),
                    None => panic!("Expected some record found None"),
                }

                let (record_index, target_index) = to_record_buffer_index.get("fruit").unwrap();
                assert_eq!(*record_index, 0usize);

                match &record_buffer[*record_index]
                {
                    Some(record) =>
                    {
                        assert_eq!(record.targets[*target_index], "fruit");
                        assert_eq!(record.sources[0], "soil");
                        assert_eq!(record.sources[1], "seed");
                        match record.command.first()
                        {
                            Some(command) =>
                            {
                                assert_eq!(command, "water every day");
                            },
                            None => panic!("Expected some command found None"),
                        }
                    }
                    None => panic!("Expected some record found None"),
                }

                assert_eq!(*to_record_buffer_index.get("fruit").unwrap(), (0usize, 1usize));
            },
            Err(_) => panic!("Error on legit rules"),
        }

    }

    #[test]
    fn rules_to_record_buffer_two_to_two()
    {
        match rules_to_record_buffer(
            vec![
                Rule
                {
                    targets: vec!["fruit".to_string()],
                    sources: vec!["plant".to_string()],
                    command: vec!["pick occasionally".to_string()],
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec!["soil".to_string(), "seed".to_string()],
                    command: vec!["water every day".to_string()],
                },
            ]
        )
        {
            Ok((record_buffer, to_record_buffer_index)) =>
            {
                assert_eq!(record_buffer.len(), 2);
                assert_eq!(to_record_buffer_index.len(), 2);

                assert_eq!(*to_record_buffer_index.get("fruit").unwrap(), (0usize, 0usize));
                assert_eq!(*to_record_buffer_index.get("plant").unwrap(), (1usize, 0usize));
            },
            Err(_) => panic!("Error on legit rules"),
        }
    }

    #[test]
    fn rules_to_record_buffer_redundancy_error()
    {
        match rules_to_record_buffer(
            vec![
                Rule
                {
                    targets: vec!["fruit".to_string()],
                    sources: vec!["plant".to_string()],
                    command: vec!["pick occasionally".to_string()],
                },
                Rule
                {
                    targets: vec!["plant".to_string(), "fruit".to_string()],
                    sources: vec!["soil".to_string(), "seed".to_string()],
                    command: vec!["water every day".to_string()],
                },
            ]
        )
        {
            Ok(_) =>
            {
                panic!("Unexpected success on rules with redundant targets");
            },
            Err(msg) =>
            {
                assert_eq!(msg, "Target found in more than one rule: fruit");
            }
        }
    }

    #[test]
    fn topological_sort_empty_is_error()
    {
        match topological_sort(vec![], "some target")
        {
            Ok(_) =>
            {
                panic!("Enexpected success on topological sort of empty");
            },
            Err(msg) =>
            {
                assert_eq!(msg, "Target missing from rules: some target");
            },
        }
    }


    #[test]
    fn topological_sort_one_rule()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec![],
                    command: vec![],
                },
            ],
            "plant")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 1);
            }
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        }
    }

    #[test]
    fn topological_sort_two_rules()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["fruit".to_string()],
                    sources: vec!["plant".to_string()],
                    command: vec!["pick occasionally".to_string()],
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec![],
                    command: vec![],
                },
            ],
            "fruit")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].targets[0], "plant");
                assert_eq!(v[1].targets[0], "fruit");
            }
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        }
    }

    #[test]
    fn topological_sort_make_records_for_sources()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["fruit".to_string()],
                    sources: vec!["plant".to_string()],
                    command: vec!["pick occasionally".to_string()],
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec!["soil".to_string(), "water".to_string(), "seed".to_string(), "sunlight".to_string()],
                    command: vec!["take care of plant".to_string()],
                },
            ],
            "fruit")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "soil");
                assert_eq!(v[1].targets[0], "water");
                assert_eq!(v[2].targets[0], "seed");
                assert_eq!(v[3].targets[0], "sunlight");
                assert_eq!(v[4].targets[0], "plant");
                assert_eq!(v[5].targets[0], "fruit");
            }
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        }
    }

    #[test]
    fn split_along_endpoints_empty()
    {
        let v = split_along_endpoints("".to_string(), vec![]);
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn split_along_endpoints_one()
    {
        let v = split_along_endpoints("apples".to_string(),
            vec![
                EndpointPair
                {
                    start: 0usize,
                    end: 3usize,
                }
            ]
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "app");
    }

    #[test]
    fn split_along_endpoints_two()
    {
        let v = split_along_endpoints("applesbananas".to_string(),
            vec![
                EndpointPair
                {
                    start: 0usize,
                    end: 6usize,
                },
                EndpointPair
                {
                    start: 6usize,
                    end: 13usize,
                },
            ]
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], "apples");
        assert_eq!(v[1], "bananas");
    }

    #[test]
    fn split_along_endpoints_two_padding()
    {
        let v = split_along_endpoints("123apples012bananas".to_string(),
            vec![
                EndpointPair
                {
                    start: 3usize,
                    end: 9usize,
                },
                EndpointPair
                {
                    start: 12usize,
                    end: 19usize,
                },
            ]
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], "apples");
        assert_eq!(v[1], "bananas");
    }

    #[test]
    fn get_line_endpoints_empty()
    {
        let v = get_line_endpoints("abcd");
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn get_line_endpoints_one()
    {
        let v = get_line_endpoints("abcd\n");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].start, 0);
        assert_eq!(v[0].end, 4);
    }

    #[test]
    fn get_line_endpoints_two()
    {
        let v = get_line_endpoints("ab\ncd\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].start, 0);
        assert_eq!(v[0].end, 2);
        assert_eq!(v[1].start, 3);
        assert_eq!(v[1].end, 5);
    }

    #[test]
    fn get_line_endpoints_rule()
    {
        let s = "a\n:\nb\n:\nc\n:\n";
        let v = get_line_endpoints(s);
        assert_eq!(v.len(), 6);
        assert_eq!(s[v[0].start..v[0].end], *"a");
        assert_eq!(s[v[1].start..v[1].end], *":");
        assert_eq!(s[v[2].start..v[2].end], *"b");
        assert_eq!(s[v[3].start..v[3].end], *":");
        assert_eq!(s[v[4].start..v[4].end], *"c");
        assert_eq!(s[v[5].start..v[5].end], *":");
    }

    #[test]
    fn split_along_endpoints_rule()
    {
        let text = "a\n:\nb\n:\nc\n:\n".to_string();
        let endpoints = get_line_endpoints(&text);
        assert_eq!(endpoints.len(), 6);

        let v = split_along_endpoints(text, endpoints);
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn parse_empty()
    {
        match parse("".to_string())
        {
            Ok(_) =>
            {
                panic!(format!("Unexpected success when parsing empty string"));
            },
            Err(why) =>
            {
                assert_eq!(why, "Unexpected end of file mid-targets")
            }
        };
    }

    #[test]
    fn parse_one()
    {
        match parse("a\n:\nb\n:\nc\n:\n".to_string())
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].targets, vec!["a".to_string()]);
                assert_eq!(v[0].sources, vec!["b".to_string()]);
                assert_eq!(v[0].command, vec!["c".to_string()]);
            },
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        };
    }

    #[test]
    fn parse_two()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        };
    }

    #[test]
    fn parse_extra_newline_error1()
    {
        match parse("\na\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(v) => panic!(format!("Unexpected success")),
            Err(why) =>
            {
                assert_eq!(why, "Unexpected empty line (1)");
            }
        };
    }

    #[test]
    fn parse_extra_newline_error2()
    {
        match parse("a\n:\nb\n\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(v) => panic!(format!("Unexpected success")),
            Err(why) =>
            {
                assert_eq!(why, "Unexpected empty line (4)");
            }
        };
    }

    #[test]
    fn parse_extra_newline_error3()
    {
        match parse("a\n:\nb\n:\nc\n:\n\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(v) => panic!(format!("Unexpected success")),
            Err(why) =>
            {
                assert_eq!(why, "Unexpected empty line (8)");
            }
        };
    }

    #[test]
    fn parse_extra_newline_error4()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\n".to_string())
        {
            Ok(v) => panic!(format!("Unexpected success")),
            Err(why) =>
            {
                assert_eq!(why, "Unexpected end of file mid-targets");
            }
        };
    }
}

