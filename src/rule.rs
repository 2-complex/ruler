use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

use crate::ticket::Ticket;
use crate::bundle::
{
    self,
    PathBundle
};

#[derive(Debug, PartialEq)]
pub struct Rule
{
    pub targets : Vec<String>,
    pub sources : Vec<String>,
    pub command : Vec<String>,
}

/*  When a rule is first parsed, it goes into this struct, the targets,
    sources and command are simply parsed into vecs.  This is before the
    topological-sort step which puts the data into a list of Nodes and
    creates Nodes for sources that are not listed as targest of rules. */
impl Rule
{
    fn new(
        mut targets : Vec<String>,
        mut sources : Vec<String>,
        command : Vec<String>) -> Rule
    {
        targets.sort();
        sources.sort();
        Rule
        {
            targets: targets,
            sources: sources,
            command: command
        }
    }
}

/*  Once the rules are topologically sorted, the data in them gets put into
    this struct.  Instead of storing each source as a path, this stores
    indices indicating which other node has the source as a target.

    Node also carries an optional Ticket.  If the Node came from a rule,
    that's the hash of the rule itself (not file content). */
#[derive(Debug)]
pub struct Node
{
    pub targets: Vec<String>,
    pub source_indices: Vec<(usize, usize)>,
    pub command : Vec<String>,
    pub rule_ticket : Option<Ticket>,
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


impl fmt::Display for Node
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match &self.rule_ticket
        {
            Some(ticket) =>
            {
                write!(f, "\n").unwrap();
                for t in self.targets.iter()
                {
                    write!(f, "{}\n", t).unwrap();
                }
                write!(f, "{}\n\n", ticket).unwrap();
            },
            None =>
            {
                for t in self.targets.iter()
                {
                    write!(f, "{}\n", t).unwrap();
                }
            }
        }

        write!(f, "")
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

    If the aprsing of any one file presents an error, this function returns the
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

struct Frame
{
    targets: Vec<String>,
    sources: Vec<String>,
    command: Vec<String>,
    rule_ticket: Option<Ticket>,
    index: usize,
    sub_index: usize,
    visited: bool,
}

impl Frame
{
    fn from_source_and_index(source : &str, index : usize) -> Frame
    {
        Frame
        {
            targets: vec![source.to_string()],
            sources: vec![],
            command: vec![],
            rule_ticket: None,
            index: index,
            sub_index: 0,
            visited: true,
        }
    }

    fn from_rule_and_index(rule : Rule, index : usize) -> Frame
    {
        let rule_ticket = Ticket::from_strings(
            &rule.targets,
            &rule.sources,
            &rule.command);

        Frame
        {
            targets: rule.targets,
            sources: rule.sources,
            command: rule.command,
            rule_ticket: Some(rule_ticket),
            index: index,
            sub_index: 0,
            visited: false,
        }
    }
}

#[derive(Debug)]
pub enum TopologicalSortError
{
    TargetMissing(String),
    SelfDependentRule(String),
    CircularDependence(Vec<String>),
    TargetInMultipleRules(String),
}

impl fmt::Display for TopologicalSortError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            TopologicalSortError::TargetMissing(target) =>
                write!(formatter, "Target missing from rules: {}", target),

            TopologicalSortError::SelfDependentRule(target)  =>
                write!(formatter, "Self-dependent target: {}", target),

            TopologicalSortError::CircularDependence(cycle) =>
            {
                write!(formatter, "Circular dependence:\n")?;
                for t in cycle.iter()
                {
                    write!(formatter, "{}\n", t)?;
                }

                Ok(())
            },

            TopologicalSortError::TargetInMultipleRules(target) =>
                write!(formatter, "Target found in more than one rule: {}", target),
        }
    }
}

/*  Consume Rules, and in their place, make Nodes.
    In each Node, leave 'source_indices' empty.

    Returns:
        frame_buffer:
            A vector of optional frames corresponding to original rules
        to_buffer_index:
            A map that tells us the index in frame_buffer of the
            ndoe that has the given string as a target, and also subindex, the index in that
            node's target list of the target in question  */
