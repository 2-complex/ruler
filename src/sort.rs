use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use crate::ticket::Ticket;
use crate::rule::Rule;

use std::fmt;


/*  When rules are converted into leaves and nodes as part of the topological sort step,
    This enum gets used to allow each Node to reference its sources either in the vec of nodes.  */
#[derive(Debug, PartialEq)]
pub enum SourceIndex
{
    /*  If the source referenced is a leaf, attach the index of that leaf in 'leaves' */
    Leaf(usize),

    /*  If the source referenced is another node in the list, use two indices:
        .0 = the index in nodes to find the source node S
        .1 = the index in the target list of S (often named sub_index in code) */
    Pair(usize, usize),
}

/*  Once the rules are topologically sorted, the data in them gets put into
    this struct.  Instead of storing each source as a path, this stores
    indices indicating which other node has the source as a target.

    Node also carries an optional Ticket.  If the Node came from a rule,
    that's the hash of the rule itself (not file content). */
#[derive(Debug, PartialEq)]
pub struct Node
{
    pub targets: Vec<String>,
    pub source_indices: Vec<SourceIndex>,
    pub command : Vec<String>,
    pub rule_ticket : Ticket,
}

impl fmt::Display for Node
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "\n").unwrap();
        for t in self.targets.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, "{}\n\n", self.rule_ticket).unwrap();
        write!(f, "")
    }
}


#[derive(PartialEq, Debug)]
struct Frame
{
    targets: Vec<String>,
    sources: Vec<String>,
    command: Vec<String>,
    rule_ticket: Ticket,
    index: usize,
    sub_index: usize,
    visited: bool,
}

impl Frame
{
    fn from_rule_and_index(rule : Rule, index : usize) -> Frame
    {
        let ticket = rule.get_ticket();
        Frame
        {
            targets: rule.targets,
            sources: rule.sources,
            command: rule.command,
            rule_ticket: ticket,
            index: index,
            sub_index: 0,
            visited: false,
        }
    }

    fn visit(self: Self) -> Frame
    {
        return Frame
        {
            targets: self.targets,
            sources: self.sources,
            command: self.command,
            rule_ticket: self.rule_ticket,
            index: self.index,
            sub_index: self.sub_index,
            visited: true
        }
    }
}

#[derive(PartialEq, Debug)]
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

#[derive(Debug, PartialEq)]
struct FrameBufferValue
{
    final_index: usize,
    opt_frame: Option<Frame>,
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
    (Vec<FrameBufferValue>, HashMap<String, (usize, usize)>),
    TopologicalSortError>
{
    let mut frame_buffer : Vec<FrameBufferValue> = Vec::new();
    let mut to_buffer_index : HashMap<String, (usize, usize)> = HashMap::new();

    let mut current_buffer_index = 0usize;
    rules.sort();
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

        frame_buffer.push(FrameBufferValue
        {
            final_index: 0,
            opt_frame: Some(Frame::from_rule_and_index(rule, current_buffer_index))
        });
        current_buffer_index += 1;
    }

    Ok((frame_buffer, to_buffer_index))
}

struct TopologicalSortMachine
{
    /*  Source paths found in one rule that aren't the targets of another rule */
    source_leaves : BTreeSet<String>,

    /*  The "buffer" referred to by variable-names here is
        the buffer of frames (frame_buffer) */
    frame_buffer : Vec<FrameBufferValue>,

    /*  Sends the target name to a pair of indices:
        - index of the rule in which it's a target
        - index of the target in the rule's target list */
    to_buffer_index : HashMap<String, (usize, usize)>,

    /*  Recall frame_buffer is a vector of options.  That's so that
        the frames can be taken from frame_buffer and added to frames_in_order */
    frames_in_order : Vec<Frame>,
}

/*  Holds the state of the topological sort, so that we can either sort from one origin,
    or continue sorting until all rules have been visited. */
impl TopologicalSortMachine
{
    pub fn new(
        frame_buffer : Vec<FrameBufferValue>,
        to_buffer_index : HashMap<String, (usize, usize)>
    )
    -> Self
    {
        TopologicalSortMachine
        {
            source_leaves : BTreeSet::new(),
            frame_buffer : frame_buffer,
            to_buffer_index : to_buffer_index,
            frames_in_order : vec![],
        }
    }

