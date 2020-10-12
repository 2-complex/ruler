use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

use crate::ticket::Ticket;

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
        for t in self.targets.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n").unwrap();
        for (t, u) in self.source_indices.iter()
        {
            write!(f, "({}, {})\n", t, u).unwrap();
        }
        write!(f, ":\n").unwrap();
        for t in self.command.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n")
    }
}

struct EndpointPair
{
    start : usize,
    end : usize,
}

/*  Iterates through the given str, returns an EndpointPair indicating
    the line content (without the newline character itself) */
fn get_line_endpoints(content : &str) -> Vec<EndpointPair>
{
    let mut endpoints = Vec::new();
    let mut last_i = 0usize;
    let mut current_i = 0usize;
    for (i, c) in content.chars().enumerate()
    {
        current_i = i;
        match c
        {
            '\n' =>
            {
                endpoints.push(EndpointPair{
                    start:last_i,
                    end:current_i,
                });
                last_i = current_i+1;
            },
            _ => {},
        }
    }
    current_i += 1;

    if current_i > 1 && last_i < current_i
    {
        endpoints.push(EndpointPair{
            start:last_i,
            end:current_i,
        });
    }

    endpoints
}

/*  Takes a String and a vector of EndpointPairs.  Consumes both inputs and
    outputs a vector of Strings split off from the input String at the indices
    indicated by the endpoints. */
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

pub enum ParseError
{
    UnexpectedEmptyLine(usize),
    UnexpectedExtraColon(usize),
    UnexpectedContent(usize),
    UnexpectedEndOfFileMidTargets(usize),
    UnexpectedEndOfFileMidSources(usize),
    UnexpectedEndOfFileMidCommand(usize),
}

impl fmt::Display for ParseError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ParseError::UnexpectedEmptyLine(line_number) =>
                write!(formatter, "Unexpected emtpy line {}", line_number),

            ParseError::UnexpectedExtraColon(line_number) =>
                write!(formatter, "Unexpected extra ':' on line {}", line_number),

            ParseError::UnexpectedContent(line_number) =>
                write!(formatter, "Unexpected content on line {}", line_number),

            ParseError::UnexpectedEndOfFileMidTargets(line_number) =>
                write!(formatter, "Unexpected end of file mid-targets line {}", line_number),

            ParseError::UnexpectedEndOfFileMidSources(line_number) =>
                write!(formatter, "Unexpected end of file mid-sources line {}", line_number),

            ParseError::UnexpectedEndOfFileMidCommand(line_number) =>
                write!(formatter, "Unexpected end of file mid-command line {}", line_number),
        }
    }
}