fn rules_to_frame_buffer(mut rules : Vec<Rule>)
-> Result<
    (Vec<Option<Frame>>, HashMap<String, (usize, usize)>),
    TopologicalSortError>
{
    let mut frame_buffer : Vec<Option<Frame>> = Vec::new();
    let mut to_buffer_index : HashMap<String, (usize, usize)> = HashMap::new();

    let mut current_buffer_index = 0usize;
    for mut rule in rules.drain(..)
    {
        rule.targets.sort();
        rule.sources.sort();
        for (sub_index, target) in rule.targets.iter().enumerate()
        {
            let t_string = target.to_string();
            match to_buffer_index.get(&t_string)
            {
                Some(_) => return Err(TopologicalSortError::TargetInMultipleRules(t_string)),
                None => to_buffer_index.insert(t_string, (current_buffer_index, sub_index)),
            };
        }

        frame_buffer.push(Some(Frame::from_rule_and_index(rule, current_buffer_index)));
        current_buffer_index += 1;
    }

    Ok((frame_buffer, to_buffer_index))
}

struct TopologicalSortMachine
{
    /*  The "buffer" referred to by variable-names here is
        the buffer of frames (frame_buffer) */
    frame_buffer : Vec<Option<Frame>>,

    /*  Sends the target name to a pair of indices:
        - index of the rule in which it's a target
        - index of the target in the rule's target list */
    to_buffer_index : HashMap<String, (usize, usize)>,

    /*  Keeps track of the next index to insert into frame_buffer with */
    current_buffer_index : usize,

    /*  Recall frame_buffer is a vector of options.  That's so that
        the frames can be taken from frame_buffer and added to frames_in_order */
    frames_in_order : Vec<Frame>,

    /*  This maps index in frame_buffer to index in frames_in_order */
    index_bijection : HashMap<usize, usize>,
}

/*  Holds the state of the topological sort, so that we can either sort from one origin,
    or continue sorting until all rules have been visited. */
impl TopologicalSortMachine
{
    pub fn new(
        frame_buffer : Vec<Option<Frame>>,
        to_buffer_index : HashMap<String, (usize, usize)>
    )
    -> TopologicalSortMachine
    {
        let frame_buffer_length = frame_buffer.len();
        TopologicalSortMachine
        {
            frame_buffer : frame_buffer,
            to_buffer_index : to_buffer_index,
            current_buffer_index : frame_buffer_length,
            frames_in_order : vec![],
            index_bijection : HashMap::new(),
        }
    }

    /*  Originates a topological sort DFS from the frame indicated by the given index, noting
        the given sub_index as the location of the goal-target in that frame's target list. */
    pub fn sort_once(&mut self, index : usize, sub_index : usize)
    -> Result<(), TopologicalSortError>
    {
        let starting_frame =
        match self.frame_buffer[index].take()
        {
            Some(mut frame) =>
            {
                frame.sub_index = sub_index;
                frame
            },
            None =>
            {
                /*  Assume we're in the middle of a build-all operation,
                    and we've already handle this rule. */
                return Ok(());
            }, 
                
        };

        let mut indices_in_stack = HashSet::new();
        indices_in_stack.insert(index);
        let mut stack = vec![starting_frame];

        /*  Depth-first traversal using 'stack' */
        while let Some(frame) = stack.pop()
        {
            indices_in_stack.remove(&frame.index);

            if frame.visited
            {
                self.index_bijection.insert(frame.index, self.frames_in_order.len());
                self.frames_in_order.push(frame);
            }
            else
            {
                let mut reverser = vec![];
                for source in frame.sources.iter()
                {
                    match self.to_buffer_index.get(source)
                    {
                        Some((buffer_index, sub_index)) =>
                        {
                            if let Some(mut frame) = self.frame_buffer[*buffer_index].take()
                            {
                                frame.sub_index = *sub_index;
                                reverser.push(frame);
                            }
                            else
                            {
                                if frame.index == *buffer_index
                                {
                                    return Err(TopologicalSortError::SelfDependentRule(
                                        frame.targets[*sub_index].clone()));
                                }

                                /*  Look for a cycle by checking the stack for another instance of the node we're
                                    currently on */
                                if indices_in_stack.contains(buffer_index)
                                {
                                    let mut target_cycle = vec![];
                                    for f in stack.iter()
                                    {
                                        target_cycle.push(f.targets[f.sub_index].clone());
                                    }
                                    target_cycle.push(frame.targets[frame.sub_index].clone());

                                    return Err(TopologicalSortError::CircularDependence(target_cycle));
                                }
                            }
                        },
                        None =>
                        {
                            self.index_bijection.insert(self.current_buffer_index, self.frames_in_order.len());
                            self.frames_in_order.push(Frame::from_source_and_index(source, self.current_buffer_index));
                            self.frame_buffer.push(None);
                            self.to_buffer_index.insert(source.to_string(), (self.current_buffer_index, 0));
                            self.current_buffer_index += 1;
                        },
                    }
                }

                stack.push(
                    Frame
                    {
                        targets: frame.targets,
                        sources: frame.sources,
                        command: frame.command,
                        rule_ticket: frame.rule_ticket,
                        index: frame.index,
                        sub_index: frame.sub_index,
                        visited: true
                    }
                );
                indices_in_stack.insert(frame.index);

                while let Some(f) = reverser.pop()
                {
                    indices_in_stack.insert(f.index);
                    stack.push(f);
                }
            }
        }

        Ok(())
    }