    /*  Originates a topological sort DFS from the frame indicated by the given index, noting
        the given sub_index as the location of the goal-target in that frame's target list. */
    pub fn sort_once(&mut self, index : usize, sub_index : usize)
    -> Result<(), TopologicalSortError>
    {
        let starting_frame =
        match self.frame_buffer[index].opt_frame.take()
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
                self.frame_buffer[frame.index].final_index = self.frames_in_order.len();
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
                            if let Some(mut frame) = self.frame_buffer[*buffer_index].opt_frame.take()
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
                            self.source_leaves.insert(source.to_owned());
                        },
                    }
                }

                let frame_index = frame.index;
                stack.push(frame.visit());
                indices_in_stack.insert(frame_index);

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
    pub fn get_result(mut self) -> Result<NodePack, TopologicalSortError>
    {
        let mut num_leaves = 0;
        let mut nodes = Vec::new();
        let mut leaves = Vec::new();
        let mut leaf_to_index = HashMap::new();

        for leaf in self.source_leaves
        {
            leaves.push(leaf.clone());
            leaf_to_index.insert(leaf, num_leaves);
            num_leaves += 1;
        }

        for mut frame in self.frames_in_order.drain(..)
        {
            let mut source_indices = vec![];
            for source in frame.sources.drain(..)
            {
                match leaf_to_index.get(&source)
                {
                    Some(index) =>
                    {
                        source_indices.push(SourceIndex::Leaf(*index));
                    },
                    None =>
                    {
                        let (buffer_index, sub_index) = self.to_buffer_index.get(&source).unwrap();
                        source_indices.push(SourceIndex::Pair(
                            self.frame_buffer[*buffer_index].final_index, *sub_index));
                    }
                }
            }

            nodes.push(
                Node
                {
                    targets: frame.targets,
                    source_indices: source_indices,
                    command: frame.command,
                    rule_ticket: frame.rule_ticket,
                }
            );
        }

        Ok(NodePack::new(leaves, nodes))
    }
}

#[derive(Debug, PartialEq)]
pub struct NodePack
{
    pub leaves: Vec<String>,
    pub nodes: Vec<Node>,
}

impl NodePack
{
    #[cfg(test)]
    fn empty() -> Self
    {
        NodePack
        {
            leaves: Vec::new(),
            nodes: Vec::new(),
        }
    }

    fn new(leaves: Vec<String>, nodes: Vec<Node>) -> Self
    {
        NodePack
        {
            leaves: leaves,
            nodes: nodes,
        }
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
    goal_target : &str) -> Result<NodePack, TopologicalSortError>
{
    let (frame_buffer, to_buffer_index) = rules_to_frame_buffer(rules)?;
    let (index, sub_index) =
    match to_buffer_index.get(goal_target)
    {
        Some((index, sub_index)) => (*index, *sub_index),
        None => return Err(TopologicalSortError::TargetMissing(goal_target.to_string())),
    };

    let mut machine = TopologicalSortMachine::new(frame_buffer, to_buffer_index);
    machine.sort_once(index, sub_index)?;
    machine.get_result()
}

/*  For building all targets.  This function calls rules_to_frame_buffer to generate frames for the rules,
    then iterates through all the frames */
pub fn topological_sort_all(
    rules : Vec<Rule>) -> Result<NodePack, TopologicalSortError>
{
    let (frame_buffer, to_buffer_index) = rules_to_frame_buffer(rules)?;
    let frame_buffer_len = frame_buffer.len();
    let mut machine = TopologicalSortMachine::new(frame_buffer, to_buffer_index);
    for index in 0..frame_buffer_len
    {
        machine.sort_once(index, 0)?;
    }

    machine.get_result()
}

#[cfg(test)]
mod tests
{
    use crate::rule::Rule;
    use crate::sort::
    {
        Node,
        NodePack,
        SourceIndex,
        rules_to_frame_buffer,
        topological_sort,
        topological_sort_all,
        TopologicalSortError,
    };


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
                match &frame_buffer[*node_index].opt_frame
                {
                    Some(frame) => assert_eq!(frame.targets[*target_index], "plant"),
                    None => panic!("Expected some node with target 'plant' found None"),
                }

                /*  to_frame_buffer_index maps a target to a pair of indices: the index of the node
                    and the index of the target in the node. */
                let (node_index, target_index) = to_frame_buffer_index.get("tangerine").unwrap();
                assert_eq!(*node_index, 0usize);