/*  Reads in a .rules file content as a String, and creates a vector of Rule objects. */
pub fn parse(content: String) -> Result<Vec<Rule>, ParseError>
{
    enum Mode
    {
        Targets,
        Sources,
        Command,
        NewLine
    }

    let mut rules = Vec::new();
    let mut targets = vec![];
    let mut sources = vec![];
    let mut command = vec![];
    let mut mode = Mode::Targets;
    let mut line_number = 1;

    let endpoints = get_line_endpoints(&content);
    for line in split_along_endpoints(content, endpoints).drain(..)
    {
        match mode
        {
            Mode::Targets =>
            {
                match line.as_ref()
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(line_number)),
                    ":" => mode = Mode::Sources,
                    _ => targets.push(line),
                }
            },
            Mode::Sources =>
            {
                match line.as_ref()
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(line_number)),
                    ":" => mode = Mode::Command,
                    _ => sources.push(line),
                }
            },
            Mode::Command =>
            {
                match line.as_ref()
                {
                    "" => return Err(ParseError::UnexpectedEmptyLine(line_number)),
                    ":" =>
                    {
                        mode = Mode::NewLine;
                        rules.push(Rule::new(targets, sources, command));
                        targets = vec![];
                        sources = vec![];
                        command = vec![];
                    }
                    _ => command.push(line),
                }
            },
            Mode::NewLine =>
            {
                match line.as_ref()
                {
                    "" => mode = Mode::Targets,
                    ":" => return Err(ParseError::UnexpectedExtraColon(line_number)),
                    _ => return Err(ParseError::UnexpectedContent(line_number)),
                }
            },
        }

        line_number += 1;
    }

    match mode
    {
        Mode::NewLine => return Ok(rules),
        Mode::Targets => return Err(ParseError::UnexpectedEndOfFileMidTargets(line_number)),
        Mode::Sources => return Err(ParseError::UnexpectedEndOfFileMidSources(line_number)),
        Mode::Command => return Err(ParseError::UnexpectedEndOfFileMidCommand(line_number)),
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
        EndpointPair,
        split_along_endpoints,
        parse,
        ParseError,
        get_line_endpoints,
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
            Err(error) => panic!(format!("Expected success, got: {}", error)),
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
            Err(error) => panic!(format!("Expected success, got: {}", error)),
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
            Err(error) => panic!(format!("Expected success, got: {}", error)),
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
            Err(error) => panic!(format!("Expected success, got: {}", error)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
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
            Err(why) => panic!(format!("Expected success, got: {}", why)),
        }
    }

    /*  Check the function split_along_endpoints returns an empty list when given the empty string. */
    #[test]
    fn split_along_endpoints_empty()
    {
        let v = split_along_endpoints("".to_string(), vec![]);
        assert_eq!(v.len(), 0);
    }

    /*  Call split_along_endpoints with a string and endpoints that cut a prefix off the string,
        Check that the prefix is returned. */
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

    /*  Call split_along_endpoints with two words and endpoints that cut the string into the two words,
        Check that two words are returned as separate strings. */
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

    /*  Call split_along_endpoints with two words with junk interspersed and endpoints that separate out the two words,
        Check that two words are returned as separate strings. */
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

    /*  Call get_line_endpoints on a string with no newlines.  Check we get that string's endpoints in a vec */
    #[test]
    fn get_line_endpoints_empty()
    {
        let v = get_line_endpoints("abcd");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].start, 0);
        assert_eq!(v[0].end, 4);
    }

    /*  Call get_line_endpoints on a string ending in a newline. Check that we get endpoints capturing the
        string upto but not including the newline. */
    #[test]
    fn get_line_endpoints_one()
    {
        let v = get_line_endpoints("abcd\n");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].start, 0);
        assert_eq!(v[0].end, 4);
    }

    /*  Call get_line_endpoints on a string with newlines interspersed. Check that we get endpoints capturing
        the non-newline content. */
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

    /*  Call get_line_endpoints on a string with no newline at the end. Check that we get endpoints capturing
        the non-newline content. */
    #[test]
    fn get_line_endpoints_no_newline_at_end()
    {
        let v = get_line_endpoints("ab\ncd");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].start, 0);
        assert_eq!(v[0].end, 2);
        assert_eq!(v[1].start, 3);
        assert_eq!(v[1].end, 5);
    }

    /*  Call get_line_endpoints on a string with newlines interspersed. Check that we get endpoints capturing
        the non-newline content.  This time by extracting substrings using the endpoints */
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

    /*  Combine get_line_endpoints and split_along_endpoints to parse a properly formatted rule. */
    #[test]
    fn split_along_endpoints_rule()
    {
        let text = "a\n:\nb\n:\nc\n:\n".to_string();
        let endpoints = get_line_endpoints(&text);
        assert_eq!(endpoints.len(), 6);

        let v = split_along_endpoints(text, endpoints);
        assert_eq!(v.len(), 6);
        assert_eq!(v[0], "a");
        assert_eq!(v[1], ":");
        assert_eq!(v[2], "b");
        assert_eq!(v[3], ":");
        assert_eq!(v[4], "c");
        assert_eq!(v[5], ":");
    }

    /*  Call parse on an empty string, check that the rule list is empty. */
    #[test]
    fn parse_empty()
    {
        match parse("".to_string())
        {
            Ok(_) =>
            {
                panic!(format!("Unexpected success when parsing empty string"));
            },
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidTargets(line_number) =>
                    {
                        assert_eq!(line_number, 1);
                    },
                    _=> panic!("Expected unexpected end of file mid-targets error"),
                }
            }
        };
    }

    /*  Call parse on a properly formatted rule, check that the targets,
        sources and command are what was in the text. */
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

    /*  Call parse on twp properly formatted rules, check that the targets,
        sources and command are what was in the text. */
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

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_extra_newline_error1()
    {
        match parse("\na\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEmptyLine(line_number) => assert_eq!(line_number, 1),
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_extra_newline_error2()
    {
        match parse("a\n:\nb\n\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEmptyLine(line_number) => assert_eq!(line_number, 4),
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_extra_newline_error3()
    {
        match parse("a\n:\nb\n:\nc\n:\n\n\nd\n:\ne\n:\nf\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEmptyLine(line_number) => assert_eq!(line_number, 8),
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets1()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidTargets(line_number) =>
                    {
                        assert_eq!(line_number, 15);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets2()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidTargets(line_number) =>
                    {
                        assert_eq!(line_number, 16);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_targets3()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf\n:\n\nt\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidTargets(line_number) =>
                    {
                        assert_eq!(line_number, 16);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources1()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidSources(line_number) =>
                    {
                        assert_eq!(line_number, 10);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources2()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ns".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidSources(line_number) =>
                    {
                        assert_eq!(line_number, 11);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_sources3()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ns\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidSources(line_number) =>
                    {
                        assert_eq!(line_number, 11);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command1()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidCommand(line_number) =>
                    {
                        assert_eq!(line_number, 12);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command2()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\n".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidCommand(line_number) =>
                    {
                        assert_eq!(line_number, 12);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

    /*  Call parse on improperly formatted rules, check the error. */
    #[test]
    fn parse_unexpected_eof_mid_command3()
    {
        match parse("a\n:\nb\n:\nc\n:\n\nd\n:\ne\n:\nf".to_string())
        {
            Ok(_) => panic!(format!("Unexpected success")),
            Err(error) =>
            {
                match error
                {
                    ParseError::UnexpectedEndOfFileMidCommand(line_number) =>
                    {
                        assert_eq!(line_number, 13);
                    },
                    error => panic!("Unexpected {}", error),
                }
            }
        };
    }

}