    /*  Remap the sources of all the nodes to indices in the new result vector. */
    pub fn get_result(mut self) -> Result<Vec<Node>, TopologicalSortError>
    {
        let mut result = vec![];
        for mut frame in self.frames_in_order.drain(..)
        {
            let mut source_indices = vec![];
            for source in frame.sources.drain(..)
            {
                let (buffer_index, sub_index) = self.to_buffer_index.get(&source).unwrap();
                source_indices.push((*self.index_bijection.get(buffer_index).unwrap(), *sub_index));
            }

            result.push(
                Node
                {
                    targets: frame.targets,
                    source_indices: source_indices,
                    command: frame.command,
                    rule_ticket: frame.rule_ticket,
                }
            );
        }

        Ok(result)
    }

}

/*  Takes a vector of Rules and goal_target, goal target is the target in whose rule the
    search originates.

    Rules contain enough information to establish a dependence tree. This function
    searches that tree to create a sorted list of another type: Node.

    Leaves (sources which are not also listed as targets) become Nodes with a non-existant
    RuleInfo and an empty list of sources. */
pub fn topological_sort(
    rules : Vec<Rule>,
    goal_target : &str) -> Result<Vec<Node>, TopologicalSortError>
{
    /*  Convert Rules to Frames.  Frame has some extra eleements
        that facilitate the topological sort. */
    match rules_to_frame_buffer(rules)
    {
        Err(error) =>
        {
            /*  If two rules have the same target, we wind up here. */
            return Err(error);
        },
        Ok((frame_buffer, to_buffer_index)) =>
        {
            let (index, sub_index) =
            match to_buffer_index.get(goal_target)
            {
                Some((index, sub_index)) =>
                {
                    (*index, *sub_index)
                },
                None => return Err(TopologicalSortError::TargetMissing(goal_target.to_string())),
            };

            let mut machine = TopologicalSortMachine::new(frame_buffer, to_buffer_index);
            machine.sort_once(index, sub_index)?;
            return machine.get_result();
        }
    }
}

/*  For building all targets.  This function calls rules_to_frame_buffer to generate frames for the rules,
    then iterates through all the frames */
pub fn topological_sort_all(
    rules : Vec<Rule>) -> Result<Vec<Node>, TopologicalSortError>
{
    /*  Convert Rules to Frames.  Frame has some extra eleements
        that facilitate the topological sort. */
    match rules_to_frame_buffer(rules)
    {
        Err(error) =>
        {
            /*  If two rules have the same target, we wind up here. */
            return Err(error);
        },
        Ok((frame_buffer, to_buffer_index)) =>
        {
            let frame_buffer_len = frame_buffer.len();

            let mut machine = TopologicalSortMachine::new(frame_buffer, to_buffer_index);

            for index in 0..frame_buffer_len
            {
                machine.sort_once(index, 0)?;
            }

            return machine.get_result();
        }
    }
}