                /*  Check that there's a node at that index with the right target */
                match &frame_buffer[*node_index].opt_frame
                {
                    Some(frame) => assert_eq!(frame.targets[*target_index], "tangerine"),
                    None => panic!("Expected some node with target 'tangerine' found None"),
                }

                /*  Get the frame (at index 0), and check that the sources and command are what was set above. */
                match &frame_buffer[*node_index].opt_frame
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
        assert_eq!(rules_to_frame_buffer(
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
        ), Err(TopologicalSortError::TargetInMultipleRules("fruit".to_string())));
    }

    /*  Topological sort the empty set of rules, but with a goal-target.  That should error. */
    #[test]
    fn topological_sort_empty_is_error()
    {
        assert_eq!(topological_sort(vec![], "prune"), Err(TopologicalSortError::TargetMissing("prune".to_string())));
    }

    /*  Topological sort all of an empty set of rules, check that the result is empty. */
    #[test]
    fn topological_sort_all_empty_is_empty()
    {
        assert_eq!(topological_sort_all(vec![]), Ok(NodePack::empty()));
    }

    /*  Topological sort a list of one rule only.  Check the result
        contains a frame with just that one rule's data. */
    #[test]
    fn topological_sort_one_rule()
    {
        let rule = Rule::new(
            vec!["plant".to_string()],
            vec![],
            vec![],
        );

        assert_eq!(
            topological_sort(vec![rule.clone()], "plant"),
            Ok(NodePack::new(
                vec![],
                vec![
                    Node
                    {
                        targets: vec!["plant".to_string()],
                        source_indices: vec![],
                        command : vec![],
                        rule_ticket : rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Topological sort a list of one rule only.  Check the result
        contains a frame with just that one rule's data. */
    #[test]
    fn topological_sort_all_one_rule()
    {
        let rule = Rule::new(
            vec!["plant".to_string()],
            vec![],
            vec![]
        );

        assert_eq!(topological_sort_all(vec![rule.clone()]),
            Ok(NodePack::new(
                vec![],
                vec![
                    Node
                    {
                        targets: vec!["plant".to_string()],
                        source_indices: vec![],
                        command: vec![],
                        rule_ticket : rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Topological sort a list of two rules only, one depends on the other as a source, but
        the order in the given list is backwards.  Check that the topological sort reverses the order. */
    #[test]
    fn topological_sort_two_rules()
    {
        let fruit_rule = Rule::new(
            vec!["fruit".to_string()],
            vec!["plant".to_string()],
            vec!["pick occasionally".to_string()],
        );
        let plant_rule = Rule::new(
            vec!["plant".to_string()],
            vec![],
            vec![],
        );

        assert_eq!(topological_sort(
            vec![
                fruit_rule.clone(),
                plant_rule.clone(),
            ],
            "fruit"),
        Ok(NodePack::new(
            vec![],
            vec![
                Node{
                    targets: vec!["plant".to_string()],
                    source_indices: vec![],
                    command: vec![],
                    rule_ticket : plant_rule.get_ticket(),
                },
                Node{
                    targets: vec!["fruit".to_string()],
                    source_indices: vec![SourceIndex::Pair(0, 0)],
                    command: vec!["pick occasionally".to_string()],
                    rule_ticket : fruit_rule.get_ticket(),
                },
            ])
        ));
    }

    /*  Topological sort all of a list of two rules only, one depends on the other as a source, but
        the order in the given list is backwards.  Check that the topological sort reverses the order. */
    #[test]
    fn topological_sort_all_two_rules()
    {
        let fruit_rule = Rule
        {
            targets: vec!["fruit".to_string()],
            sources: vec!["plant".to_string()],
            command: vec!["pick occasionally".to_string()],
        };

        let plant_rule = Rule
        {
            targets: vec!["plant".to_string()],
            sources: vec![],
            command: vec!["take care of plant".to_string()],
        };

        assert_eq!(topological_sort_all(
            vec![
                plant_rule.clone(),
                fruit_rule.clone(),
            ]),
            Ok(NodePack::new(
                vec![],
                vec![
                    Node
                    {
                        targets: vec!["plant".to_string()],
                        source_indices: vec![],
                        rule_ticket: plant_rule.get_ticket(),
                        command: vec!["take care of plant".to_string()],
                    },
                    Node
                    {
                        targets: vec!["fruit".to_string()],
                        source_indices: vec![SourceIndex::Pair(0,0)],
                        rule_ticket: fruit_rule.get_ticket(),
                        command: vec!["pick occasionally".to_string()],
                    },
                ]
            ))
        );
    }

    /*  Topological sort a DAG that is not a tree.  Four nodes math, physics, graphics, game
        physics and graphics both depend on math, and game depends on physics and graphics. */
    #[test]
    fn topological_sort_four_rules_diamond_already_in_order()
    {
        let math_rule = Rule
        {
            targets: vec!["math".to_string()],
            sources: vec![],
            command: vec!["build math".to_string()],
        };
        let graphics_rule = Rule
        {
            targets: vec!["graphics".to_string()],
            sources: vec!["math".to_string()],
            command: vec!["build graphics".to_string()],
        };
        let physics_rule = Rule
        {
            targets: vec!["physics".to_string()],
            sources: vec!["math".to_string()],
            command: vec!["build physics".to_string()],
        };
        let game_rule = Rule
        {
            targets: vec!["game".to_string()],
            sources: vec!["graphics".to_string(), "physics".to_string()],
            command: vec!["build game".to_string()],
        };

        assert_eq!(topological_sort(
            vec![
                math_rule.clone(),
                graphics_rule.clone(),
                physics_rule.clone(),
                game_rule.clone(),
            ],
            "game"),
            Ok(NodePack::new(
                vec![],
                vec![
                    Node
                    {
                        targets: vec!["math".to_string()],
                        source_indices: vec![],
                        rule_ticket: math_rule.get_ticket(),
                        command: vec!["build math".to_string()],
                    },
                    Node
                    {
                        targets: vec!["graphics".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0)],
                        rule_ticket: graphics_rule.get_ticket(),
                        command: vec!["build graphics".to_string()],
                    },
                    Node
                    {
                        targets: vec!["physics".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0)],
                        rule_ticket: physics_rule.get_ticket(),
                        command: vec!["build physics".to_string()],
                    },
                    Node
                    {
                        targets: vec!["game".to_string()],
                        source_indices: vec![SourceIndex::Pair(1, 0), SourceIndex::Pair(2, 0),],
                        rule_ticket: game_rule.get_ticket(),
                        command: vec!["build game".to_string()],
                    },
                ]
            )
        ));
    }

    /*  Topological sort a DAG that is not a tree.  Four nodes math, physics, graphics, game
        physics and graphics both depend on math, and game depends on physics and graphics.

        This is the same test as above, except the given vector is in the wrong order.  The result
        should be the same as the above.  Part of this is to test well-definedness of order. */
    #[test]
    fn topological_sort_four_rules_diamond_scrambled()
    {
        let math_rule = Rule::new(
            vec!["math".to_string()],
            vec![],
            vec!["build math".to_string()],
        );
        let graphics_rule = Rule::new(
            vec!["graphics".to_string()],
            vec!["math".to_string()],
            vec!["build graphics".to_string()],
        );
        let physics_rule = Rule::new(
            vec!["physics".to_string()],
            vec!["math".to_string()],
            vec!["build physics".to_string()],
        );
        let game_rule = Rule::new(
            vec!["game".to_string()],
            vec!["physics".to_string(), "graphics".to_string()],
            vec!["build game".to_string()],
        );

        assert_eq!(topological_sort(
            vec![
                game_rule.clone(),
                graphics_rule.clone(),
                math_rule.clone(),
                physics_rule.clone(),
            ],
            "game"),
            Ok(NodePack::new(
                vec![],
                vec![
                    Node
                    {
                        targets: vec!["math".to_string()],
                        source_indices: vec![],
                        rule_ticket: math_rule.get_ticket(),
                        command: vec!["build math".to_string()],
                    },
                    Node
                    {
                        targets: vec!["graphics".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0)],
                        rule_ticket: graphics_rule.get_ticket(),
                        command: vec!["build graphics".to_string()],
                    },
                    Node
                    {
                        targets: vec!["physics".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0)],
                        rule_ticket: physics_rule.get_ticket(),
                        command: vec!["build physics".to_string()],
                    },
                    Node
                    {
                        targets: vec!["game".to_string()],
                        source_indices: vec![SourceIndex::Pair(1, 0), SourceIndex::Pair(2, 0),],
                        rule_ticket: game_rule.get_ticket(),
                        command: vec!["build game".to_string()],
                    },
                ]
            )
        ));
    }

    /*  Topological sort a poetry example.  This has two intermediate build results that share
        a source file.  It's a bit like the diamond, except the shared source is not a rule,
        just a file in the file system, and there are other source-files, too.

        The topologial sort should not only put the nodes in order, but also create nodes for the
        source files not specifically represented as rules. */
    #[test]
    fn topological_sort_poem_stright()
    {
        let poem_rule = Rule::new(
            vec!["poem".to_string()],
            vec!["stanza1".to_string(), "stanza2".to_string()],
            vec!["poemcat stanza1 stanza2".to_string()],
        );

        let stanza1_rule = Rule::new(
            vec!["stanza1".to_string()],
            vec!["chorus".to_string(), "verse1".to_string()],
            vec!["poemcat verse1 chorus".to_string()],
        );

        let stanza2_rule = Rule::new(
            vec!["stanza2".to_string()],
            vec!["chorus".to_string(), "verse2".to_string()],
            vec!["poemcat verse2 chorus".to_string()],
        );

        assert_eq!(topological_sort(
            vec![
                stanza1_rule.clone(),
                stanza2_rule.clone(),
                poem_rule.clone(),
            ], "poem"),
            Ok(NodePack::new(
                vec![
                    "chorus".to_string(),
                    "verse1".to_string(),
                    "verse2".to_string(),
                ],
                vec![
                    Node
                    {
                        targets: vec!["stanza1".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(1)],
                        command: vec!["poemcat verse1 chorus".to_string()],
                        rule_ticket: stanza1_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["stanza2".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(2)],
                        command: vec!["poemcat verse2 chorus".to_string()],
                        rule_ticket: stanza2_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["poem".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0), SourceIndex::Pair(1, 0)],
                        command: vec!["poemcat stanza1 stanza2".to_string()],
                        rule_ticket: poem_rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Topological sort a poetry example.  This test is just like the one above but with the
        given list of rules in a different order.  The result should be the same. */
    #[test]
    fn topological_sort_poem_scrambled()
    {
        let poem_rule = Rule::new(
            vec!["poem".to_string()],
            vec!["stanza2".to_string(), "stanza1".to_string()],
            vec!["poemcat stanza1 stanza2".to_string()],
        );

        let stanza1_rule = Rule::new(
            vec!["stanza1".to_string()],
            vec!["verse1".to_string(), "chorus".to_string()],
            vec!["poemcat verse1 chorus".to_string()],
        );

        let stanza2_rule = Rule::new(
            vec!["stanza2".to_string()],
            vec!["verse2".to_string(), "chorus".to_string()],
            vec!["poemcat verse2 chorus".to_string()],
        );

        assert_eq!(topological_sort(
            vec![
                stanza2_rule.clone(),
                poem_rule.clone(),
                stanza1_rule.clone(),
            ], "poem"),
            Ok(NodePack::new(
                vec![
                    "chorus".to_string(),
                    "verse1".to_string(),
                    "verse2".to_string(),
                ],
                vec![
                    Node
                    {
                        targets: vec!["stanza1".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(1)],
                        command: vec!["poemcat verse1 chorus".to_string()],
                        rule_ticket: stanza1_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["stanza2".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(2)],
                        command: vec!["poemcat verse2 chorus".to_string()],
                        rule_ticket: stanza2_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["poem".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0), SourceIndex::Pair(1, 0)],
                        command: vec!["poemcat stanza1 stanza2".to_string()],
                        rule_ticket: poem_rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Topological sort a poetry example.  This test is just like the one above but with the
        given list of rules in a different order.  The result should be the same. */
    #[test]
    fn topological_sort_all_poem_scrambled()
    {
        let poem_rule = Rule::new(
            vec!["poem".to_string()],
            vec!["stanza2".to_string(), "stanza1".to_string()],
            vec!["poemcat stanza1 stanza2".to_string()],
        );

        let stanza1_rule = Rule::new(
            vec!["stanza1".to_string()],
            vec!["verse1".to_string(), "chorus".to_string()],
            vec!["poemcat verse1 chorus".to_string()],
        );

        let stanza2_rule = Rule::new(
            vec!["stanza2".to_string()],
            vec!["verse2".to_string(), "chorus".to_string()],
            vec!["poemcat verse2 chorus".to_string()],
        );

        assert_eq!(topological_sort_all(
            vec![
                stanza2_rule.clone(),
                poem_rule.clone(),
                stanza1_rule.clone(),
            ]),
            Ok(NodePack::new(
                vec![
                    "chorus".to_string(),
                    "verse1".to_string(),
                    "verse2".to_string(),
                ],
                vec![
                    Node
                    {
                        targets: vec!["stanza1".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(1)],
                        command: vec!["poemcat verse1 chorus".to_string()],
                        rule_ticket: stanza1_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["stanza2".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0), SourceIndex::Leaf(2)],
                        command: vec!["poemcat verse2 chorus".to_string()],
                        rule_ticket: stanza2_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["poem".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0), SourceIndex::Pair(1, 0)],
                        command: vec!["poemcat stanza1 stanza2".to_string()],
                        rule_ticket: poem_rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Two independent rules alongside each other, depending on disconnected sources */
    #[test]
    fn topological_sort_all_disconnected_two()
    {
        let poem_rule = Rule::new(
            vec!["poem".to_string()],
            vec!["imagination".to_string()],
            vec!["poemcat stanza1".to_string()],
        );

        let cookie_rule = Rule::new(
            vec!["cookies".to_string()],
            vec!["cookie recipe".to_string()],
            vec!["bake cookies".to_string()],
        );

        assert_eq!(topological_sort_all(
            vec![
                poem_rule.clone(),
                cookie_rule.clone(),
            ]),
            Ok(NodePack::new(
                vec![
                    "cookie recipe".to_string(),
                    "imagination".to_string()
                ],
                vec![
                    Node
                    {
                        targets: vec!["cookies".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0)],
                        command: vec!["bake cookies".to_string()],
                        rule_ticket: cookie_rule.get_ticket(),
                    },
                    Node
                    {
                        targets: vec!["poem".to_string()],
                        source_indices: vec![SourceIndex::Leaf(1)],
                        command: vec!["poemcat stanza1".to_string()],
                        rule_ticket: poem_rule.get_ticket(),
                    }
                ]
            ))
        );
    }

    /*  Two independent rules alongside each other, depending on disconnected sources */
    #[test]
    fn topological_sort_disconnected_two()
    {
        let poem_rule = Rule::new(
            vec!["poem".to_string()],
            vec!["imagination".to_string()],
            vec!["poemcat stanza1".to_string()],
        );

        let cookie_rule = Rule::new(
            vec!["cookies".to_string()],
            vec!["cookie recipe".to_string()],
            vec!["bake cookies".to_string()],
        );

        assert_eq!(topological_sort(
            vec![
                poem_rule.clone(),
                cookie_rule.clone(),
            ], "poem"),
            Ok(NodePack::new(
                vec![
                    "imagination".to_string()
                ],
                vec![
                    Node
                    {
                        targets: vec!["poem".to_string()],
                        source_indices: vec![SourceIndex::Leaf(0)],
                        command: vec!["poemcat stanza1".to_string()],
                        rule_ticket: poem_rule.get_ticket(),
                    }
                ]
            ))
        );
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
    fn topological_sort_make_leaves_for_sources()
    {
        let fruit_rule = Rule
        {
            targets: vec!["fruit".to_string()],
            sources: vec!["plant".to_string()],
            command: vec!["pick occasionally".to_string()],
        };

        let plant_rule = Rule
        {
            targets: vec!["plant".to_string()],
            sources: vec![
                "seed".to_string(),
                "soil".to_string(),
                "sunlight".to_string(),
                "water".to_string(),
            ],
            command: vec!["take care of plant".to_string()],
        };

        assert_eq!(topological_sort(
            vec![
                plant_rule.clone(),
                fruit_rule.clone(),
            ],
            "fruit"),
            Ok(NodePack::new(
                vec![
                    "seed".to_string(),
                    "soil".to_string(),
                    "sunlight".to_string(),
                    "water".to_string(),
                ],
                vec![
                    Node
                    {
                        targets: vec!["plant".to_string()],
                        source_indices: vec![
                            SourceIndex::Leaf(0),
                            SourceIndex::Leaf(1),
                            SourceIndex::Leaf(2),
                            SourceIndex::Leaf(3)
                        ],
                        rule_ticket: plant_rule.get_ticket(),
                        command: vec!["take care of plant".to_string()],
                    },
                    Node
                    {
                        targets: vec!["fruit".to_string()],
                        source_indices: vec![SourceIndex::Pair(0, 0)],
                        rule_ticket: fruit_rule.get_ticket(),
                        command: vec!["pick occasionally".to_string()],
                    },
                ]
            ))
        );
    }
}