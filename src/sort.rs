use std::collections::HashMap;

use crate::rule::Rule;

/*  Assuming source leaf paths and nodes come in two separate lists,
    this enum encodes a reference by index into one of those lists. */
#[derive(Debug, PartialEq, Ord, Eq, PartialOrd)]
pub enum SourceReference
{
    Leaf(String),
    Rule(usize),
}

#[derive(Debug, PartialEq)]
pub struct Node
{
    pub targets: Vec<String>,
    pub sources: Vec<SourceReference>,
    pub command : Vec<String>,
    // pub rule_ticket : Ticket,
}

#[derive(Debug, PartialEq)]
pub struct RuleSortResult
{
    nodes : Vec<Node>
}

/*  Look at rules, build a map sending each target path to the index
    of that target in the rules vector. */
fn get_target_to_node_index(rules : &Vec<Rule>) -> HashMap<String, usize>
{
    let mut target_to_node_index = HashMap::new();
    for (index, rule) in rules.iter().enumerate()
    {
        for target in &rule.targets
        {
            target_to_node_index.insert(target.to_string(), index);
        }
    }
    target_to_node_index
}

/*  Construct this object, consuming a vector of Rules, to get a vector of Nodes. */
impl RuleSortResult
{
    fn new(rules : Vec<Rule>) -> RuleSortResult
    {
        let target_to_node_index = get_target_to_node_index(&rules);

        let mut nodes = vec![];
        for rule in rules.into_iter()
        {
            let mut node_sources = vec![];
            for source in rule.sources.into_iter()
            {
                node_sources.push(
                    match target_to_node_index.get(&source)
                    {
                        None => SourceReference::Leaf(source),
                        Some(index) => SourceReference::Rule(*index),
                    }
                );
            }

            node_sources.sort();

            nodes.push(Node{
                targets: rule.targets,
                sources: node_sources,
                command: rule.command,
            });
        }

        RuleSortResult
        {
            nodes : nodes
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::rule::Rule;
    use crate::sort::
    {
        RuleSortResult,
        Node,
        SourceReference
    };

    /*  Sort an empty rule vector and check for an empty result */
    #[test]
    fn sort_empty()
    {
        assert_eq!(RuleSortResult::new(vec![]),
            RuleSortResult
            {
                nodes: vec![],
            }
        );
    }

    /*  Sort one rule, check the result */
    #[test]
    fn sort_one_source()
    {
        assert_eq!(
            RuleSortResult::new(vec![
                Rule
                {
                    sources: vec!["apple.c".to_string()],
                    targets: vec!["apple.o".to_string()],
                    command: vec!["compile apple.c to apple.o".to_string()],
                }
            ]),
            RuleSortResult
            {
                nodes: vec![
                    Node
                    {
                        sources:vec![SourceReference::Leaf("apple.c".to_string())],
                        targets: vec!["apple.o".to_string()],
                        command: vec!["compile apple.c to apple.o".to_string()],
                    }
                ],
            }
        );
    }

    #[test]
    fn sort_two_sources()
    {
        assert_eq!(
            RuleSortResult::new(vec![
                Rule
                {
                    sources: vec![
                        "math.c".to_string(),
                        "physics.c".to_string(),
                    ],
                    targets: vec!["apple.o".to_string()],
                    command: vec!["compile".to_string()],
                }
            ]),
            RuleSortResult
            {
                nodes: vec![
                    Node
                    {
                        sources:vec![
                            SourceReference::Leaf("math.c".to_string()),
                            SourceReference::Leaf("physics.c".to_string()),
                        ],
                        targets: vec!["apple.o".to_string()],
                        command: vec!["compile".to_string()],
                    }
                ],
            }
        );
    }

    #[test]
    fn sort_three_sources_sorted()
    {
        assert_eq!(
            RuleSortResult::new(vec![
                Rule
                {
                    sources: vec![
                        "c.c".to_string(),
                        "a.c".to_string(),
                        "b.c".to_string(),
                    ],
                    targets: vec!["apple.o".to_string()],
                    command: vec!["compile".to_string()],
                }
            ]),
            RuleSortResult
            {
                nodes: vec![
                    Node
                    {
                        sources:vec![
                            SourceReference::Leaf("a.c".to_string()),
                            SourceReference::Leaf("b.c".to_string()),
                            SourceReference::Leaf("c.c".to_string()),
                        ],
                        targets: vec!["apple.o".to_string()],
                        command: vec!["compile".to_string()],
                    }
                ],
            }
        );
    }
}