#[cfg(test)]
mod tests
{
    use crate::rule::
    {
        Rule,
        rules_to_frame_buffer,
        topological_sort,
        topological_sort_all,
        TopologicalSortError,
        parse,
        parse_all,
        ParseError,
    };

    /*  Use the Rule constructor with some vectors of strings, and check that the
        strings end up in the right place. */
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

    /*  Call rules_to_frame_buffer with an empty vector, make sure we get an empty
        frame_buffer and an empty map. */
    #[test]
    fn rules_to_frame_buffer_empty_to_empty()
    {
        match rules_to_frame_buffer(vec![])
        {
            Ok((frame_buffer, to_frame_buffer_index)) =>
            {
                assert_eq!(frame_buffer.len(), 0);
                assert_eq!(to_frame_buffer_index.len(), 0);
            },
            Err(_) => panic!("Error on empty vector"),
        }
    }

    /*  Call rules_to_frame_buffer with a vector with just one rule in it,
        one rule with a A couple sources a couple targets and a command. */
    #[test]
    fn rules_to_frame_buffer_one_to_one()
    {
        match rules_to_frame_buffer(
                vec![
                    Rule
                    {
                        targets: vec!["plant".to_string(), "tangerine".to_string()],
                        sources: vec!["seed".to_string(), "soil".to_string()],
                        command: vec!["water every day".to_string()],
                    },
                ]
            )
        {
            Ok((frame_buffer, to_frame_buffer_index)) =>
            {
                /*  There should be one frame, and pairs in the map:
                    plant -> (0, 0)
                    tangerine -> (0, 1) */
                assert_eq!(frame_buffer.len(), 1);
                assert_eq!(to_frame_buffer_index.len(), 2);

                assert_eq!(*to_frame_buffer_index.get("plant").unwrap(), (0usize, 0usize));
                assert_eq!(*to_frame_buffer_index.get("tangerine").unwrap(), (0usize, 1usize));

                /*  to_frame_buffer_index maps a target to a pair of indices: the index of the node
                    and the index of the target in the node. */
                let (node_index, target_index) = to_frame_buffer_index.get("plant").unwrap();
                assert_eq!(*node_index, 0usize);

                /*  Check that there's a node at that index with the right target */
                match &frame_buffer[*node_index]
                {
                    Some(frame) => assert_eq!(frame.targets[*target_index], "plant"),
                    None => panic!("Expected some node with target 'plant' found None"),
                }

                /*  to_frame_buffer_index maps a target to a pair of indices: the index of the node
                    and the index of the target in the node. */
                let (node_index, target_index) = to_frame_buffer_index.get("tangerine").unwrap();
                assert_eq!(*node_index, 0usize);

                /*  Check that there's a node at that index with the right target */
                match &frame_buffer[*node_index]
                {
                    Some(frame) => assert_eq!(frame.targets[*target_index], "tangerine"),
                    None => panic!("Expected some node with target 'tangerine' found None"),
                }

                /*  Get the frame (at index 0), and check that the sources and command are what was set above. */
                match &frame_buffer[*node_index]
                {
                    Some(frame) =>
                    {
                        assert_eq!(frame.targets[*target_index], "tangerine");
                        assert_eq!(frame.sources[0], "seed");
                        assert_eq!(frame.sources[1], "soil");
                        match frame.command.first()
                        {
                            Some(command) =>
                            {
                                assert_eq!(command, "water every day");
                            },
                            None => panic!("Expected some command found None"),
                        }
                    }
                    None => panic!("Expected some node found None"),
                }

                assert_eq!(*to_frame_buffer_index.get("tangerine").unwrap(), (0usize, 1usize));
            },
            Err(_) => panic!("Error on legit rules"),
        }

    }

