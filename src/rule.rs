extern crate regex;

use regex::Regex;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt;

pub struct Rule
{
    pub targets : Vec<String>,
    pub sources : Vec<String>,
    pub command : VecDeque<String>,
}

pub struct Record
{
    pub targets: Vec<String>,
    pub source_indices: Vec<(usize, usize)>,
    pub command : VecDeque<String>,
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

struct Frame
{
    record: Record,
    visited: bool,
}

pub fn parse(content: String) -> Result<Vec<Rule>, String>
{
    let result = Vec::new();
    let big_re = Regex::new(r"([^\n:][^:]*\n:\n[^\n:][^:]*\n:\n[^\n:][^:]*\n:\n)").unwrap();

    for mat in big_re.find_iter(&content)
    {
        println!("{:?}", mat);

        /*
        let rule_re = Regex::new(r"([^\n:][^:]*)\n:\n([^\n:][^:]*)\n:\n([^\n:][^:]*)\n:\n").unwrap();
        let all = big_caps.get(1).unwrap().as_str();

        for mat in rule_re.find_iter(all)
        {
            println!("{:?}", mat);
        }
        */
    }

    Ok(result)
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
                                        command: VecDeque::new(),
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
    use crate::rule::parse;
    use crate::rule::Rule;
    use std::collections::VecDeque;

    #[test]
    fn rules_are_rules()
    {
        let rulefile = "abc".to_string();
        let r = Rule
        {
            targets : vec![rulefile[0..1].to_string()],
            sources : vec![rulefile[1..2].to_string()],
            command : VecDeque::from(vec![rulefile[2..3].to_string()]),
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
                        command: VecDeque::from(vec!["water every day".to_string()]),
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
                        match record.command.front()
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
                    command: VecDeque::from(vec!["pick occasionally".to_string()]),
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec!["soil".to_string(), "seed".to_string()],
                    command: VecDeque::from(vec!["water every day".to_string()]),
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
                    command: VecDeque::from(vec!["pick occasionally".to_string()]),
                },
                Rule
                {
                    targets: vec!["plant".to_string(), "fruit".to_string()],
                    sources: vec!["soil".to_string(), "seed".to_string()],
                    command: VecDeque::from(vec!["water every day".to_string()]),
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
                    command: VecDeque::from(vec![]),
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
                    command: VecDeque::from(vec!["pick occasionally".to_string()]),
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec![],
                    command: VecDeque::from(vec![]),
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
                    command: VecDeque::from(vec!["pick occasionally".to_string()]),
                },
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec!["soil".to_string(), "water".to_string(), "seed".to_string(), "sunlight".to_string()],
                    command: VecDeque::from(vec!["take care of plant".to_string()]),
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
    fn parse_empty()
    {
        match parse("".to_string())
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 0);
            },
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            },
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        };
    }
}