    #[test]
    fn rules_to_frame_buffer_two_to_two()
    {
        match rules_to_frame_buffer(
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
            Ok((frame_buffer, to_frame_buffer_index)) =>
            {
                assert_eq!(frame_buffer.len(), 2);
                assert_eq!(to_frame_buffer_index.len(), 2);

                assert_eq!(*to_frame_buffer_index.get("fruit").unwrap(), (0usize, 0usize));
                assert_eq!(*to_frame_buffer_index.get("plant").unwrap(), (1usize, 0usize));
            },
            Err(_) => panic!("Error on legit rules"),
        }
    }

    /*  Create a list of rules where two rules list the same target.
        Try to call rules_to_frame_buffer, and check that an error-result is returned reporting the redundant target */
    #[test]
    fn rules_to_frame_buffer_redundancy_error()
    {
        match rules_to_frame_buffer(
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
            Err(error) =>
            {
                match error
                {
                    TopologicalSortError::TargetInMultipleRules(target) => assert_eq!(target, "fruit"),
                    _ => panic!("Unexpected error type when multiple fruit expected")
                }
            }
        }
    }

    /*  Topological sort the empty set of rules, but with a goal-target.  That should error. */
    #[test]
    fn topological_sort_empty_is_error()
    {
        match topological_sort(vec![], "prune")
        {
            Ok(_) =>
            {
                panic!("Enexpected success on topological sort of empty");
            },
            Err(error) =>
            {
                match error
                {
                    TopologicalSortError::TargetMissing(target) => assert_eq!(target, "prune"),
                    _ => panic!("Expected target missing prune, got another type of error")
                }
            },
        }
    }

    /*  Topological sort all of an empty set of rules, check that the result is empty. */
    #[test]
    fn topological_sort_all_empty_is_empty()
    {
        match topological_sort_all(vec![])
        {
            Ok(result) =>
            {
                assert_eq!(result.len(), 0);
            },
            Err(error) =>
            {
                panic!("Expected success topological sorting empty vector of rules, got {}", error);
            },
        }
    }

    /*  Topological sort a list of one rule only.  Check the result
        contains a frame with just that one rule's data. */
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
            Ok(nodes) =>
            {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].targets[0], "plant");
            }
            Err(error) => panic!("Expected success, got: {}", error),
        }
    }

    /*  Topological sort a list of one rule only.  Check the result
        contains a frame with just that one rule's data. */
    #[test]
    fn topological_sort_all_one_rule()
    {
        match topological_sort_all(
            vec![
                Rule
                {
                    targets: vec!["plant".to_string()],
                    sources: vec![],
                    command: vec![],
                },
            ])
        {
            Ok(nodes) =>
            {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].targets[0], "plant");
            }
            Err(error) => panic!("Expected success, got: {}", error),
        }
    }

    /*  Topological sort a list of two rules only, one depends on the other as a source, but
        the order in the given list is backwards.  Check that the topological sort reverses the order. */
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
            Ok(nodes) =>
            {
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0].targets[0], "plant");
                assert_eq!(nodes[1].targets[0], "fruit");
            }
            Err(error) => panic!("Expected success, got: {}", error),
        }
    }

    /*  Topological sort all of a list of two rules only, one depends on the other as a source, but
        the order in the given list is backwards.  Check that the topological sort reverses the order. */
    #[test]
    fn topological_sort_all_two_rules()
    {
        match topological_sort_all(
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
            ])
        {
            Ok(nodes) =>
            {
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0].targets[0], "plant");
                assert_eq!(nodes[1].targets[0], "fruit");
            }
            Err(error) => panic!("Expected success, got: {}", error),
        }
    }

    /*  Topological sort a DAG that is not a tree.  Four nodes math, physics, graphics, game
        physics and graphics both depend on math, and game depends on physics and graphics. */
    #[test]
    fn topological_sort_four_rules_diamond_already_in_order()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["math".to_string()],
                    sources: vec![],
                    command: vec![],
                },
                Rule
                {
                    targets: vec!["physics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build physics".to_string()],
                },
                Rule
                {
                    targets: vec!["graphics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build graphics".to_string()],
                },
                Rule
                {
                    targets: vec!["game".to_string()],
                    sources: vec!["graphics".to_string(), "physics".to_string()],
                    command: vec!["build game".to_string()],
                },
            ],
            "game")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 4);
                assert_eq!(v[0].targets[0], "math");
                assert_eq!(v[1].targets[0], "graphics");
                assert_eq!(v[2].targets[0], "physics");
                assert_eq!(v[3].targets[0], "game");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 1);
                assert_eq!(v[1].source_indices[0], (0, 0));
                assert_eq!(v[2].source_indices.len(), 1);
                assert_eq!(v[2].source_indices[0], (0, 0));
                assert_eq!(v[3].source_indices.len(), 2);
                assert_eq!(v[3].source_indices[0], (1, 0));
                assert_eq!(v[3].source_indices[1], (2, 0));
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }


    /*  Topological sort a DAG that is not a tree.  Four nodes math, physics, graphics, game
        physics and graphics both depend on math, and game depends on physics and graphics.

        This is the same test as above, except the given vector is in the wrong order.  The result
        should be the same as the above.  Part of this is to test well-definedness of order. */
    #[test]
    fn topological_sort_four_rules_diamond_scrambled()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["graphics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build graphics".to_string()],
                },
                Rule
                {
                    targets: vec!["physics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build physics".to_string()],
                },
                Rule
                {
                    targets: vec!["math".to_string()],
                    sources: vec![],
                    command: vec![],
                },
                Rule
                {
                    targets: vec!["game".to_string()],
                    sources: vec!["physics".to_string(), "graphics".to_string()],
                    command: vec!["build game".to_string()],
                },
            ],
            "game")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 4);
                assert_eq!(v[0].targets[0], "math");
                assert_eq!(v[1].targets[0], "graphics");
                assert_eq!(v[2].targets[0], "physics");
                assert_eq!(v[3].targets[0], "game");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 1);
                assert_eq!(v[1].source_indices[0], (0, 0));
                assert_eq!(v[2].source_indices.len(), 1);
                assert_eq!(v[2].source_indices[0], (0, 0));
                assert_eq!(v[3].source_indices.len(), 2);
                assert_eq!(v[3].source_indices[0], (1, 0));
                assert_eq!(v[3].source_indices[1], (2, 0));
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort all rules in a DAG that is not a tree.  Four nodes math, physics, graphics, game
        physics and graphics both depend on math, and game depends on physics and graphics.

        This is the same test as above, except the given vector is in the wrong order.  The result
        should be the same as the above.  Part of this is to test well-definedness of order. */
    #[test]
    fn topological_sort_all_four_rules_diamond_scrambled()
    {
        match topological_sort_all(
            vec![
                Rule
                {
                    targets: vec!["graphics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build graphics".to_string()],
                },
                Rule
                {
                    targets: vec!["physics".to_string()],
                    sources: vec!["math".to_string()],
                    command: vec!["build physics".to_string()],
                },
                Rule
                {
                    targets: vec!["math".to_string()],
                    sources: vec![],
                    command: vec![],
                },
                Rule
                {
                    targets: vec!["game".to_string()],
                    sources: vec!["physics".to_string(), "graphics".to_string()],
                    command: vec!["build game".to_string()],
                },
            ])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 4);
                assert_eq!(v[0].targets[0], "math");
                assert_eq!(v[1].targets[0], "graphics");
                assert_eq!(v[2].targets[0], "physics");
                assert_eq!(v[3].targets[0], "game");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 1);
                assert_eq!(v[1].source_indices[0], (0, 0));
                assert_eq!(v[2].source_indices.len(), 1);
                assert_eq!(v[2].source_indices[0], (0, 0));
                assert_eq!(v[3].source_indices.len(), 2);
                assert_eq!(v[3].source_indices[0], (1, 0));
                assert_eq!(v[3].source_indices[1], (2, 0));
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort a poetry example.  This has two intermediate build results that share
        a source file.  It's a bit like the diamond, except the shared source is not a rule,
        just a file in the file system, and there are other source-files, too.

        The topologial sort should not only put the nodes in order, but also create nodes for the
        source files not specifically represented as rules. */
    #[test]
    fn topological_sort_poem()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["stanza1".to_string()],
                    sources: vec!["chorus".to_string(), "verse1".to_string()],
                    command: vec!["poemcat verse1 chorus".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza2".to_string()],
                    sources: vec!["chorus".to_string(), "verse2".to_string()],
                    command: vec!["poemcat verse2 chorus".to_string()],
                },
                Rule
                {
                    targets: vec!["poem".to_string()],
                    sources: vec!["stanza1".to_string(), "stanza2".to_string()],
                    command: vec!["poemcat stanza1 stanza2".to_string()],
                },
            ],
            "poem")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "chorus");
                assert_eq!(v[1].targets[0], "verse1");
                assert_eq!(v[2].targets[0], "stanza1");
                assert_eq!(v[3].targets[0], "verse2");
                assert_eq!(v[4].targets[0], "stanza2");
                assert_eq!(v[5].targets[0], "poem");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 0);
                assert_eq!(v[3].source_indices.len(), 0);

                assert_eq!(v[2].source_indices, [(0, 0), (1, 0)]);
                assert_eq!(v[4].source_indices, [(0, 0), (3, 0)]);
                assert_eq!(v[5].source_indices, [(2, 0), (4, 0)]);
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort a poetry example.  This test is just like the one above but with the
        given list of rules in a different order.  The result should be the same. */
    #[test]
    fn topological_sort_poem_scrambled()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["poem".to_string()],
                    sources: vec!["stanza1".to_string(), "stanza2".to_string()],
                    command: vec!["poemcat stanza1 stanza2".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza2".to_string()],
                    sources: vec!["verse2".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse2 chorus".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza1".to_string()],
                    sources: vec!["verse1".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse1 chorus".to_string()],
                },
            ],
            "poem")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "chorus");
                assert_eq!(v[1].targets[0], "verse1");
                assert_eq!(v[2].targets[0], "stanza1");
                assert_eq!(v[3].targets[0], "verse2");
                assert_eq!(v[4].targets[0], "stanza2");
                assert_eq!(v[5].targets[0], "poem");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 0);
                assert_eq!(v[3].source_indices.len(), 0);

                assert_eq!(v[2].source_indices, [(0, 0), (1, 0)]);
                assert_eq!(v[4].source_indices, [(0, 0), (3, 0)]);
                assert_eq!(v[5].source_indices, [(2, 0), (4, 0)]);
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort a poetry example.  This test is just like the one above but with the
        given list of rules in a different order.  The result should be the same. */
    #[test]
    fn topological_sort_all_poem_scrambled()
    {
        match topological_sort_all(
            vec![
                Rule
                {
                    targets: vec!["poem".to_string()],
                    sources: vec!["stanza1".to_string(), "stanza2".to_string()],
                    command: vec!["poemcat stanza1 stanza2".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza2".to_string()],
                    sources: vec!["verse2".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse2 chorus".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza1".to_string()],
                    sources: vec!["verse1".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse1 chorus".to_string()],
                },
            ])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "chorus");
                assert_eq!(v[1].targets[0], "verse1");
                assert_eq!(v[2].targets[0], "stanza1");
                assert_eq!(v[3].targets[0], "verse2");
                assert_eq!(v[4].targets[0], "stanza2");
                assert_eq!(v[5].targets[0], "poem");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 0);
                assert_eq!(v[3].source_indices.len(), 0);

                assert_eq!(v[2].source_indices, [(0, 0), (1, 0)]);
                assert_eq!(v[4].source_indices, [(0, 0), (3, 0)]);
                assert_eq!(v[5].source_indices, [(2, 0), (4, 0)]);
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort a poetry example.  This test is just like the one above but with the
        given list of rules in a different order.  The result should be the same. */
    #[test]
    fn topological_sort_all_disconnected_graph()
    {
        match topological_sort_all(
            vec![
                Rule
                {
                    targets: vec!["poem".to_string()],
                    sources: vec!["stanza1".to_string(), "stanza2".to_string()],
                    command: vec!["poemcat stanza1 stanza2".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza2".to_string()],
                    sources: vec!["verse2".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse2 chorus".to_string()],
                },
                Rule
                {
                    targets: vec!["stanza1".to_string()],
                    sources: vec!["verse1".to_string(), "chorus".to_string()],
                    command: vec!["poemcat verse1 chorus".to_string()],
                },
            ])
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "chorus");
                assert_eq!(v[1].targets[0], "verse1");
                assert_eq!(v[2].targets[0], "stanza1");
                assert_eq!(v[3].targets[0], "verse2");
                assert_eq!(v[4].targets[0], "stanza2");
                assert_eq!(v[5].targets[0], "poem");

                assert_eq!(v[0].source_indices.len(), 0);
                assert_eq!(v[1].source_indices.len(), 0);
                assert_eq!(v[3].source_indices.len(), 0);

                assert_eq!(v[2].source_indices, [(0, 0), (1, 0)]);
                assert_eq!(v[4].source_indices, [(0, 0), (3, 0)]);
                assert_eq!(v[5].source_indices, [(2, 0), (4, 0)]);
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Topological sort a dependence graph with a cycle in it.  Check that the error
        returned points to the cycle. */
    #[test]
    fn topological_sort_circular()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["Quine".to_string(), "SomethingElse".to_string()],
                    sources: vec!["Hofstadter".to_string()],
                    command: vec!["poemcat Hofstadter".to_string()],
                },
                Rule
                {
                    targets: vec!["AnotherThing".to_string(), "Hofstadter".to_string()],
                    sources: vec!["Quine".to_string()],
                    command: vec!["poemcat Quine".to_string()],
                },
            ],
            "Quine")
        {
            Ok(_) => panic!("Unexpected success topologically sorting with a circular dependence"),
            Err(error) =>
            {
                match error
                {
                    TopologicalSortError::CircularDependence(cycle) =>
                    {
                        assert_eq!(cycle[0], "Quine");
                        assert_eq!(cycle[1], "Hofstadter");
                    },
                    _ => panic!("Expected circular dependence, got another type of error")
                }
            },
        }
    }

    /*  Make a Rule that depends on /itself/ as a source.  Try to topologial sort,
        expect the error to reflect the self-dependence  */
    #[test]
    fn topological_sort_self_reference()
    {
        match topological_sort(
            vec![
                Rule
                {
                    targets: vec!["Hofstadter".to_string()],
                    sources: vec!["Hofstadter".to_string()],
                    command: vec!["poemcat Hofstadter".to_string()],
                },
            ],
            "Hofstadter")
        {
            Ok(_) => panic!("Unexpected success topologically sorting with a self-dependent rule"),
            Err(error) =>
            {
                match error
                {
                    TopologicalSortError::SelfDependentRule(target) => assert_eq!(target, "Hofstadter"),
                    _ => panic!("Expected self-dependent rule, got another type of error")
                }
            },
        }
    }

    /*  Create a rule with a few sources that don't exist as targets of other rules.
        Perform a topological sort and check that the sources are created as nodes. */
    #[test]
    fn topological_sort_make_nodes_for_sources()
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
                    sources: vec![
                        "seed".to_string(),
                        "soil".to_string(),
                        "sunlight".to_string(),
                        "water".to_string(),
                    ],
                    command: vec!["take care of plant".to_string()],
                },
            ],
            "fruit")
        {
            Ok(v) =>
            {
                assert_eq!(v.len(), 6);
                assert_eq!(v[0].targets[0], "seed");
                assert_eq!(v[1].targets[0], "soil");
                assert_eq!(v[2].targets[0], "sunlight");
                assert_eq!(v[3].targets[0], "water");
                assert_eq!(v[4].targets[0], "plant");
                assert_eq!(v[5].targets[0], "fruit");
            }
            Err(why) => panic!("Expected success, got: {}", why),
        }
    }

    /*  Call parse on an empty string, check that the rule list is empty. */
    #[test]
    fn parse_empty()
    {
        assert_eq!(parse("spool.rules".to_string(), "".to_string()).unwrap(), vec![]);
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
